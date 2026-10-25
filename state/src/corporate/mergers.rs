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
use crate::society::housing::CommercialBuilding;
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
    commercial_buildings: &mut [CommercialBuilding],
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

    // Inventory-asset recognition for the distress test: a retail chain that
    // restocked on trade credit holds the goods as a real asset (sellable
    // stock at wholesale cost), but `liquid_capital`/`company_capital` only
    // register the liabilities side. Without this offset, the first big
    // wholesale pull reads as insolvency and cascades the entire retail
    // sector into forced consolidation within ten turns.
    let mut owner_inventory_value: HashMap<String, f64> = HashMap::default();
    for cb in commercial_buildings.iter() {
        if cb.owner_id.is_empty() {
            continue;
        }
        let v: f64 = cb
            .current_inventory
            .values()
            .flat_map(|batches| batches.iter())
            .map(|b| b.quantity * b.acquisition_cost_per_unit.max(0.0))
            .sum();
        if v > 0.0 {
            *owner_inventory_value.entry(cb.owner_id.clone()).or_default() += v;
        }
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
            // v4.21 WS3: Municipal service utilities are never acquisition
            // targets. `MUN-*` companies are non-privatizable
            // (`privatizable: false`, `state_share = 1.0`) public
            // concessions whose "distress" is a JST funding shortfall, not
            // a market failure — every region also seeds a private
            // same-sector PublicServices company
            // (`seed_minimum_viable_supply_chain`), so an underfunded JST
            // would let the private firm absorb the utility AND its
            // public-service buildings in one turn, permanently orphaning
            // the region's municipal staffing. Resolution runs through the
            // JST contract (`ensure_municipal_services_employers`), not the
            // going-concern market. (A MUN may still appear as the
            // ACQUIRER below — municipalization of a failing private
            // service is the intended direction.)
            || matches!(
                target.legal_form,
                crate::entities::legal_form::LegalForm::MunicipalCompany(_)
            )
        {
            continue;
        }

        // Distress trigger: negative cash (overdrawn) or insolvency
        // (liquid capital below liabilities). A merely zero-cash company is
        // illiquid, not distressed — in a thin cash circuit nearly every
        // solvent firm sits at ~0 between settlement and payroll, so using
        // `available_cash <= 0` as a trigger mass-consolidates the economy
        // every turn and destroys the absorbed firms' labor demand.
        let net_liquid_worth = target.liquid_capital
            + owner_inventory_value
                .get(&target.id)
                .copied()
                .unwrap_or(0.0)
            // v4.17 FS-7: escrowed contract receivables are real assets —
            // tranches already debited from the investor, owed on
            // milestone completion. A contractor buying project materials
            // on procurement_credit otherwise reads as insolvent while
            // holding guaranteed contract payments, and gets absorbed —
            // which is how the construction sector consolidated from ~80
            // firms to 4. 50% haircut: cancellation refunds unreleased
            // escrow to the investor, so the receivable is contingent.
            + target
                .extra
                .get("contract_receivables")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                .max(0.0)
                * 0.5;
        // `procurement_credit` is debt too — trade-credit restock books it
        // when the buyer has no bank to issue the overdraft (it flows
        // through `compute_company_capital` as a liability already). A
        // store carrying supplier credit is as leveraged as one carrying a
        // loan; omitting it lets insolvent firms dodge the distress check.
        let is_distressed = target.available_cash < 0.0
            || net_liquid_worth < target.liabilities + target.procurement_credit;
        let target_sector = target.sector;
        let target_region = target.region_id.clone();

        if !is_distressed {
            continue;
        }

        #[cfg(feature = "diagnostic")]
        if target.sector == crate::registries::enums::Sector::LocalServices {
            eprintln!(
                "LS_DISTRESS: id={} cash={:.0} liquid={:.0} inv={:.0} liab={:.0} fte={}/{} rev_last={:.0}",
                target.id,
                target.available_cash,
                target.liquid_capital,
                owner_inventory_value
                    .get(&target.id)
                    .copied()
                    .unwrap_or(0.0),
                target.liabilities,
                target.fulfilled_fte,
                target.target_fte_demand,
                target
                    .financial_history
                    .last()
                    .and_then(|h| h.get("revenue").and_then(|v| v.as_f64()))
                    .unwrap_or(0.0)
            );
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

    // ------------------------------------------------------------------
    // E.2e: Commercial-building titles follow the same going-concern rule
    // as industrial plants — an acquired retail chain's stores belong to the
    // acquirer. The tombstoned targets still carry `merged_into` until the
    // caller's `retain` drops them, so this single sweep can re-point every
    // orphaned store before the owner ids go dangling (which would leave the
    // restock/settlement paths unable to resolve the owner).
    // ------------------------------------------------------------------
    let tombstone_map: HashMap<String, String> = companies
        .iter()
        .filter_map(|c| c.merged_into.clone().map(|acq| (c.id.clone(), acq)))
        .collect();
    if !tombstone_map.is_empty() {
        // Resolve to the live root of each acquisition chain — an
        // acquirer may itself be tombstoned by a later plan in the same
        // sweep, and a single hop would leave the title dangling on a
        // dead company.
        let resolve_root = |start: &str| -> Option<String> {
            let mut cur = start.to_string();
            for _ in 0..8 {
                match tombstone_map.get(&cur) {
                    Some(next) => cur = next.clone(),
                    None => return Some(cur),
                }
            }
            None
        };
        // v4.17 FS-7: contract novation. An acquired contractor's live
        // projects keep `main_contractor_id`/subcontractor ids pointing at
        // the tombstone — the labor-demand and tranche-release lookups then
        // match no company, so the site starves and cancels at progress
        // 0.00. Acquisition novates the going-concern contracts to the
        // acquirer (same-sector, same-region), transferring both the
        // receivable and the obligation to finish the work.
        for building in buildings.iter_mut() {
            if let Some(project) = building.active_project.as_mut() {
                if tombstone_map.contains_key(&project.main_contractor_id) {
                    if let Some(root) = resolve_root(&project.main_contractor_id) {
                        project.main_contractor_id = root;
                    }
                }
                for sub in project.subcontractors.iter_mut() {
                    if tombstone_map.contains_key(&sub.subcontractor_id) {
                        if let Some(root) = resolve_root(&sub.subcontractor_id) {
                            sub.subcontractor_id = root;
                        }
                    }
                }
            }
        }
        if !commercial_buildings.is_empty() {
            for store in commercial_buildings.iter_mut() {
                if tombstone_map.contains_key(&store.owner_id) {
                    if let Some(root) = resolve_root(&store.owner_id) {
                        store.owner_id = root;
                    }
                }
                // Stock batches keep the going-concern rule too — goods owned
                // by a tombstoned company retitle to that company's acquirer
                // (usually the store's new owner; consigned stock follows its
                // own owner's acquisition chain).
                for batches in store.current_inventory.values_mut() {
                    for batch in batches.iter_mut() {
                        if tombstone_map.contains_key(&batch.owner_id) {
                            if let Some(root) = resolve_root(&batch.owner_id) {
                                batch.owner_id = root;
                            }
                        }
                    }
                }
            }
        }
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
    companies[acquirer_idx].open_bid_credit += companies[target_idx].open_bid_credit;
    companies[acquirer_idx].construction_escrow +=
        companies[target_idx].construction_escrow;
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
    // W11: credit_cash and debit_cash were aggregated onto the acquirer
    // above — the tombstone must surrender them or the same encumbrance is
    // counted twice (debit_cash is M0 for unbanked targets: fiat creation).
    companies[target_idx].credit_cash = 0.0;
    companies[target_idx].debit_cash = 0.0;
    companies[target_idx].open_bid_credit = 0.0;
    companies[target_idx].construction_escrow = 0.0;
    companies[target_idx].outstanding_loans.clear();

    companies[acquirer_idx].company_capital = (companies[acquirer_idx].fixed_capital
        + companies[acquirer_idx].liquid_capital
        - companies[acquirer_idx].liabilities)
        .max(0.0);
    if companies[acquirer_idx].worker_capacity >= 25_000 {
        companies[acquirer_idx].is_national_champion = true;
    }
}



//! B2B order submission, trade settlement, and production execution.
//!
//! This module implements the core Phase 6.3 (Production Planning),
//! Phase 6.4a (Double-Entry Settlement), and Phase 6.4b (Production Execution)
//! of the real economy turn loop.
//!
//! # Key Invariants
//! * Inventory lives ONLY on `Building.inventory`. There is no `Company.inventory` field.
//! * Company aggregate inventory is computed dynamically via `compute_company_inventory`.
//! * Goods in trades route directly to the specific `Building.inventory` that requested them.
//! * Wages are NOT paid here — they are settled in Phase W1 by `resolve_regional_labor_market`.
//! * Overflow that cannot be warehoused perishes immediately (no next-turn buffer).

use crate::economy::b2b_config::B2bOrderConfig;
use crate::economy::fixed_assets::draft_animal_maintenance_needed;
use crate::economy::generative_goods_config::GenerativeGoodsConfig;
use crate::economy::market_history::{get_reference_price, moving_average_vwap, MarketHistory};
use crate::economy::order_book::{Ask, Bid, OrderBook, Trade};
use crate::economy::production::ProductionResult;
use crate::economy::transfer_settler::{credit_company_by_id, debit_company_by_id};
use crate::entities::{Building, Company};
use crate::entities::legal_form::LegalForm;
use crate::registries::enums::{Commodity, Sector};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

type HashMap<K, V> = FxHashMap<K, V>;

/// Dynamically compute aggregate inventory for a company by iterating its buildings.
///
/// # Arguments
/// * `company` - The company to compute inventory for.
/// * `buildings` - All buildings (filtered by `owner_id == company.id`).
///
/// # Returns
/// A `BTreeMap<Commodity, f64>` of total inventory quantities.
///
/// # Rules
/// * Read-only computation — NO state is stored on `Company`.
/// * Prevents data duplication and duping bugs.
pub fn compute_company_inventory(
    company: &Company,
    buildings: &[Building],
) -> BTreeMap<Commodity, f64> {
    let mut total: BTreeMap<Commodity, f64> = BTreeMap::new();
    for b in buildings.iter().filter(|b| b.owner_id == company.id) {
        for (&commodity, &qty) in &b.inventory {
            *total.entry(commodity).or_insert(0.0) += qty;
        }
    }
    total
}

/// Phase 94: Index-based variant — compute aggregate inventory using
/// pre-looked-up building indices instead of scanning all buildings.
pub fn compute_company_inventory_indexed(
    buildings: &[Building],
    building_indices: &[usize],
) -> BTreeMap<Commodity, f64> {
    let mut total: BTreeMap<Commodity, f64> = BTreeMap::new();
    for &i in building_indices {
        for (&commodity, &qty) in &buildings[i].inventory {
            *total.entry(commodity).or_insert(0.0) += qty;
        }
    }
    total
}

/// Calculate the total inventory capacity for a company's buildings.
#[allow(dead_code)]
fn compute_company_inventory_capacity(company: &Company, buildings: &[Building]) -> f64 {
    buildings
        .iter()
        .filter(|b| b.owner_id == company.id)
        .map(|b| b.inventory_capacity)
        .sum()
}

/// Phase 94: Index-based variant — compute inventory capacity using
/// pre-looked-up building indices instead of scanning all buildings.
fn compute_company_inventory_capacity_indexed(
    buildings: &[Building],
    building_indices: &[usize],
) -> f64 {
    building_indices
        .iter()
        .map(|&i| buildings[i].inventory_capacity)
        .sum()
}

/// Calculate the utilization ratio of a company's inventory.
///
/// # Returns
/// A value in `[0.0, 1.0+]` representing current inventory / max capacity.
#[allow(dead_code)]
fn compute_inventory_utilization(company: &Company, buildings: &[Building]) -> f64 {
    let capacity = compute_company_inventory_capacity(company, buildings);
    if capacity <= 0.0 {
        return 0.0;
    }
    let total_qty: f64 = compute_company_inventory(company, buildings).values().sum();
    total_qty / capacity
}

/// Phase 94: Index-based variant — compute inventory utilization using
/// pre-looked-up building indices instead of scanning all buildings.
fn compute_inventory_utilization_indexed(
    buildings: &[Building],
    building_indices: &[usize],
) -> f64 {
    let capacity = compute_company_inventory_capacity_indexed(buildings, building_indices);
    if capacity <= 0.0 {
        return 0.0;
    }
    let mut total_qty: f64 = 0.0;
    for &i in building_indices {
        for (&_commodity, &qty) in &buildings[i].inventory {
            total_qty += qty;
        }
    }
    total_qty / capacity
}

/// Calculate dynamic markup based on inventory utilization.
///
/// # Arguments
/// * `utilization` - Current inventory utilization (0.0 to 1.0+).
/// * `config` - B2B order configuration with thresholds.
///
/// # Returns
/// Markup ratio to apply to unit cost.
///
/// # Rules
/// * `utilization >= fire_sale_threshold` → `min_markup_ratio` (fire sale pricing).
/// * `utilization <= scarcity_threshold` → `max_markup_ratio` (scarcity premium).
/// * Between thresholds → linear interpolation.
pub fn calculate_dynamic_markup(utilization: f64, config: &B2bOrderConfig) -> f64 {
    if utilization >= config.fire_sale_threshold {
        return config.min_markup_ratio;
    }
    if utilization <= config.scarcity_threshold {
        return config.max_markup_ratio;
    }
    // Linear interpolation between scarcity (high markup) and fire sale (low markup)
    let range = config.fire_sale_threshold - config.scarcity_threshold;
    if range <= 0.0 {
        return config.min_markup_ratio;
    }
    let t = (utilization - config.scarcity_threshold) / range;
    config.max_markup_ratio * (1.0 - t) + config.min_markup_ratio * t
}

/// Calculate the unit cost of production for a building.
///
/// # Arguments
/// * `building` - Building with active production method.
/// * `reference_prices` - Market reference prices for input commodities.
/// * `base_wage` - Average wage per FTE.
///
/// # Returns
/// Unit cost per output unit.
///
/// # Rules
/// * Unit Cost = (sum of input_qty × reference_price + wage_cost) / output_volume.
/// * If output_volume is zero, returns 0.0.
pub fn calculate_unit_cost(
    building: &Building,
    reference_prices: &HashMap<Commodity, f64>,
    base_wage: f64,
) -> f64 {
    let method = &building.active_method;
    // `current_employment` is per-virtual-plant; per-1k rates apply to the
    // entity's total workforce (per-plant x scale_factor). Using the raw
    // value under-costs every multi-plant entity by scale_factor.
    let entity_employment =
        building.current_employment as f64 * building.scale_factor.max(1) as f64;
    let production_scale = entity_employment / 1000.0;

    let mut sorted_inputs: Vec<(Commodity, f64)> =
        method.inputs.iter().map(|(&c, &q)| (c, q)).collect();
    sorted_inputs.sort_by_key(|(c, _)| *c);
    let mut input_cost = 0.0;
    for (commodity, qty_per_1k) in &sorted_inputs {
        // Worker sustenance is covered by wages (workers buy food B2C) —
        // Food in a BOM is not a production cost borne by the firm.
        if *commodity == Commodity::Food {
            continue;
        }
        let price = reference_prices.get(commodity).copied().unwrap_or(0.0);
        input_cost += qty_per_1k * price * production_scale;
    }

    let wage_cost = entity_employment * base_wage;

    let mut output_volume = 0.0;
    let mut sorted_outputs: Vec<f64> =
        method.outputs.values().copied().collect();
    sorted_outputs.sort_by(|a, b| a.total_cmp(b));
    for qty_per_1k in &sorted_outputs {
        output_volume += qty_per_1k * production_scale;
    }

    if output_volume > 0.0 {
        (input_cost + wage_cost) / output_volume
    } else {
        0.0
    }
}

/// W7: Encumber bid cash from the pocket that actually holds the company's
/// spendable money. For banked companies the funds live in
/// `brokerage_account.cash` (mirrored by the primary-bank deposit liability);
/// `available_cash` is a phantom pocket there. Draining it drove every banked
/// buyer's spendable balance permanently negative — refunds returned escrow
/// to brokerage.cash while available_cash kept the deficit, so each bid cycle
/// silently moved the encumbrance into a dead ledger line.
fn encumber_cash(company: &mut Company, amount: f64) {
    // Cash pockets floor at 0.0 — base cash may never go negative (the
    // −5.10B void). The encumbrance portion that exceeds spendable cash is
    // booked as explicit `procurement_credit`: solvency-bounded trade
    // credit that shrinks `company_capital`, is contra-counted in the M0
    // walk for unbanked firms, and is retired by revenue garnish. The
    // credit-backed share of the open encumbrance is also tracked in
    // `open_bid_credit` so an unfilled bid can discharge the obligation
    // it provisionally booked instead of ratcheting debt forever.
    let pocket = if let Some(ba) = &mut company.brokerage_account {
        &mut ba.cash
    } else {
        &mut company.available_cash
    };
    let paid = amount.min((*pocket).max(0.0));
    *pocket -= paid;
    let credit = amount - paid;
    if credit > 0.0 {
        company.procurement_credit += credit;
        company.open_bid_credit += credit;
    }
    company.debit_cash += amount;
}

/// Working-capital facility for generic procurement: spendable liquid plus
/// net worth MINUS the contract-receivable share of `company_capital`.
/// Receivables count at 0.5 for solvency (an escrowed tranche owed on
/// certified work is a real IFRS-15 contract asset — `compute_company_capital`),
/// but the same receivable is already spoken for: it funds the payroll and
/// site materials of the project that earned it. Re-collateralizing it for
/// restock/maintenance/capex bids let stalled contractors draw ~45M/turn of
/// procurement credit their crews could never work off; the ratcheted balance
/// then zeroed payroll capacity (construction pacing subtracts the full
/// credit stock), pinning the national sector at site-skeleton crews
/// (v4.19 FS-7 fix: FTE had decayed 134K → 2K). Firms without receivables
/// are unaffected.
fn procurement_working_capital_facility(company: &Company, liquid: f64) -> f64 {
    let receivable_collateral = company
        .extra
        .get("contract_receivables")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
        .max(0.0)
        * 0.5;
    liquid + (company.company_capital - receivable_collateral).max(0.0)
}

/// Release the `(limit − exec) × qty` slack on a filled bid portion.
///
/// The bid's encumbrance locked `qty × limit`; settlement consumed only
/// `qty × exec` (plus freight drawn separately). The residual slack was never
/// owed to any counterparty — without an explicit release it stays locked in
/// `debit_cash` forever, and its credit-backed share keeps ratcheting
/// `procurement_credit` against obligations that never existed (the
/// ministries' `bid_limit_price − execution_price` refund is the same
/// reconciliation, `ministries.rs::run_b2b_procurement`). The cash-backed
/// share returns to the pocket it was drawn from (brokerage first, matching
/// `encumber_cash`); the credit-backed share discharges as a phantom
/// obligation via `release_bid_encumbrance`.
fn release_fill_slack(buyer: &mut Company, quantity: f64, limit: f64, exec: f64) {
    let slack = quantity * (limit - exec).max(0.0);
    if slack <= 0.0 {
        return;
    }
    let cash_back = buyer.release_bid_encumbrance(slack);
    if let Some(ba) = &mut buyer.brokerage_account {
        ba.cash += cash_back;
    } else {
        buyer.available_cash += cash_back;
    }
}

/// Submit B2B Buy Bids and Sell Asks for all companies based on their buildings' BOMs.
///
/// # Arguments
/// * `companies` - Mutable slice of all companies (cash is encumbered).
/// * `buildings` - Slice of all buildings (read-only for BOM and inventory lookup).
/// * `order_book` - Mutable order book to submit bids and asks into.
/// * `market_history` - Historical price data for reference prices.
/// * `config` - B2B order configuration.
///
/// # Returns
/// A vector of diagnostic messages.
///
/// # Rules
/// * For each company, iterate its buildings to compute aggregate inventory.
/// * Submit Buy Bids for each input commodity in each building's BOM.
/// * Submit Sell Asks for each output commodity in each building's BOM.
/// * Cash is encumbered: `available_cash -= encumbrance`, `debit_cash += encumbrance`.
/// * Skip bids if `available_cash` is insufficient.
/// * Sell Ask limit price = unit_cost × (1 + dynamic_markup).
/// * Buy Bid limit price = reference_price × (1 + buy_premium_ratio).
pub fn submit_company_b2b_orders(
    companies: &mut [Company],
    buildings: &[Building],
    order_book: &mut OrderBook,
    market_history: &MarketHistory,
    config: &B2bOrderConfig,
    gen_config: &GenerativeGoodsConfig,
    owner_to_buildings: &rustc_hash::FxHashMap<String, Vec<usize>>,
) -> Vec<String> {
    let mut messages = Vec::new();

    // Phase 95: Build a global blueprint lookup (blueprint_id → (quality, durability))
    // before the mutable company loop to avoid borrow conflicts.
    let all_bp_metadata: std::collections::HashMap<String, (f64, f64)> = companies
        .iter()
        .flat_map(|c| c.blueprints.iter())
        .map(|bp| (bp.id.clone(), (bp.quality, bp.durability)))
        .collect();

    #[cfg(feature = "diagnostic")]
    {
        // HC probe: how much HardCoal demand SHOULD post (capacity basis).
        let mut consumers = 0usize;
        let mut want_units = 0.0f64;
        let mut orphan_bldg = 0usize;
        for b in buildings {
            if let Some(&q1k) = b.active_method.inputs.get(&Commodity::HardCoal) {
                consumers += 1;
                let ps = b.worker_capacity as f64 * b.scale_factor.max(1) as f64 / 1000.0;
                let want = q1k * ps * 2.0
                    - b.inventory
                        .get(&Commodity::HardCoal)
                        .copied()
                        .unwrap_or(0.0);
                if want > 0.0 {
                    want_units += want;
                }
            }
        }
        for b in buildings {
            let owned = owner_to_buildings
                .get(&b.owner_id)
                .map(|v| !v.is_empty())
                .unwrap_or(false);
            if !owned {
                orphan_bldg += 1;
            }
        }
        eprintln!(
            "HC_WANT: consumers={} want_units={:.3e} orphan_bldg={}/{}",
            consumers,
            want_units,
            orphan_bldg,
            buildings.len()
        );
    }
    #[cfg(feature = "diagnostic")]
    let mut diag_companies_total = 0usize;
    #[cfg(feature = "diagnostic")]
    let mut diag_companies_no_bldg = 0usize;
    #[cfg(feature = "diagnostic")]
    let (
        mut diag_desired,
        mut diag_maxenc,
        mut diag_posted,
        mut diag_liquid,
        mut diag_facility,
        mut diag_payroll_reserve,
    ) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    #[cfg(feature = "diagnostic")]
    let (mut diag_budget_limited, mut diag_need_limited, mut diag_arrears_cos) =
        (0usize, 0usize, 0usize);
    #[cfg(feature = "diagnostic")]
    let mut diag_top_desire: Vec<(f64, f64, String, String)> = Vec::new();

    for company in companies.iter_mut() {
        #[cfg(feature = "diagnostic")]
        {
            diag_companies_total += 1;
        }
        // Sync available_cash from brokerage account
        let liquid = company
            .brokerage_account
            .as_ref()
            .map(|ba| ba.cash.max(0.0))
            .unwrap_or_else(|| company.available_cash.max(0.0));
        // Phase 94: Do NOT overwrite available_cash (M0 duplication for unbanked companies)

        // Phase 94: Use pre-built index map instead of O(B) filter scan.
        let company_building_indices = match owner_to_buildings.get(&company.id) {
            Some(indices) if !indices.is_empty() => indices.clone(),
            _ => {
                #[cfg(feature = "diagnostic")]
                {
                    diag_companies_no_bldg += 1;
                }
                continue;
            }
        };

        // Compute inventory utilization for dynamic pricing (index-based)
        let utilization =
            compute_inventory_utilization_indexed(buildings, &company_building_indices);
        let dynamic_markup = calculate_dynamic_markup(utilization, config);

        // Phase 76: Bootstrap pricing when no VWAP exists (Turn 0 condition).
        // When the market has no transaction history, the dynamic markup would
        // set seller asks at 3× reference (scarcity pricing for empty inventory),
        // while buyers bid at 1.05× reference. This spread never crosses, so no
        // trades execute and no VWAP is ever established — a deadlock.
        // Bootstrap: use min_markup_ratio (0.0) so sellers ask at ref×1.0 while
        // buyers bid at ref×1.05, guaranteeing spread crossing on Turn 0.
        let is_bootstrap = market_history.vwap_per_commodity.is_empty()
            && market_history.last_trade_price.is_empty();
        let markup = if is_bootstrap {
            config.min_markup_ratio
        } else {
            dynamic_markup
        };

        // Maximum cash to encumber for input purchases.
        // 12-turn mandate: procurement is solvency-bounded, not
        // spendable-cash-bounded. Payroll and arrears drain `liquid` to ≤0
        // economy-wide while companies still hold positive equity — under a
        // pure cash gate, bids collapse to zero, sellers never get paid, and
        // the input chain freezes permanently. Treat a bounded share of
        // `company_capital` (fixed+liquid−liabilities) as a working-capital
        // facility: the deficit lands in `available_cash` as implicit trade
        // credit (same pattern as wage arrears) and is expensed into
        // `liquid_capital` when the stock is consumed — insolvent firms
        // still post nothing.
        // `desired_qty` (2-turn input cover − on-hand) already bounds the
        // bid volume to actual need, so the facility can extend to full net
        // worth without over-procuring — the solvency bound is the real
        // safety limit.
        // v4.19 FS-7: the facility excludes contract receivables — they are
        // already spoken for by the project that earned them (payroll +
        // site materials). See `procurement_working_capital_facility`.
        let working_capital_facility = procurement_working_capital_facility(company, liquid);
        // 24-turn fix: reserve the payroll before procurement — the facility
        // extends to net worth, which let restock bids consume the cash (and
        // implicit credit) needed to cover this turn's wage bill plus the
        // outstanding arrears backlog. Wages come first.
        // The reserve is capped at spendable liquidity: wage bills can exceed
        // total cash by orders of magnitude, and an uncapped obligation would
        // zero the facility entirely (no inputs → no production → no sales →
        // wages never get paid anyway). We protect the cash that exists;
        // implicit credit still covers procurement against net worth.
        // The reserve must cover TWO payrolls, not one: `evaluate_furlough`
        // triggers cash-flow distress at operational_cash < 2× payroll. A
        // 1× reserve let procurement encumber the cash cushion that keeps a
        // company out of the furlough check — the bids that were meant to
        // restart demand instead induced the furlough they replaced.
        // The obligation is reserved against the WHOLE facility base, not
        // just spendable liquid: capping at `liquid` collapsed the reserve
        // to zero precisely when arrears were largest, letting procurement
        // draw the full net-worth facility while payroll went unpaid
        // (ARREARS-1). A firm whose arrears exceed its facility base is
        // balance-sheet insolvent — it must not keep buying inputs on
        // credit while its workers starve.
        let payroll_reserve = (company.wages_paid_this_turn * 2.0
            + company.wage_arrears
            + company.severance_arrears)
            .min(working_capital_facility.max(0.0));
        let mut max_encumber =
            (working_capital_facility - payroll_reserve).max(0.0)
                * config.max_cash_encumbrance_ratio;
        // ARREARS-1: a company carrying unpaid wages may not encumber beyond
        // the cash it actually holds. Implicit procurement credit against net
        // worth pre-commits revenue before payroll ever sees it — the pocket
        // ratchets negative while the books show profit. While arrears exist,
        // procurement is cash-only; inflows then refill the pocket and the
        // payroll sweep retires the arrears.
        // Macro-Remediation: ...except a bounded restart tranche. An
        // arrears-drowned firm under pure cash-only procurement can never
        // buy inputs, so it can never produce, earn, or repay — the state
        // is absorbing (the 4.1M-worker furlough trap: material-starved
        // plants with zero cash and unpaid crews, unable to bid at all).
        // A small equity-bounded tranche lets the plant partially restock
        // and restart; the payroll sweep routes the resulting revenue to
        // the arrears first, so the tranche accelerates repayment instead
        // of pre-committing it. Bounded by net worth — an insolvent firm
        // (facility <= 0) still posts nothing.
        if company.wage_arrears > 0.01 || company.severance_arrears > 0.01 {
            // 0.15 left trapped firms at ~6% BOM cover — enough to bid, never
            // enough to clear the min-input pin, so the tranche funded slivers
            // of inventory while the plant stayed throttled. 0.40 gives a real
            // restart draw while remaining inside the net-worth facility.
            const ARREARS_RESTART_FRACTION: f64 = 0.40;
            let restart_tranche = working_capital_facility.max(0.0)
                * config.max_cash_encumbrance_ratio
                * ARREARS_RESTART_FRACTION;
            max_encumber = max_encumber.min(liquid.max(0.0)).max(restart_tranche);
        }
        let mut total_encumbered = 0.0;

        // Phase 94: Use pre-looked-up building indices instead of O(B) filter.
        let company_buildings: Vec<&Building> = company_building_indices
            .iter()
            .map(|&i| &buildings[i])
            .collect();

        if company_buildings.is_empty() {
            continue;
        }

        // Phase 95: Build a blueprint lookup for this company's owned + licensed blueprints.
        // blueprint_id → (quality, durability)
        let bp_lookup: std::collections::HashMap<String, (f64, f64)> = {
            let mut m = std::collections::HashMap::new();
            for bp in &company.blueprints {
                m.insert(bp.id.clone(), (bp.quality, bp.durability));
            }
            for licensed in &company.licensed_blueprints {
                if let Some(&(q, d)) = all_bp_metadata.get(&licensed.blueprint_id) {
                    m.insert(licensed.blueprint_id.clone(), (q, d));
                }
            }
            m
        };

        // Submit Buy Bids for inputs.
        // 12-turn mandate: collect every input requirement first, then post
        // all bids at a uniform coverage fraction of the facility. The naive
        // in-order loop funded early BOM entries to 2-turn cover and starved
        // later ones entirely — but fulfillment_ratio is the MINIMUM input
        // ratio, so a single missing input pins production at 0 even when
        // 90% of the BOM arrived. Covering every input at `coverage`
        // fraction keeps the min ratio > 0 and production alive.
        // (commodity, desired_qty, limit_price, unit_price_with_freight)
        let mut input_requirements: Vec<(Commodity, f64, f64, f64)> =
            Vec::new();
        for building in &company_buildings {
            let method = &building.active_method;
            let mut sorted_inputs: Vec<(Commodity, f64)> = method
                .inputs
                .iter()
                .map(|(&c, &q)| (c, q))
                .collect();
            sorted_inputs.sort_by_key(|(c, _)| *c);
            // W9: procure against physical capacity, not current employment.
            // A furloughed building (employment→0) would otherwise bid zero —
            // its buffer then sells/drains unprotected and it can never
            // restock to restart, a permanent-furlough ratchet. Capacity is
            // also the genesis seed basis, keeping bid cover symmetric with
            // the endowment it replaces.
            // `worker_capacity` is per-virtual-plant — entity procurement
            // needs the full seat count (x scale_factor), else a multi-plant
            // entity bids for one plant while consuming at entity scale.
            let production_scale = building.worker_capacity as f64
                * building.scale_factor.max(1) as f64
                / 1000.0;

            for &(commodity, qty_per_1k) in &sorted_inputs {
                // Phase 19B: Fixed-asset commodities (machinery/vehicles) are no
                // longer consumed per-turn — they're installed as FixedAssetCohorts.
                // Skip them here; they're procured via separate asset-purchase bids.
                if commodity.is_fixed_asset() {
                    continue;
                }
                // Phase 80: Skip local utility commodities (Energy, Heat).
                // These are distributed by the grid, not procured via B2B market.
                if commodity.is_local_utility() {
                    continue;
                }
                // Food is worker sustenance bought B2C from wages, not a
                // firm-procured input — never bid for it.
                if commodity == Commodity::Food {
                    continue;
                }

                // W7: bid the shortfall against on-hand stock, not the full
                // per-turn requirement. A building already holding a 2-turn
                // buffer bids nothing; one below cover tops up.
                let per_turn_need = qty_per_1k * production_scale;
                let on_hand = building.inventory.get(&commodity).copied().unwrap_or(0.0);
                // Deficit-only restock bidding (two-turn stock cover).
                // Fulfillment-scaled consumption replenishment was tested
                // and reverted: bidding at consumption rate while revenue
                // is zero burns working capital into insolvency — controlled
                // A/B runs cost ~40 points of aggregate unemployment
                // (11.9% → 51.4%). The economy is income-demand
                // constrained, not inventory-signaling constrained.
                let restock_deficit = (per_turn_need * 2.0 - on_hand).max(0.0);
                let desired_qty = restock_deficit;
                if desired_qty <= 0.0 {
                    continue;
                }

                let ref_price = get_reference_price(&commodity, market_history);
                if ref_price.is_none() {
                    messages.push(format!(
                        "Company {}: No reference price for {:?}, skipping buy bid",
                        company.id, commodity
                    ));
                    continue;
                }

                // Phase 45: Dynamic buyer pricing with unfilled-order feedback.
                // If this commodity had an unfilled bid last turn, raise the bid
                // price by 20% above the last unfilled price. The ONLY ceiling
                // is the buyer's max_affordable_budget (cash encumbrance limit).
                // NO profitability ceiling is applied.
                // v4.18: the ratchet is bounded by the seller-side ask ceiling.
                // `limit_price` sizes the cash encumbrance, so an unbounded
                // ratchet let a single chronically-unfilled input compound to
                // ~10^8 × reference, exploding `total_desired_encumbrance` and
                // driving the shared `coverage` ratio to ~0 — every bid the
                // company posted scaled to zero quantity and procurement
                // collapsed economy-wide. The cap mirrors `max_markup_ratio`:
                // sellers never ask above (1+markup) × cost, so a bid at the
                // same multiple of the market reference can cross any ask the
                // book can legitimately carry; unfilled asks decay toward the
                // cap on the seller side.
                let base_price = ref_price.unwrap();
                let last_unfilled = company
                    .unfilled_bid_prices
                    .get(&commodity)
                    .copied()
                    .unwrap_or(0.0);
                let limit_cap = base_price * (1.0 + config.max_markup_ratio);
                let limit_price = if last_unfilled > 0.0 {
                    (last_unfilled * 1.20)
                        .min(limit_cap)
                        .max(base_price * (1.0 + config.buy_premium_ratio))
                } else {
                    base_price * (1.0 + config.buy_premium_ratio)
                };
                // Phase 25: Include a freight cost reserve in the encumbrance so
                // buyers have enough cash to pay for transport when the trade settles.
                let unit_price_with_freight =
                    limit_price * (1.0 + config.freight_cost_reserve_ratio);
                input_requirements.push((
                    commodity,
                    desired_qty,
                    limit_price,
                    unit_price_with_freight,
                ));
                #[cfg(feature = "diagnostic")]
                if commodity == Commodity::HardCoal {
                    eprintln!(
                        "HC_REQ: buyer={} bldg={} desired={:.1} px={:.1} fac={:.3e} enc={:.3e}",
                        company.id, building.id, desired_qty, limit_price,
                        working_capital_facility,
                        desired_qty * unit_price_with_freight
                    );
                }
            }
        }

        let total_desired_encumbrance: f64 = input_requirements
            .iter()
            .map(|&(_, qty, _, upf)| qty * upf)
            .sum();
        let coverage = if total_desired_encumbrance > max_encumber
            && total_desired_encumbrance > 0.0
        {
            (max_encumber / total_desired_encumbrance).max(0.0)
        } else {
            1.0
        };
        #[cfg(feature = "diagnostic")]
        {
            diag_desired += total_desired_encumbrance;
            diag_maxenc += max_encumber;
            diag_posted += total_desired_encumbrance * coverage;
            if total_desired_encumbrance > 1.0 {
                if coverage < 0.999 {
                    diag_budget_limited += 1;
                } else {
                    diag_need_limited += 1;
                }
            }
            if company.wage_arrears > 0.01 || company.severance_arrears > 0.01 {
                diag_arrears_cos += 1;
            }
            diag_liquid += liquid.max(0.0);
            diag_facility += working_capital_facility.max(0.0);
            diag_payroll_reserve += payroll_reserve;
            if total_desired_encumbrance > 1.0 {
                diag_top_desire.push((
                    total_desired_encumbrance,
                    coverage,
                    company.id.clone(),
                    input_requirements
                        .iter()
                        .map(|&(c, q, lp, _)| format!("{:?}:{:.0}@{:.0}", c, q, lp))
                        .collect::<Vec<_>>()
                        .join(","),
                ));
            }
        }
        for &(commodity, desired_qty, limit_price, unit_price_with_freight)
            in &input_requirements
        {
            let bid_qty = desired_qty * coverage;
            let bid_encumbrance = bid_qty * unit_price_with_freight;
            if bid_qty <= 0.0 || bid_encumbrance <= 0.0 {
                continue;
            }
            encumber_cash(company, bid_encumbrance);
            total_encumbered += bid_encumbrance;

            order_book.bids.entry(commodity).or_default().push(Bid {
                buyer_id: company.id.clone(),
                commodity,
                quantity: bid_qty,
                limit_price,
                blueprint_id: None,
                min_quality: None,
            });
        }

        // Phase 23A: Submit Buy Bids for Draft Animal maintenance (Fodder + Water).
        // Buildings with DraftAnimals cohorts need animal feed, distinct from
        // the MaintenanceServices consumed by machinery cohorts.
        for building in &company_buildings {
            let needed = draft_animal_maintenance_needed(&building.fixed_assets, gen_config);
            if needed.is_empty() {
                continue;
            }
            let mut sorted_needed: Vec<(Commodity, f64)> =
                needed.iter().map(|(&c, &q)| (c, q)).collect();
            sorted_needed.sort_by_key(|(c, _)| *c);
            for &(commodity, desired_qty) in &sorted_needed {
                if desired_qty <= 0.0 {
                    continue;
                }
                let ref_price = get_reference_price(&commodity, market_history);
                let limit_price = match ref_price {
                    Some(p) => p * (1.0 + config.buy_premium_ratio),
                    None => continue,
                };
                // Phase 25 parity: encumber the freight reserve as well,
                // matching the main input-bid path above.
                let commodity_cost = desired_qty * limit_price;
                let encumbrance =
                    commodity_cost * (1.0 + config.freight_cost_reserve_ratio);

                if total_encumbered + encumbrance > max_encumber {
                    let remaining = max_encumber - total_encumbered;
                    if remaining <= 0.0 || limit_price <= 0.0 {
                        continue;
                    }
                    let total_per_unit =
                        limit_price * (1.0 + config.freight_cost_reserve_ratio);
                    let affordable_qty = remaining / total_per_unit;
                    if affordable_qty <= 0.0 {
                        continue;
                    }
                    let partial_encumbrance = affordable_qty * total_per_unit;
                    encumber_cash(company, partial_encumbrance);
                    total_encumbered += partial_encumbrance;

                    order_book.bids.entry(commodity).or_default().push(Bid {
                        buyer_id: company.id.clone(),
                        commodity,
                        quantity: affordable_qty,
                        limit_price,
                        blueprint_id: None,
                        min_quality: None,
                    });
                } else {
                    encumber_cash(company, encumbrance);
                    total_encumbered += encumbrance;

                    order_book.bids.entry(commodity).or_default().push(Bid {
                        buyer_id: company.id.clone(),
                        commodity,
                        quantity: desired_qty,
                        limit_price,
                        blueprint_id: None,
                        min_quality: None,
                    });
                }
            }
        }

        // Submit Sell Asks for outputs
        for building in &company_buildings {
            let method = &building.active_method;
            let production_scale = building.current_employment as f64
                * building.scale_factor.max(1) as f64
                / 1000.0;

            // Calculate unit cost for this building's output
            let mut ref_prices: HashMap<Commodity, f64> = HashMap::default();
            for &input_commodity in method.inputs.keys() {
                if let Some(price) = get_reference_price(&input_commodity, market_history) {
                    ref_prices.insert(input_commodity, price);
                }
            }

            let unit_cost = calculate_unit_cost(building, &ref_prices, 0.0);

            for (&commodity, &qty_per_1k) in &method.outputs {
                // Phase 87+ Recovery: Bounded Rationality Sales Algorithm.
                // Replaces the hardcoded MAX_THROUGHPUT_MULTIPLIER clamp with
                // a price-signal and cash-flow-distress-driven sell fraction.
                //
                // Rational actors sell more when:
                //   - Prices are high (VWAP > moving average → price_signal > 0)
                //   - Cash is short (payroll coverage low → distress_signal > 0)
                // And sell less when prices are low and cash is ample.
                //
                // sell_fraction = 0.5 (base) + price_signal * 0.3 + distress_signal * 0.2
                // clamped to [0.2, 1.0] — never floods nor starves the market.
                let production_qty =
                    qty_per_1k * production_scale * building.last_fulfillment_ratio;
                let inventory_qty = building.inventory.get(&commodity).copied().unwrap_or(0.0);
                // W7: reserve the seeded production buffer for dual-use
                // commodities — a coal mine that sells its whole HardCoal
                // stock cannot produce next turn (self-consumption loop).
                // Selling seed corn is how the extraction economy died.
                // Scale by worker_capacity (the genesis seed basis), not
                // current_employment: a furloughed building must retain its
                // restart buffer instead of fire-selling it into the ask.
                // W9: reserve 6 turns while SEED_INVENTORY_TURNS is 8 —
                // buildings keep more than the full 4-turn bootstrap window
                // covered yet still post the outer ~2 turns of seed as
                // marketable surplus, seeding Turn-0 order books.
                let input_reserve = method
                    .inputs
                    .get(&commodity)
                    .copied()
                    .unwrap_or(0.0)
                    * (building.worker_capacity as f64
                        * building.scale_factor.max(1) as f64
                        / 1000.0)
                    * 6.0;
                let total_available =
                    (inventory_qty + production_qty - input_reserve).max(0.0);
                if total_available <= 0.0 {
                    continue;
                }

                // Price signal: VWAP vs moving average VWAP (-0.5 to +0.5)
                let vwap = market_history.vwap_per_commodity.get(&commodity).copied();
                let ma_vwap = moving_average_vwap(market_history, &commodity);
                let price_signal = match (vwap, ma_vwap) {
                    (Some(v), Some(ma)) if ma > 0.0 => (v / ma - 1.0).clamp(-0.5, 0.5),
                    _ => 0.0, // No history — neutral
                };

                // Cash-flow distress: payroll coverage ratio (0.0 = broke, 1.0 = covered)
                let total_payroll = company.fulfilled_fte as f64 * company.offered_wage_per_fte;
                let op_cash = company.available_cash.max(0.0)
                    + company
                        .brokerage_account
                        .as_ref()
                        .map(|b| b.cash.max(0.0))
                        .unwrap_or(0.0);
                let payroll_coverage = if total_payroll > 0.0 {
                    op_cash / total_payroll
                } else {
                    99.0
                };
                let distress_signal =
                    (1.0 - payroll_coverage.clamp(0.0, 2.0) / 2.0).clamp(0.0, 1.0);

                // Sell fraction: base 0.5 + price 0.3 + distress 0.2, clamped [0.2, 1.0]
                let sell_fraction =
                    (0.5 + price_signal * 0.3 + distress_signal * 0.2).clamp(0.2, 1.0);
                let sell_qty = total_available * sell_fraction;
                if sell_qty <= 0.0 {
                    continue;
                }
                // Phase 80: Skip local utility commodities (Energy, Heat).
                // These are distributed by the grid, not sold on the B2B market.
                if commodity.is_local_utility() {
                    continue;
                }

                // Phase 37/76/80: Per-commodity sell price with fallback chain:
                // 1. unit_cost * (1 + markup) — cost-based pricing
                // 2. get_reference_price(commodity) * (1 + markup) — market-based
                // 3. global_base_prices[commodity] * (1 + min_markup) — floor price
                // This ensures producers always submit asks when they have workers,
                // breaking the "no VWAP → no asks → no trades → no VWAP" deadlock.
                //
                // Phase 80: Bootstrap override — when no VWAP exists (turn 0),
                // use reference_price * (1 + min_markup) as the sell price for ALL
                // commodities, ignoring unit_cost. This guarantees that the sell
                // price can cross the buy bid of ref_price * (1 + buy_premium).
                // Without this, manufactured goods with high unit_cost (e.g. Trains
                // at ~333,000) never cross the buy bid (~105) and the market deadlocks.
                // Once VWAP is established (turn 1+), the rational-actor floor takes over.
                let sell_price = if is_bootstrap {
                    // Bootstrap: use reference price with minimal markup to guarantee crossing
                    if let Some(ref_p) = get_reference_price(&commodity, market_history) {
                        ref_p * (1.0 + config.min_markup_ratio)
                    } else if let Some(base_p) =
                        market_history.global_base_prices.get(&commodity).copied()
                    {
                        base_p * (1.0 + config.min_markup_ratio)
                    } else {
                        continue;
                    }
                } else {
                    // Post-bootstrap: anchor the ask to the market reference
                    // price. Markup scales max(unit_cost, ref) and the ask is
                    // capped at ref × (1 + buy_premium) — the exact ceiling
                    // buyers already open at — so cap-bound asks cross bids
                    // immediately instead of ratcheting for turns. The old
                    // ref×1.5 cap structurally deadlocked spread crossing
                    // (bid ceiling ref×1.05 vs ask floor ref×1.5 = 1.43× gap).
                    // The Rule-8 unit_cost floor below still applies AFTER this
                    // cap (a seller whose unit_cost exceeds the cap refuses
                    // below cost).
                    let ref_p = get_reference_price(&commodity, market_history)
                        .or_else(|| {
                            market_history.global_base_prices.get(&commodity).copied()
                        });
                    match ref_p {
                        Some(rp) => {
                            if building.current_employment == 0 && inventory_qty > 0.0 {
                                // Idle producer liquidating stock: fire-sale at
                                // ref × (1 + min_markup) — a non-producing
                                // seller must not charge the scarcity premium.
                                rp * (1.0 + config.min_markup_ratio)
                            } else {
                                let base = if unit_cost > 0.0 {
                                    unit_cost.max(rp)
                                } else {
                                    rp
                                };
                                (base * (1.0 + markup)).min(rp * (1.0 + config.buy_premium_ratio))
                            }
                        }
                        None => continue,
                    }
                };

                // Unfilled-ask decay: if this seller's ask for the commodity
                // went unfilled last turn, cap this turn's ask at
                // last_unfilled × (1 − decay). The Rule-8 floor below still
                // applies afterwards, so decay converges toward unit_cost but
                // never below it — clearing the capped-ask deadlock without
                // violating rational-actor pricing.
                let sell_price = if let Some(&last_ask) =
                    company.unfilled_ask_prices.get(&commodity)
                {
                    if last_ask > 0.0 {
                        sell_price.min(last_ask * (1.0 - config.ask_decay_ratio))
                    } else {
                        sell_price
                    }
                } else {
                    sell_price
                };

                // Phase 76: Rule 8 — Rational Actor Pricing Floor.
                // Never sell below actual production cost (except during bootstrap).
                let sell_price = if !is_bootstrap && unit_cost > 0.0 {
                    sell_price.max(unit_cost)
                } else {
                    sell_price
                };

                if sell_price <= 0.0 {
                    continue;
                }

                // Phase 95: Look up blueprint metadata for this building's output.
                let (bp_id, bp_quality, bp_durability) = building
                    .active_method
                    .active_blueprint
                    .as_ref()
                    .and_then(|id| {
                        bp_lookup
                            .get(id)
                            .map(|&(q, d)| (Some(id.clone()), Some(q), Some(d)))
                    })
                    .unwrap_or((None, None, None));

                order_book.asks.entry(commodity).or_default().push(Ask {
                    seller_id: company.id.clone(),
                    commodity,
                    quantity: sell_qty,
                    limit_price: sell_price,
                    blueprint_id: bp_id,
                    quality: bp_quality,
                    durability: bp_durability,
                });
            }

            // W7: Surplus-stock asks — sell inventory commodities that the
            // active method neither produces nor consumes. The outputs loop
            // above can only see BOM outputs, so harvest by-products (Fodder
            // from a wheat crop, Fruit from an orchard) and physically
            // delivered goods outside the BOM sat in Building.inventory
            // forever — agricultural supply posted exactly 0 asks despite
            // billions of units harvested. Settlement already resolves any
            // seller building holding the stock (inventory fallback), so
            // these asks are physically deliverable.
            for (&commodity, &qty) in &building.inventory {
                if qty <= 0.0
                    || method.outputs.contains_key(&commodity)
                    || method.inputs.contains_key(&commodity)
                    || commodity.is_local_utility()
                    || commodity.is_fixed_asset()
                {
                    continue;
                }
                let ref_price = match get_reference_price(&commodity, market_history) {
                    Some(p) => p,
                    None => market_history
                        .global_base_prices
                        .get(&commodity)
                        .copied()
                        .unwrap_or(0.0),
                };
                if ref_price <= 0.0 {
                    continue;
                }
                order_book.asks.entry(commodity).or_default().push(Ask {
                    seller_id: company.id.clone(),
                    commodity,
                    quantity: qty * 0.8,
                    limit_price: ref_price * (1.0 + config.min_markup_ratio),
                    blueprint_id: None,
                    quality: None,
                    durability: None,
                });
            }
        }

        // Also submit Sell Asks for excess inventory (fire sale if utilization is high)
        if utilization >= config.fire_sale_threshold {
            let company_inventory =
                compute_company_inventory_indexed(buildings, &company_building_indices);
            for (&commodity, &qty) in &company_inventory {
                if qty <= 0.0 {
                    continue;
                }
                let ref_price = match get_reference_price(&commodity, market_history) {
                    Some(p) => p,
                    None => continue,
                };
                let fire_sale_price = ref_price * (1.0 + config.min_markup_ratio);

                order_book.asks.entry(commodity).or_default().push(Ask {
                    seller_id: company.id.clone(),
                    commodity,
                    quantity: qty * 0.5, // Sell half of excess inventory
                    limit_price: fire_sale_price,
                    blueprint_id: None,
                    quality: None,
                    durability: None,
                });
            }
        }
    }

    #[cfg(feature = "diagnostic")]
    eprintln!(
        "HC_CO: companies={} no_buildings={}",
        diag_companies_total, diag_companies_no_bldg
    );
    #[cfg(feature = "diagnostic")]
    eprintln!(
        "ENCPROBE: desired={:.3e} maxenc={:.3e} posted={:.3e} liquid={:.3e} facility={:.3e} payroll_res={:.3e} budget_ltd={} need_ltd={} arrears_cos={}",
        diag_desired,
        diag_maxenc,
        diag_posted,
        diag_liquid,
        diag_facility,
        diag_payroll_reserve,
        diag_budget_limited,
        diag_need_limited,
        diag_arrears_cos
    );
    #[cfg(feature = "diagnostic")]
    {
        diag_top_desire.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        for (d, cov, id, bom) in diag_top_desire.iter().take(5) {
            eprintln!(
                "TOPDESIRE: co={} desired={:.3e} cov={:.4} bom={}",
                id, d, cov, bom
            );
        }
    }

    messages
}

/// Settle executed trades with double-entry accounting and physical inventory routing.
///
/// # Arguments
/// * `trades` - Slice of executed trades from `match_orders`.
/// * `companies` - Mutable slice of all companies for cash settlement.
/// * `buildings` - Mutable slice of all buildings for inventory routing.
///
/// # Returns
/// A vector of diagnostic messages.
///
/// # Rules
/// * Cash: `buyer.debit_cash -= trade_value`, `seller.available_cash += trade_value`.
/// * Inventory: routes directly to `Building.inventory` — no Company-level field.
/// * Buyer building: belongs to `buyer_id` company + has commodity in BOM inputs.
/// * Seller building: belongs to `seller_id` company + has commodity in BOM outputs.
/// * If seller's building has insufficient inventory, trade is clamped to available.
/// * Double-entry: every debit has a matching credit.
/// * W3/R3 (`company_country`): global company→country map used to settle
///   the payer-side bank legs of cross-border trades. When the seller is
///   foreign, this task cannot credit the seller — but the payer's bank
///   MUST still surrender deposits + reserves here, otherwise the seller's
///   country task credits bank reserves with no offsetting debit
///   (the `b2b_settlement_post` FiatCreation leak).
pub fn settle_trades(
    trades: &[Trade],
    companies: &mut [Company],
    buildings: &mut [Building],
    country: &mut crate::state::Country,
    current_turn: u32,
    gen_config: &GenerativeGoodsConfig,
    company_country: &std::collections::HashMap<String, String>,
) -> Vec<String> {
    let mut messages = Vec::new();

    // W12-DIAG: attribute settled value by payer-leg outcome. Printed once
    // per task under the diagnostic feature to explain b2b_settlement_post
    // fiat deltas.
    #[cfg(feature = "diagnostic")]
    let mut leg_ledger: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    #[cfg(feature = "diagnostic")]
    macro_rules! leg {
        ($k:expr, $v:expr) => {
            *leg_ledger.entry($k.to_string()).or_insert(0.0) += $v;
        };
    }

    // Phase C: O(1) lookup maps. Built once per batch instead of O(N*M) scans.
    let company_id_to_idx: HashMap<String, usize> = companies
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.clone(), i))
        .collect();
    let mut owner_to_buildings: HashMap<String, Vec<usize>> = HashMap::default();
    // v4.17 FS-7: contractor/subcontractor → project-site buildings. A
    // construction bid names the company as buyer, but the goods are
    // bought FOR a specific site's laydown — without direct routing they
    // pool in the bidder's own plants, where the plant's 4-turn production
    // reserve absorbs them before `deploy_project_materials` can forward
    // them (the stalled-cancellation root cause: 1,601 projects cancelled
    // at progress 0.00 despite matched bids and deep asks).
    let mut buyer_to_sites: HashMap<String, Vec<usize>> = HashMap::default();
    for (i, b) in buildings.iter().enumerate() {
        owner_to_buildings
            .entry(b.owner_id.clone())
            .or_default()
            .push(i);
        if let Some(p) = &b.active_project {
            if !p.main_contractor_id.is_empty() {
                buyer_to_sites
                    .entry(p.main_contractor_id.clone())
                    .or_default()
                    .push(i);
            }
            for sub in &p.subcontractors {
                if !sub.subcontractor_id.is_empty() && !sub.completed {
                    buyer_to_sites
                        .entry(sub.subcontractor_id.clone())
                        .or_default()
                        .push(i);
                }
            }
        }
    }

    for trade in trades {
        let trade_value = trade.quantity * trade.execution_price;

        // --- Cash Settlement (Phase 24A.4: via TransferSettler) ---
        // Phase 94: For unbanked buyers, the cash was already encumbered
        // (available_cash → debit_cash) at order submission. Calling
        // settle_transfer_mapped would debit brokerage_account.cash AGAIN,
        // causing M0 destruction for unbanked companies. For unbanked buyers,
        // we release the encumbrance and credit the seller directly. For
        // banked buyers, settle_transfer_mapped handles bank reserve sync.
        if trade.buyer_id != "MIN-DEF" {
            if let (Some(&bi), Some(&si)) = (
                company_id_to_idx.get(&trade.buyer_id),
                company_id_to_idx.get(&trade.seller_id),
            ) {
                #[cfg(feature = "diagnostic")]
                leg!("BOTH_IN_SLICE", trade_value);
                let buyer_is_banked = companies[bi].primary_bank_id.is_some();
                let buyer_is_bank = companies[bi].sector
                    == crate::registries::enums::Sector::Banking;
                if buyer_is_banked {
                    // W7: The bid encumbrance already moved the payment into
                    // escrow (available_cash → debit_cash). Releasing the
                    // escrow IS the cash leg — calling settle_transfer_mapped
                    // here would debit the spendable pocket a SECOND time
                    // (M0 destruction on success; InsufficientCash refund +
                    // cancelled trade on depleted pockets). Consume the
                    // escrow, then apply the payer-bank legs (deposits and
                    // reserves fall as the liability moves to the seller's
                    // bank; ELA covers so reserves never go negative) — the
                    // same structure as the cross-border path below.
                    #[cfg(feature = "diagnostic")]
                    if companies[bi].debit_cash - trade_value < -0.01 {
                        eprintln!(
                            "NEGSET[BANKED]: buyer={} com={:?} qty={:.3e} exec={:.4} val={:.3e} debit_pre={:.3e}",
                            trade.buyer_id, trade.commodity, trade.quantity,
                            trade.execution_price, trade_value, companies[bi].debit_cash
                        );
                    }
                    companies[bi].consume_bid_encumbrance(trade_value);
                    release_fill_slack(
                        &mut companies[bi],
                        trade.quantity,
                        trade.bid_limit_price,
                        trade.execution_price,
                    );
                    let buyer_bank_id = companies[bi].primary_bank_id.clone();
                    if let Some(ref bank_id) = buyer_bank_id {
                        if let Some(&bidx) = company_id_to_idx.get(bank_id) {
                            if let Some(ref mut bs) = companies[bidx].balance_sheet {
                                crate::state::banking::ela_cover_debit(
                                    bs,
                                    &mut country.central_bank,
                                    trade_value,
                                );
                            }
                        }
                        crate::economy::transfer_settler::adjust_bank_balance(
                            companies,
                            &company_id_to_idx,
                            bank_id,
                            -trade_value,
                            -trade_value,
                        );
                    }
                    let seller_id = companies[si].id.clone();
                    credit_company_by_id(companies, &seller_id, trade_value);
                    companies[si].turn_settled_sales += trade_value;
                } else if buyer_is_bank {
                    // W11: The buyer IS a bank paying from its own reserves
                    // (banks hold no primary_bank_id — the unbanked branch
                    // would release only the uncounted debit_cash memo and
                    // never surrender reserves, while the seller's bank is
                    // still credited: pure M0 creation — the
                    // b2b_settlement_post FiatCreation leak). Debit the
                    // buyer-bank's own sheet like the cross-border path
                    // below; ELA covers so reserves never go negative.
                    #[cfg(feature = "diagnostic")]
                    if companies[bi].debit_cash - trade_value < -0.01 {
                        eprintln!(
                            "NEGSET[BANKBUYER]: buyer={} com={:?} qty={:.3e} exec={:.4} val={:.3e} debit_pre={:.3e}",
                            trade.buyer_id, trade.commodity, trade.quantity,
                            trade.execution_price, trade_value, companies[bi].debit_cash
                        );
                    }
                    companies[bi].consume_bid_encumbrance(trade_value);
                    release_fill_slack(
                        &mut companies[bi],
                        trade.quantity,
                        trade.bid_limit_price,
                        trade.execution_price,
                    );
                    if let Some(ref mut bs) = companies[bi].balance_sheet {
                        crate::state::banking::ela_cover_debit(
                            bs,
                            &mut country.central_bank,
                            trade_value,
                        );
                        bs.reserves_at_central_bank -= trade_value;
                        // Own-account purchase: equity bears the cost, not
                        // the customer-deposit liability.
                        bs.tier_1_capital -= trade_value;
                    }
                    let seller_id = companies[si].id.clone();
                    credit_company_by_id(companies, &seller_id, trade_value);
                    companies[si].turn_settled_sales += trade_value;
                } else {
                    // Phase 94: Unbanked buyer — cash was already encumbered
                    // (available_cash → debit_cash). Release encumbrance and
                    // credit seller directly. This preserves M0: debit_cash
                    // (M0) → seller cash (M0).
                    #[cfg(feature = "diagnostic")]
                    if trade.buyer_id.starts_with("STRATEGIC_RESERVE") {
                        eprintln!(
                            "SRA_SETTLE: buyer={} com={:?} qty={:.3e} exec={:.4} val={:.3e} debit_pre={:.3e}",
                            trade.buyer_id, trade.commodity, trade.quantity,
                            trade.execution_price, trade_value, companies[bi].debit_cash
                        );
                    }
                    #[cfg(feature = "diagnostic")]
                    if companies[bi].debit_cash - trade_value < -0.01 {
                        eprintln!(
                            "NEGSET[UNBANKED]: buyer={} com={:?} qty={:.3e} exec={:.4} val={:.3e} debit_pre={:.3e}",
                            trade.buyer_id, trade.commodity, trade.quantity,
                            trade.execution_price, trade_value, companies[bi].debit_cash
                        );
                    }
                    companies[bi].consume_bid_encumbrance(trade_value);
                    release_fill_slack(
                        &mut companies[bi],
                        trade.quantity,
                        trade.bid_limit_price,
                        trade.execution_price,
                    );
                    let seller_id = companies[si].id.clone();
                    credit_company_by_id(companies, &seller_id, trade_value);
                    companies[si].turn_settled_sales += trade_value;
                }
                companies[bi].unfilled_bid_prices.remove(&trade.commodity);
                // Seller filled an ask this turn — clear the decay marker so
                // next turn's ask isn't capped below the clearing price.
                companies[si].unfilled_ask_prices.remove(&trade.commodity);
            } else {
                // W3/R3: Partial-match settlement — exactly one side of the
                // trade is in this country's company slice (cross-border trade)
                // or one side is a non-company (phantom seller such as
                // "fishing_sector"). Both cash legs must still balance.
                if let Some(&bi) = company_id_to_idx.get(&trade.buyer_id) {
                    #[cfg(feature = "diagnostic")]
                    {
                        if company_country.contains_key(&trade.seller_id) {
                            leg!("PAYER_domestic_foreign_seller", trade_value);
                        } else {
                            leg!("PAYER_domestic_phantom_seller", trade_value);
                        }
                    }
                    // Release the buyer's encumbered cash (the payer-side
                    // cash leg — encumbered available_cash → debit_cash at
                    // bid submission; releasing it completes the payment).
                    // For UNBANKED buyers this release IS the M0 debit —
                    // their cash pockets (available + debit + brokerage)
                    // are all physical fiat in the walk, so no further
                    // cash-pocket debit may follow (that would destroy
                    // fiat a second time — the post-fix FiatDestruction).
                    #[cfg(feature = "diagnostic")]
                    if trade.buyer_id.starts_with("STRATEGIC_RESERVE") {
                        eprintln!(
                            "SRA_SETTLE_XB: buyer={} com={:?} qty={:.3e} exec={:.4} val={:.3e} debit_pre={:.3e}",
                            trade.buyer_id, trade.commodity, trade.quantity,
                            trade.execution_price, trade_value, companies[bi].debit_cash
                        );
                    }
                    #[cfg(feature = "diagnostic")]
                    if companies[bi].debit_cash - trade_value < -0.01 {
                        eprintln!(
                            "NEGSET[XB]: buyer={} com={:?} qty={:.3e} exec={:.4} val={:.3e} debit_pre={:.3e}",
                            trade.buyer_id, trade.commodity, trade.quantity,
                            trade.execution_price, trade_value, companies[bi].debit_cash
                        );
                    }
                    companies[bi].consume_bid_encumbrance(trade_value);
                    release_fill_slack(
                        &mut companies[bi],
                        trade.quantity,
                        trade.bid_limit_price,
                        trade.execution_price,
                    );
                    let buyer_bank_id = companies[bi].primary_bank_id.clone();
                    let buyer_is_bank = companies[bi].sector
                        == crate::registries::enums::Sector::Banking;
                    // W7: No spendable-pocket debit here — the bid encumbrance
                    // already moved the payment into debit_cash, and the
                    // release above IS the cash leg. Re-debiting
                    // brokerage/available_cash would charge banked buyers
                    // twice (M0 destruction). Only the bank balance-sheet
                    // legs below remain: the deposit liability backing the
                    // escrowed cash moves to the seller's bank.
                    // W3/R3: If the buyer is banked, the payment leaves the
                    // payer's bank — deposits AND reserves must fall here,
                    // mirroring the payer leg of settle_transfer_mapped.
                    // The seller's own country task credits the seller's bank
                    // +deposits/+reserves; without this debit the pair
                    // creates fiat (the b2b_settlement_post M0 leak).
                    // ELA covers the debit so reserves never go negative.
                    if let Some(ref bank_id) = buyer_bank_id {
                        if let Some(&bidx) = company_id_to_idx.get(bank_id) {
                            if let Some(ref mut bs) = companies[bidx].balance_sheet {
                                crate::state::banking::ela_cover_debit(
                                    bs,
                                    &mut country.central_bank,
                                    trade_value,
                                );
                            }
                        }
                        let debited = crate::economy::transfer_settler::adjust_bank_balance(
                            companies,
                            &company_id_to_idx,
                            bank_id,
                            -trade_value,
                            -trade_value,
                        );
                        #[cfg(feature = "diagnostic")]
                        if !debited {
                            eprintln!("XB_NOPAYBANK: buyer={} bank={} value={:.2} — payer bank not in slice",
                                companies[bi].id, bank_id, trade_value);
                        }
                    } else if buyer_is_bank {
                        // The buyer IS a bank paying from its own reserves —
                        // debit its own balance sheet (same convention as
                        // settle_transfer_mapped's Banking-sector payer leg).
                        if let Some(ref mut bs) = companies[bi].balance_sheet {
                            crate::state::banking::ela_cover_debit(
                                bs,
                                &mut country.central_bank,
                                trade_value,
                            );
                            bs.reserves_at_central_bank -= trade_value;
                            // Own-account purchase: equity bears the cost,
                            // not the customer-deposit liability.
                            bs.tier_1_capital -= trade_value;
                        }
                    }
                    // else: unbanked buyer — the debit_cash release above
                    // already moved M0 out of the payer's pockets.
                }
                if let Some(&si) = company_id_to_idx.get(&trade.seller_id) {
                    // Phase 94: Use credit_company_by_id to sync bank reserves
                    // (M0). When the buyer is a non-company (ministry, cultural
                    // institution, etc.), the manual credit_company_by_id path
                    // ensures the seller's bank reserves are adjusted to keep
                    // M0 neutral (ministry cash ↓ = bank reserves ↑).
                    let seller_id = companies[si].id.clone();
                    credit_company_by_id(companies, &seller_id, trade_value);
                    companies[si].turn_settled_sales += trade_value;
                    companies[si].unfilled_ask_prices.remove(&trade.commodity);
                    if trade.buyer_id == "FOREIGN-SECTOR" {
                        let prev = companies[si]
                            .extra
                            .get("export_revenue_earned")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        companies[si].extra.insert(
                            "export_revenue_earned".to_string(),
                            serde_json::Value::from(prev + trade_value),
                        );
                    }
                    // Export-earnings repatriation: a payer outside this
                    // country (the unmodeled FOREIGN-SECTOR, or a company in
                    // another country) settles the trade in numeraire, and
                    // the seller's central bank accumulates the trade value
                    // as FX reserves — the CB converts the exporter's foreign
                    // receipts. This is the only inflow channel for
                    // fx_reserves: the pool otherwise drains monotonically on
                    // import settlement until every cross-border trade vetoes.
                    // Vehicle-settlement drains all buckets pro-rata, so a
                    // single USD reserve bucket is sufficient. Reserves are
                    // foreign-currency assets, not domestic M0 — outside the
                    // fiat walk.
                    let buyer_abroad = trade.buyer_id == "FOREIGN-SECTOR"
                        || company_country
                            .get(&trade.buyer_id)
                            .map(|bc| bc != &country.name)
                            .unwrap_or(false);
                    if buyer_abroad {
                        *country
                            .central_bank
                            .fx_reserves
                            .entry("USD".to_string())
                            .or_insert(0.0) += trade_value;
                    }
                    #[cfg(feature = "diagnostic")]
                    {
                        if company_country.contains_key(&trade.buyer_id) {
                            leg!("SELLER_LEG_foreign_buyer", trade_value);
                        } else {
                            leg!("SELLER_LEG_nonco", trade_value);
                        }
                    }
                } else if company_id_to_idx.contains_key(&trade.buyer_id)
                    && !company_country.contains_key(&trade.seller_id)
                {
                    // W3: Phantom seller (e.g. the "fishing_sector" aggregate
                    // ask) — no company exists in ANY country to receive the
                    // payment. Credit the domestic treasury as the informal/
                    // state-sector counterparty so the payer's debit has a
                    // matching credit and M0 stays conserved.
                    // Macro-Remediation: FOREIGN-SECTOR imports settle
                    // against the foreign purse, not the treasury — the
                    // caller credits `market.foreign_sector_balance` with
                    // the spend; crediting the treasury here as well
                    // would double-count the inflow.
                    if trade.seller_id != "FOREIGN-SECTOR" {
                        country.budget.liquid_reserves += trade_value;
                    }
                }
            }
        } else if let Some(&bi) = company_id_to_idx.get(&trade.buyer_id) {
            companies[bi].consume_bid_encumbrance(trade_value);
            release_fill_slack(
                &mut companies[bi],
                trade.quantity,
                trade.bid_limit_price,
                trade.execution_price,
            );
        } else {
            // Phase 94: MIN-DEF or other non-company buyer. The buyer's cash
            // was already encumbered (deducted from liquid_reserves). Credit
            // the seller via credit_company_by_id to sync bank reserves (M0).
            // Phase 94 fix: For MIN-DEF trades, settle_defense_trades handles
            // the full settlement (treasury debit + seller credit). Crediting
            // the seller here would double-credit and create fiat (Rule 1).
            if trade.buyer_id != "MIN-DEF" {
                if let Some(&si) = company_id_to_idx.get(&trade.seller_id) {
                    let seller_id = companies[si].id.clone();
                    credit_company_by_id(companies, &seller_id, trade_value);
                    companies[si].turn_settled_sales += trade_value;
                    companies[si].unfilled_ask_prices.remove(&trade.commodity);
                }
            }
        }

        // --- Physical Inventory Routing ---
        let buyer_buildings = owner_to_buildings
            .get(&trade.buyer_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        // W11: Company-level bids aggregate every consuming building's need,
        // so a filled trade must be SPLIT across those buildings. Routing the
        // whole quantity to the first match concentrated all purchased inputs
        // in one building while its siblings starved at fulfillment_ratio=0 —
        // multi-building companies ended up with one producing plant and N
        // payroll-only clones. Weight each consumer's share by its remaining
        // input deficit (the same need×2 − on_hand basis the bid was sized
        // on); a stocked building steps aside until its deficit reopens.
        // (idx, deficit, gross_need)
        let mut recipients: Vec<(usize, f64, f64)> = Vec::new();
        // v4.17 FS-7: site-committed delivery takes precedence. When the
        // buyer is a project's main contractor or subcontractor, materials
        // deliver straight to that site's laydown inventory (weighted by
        // outstanding BOM need) rather than pooling in the bidder's own
        // plants where its production reserves absorb them.
        let mut site_committed = false;
        for &i in buyer_to_sites
            .get(&trade.buyer_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
        {
            let b = &buildings[i];
            let project = match &b.active_project {
                Some(p) => p,
                None => continue,
            };
            let required = project
                .required_materials
                .get(&trade.commodity)
                .copied()
                .unwrap_or(0.0);
            let delivered = project
                .delivered_materials
                .get(&trade.commodity)
                .copied()
                .unwrap_or(0.0);
            let site_scale =
                b.worker_capacity as f64 * b.scale_factor.max(1) as f64 / 1000.0;
            let site_reserve = b
                .active_method
                .inputs
                .get(&trade.commodity)
                .copied()
                .unwrap_or(0.0)
                * site_scale
                * 4.0;
            let staged = (b
                .inventory
                .get(&trade.commodity)
                .copied()
                .unwrap_or(0.0)
                - site_reserve)
                .max(0.0);
            let want = (required - delivered - staged).max(0.0);
            if want > 0.0 {
                site_committed = true;
                recipients.push((i, want, want));
            }
        }
        if recipients.is_empty() {
            for &i in buyer_buildings {
                let b = &buildings[i];
                let in_method = b.active_method.inputs.contains_key(&trade.commodity);
                let in_project = b
                    .active_project
                    .as_ref()
                    .map(|p| p.required_materials.contains_key(&trade.commodity))
                    .unwrap_or(false);
                if !in_method && !in_project {
                    continue;
                }
                let need = b
                    .active_method
                    .inputs
                    .get(&trade.commodity)
                    .copied()
                    .unwrap_or(0.0)
                    * (b.worker_capacity as f64 * b.scale_factor.max(1) as f64 / 1000.0);
                let on_hand = b
                    .inventory
                    .get(&trade.commodity)
                    .copied()
                    .unwrap_or(0.0);
                let deficit = (need * 2.0 - on_hand).max(0.0);
                recipients.push((i, deficit, need));
            }
            if recipients.is_empty() {
                // No building consumes it — legacy fallback: first building.
                if let Some(&i) = buyer_buildings.first() {
                    recipients.push((i, 1.0, 1.0));
                }
            } else if recipients.iter().all(|r| r.1 <= 0.0) {
                // Every consumer is stocked past its buffer — split by gross need
                // so the largest plants still take the largest share.
                for r in &mut recipients {
                    r.1 = r.2.max(1.0);
                }
            }
        }

        // Phase 95: If the commodity is a fixed asset, install it as a
        // FixedAssetCohort with the trade's blueprint/quality metadata.
        // This ensures physical asset capacity is increased only by
        // delivered goods with known provenance (Rule 15, Rule 4).
        // Discrete asset units are not fractionally splittable — deliver the
        // whole cohort to the largest-deficit consumer.
        // v4.17 FS-7: site-committed deliveries stay in flat `inventory` —
        // `consume_delivered_materials` installs from the laydown map and
        // cannot see asset cohorts.
        if trade.commodity.is_fixed_asset() && !site_committed {
            if let Some(&(idx, _, _)) = recipients
                .iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            {
                let building = &mut buildings[idx];
                let blueprint_id = trade.blueprint_id.clone().unwrap_or_default();
                let quality = trade.quality.unwrap_or(0.0);
                let durability = trade.durability.unwrap_or(0.0);
                // Desynchronize fleet end-of-life: every cohort installed
                // with identical durability scraps on the same turn, and
                // because the genesis fleet is built in one wave the entire
                // economy repurchases in a single synchronized burst
                // (observed: I=0 for 100+ turns then a 313B spike).
                // A deterministic per-(building, blueprint, lot) jitter of
                // ±20% spreads the scrap window without consuming RNG draws.
                let durability = {
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    building.id.hash(&mut h);
                    blueprint_id.hash(&mut h);
                    trade.quantity.to_bits().hash(&mut h);
                    durability * (0.8 + 0.4 * (h.finish() % 1000) as f64 / 1000.0)
                };
                let cohort = crate::economy::fixed_assets::FixedAssetCohort::new(
                    blueprint_id,
                    trade.commodity,
                    trade.quantity,
                    quality,
                    durability,
                    String::new(), // base_tech — not tracked in Trade (simplified)
                    0,             // base_tech_year — not tracked in Trade
                    current_turn,
                );
                // Install the cohort (handles compaction).
                crate::economy::fixed_assets::install_fixed_asset(
                    &mut building.fixed_assets,
                    cohort,
                    gen_config,
                );
            }
        } else if site_committed {
            // v4.18 WS2: milestone-seeking fill order. Splitting each fill
            // pro-rata across EVERY site the contractor serves atomizes the
            // shipment — a site needing 0.5 units receives crumbs forever,
            // and since `compute_progress` is min-over-lines every BOM line
            // parked ~2% short permanently (observed: sites frozen at
            // progress 0.92-0.94 for 50+ turns with inv=0).
            // A shipment goes to the site NEAREST completion first — the
            // contractor's real incentive (each finished site releases the
            // next milestone tranche) — with any remainder flowing on to
            // the next-neediest site. Delivered quantity is conserved; only
            // the allocation order changes.
            #[cfg(feature = "diagnostic")]
            eprintln!(
                "CONST_DBG[SITEFILL]: buyer={} com={:?} qty={:.1} exec={:.2} sites={}",
                trade.buyer_id, trade.commodity, trade.quantity, trade.execution_price,
                recipients.len()
            );
            let mut ordered = recipients;
            ordered.sort_by(|a, b| {
                a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
            });
            let mut remaining_fill = trade.quantity;
            for (idx, want, _) in ordered {
                if remaining_fill <= 0.0 {
                    break;
                }
                let share = want.min(remaining_fill);
                if share <= 0.0 {
                    continue;
                }
                remaining_fill -= share;
                let building = &mut buildings[idx];
                *building.inventory.entry(trade.commodity).or_insert(0.0) += share;
                // v4.18 WS2: project sites accrue the PAID cost basis
                // (`execution_price`) so `cost_spent` reflects what the
                // contractor actually settled — not spot reference prices.
                if let Some(p) = building.active_project.as_mut() {
                    if p.required_materials.contains_key(&trade.commodity) {
                        *p.staging_cost_basis.entry(trade.commodity).or_insert(0.0) +=
                            share * trade.execution_price;
                        *p.staging_qty.entry(trade.commodity).or_insert(0.0) += share;
                    }
                }

                // Phase 95: If blueprint-eligible, also add to inventory cohorts.
                if trade.commodity.is_blueprint_eligible() {
                    let cohort = crate::economy::fixed_assets::InventoryCohort::new(
                        trade.blueprint_id.clone().unwrap_or_default(),
                        trade.commodity,
                        share,
                        trade.quality.unwrap_or(0.0),
                        trade.durability.unwrap_or(0.0),
                        current_turn,
                    );
                    // v4.17: merge into a compatible existing cohort rather
                    // than pushing unconditionally — deliveries arrive every
                    // turn and `compact_inventory_cohorts` is never invoked,
                    // so the vec grew ~×1.8/turn on hot sites (2,400+ entries
                    // by t=7 → building-state OOM). Merging preserves
                    // quantity and the earliest produced_turn for FIFO.
                    if let Some(existing) = building
                        .inventory_cohorts
                        .iter_mut()
                        .find(|c| c.is_mergeable_with(&cohort))
                    {
                        existing.quantity += cohort.quantity;
                        existing.produced_turn =
                            existing.produced_turn.min(cohort.produced_turn);
                    } else {
                        building.inventory_cohorts.push(cohort);
                    }
                }
            }
        } else {
            let total_w: f64 = recipients.iter().map(|r| r.1).sum();
            for &(idx, w, _) in &recipients {
                let share = if total_w > 0.0 {
                    trade.quantity * w / total_w
                } else {
                    0.0
                };
                if share <= 0.0 {
                    continue;
                }
                let building = &mut buildings[idx];
                // Non-fixed-asset: add to flat inventory.
                *building.inventory.entry(trade.commodity).or_insert(0.0) += share;
                // v4.18 WS2: same paid-basis accrual for project sites on
                // the general recipient leg.
                if let Some(p) = building.active_project.as_mut() {
                    if p.required_materials.contains_key(&trade.commodity) {
                        *p.staging_cost_basis.entry(trade.commodity).or_insert(0.0) +=
                            share * trade.execution_price;
                        *p.staging_qty.entry(trade.commodity).or_insert(0.0) += share;
                    }
                }

                // Phase 95: If blueprint-eligible, also add to inventory cohorts.
                if trade.commodity.is_blueprint_eligible() {
                    let cohort = crate::economy::fixed_assets::InventoryCohort::new(
                        trade.blueprint_id.clone().unwrap_or_default(),
                        trade.commodity,
                        share,
                        trade.quality.unwrap_or(0.0),
                        trade.durability.unwrap_or(0.0),
                        current_turn,
                    );
                    // v4.17: merge into a compatible existing cohort rather
                    // than pushing unconditionally — deliveries arrive every
                    // turn and `compact_inventory_cohorts` is never invoked,
                    // so the vec grew ~×1.8/turn on hot sites (2,400+ entries
                    // by t=7 → building-state OOM). Merging preserves
                    // quantity and the earliest produced_turn for FIFO.
                    if let Some(existing) = building
                        .inventory_cohorts
                        .iter_mut()
                        .find(|c| c.is_mergeable_with(&cohort))
                    {
                        existing.quantity += cohort.quantity;
                        existing.produced_turn =
                            existing.produced_turn.min(cohort.produced_turn);
                    } else {
                        building.inventory_cohorts.push(cohort);
                    }
                }
            }
        }

        // SRA physical reserves: Strategic Reserve Agency purchases deliver
        // into its warehouses above; mirror the settled quantity into
        // `commodity_reserves` so purchase/release triggers and capacity
        // clamps see real stock. The map was previously credited at order
        // time on forecast-layer orders that never settled — a phantom
        // reserve that also violated physical conservation.
        if let Some(&bi) = company_id_to_idx.get(&trade.buyer_id) {
            if let LegalForm::StrategicReserveAgency(data) =
                &mut companies[bi].legal_form
            {
                *data
                    .commodity_reserves
                    .entry(trade.commodity.to_string())
                    .or_insert(0.0) += trade.quantity;
            }
        }

        let seller_buildings = owner_to_buildings
            .get(&trade.seller_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        // W7: Company-level asks aggregate per-building offers, but a single
        // trade's decrement must not silently eat another building's INPUT
        // stock. Draw down in preference order: (1) buildings whose method
        // outputs the commodity, (2) buildings where it is surplus stock,
        // (3) input-holding buildings only above their next-turn requirement
        // — selling a sibling's production buffer is eating seed corn.
        let mut seller_candidates: Vec<usize> = seller_buildings.to_vec();
        seller_candidates.sort_by_key(|&i| {
            let b = &buildings[i];
            if b.active_method.outputs.contains_key(&trade.commodity) {
                0
            } else if b.active_method.inputs.contains_key(&trade.commodity) {
                2
            } else {
                1
            }
        });
        let mut remaining_to_ship = trade.quantity;
        for &idx in &seller_candidates {
            if remaining_to_ship <= 0.0 {
                break;
            }
            let b = &buildings[idx];
            let held = b
                .inventory
                .get(&trade.commodity)
                .copied()
                .unwrap_or(0.0);
            // Input-holding buildings ship only stock above the seeded
            // production horizon (4 turns of input need) — anything less is
            // production capital, and draining it is eating seed corn. The
            // reserve scales with worker_capacity (the genesis seed basis),
            // NOT current_employment: a furloughed building must keep its
            // restart buffer, otherwise one bad turn permanently liquidates
            // its inputs through sibling asks and it can never reopen.
            let shippable = if b.active_method.outputs.contains_key(&trade.commodity)
                || !b.active_method.inputs.contains_key(&trade.commodity)
            {
                held
            } else {
                let reserve = b
                    .active_method
                    .inputs
                    .get(&trade.commodity)
                    .copied()
                    .unwrap_or(0.0)
                    * (b.worker_capacity as f64 * b.scale_factor.max(1) as f64 / 1000.0)
                    * 4.0;
                (held - reserve).max(0.0)
            };
            let ship = shippable.min(remaining_to_ship);
            if ship <= 0.0 {
                continue;
            }
            let building = &mut buildings[idx];
            let new_qty = held - ship;
            if new_qty > 0.0 {
                building.inventory.insert(trade.commodity, new_qty);
            } else {
                building.inventory.remove(&trade.commodity);
            }
            remaining_to_ship -= ship;

            // Phase 95: Consume from inventory cohorts (FIFO — oldest first).
            // This tracks the actual physical goods sold, preserving blueprint
            // provenance for the buyer.
            if trade.commodity.is_blueprint_eligible() {
                consume_inventory_cohorts(
                    &mut building.inventory_cohorts,
                    trade.commodity,
                    ship,
                );
            }
        }
        if remaining_to_ship > 0.0 && company_id_to_idx.contains_key(&trade.seller_id) {
            messages.push(format!(
                "WARNING: Seller {} short-shipped {:.1} of {} {:?} (no building held the stock)",
                trade.seller_id, remaining_to_ship, trade.quantity, trade.commodity
            ));
        }

        // SRA physical reserves: decrement the reserve mirror by the quantity
        // that actually shipped from the agency's warehouses. Unfilled asks
        // ship nothing and leave the mirror untouched.
        let shipped = (trade.quantity - remaining_to_ship).max(0.0);
        if shipped > 0.0 {
            if let Some(&si) = company_id_to_idx.get(&trade.seller_id) {
                if let LegalForm::StrategicReserveAgency(data) =
                    &mut companies[si].legal_form
                {
                    let key = trade.commodity.to_string();
                    let cur = data
                        .commodity_reserves
                        .get(&key)
                        .copied()
                        .unwrap_or(0.0);
                    data.commodity_reserves
                        .insert(key, (cur - shipped).max(0.0));
                }
            }
        }
    }

    #[cfg(feature = "diagnostic")]
    if !leg_ledger.is_empty() {
        eprintln!("SETTLE_LEGS: {:?}", leg_ledger);
    }

    messages
}

/// Settle trades with physical tariff collection (Phase 11).
///
/// Wraps `settle_trades` for cash/inventory movement, then performs a second
/// pass to collect tariffs on cross-border trades. The tariff amount is
/// debited from the buyer's encumbered cash and credited to the buyer's
/// country treasury.
///
/// # Arguments
/// * `trades` - Executed trades to settle.
/// * `companies` - Mutable companies for cash settlement.
/// * `buildings` - Mutable buildings for inventory routing.
/// * `country` - The country whose companies are being settled (receives tariff revenue).
/// * `company_country` - Ephemeral lookup: company_id → country_name.
/// * `diplomacy` - Bilateral diplomatic relations matrix.
///
/// # Rules
/// * Calls `settle_trades` first for standard cash + inventory settlement.
/// * For each trade where buyer and seller are in different countries:
///   - Looks up the buyer's import tariff rate for the traded commodity.
///   - `tariff_amount = trade_value * tariff_rate`.
///   - Debits `tariff_amount` from buyer's `debit_cash` (additional encumbrance release).
///   - Credits `tariff_amount` to `country.budget.liquid_reserves`.
/// * Same-country trades incur no tariff.
/// * Companies not in the lookup table bypass tariff checks.
/// * Double-entry: buyer loses `trade_value + tariff`, seller gains `trade_value`,
///   treasury gains `tariff`. Sum = 0.
pub fn settle_trades_with_tariffs(
    trades: &[Trade],
    companies: &mut [Company],
    buildings: &mut [Building],
    country: &mut crate::state::Country,
    company_country: &std::collections::HashMap<String, String>,
    diplomacy: &std::collections::HashMap<
        String,
        std::collections::HashMap<String, crate::international::DiplomaticRelation>,
    >,
    _country_to_currency: &std::collections::HashMap<String, String>,
    _currency_rates: &std::collections::HashMap<String, f64>,
    current_turn: u32,
    gen_config: &GenerativeGoodsConfig,
) -> Vec<String> {
    // Phase 1: Standard settlement (cash + inventory)
    let messages = settle_trades(
        trades,
        companies,
        buildings,
        country,
        current_turn,
        gen_config,
        company_country,
    );

    // Phase 42: FX Reserves — forced currency conversion and import hard floor.
    // For cross-border trades, the buyer's country Central Bank must:
    // - Exports (seller is domestic): credit foreign currency to FX reserves,
    //   debit domestic currency to the exporter.
    // - Imports (buyer is domestic): check FX reserves for seller's currency;
    //   if insufficient, the trade fails (revert settlement).
    let _domestic_ccy = country.macro_indicators.currency.clone();

    // Phase 2: Tariff collection on cross-border trades + FX conversion.
    // v4.17.0: the customs ledger is the per-turn revenue record the tax
    // module reads — reset it here, then increment on each settlement
    // below. Generated countries start with `customs_state = None`
    // (tax.rs lazily creates it on the first fiscal turn); the writer
    // must exist before then or every collection is silently dropped.
    {
        let customs = country
            .politics
            .customs_state
            .get_or_insert_with(crate::politics::CustomsState::default);
        customs.tariff_revenue_collected = 0.0;
    }
    for trade in trades {
        let trade_value = trade.quantity * trade.execution_price;

        let buyer_country = match company_country.get(&trade.buyer_id) {
            Some(bc) => bc.clone(),
            None => continue, // Unknown company — bypass
        };
        let seller_country = match company_country.get(&trade.seller_id) {
            Some(sc) => sc.clone(),
            None => continue, // Unknown company — bypass
        };

        // Same-country trades incur no tariff and no FX conversion
        if buyer_country == seller_country {
            continue;
        }

        // Phase 42: FX conversion for cross-border trades.
        // Agent 4 — Phase 3: Replaced hardcoded exchange_rate = 1.0 with real
        // currency rates from state.currencies. Removed export-side fx_reserves
        // credit (was fiat creation — settle_trades already paid the seller).
        // The import-side fx_reserves debit is retained as a liquidity constraint.
        let is_import = buyer_country == country.name;
        let is_export = seller_country == country.name;

        if is_export {
            // Export: settle_trades already credited the domestic seller's cash
            // and debited the foreign buyer's cash. The FX reserves adjustment
            // was removed (Agent 4) because it credited fx_reserves with no
            // offsetting debit — pure fiat creation (Rule 1 violation).
            // The Central Bank's FX reserves are adjusted by settle_trade_deficits
            // in a later phase, which handles the actual balance-of-payments
            // settlement via Forex/Gold/sovereign default.
            //
            // NOTE: A proper model would route the export payment through the
            // Central Bank (foreign buyer pays foreign currency → CB converts
            // to domestic → credits exporter). This requires refactoring
            // settle_trades to not handle cross-border cash directly. Deferred
            // to a future phase to avoid breaking the settlement pipeline.
        }
        // W12: Import FX feasibility is vetted pre-settlement in turn.rs —
        // infeasible trades are dropped from every task's view and feasible
        // ones already debited the importer's fx_reserves there. The old
        // post-settlement revert was removed: it refunded the domestic buyer
        // but could not claw back the foreign seller's credit (M0 creation).

        // Phase 29: Import tariff collection (only when the current country
        // is the importer — buyer is domestic).
        // Agent 4: Guarded with is_import to prevent incorrect tariff
        // application on export trades. Added export tax collection below.
        if is_import {
            // Check FTA/customs union between buyer and seller countries.
            // If they have a free trade agreement or customs union, tariffs are
            // reduced or eliminated. Embargo penalties add to the tariff.
            let mut tariff_rate = country
                .trade_policy
                .import_tariffs
                .get(&trade.commodity)
                .copied()
                .unwrap_or(0.0);

            // Check diplomatic relations for FTA/embargo overrides
            if let Some(buyer_diplomacy) = diplomacy.get(&buyer_country) {
                if let Some(rel) = buyer_diplomacy.get(&seller_country) {
                    if rel.customs_union {
                        // Customs union: eliminate all tariffs
                        tariff_rate = 0.0;
                    } else if rel.free_trade {
                        // FTA: reduce tariff by 90% (not fully eliminated for non-customs-union)
                        tariff_rate *= 0.10;
                    }
                    // Embargo penalty adds to the effective tariff
                    if rel.embargo_penalty > 0.0 {
                        tariff_rate += rel.embargo_penalty;
                    }
                }
            }

            if tariff_rate > 0.0 {
                let tariff_amount = trade_value * tariff_rate;

                // Phase 24A.4: Use TransferSettler for tariff collection (double-entry).
                // Debits buyer's cash and credits treasury via proper bank sync.
                // Agent 4: Removed the fiat-creating fallback that credited
                // liquid_reserves without debiting real cash. If the buyer is not
                // found, the tariff is simply not collected (revenue loss) — no fiat.
                if let Some(buyer_idx) = companies.iter().position(|c| c.id == trade.buyer_id) {
                    // Phase 94: For unbanked buyers, the cash was already
                    // encumbered (available_cash → debit_cash). Debiting
                    // brokerage_account.cash via settle_transfer_to_treasury
                    // would double-debit M0. For unbanked buyers, release
                    // the encumbrance and credit treasury directly.
                    let buyer_is_banked = companies[buyer_idx].primary_bank_id.is_some();
                    if buyer_is_banked {
                        let _ = crate::economy::transfer_settler::settle_transfer_to_treasury(
                            companies,
                            buyer_idx,
                            tariff_amount,
                            country,
                        );
                    } else {
                        country.budget.liquid_reserves += tariff_amount;
                    }
                    // Also release the buyer's encumbered debit_cash for the tariff
                    if let Some(buyer) = companies.get_mut(buyer_idx) {
                        buyer.consume_bid_encumbrance(tariff_amount);
                    }
                    // v4.17.0: record the import tariff on the customs ledger
                    // — the money moved above; this is the reporting leg the
                    // Finance tab and tax module read.
                    if let Some(ref mut customs) = country.politics.customs_state {
                        customs.tariff_revenue_collected += tariff_amount;
                    }
                }
                // Agent 4: No else branch — if buyer not found, no tariff collected.
                // Previously, the fallback created fiat by crediting liquid_reserves
                // without debiting real cash (Rule 1 violation).
            }
        }

        // Agent 4 — Phase 1.5: Export tax collection (only when the current
        // country is the exporter — seller is domestic).
        // Previously, export taxes were only applied as a price reduction in
        // clearing.rs::resolve_surplus, but the tax revenue was never collected
        // by the treasury. This branch debits the seller (exporter) and credits
        // the domestic treasury via settle_transfer_to_treasury (Rule 1).
        if is_export {
            let mut export_tax_rate = country
                .trade_policy
                .export_taxes
                .get(&trade.commodity)
                .copied()
                .unwrap_or(0.0);

            // Check diplomatic relations for FTA/embargo overrides on export side
            if let Some(seller_diplomacy) = diplomacy.get(&seller_country) {
                if let Some(rel) = seller_diplomacy.get(&buyer_country) {
                    if rel.customs_union {
                        export_tax_rate = 0.0;
                    } else if rel.free_trade {
                        export_tax_rate *= 0.10;
                    }
                }
            }

            if export_tax_rate > 0.0 {
                let export_tax_amount = trade_value * export_tax_rate;

                // Debit the seller (exporter) and credit the domestic treasury.
                // The seller is a domestic company, so it's in the `companies` slice.
                if let Some(seller_idx) = companies.iter().position(|c| c.id == trade.seller_id) {
                    let _ = crate::economy::transfer_settler::settle_transfer_to_treasury(
                        companies,
                        seller_idx,
                        export_tax_amount,
                        country,
                    );
                    // Release the seller's encumbered debit_cash for the tax amount.
                    if let Some(seller) = companies.get_mut(seller_idx) {
                        seller.consume_bid_encumbrance(export_tax_amount);
                    }
                    // v4.17.0: record export duty on the same customs ledger.
                    if let Some(ref mut customs) = country.politics.customs_state {
                        customs.tariff_revenue_collected += export_tax_amount;
                    }
                }
                // If seller not found, no tax collected — no fiat creation.
            }
        }
    }

    messages
}

/// Refund unfilled bids after order matching.
///
/// # Arguments
/// * `order_book` - Order book with remaining unfilled bids.
/// * `companies` - Mutable slice of companies for refund.
/// * `freight_reserve_ratio` - The `freight_cost_reserve_ratio` that inflated
///   the posted encumbrance (`qty × limit × (1+ratio)`) for bids submitted via
///   the general B2B path. Releasing only `qty × limit` stranded the freight
///   share in `debit_cash`/`open_bid_credit`/`procurement_credit` every turn —
///   a chronic leak under sustained unfilled bids. Bids posted without the
///   reserve (construction, agency) release `qty × limit` exactly: the
///   `release_bid_encumbrance` floor at `debit_cash` bounds the release to
///   what the company actually holds, so over-asking is company-local and
///   self-corrects within the same pocket.
///
/// # Rules
/// * Refunds encumbered cash: `debit_cash -= refund`, `available_cash += refund`.
/// * Also restores to brokerage account so it's spendable next turn.
/// * The credit-backed share of the encumbrance discharges instead of
///   refunding (no goods delivered → no payable survives) — see
///   `Company::release_bid_encumbrance`.
pub fn refund_unfilled_bids(
    order_book: &OrderBook,
    companies: &mut [Company],
    company_id_to_idx: &rustc_hash::FxHashMap<String, usize>,
    freight_reserve_ratio: f64,
) {
    for bids in order_book.bids.values() {
        for bid in bids {
            // Phase 94: O(1) lookup via pre-built index map.
            if let Some(&idx) = company_id_to_idx.get(&bid.buyer_id) {
                let company = &mut companies[idx];
                let refund = bid.quantity * bid.limit_price * (1.0 + freight_reserve_ratio);
                #[cfg(feature = "diagnostic")]
                if bid.buyer_id.starts_with("STRATEGIC_RESERVE") {
                    eprintln!(
                        "SRA_REFUND: buyer={} com={:?} qty={:.3e} limit={:.4} refund={:.3e} debit_pre={:.3e}",
                        bid.buyer_id, bid.commodity, bid.quantity,
                        bid.limit_price, refund, company.debit_cash
                    );
                }
                #[cfg(feature = "diagnostic")]
                if company.debit_cash - refund < -0.01 {
                    eprintln!(
                        "NEGSET[REFUND]: buyer={} com={:?} qty={:.3e} limit={:.4} refund={:.3e} debit_pre={:.3e}",
                        bid.buyer_id, bid.commodity, bid.quantity,
                        bid.limit_price, refund, company.debit_cash
                    );
                }
                // Release only what is still encumbered — a refund larger than
                // the outstanding escrow must not push debit_cash negative or
                // credit cash that was never locked (double-release voids M0
                // conservation). The diagnostic above still flags the mismatch.
                //
                // v4.18 FS-7: the bid's encumbrance is part real cash, part
                // `procurement_credit` (the solvency-bounded share booked at
                // submission). Nothing was delivered, so the credit share is a
                // phantom obligation — discharging it keeps unfilled bids from
                // ratcheting `procurement_credit` to infinity (observed:
                // construction contractors carrying 2.5B of never-delivered
                // "payables", payroll capacity permanently negative, crews
                // pinned at skeleton). `release_bid_encumbrance` returns only
                // the cash-backed share — crediting the full release would
                // mint money equal to the discharged credit.
                let cash_back = company.release_bid_encumbrance(refund);
                // Phase 94: Only credit one cash field to avoid M0 duplication.
                if let Some(ba) = &mut company.brokerage_account {
                    ba.cash += cash_back;
                } else {
                    company.available_cash += cash_back;
                }
                // Phase 45: Track unfilled bid prices for dynamic price feedback.
                // Store the limit price so next turn's bid can be raised.
                company
                    .unfilled_bid_prices
                    .insert(bid.commodity, bid.limit_price);
            }
        }
    }

    // Unfilled-ask decay bookkeeping (seller-side counterpart to the bid
    // ratchet): every ask left in the book after matching is unfilled, so the
    // seller's next ask is capped lower. Near-zero residual quantities mean
    // the ask filled — skip those so a sold-out ask doesn't spuriously decay.
    for asks in order_book.asks.values() {
        for ask in asks {
            if ask.quantity < 1e-9 {
                continue;
            }
            if let Some(&idx) = company_id_to_idx.get(&ask.seller_id) {
                companies[idx]
                    .unfilled_ask_prices
                    .entry(ask.commodity)
                    .and_modify(|p| *p = p.min(ask.limit_price))
                    .or_insert(ask.limit_price);
            }
        }
    }
}

/// Settle defense procurement trades by syncing the seller's bank balance sheet.
///
/// This handles trades where `buyer_id == "MIN-DEF"`. The seller was already credited
/// by `settle_trades` (cash + brokerage_account), and the Treasury was already
/// debited (encumbered) at bid submission time. This function only syncs the
/// seller's cash and bank balance sheet (deposits + reserves) via
/// `TransferSettler::credit_company_by_id`, ensuring proper double-entry
/// accounting. `settle_trades` skips the cash credit for defense trades
/// (buyer_id == "MIN-DEF") so that this function can perform the full
/// credit atomically through the TransferSettler module (Black Hole 1.19).
///
/// # Arguments
/// * `trades` - All executed trades (only defense trades are processed).
/// * `companies` - Mutable slice of companies (for cash credit + bank sync).
/// * `_country` - Mutable country (unused — Treasury was already debited at encumbrance).
pub fn settle_defense_trades(
    trades: &[Trade],
    companies: &mut [Company],
    country: &mut crate::state::Country,
) {
    for trade in trades {
        if trade.buyer_id != "MIN-DEF" {
            continue;
        }
        let trade_value = trade.quantity * trade.execution_price;
        if trade_value <= 0.0 {
            continue;
        }

        // Phase 94: Debit treasury (buyer) for the trade value. Previously,
        // cash was encumbered at bid submission time (previous turn), creating
        // a cross-turn M0 destruction. Now we debit at settlement time.
        country.budget.liquid_reserves -= trade_value;

        // Credit seller's cash AND sync bank balance sheet atomically
        // via TransferSettler (Black Hole 1.19).
        if crate::economy::transfer_settler::credit_company_by_id(
            companies,
            &trade.seller_id,
            trade_value,
        ) {
            // Revenue recognition for realized-profit accounting: a defense
            // sale is settled revenue the same as a B2B goods sale.
            if let Some(c) = companies.iter_mut().find(|c| c.id == trade.seller_id) {
                c.turn_settled_sales += trade_value;
            }
        }
    }
}

/// Refund unfilled defense bids back to the Treasury (per-country).
///
/// Defense bids (`buyer_id == "MIN-DEF"`) are not associated with any company,
/// so `refund_unfilled_bids` cannot refund them. This function computes the
/// refund as `total_encumbered - filled_encumbered`, where:
/// * `total_encumbered` = sum of all original defense bid values.
/// * `filled_encumbered` = sum of `trade.quantity * trade.bid_limit_price` for
///   defense trades where the seller is a company in this country.
///
/// The refund covers both unfilled bid portions and the price difference
/// between `limit_price` and `execution_price` on filled trades.
///
/// # Arguments
/// * `original_bids` - The defense bids originally submitted by this country's MoD.
/// * `trades` - All executed trades (only defense trades are considered).
/// * `companies` - Slice of this country's companies (to match sellers).
/// * `country` - Mutable country (for Treasury refund).
pub fn refund_unfilled_defense_bids_per_country(
    original_bids: &[crate::economy::order_book::Bid],
    trades: &[Trade],
    companies: &[Company],
    country: &mut crate::state::Country,
) {
    let total_encumbered: f64 = original_bids
        .iter()
        .map(|b| b.quantity * b.limit_price)
        .sum();

    let filled_paid: f64 = trades
        .iter()
        .filter(|t| t.buyer_id == "MIN-DEF" && companies.iter().any(|c| c.id == t.seller_id))
        .map(|t| t.quantity * t.execution_price)
        .sum();

    // Phase 94: No refund needed — we no longer encumber cash at bid
    // submission time. The treasury is debited in settle_defense_trades
    // only for executed trades. Unfilled bids don't move cash.
    let _refund = (total_encumbered - filled_paid).max(0.0);
    let _ = &original_bids; // suppress unused warning
}

/// Execute production for all buildings after trade settlement.
///
/// # Arguments
/// * `buildings` - Mutable slice of all buildings (inventory consumed and produced).
/// * `commercial_buildings` - Mutable slice of warehouse buildings for overflow.
/// * `companies` - Mutable slice of companies (for warehouse fee deduction).
/// * `config` - B2B order configuration (for inventory capacity and storage fees).
///
/// # Returns
/// A vector of `ProductionResult` for each building.
///
/// # Rules
/// * Check delivered inputs in `building.inventory`.
/// * Calculate `fulfillment_ratio = min(input_available / input_required)` per BOM.
/// * Consume inputs: `building.inventory[commodity] -= required × fulfillment_ratio`.
/// * Produce outputs: `building.inventory[commodity] += output_per_1k × scale × fulfillment_ratio × efficiency`.
/// * Wages are NOT paid here — already settled in Phase W1.
/// * Overflow above `inventory_capacity` routes to warehouse (storage fees).
/// * If no warehouse capacity, excess perishes immediately (NO next-turn buffer).
pub fn execute_production_cycle(
    buildings: &mut [Building],
    commercial_buildings: &mut [crate::society::housing::CommercialBuilding],
    companies: &mut [Company],
    config: &B2bOrderConfig,
    sector_filter: Option<Sector>,
    exclude_sector: Option<Sector>,
    efficiency_penalties: Option<&std::collections::HashMap<String, f64>>,
    gen_config: &crate::economy::generative_goods_config::GenerativeGoodsConfig,
    frontier_year: u32,
) -> Vec<ProductionResult> {
    use crate::economy::fixed_assets::machinery_factor;

    let mut results = Vec::new();

    for building in buildings.iter_mut() {
        // Skip brand-new construction sites (no workers, active project)
        if building.active_project.is_some() && building.worker_capacity == 0 {
            continue;
        }

        // Wave filtering: if sector_filter is Some, only process buildings in that sector
        if let Some(filter_sector) = sector_filter {
            if building.sector != filter_sector {
                continue;
            }
        }

        // Fix 1.20-C: Exclude a sector that was already processed in an earlier
        // wave. The turn loop runs execute_production_cycle twice per country
        // per turn: wave 1 with sector_filter=Some(Energy), then wave 2 with
        // sector_filter=None (i.e. "all"). Without an exclusion, wave 2
        // re-processes every Energy building. Energy buildings that consumed
        // their inputs in wave 1 have fulfillment_ratio=0 in wave 2, so
        // outputs_produced is empty and building.last_profit is overwritten to
        // -write_down, clobbering wave 1's real profit. Passing
        // exclude_sector=Some(Sector::Energy) on the wave-2 call prevents this
        // double-processing. (No-input extractive buildings would otherwise
        // also double-produce.)
        if let Some(excl_sector) = exclude_sector {
            if building.sector == excl_sector {
                continue;
            }
        }

        let method = &building.active_method;
        // `current_employment` is per-virtual-plant; per-1k BOM rates (and
        // genesis output calibrations) target the entity's total workforce,
        // so scale must be per-plant x scale_factor / 1000.
        let production_scale = building.current_employment as f64
            * building.scale_factor.max(1) as f64
            / 1000.0;
        let efficiency = method.efficiency;

        // Phase 19B: Fixed-asset cohorts provide a capacity multiplier.
        // Empty cohorts → factor = 1.0 (pre-Phase-19 behavior, no GDP cliff).
        // Installed machinery → factor = 1.0 + Σ(count × quality × condition × obs × unit_capacity).
        let machinery_factor = machinery_factor(&building.fixed_assets, frontier_year, gen_config);

        // Calculate required inputs and available inputs.
        // Phase 19B: Skip fixed-asset commodities — they're not consumed per-turn.
        let mut fulfillment_ratio = 1.0;
        #[cfg(feature = "diagnostic")]
        let mut binding_input: Option<Commodity> = None;
        for (&commodity, &qty_per_1k) in &method.inputs {
            if commodity.is_fixed_asset() {
                continue; // Fixed assets provide capacity, not consumable input.
            }
            // Phase 94: Skip local utility commodities (Energy, Heat, Water).
            // Utilities are distributed by the grid and never deposited into
            // building.inventory; shortage is expressed via efficiency_penalties
            // (applied below at the fulfillment_ratio clamp). Without this
            // skip, utility-input methods are permanently starved at ratio=0.
            // Mirrors the bid-side skip at line 306.
            if commodity.is_local_utility() {
                continue;
            }
            // Food is worker sustenance purchased B2C from wages, not a
            // firm-held production factor. Gating extraction on it creates
            // a circular deadlock: food requires harvest+processing, but
            // mines/forests cannot run without it.
            if commodity == Commodity::Food {
                continue;
            }
            // 24-turn crucible W12: for pm_thermal methods, fuel inputs are
            // OR-ed capacity slots (max fuel the plant can accept), not a
            // Leontief AND-list — Coal-Fired Boilers burn HardCoal OR
            // BrownCoal OR Peat. The forecast path (process_building_cycle)
            // already picks the cheapest available mix, but this physical
            // gate required EVERY listed fuel, so plants holding two of
            // three fuels starved at ratio=0 — the mass Energy-sector
            // stall. Fuel coverage is folded in below as aggregate MJ.
            if method.thermal_efficiency > 0.0 && commodity.is_fuel() {
                continue;
            }
            let required = qty_per_1k * production_scale;
            if required > 0.0 {
                let available = building.inventory.get(&commodity).copied().unwrap_or(0.0);
                let ratio = available / required;
                if ratio < fulfillment_ratio {
                    fulfillment_ratio = ratio;
                    #[cfg(feature = "diagnostic")]
                    {
                        binding_input = Some(commodity);
                    }
                }
            }
        }

        // W12: thermal fuel coverage — each slot contributes up to its
        // listed capacity (the plant's max acceptance rate for that fuel),
        // counted as calorific MJ. The fuel-side fulfillment ratio is the
        // achievable MJ fraction of the full-load requirement, mirroring
        // process_building_cycle's required_mj = target_energy / eta.
        if method.thermal_efficiency > 0.0 {
            let target_energy = method
                .outputs
                .get(&Commodity::Energy)
                .copied()
                .unwrap_or(0.0)
                * production_scale;
            let required_mj = target_energy / method.thermal_efficiency;
            if required_mj > 0.0 {
                let mut provided_mj = 0.0;
                let mut scarcest: Option<Commodity> = None;
                let mut fuel_slots: Vec<(Commodity, f64)> = method
                    .inputs
                    .iter()
                    .filter(|(c, _)| c.is_fuel())
                    .map(|(&c, &v)| (c, v))
                    .collect();
                fuel_slots.sort_by_key(|(c, _)| *c);
                for (fuel, slot_per_1k) in fuel_slots.iter() {
                    let slot_cap = *slot_per_1k * production_scale;
                    let on_hand = building
                        .inventory
                        .get(fuel)
                        .copied()
                        .unwrap_or(0.0)
                        .min(slot_cap);
                    provided_mj += on_hand * fuel.calorific_value_mj_per_unit();
                    if on_hand < slot_cap && scarcest.is_none() {
                        scarcest = Some(*fuel);
                    }
                }
                let fuel_ratio = (provided_mj / required_mj).min(1.0);
                if fuel_ratio < fulfillment_ratio {
                    fulfillment_ratio = fuel_ratio;
                    #[cfg(feature = "diagnostic")]
                    {
                        binding_input = scarcest.or(binding_input);
                    }
                }
            }
        }
        #[cfg(feature = "diagnostic")]
        if fulfillment_ratio < 0.1 {
            let inv_qty = binding_input
                .and_then(|c| building.inventory.get(&c).copied())
                .unwrap_or(0.0);
            let req_qty = binding_input
                .and_then(|c| method.inputs.get(&c).copied())
                .unwrap_or(0.0)
                * production_scale;
            let inv_total: f64 = building.inventory.values().sum();
            let mut top: Vec<(&Commodity, &f64)> = building.inventory.iter().collect();
            top.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
            let top3: Vec<String> = top
                .iter()
                .take(3)
                .map(|(c, q)| format!("{:?}={:.0}", c, q))
                .collect();
            eprintln!(
                "BINDING_INPUT[{}]: sector={:?} year={} fy={} built={} mname={} input={:?} inv={:.1} req={:.1} emp={} cap={} invtot={:.0} top={} ratio={:.3}",
                building.id,
                building.sector,
                method.year,
                frontier_year,
                building.year_built,
                building.active_method.active_methods.production,
                binding_input,
                inv_qty,
                req_qty,
                building.current_employment,
                building.worker_capacity,
                inv_total,
                top3.join(","),
                fulfillment_ratio
            );
        }

        // Clamp fulfillment ratio to [0, 1]
        fulfillment_ratio = fulfillment_ratio.clamp(0.0, 1.0);

        // Apply blackout efficiency penalty (Wave 3 only)
        // Phase 81: IndustrialBuff is a negative penalty (buff). The buff
        // increases effective production but CANNOT bypass physical BOM
        // availability. The fulfillment_ratio is re-clamped to [0, 1] after
        // the buff is applied — cheap energy cannot synthesize matter.
        if let Some(penalties) = efficiency_penalties {
            if let Some(&penalty) = penalties.get(&building.id) {
                fulfillment_ratio *= (1.0 - penalty).max(0.0);
            }
        }

        // Phase 81: Re-clamp fulfillment_ratio to [0, 1] after any buff/penalty.
        // This prevents the IndustrialBuff from pushing fulfillment above 1.0,
        // which would allow output beyond physical input availability.
        fulfillment_ratio = fulfillment_ratio.clamp(0.0, 1.0);

        // 24-turn fix: demand-side stock-cover throttle. Physical inventory
        // is capacity-bounded — a building already holding ≥ COVER_TURNS of
        // unsold output has nowhere to put more. Producing into a full
        // warehouse feeds only the overflow-destroy path: inputs burn, output
        // is written off, and `inventory_write_down ≈ output value` hits the
        // P&L every turn (the −trillion sector losses). Throttling the run
        // stops input consumption AND overflow destruction at the source;
        // the building reports material distress (`last_fulfillment_ratio`)
        // so the corporate AI furloughs surplus labor rather than producing
        // into the void. Local-utility outputs (grid-dispatched, never
        // inventoried) and fixed-asset outputs are excluded from the cover.
        {
            const OUTPUT_STOCK_COVER_TURNS: f64 = 2.0;
            let per_turn_output: f64 = method
                .outputs
                .iter()
                .filter(|(c, _)| !c.is_local_utility() && !c.is_fixed_asset())
                .map(|(_, q)| *q)
                .sum::<f64>()
                * production_scale;
            if per_turn_output > 0.0 {
                let on_hand_output: f64 = method
                    .outputs
                    .keys()
                    .filter(|c| !c.is_local_utility() && !c.is_fixed_asset())
                    .map(|c| building.inventory.get(c).copied().unwrap_or(0.0))
                    .sum();
                let cover = on_hand_output / per_turn_output;
                let throttle =
                    (1.0 - cover / OUTPUT_STOCK_COVER_TURNS).clamp(0.0, 1.0);
                if throttle < fulfillment_ratio {
                    fulfillment_ratio = throttle;
                }
            }
        }

        // Emergency Stabilization: Store the fulfillment ratio on the building
        // so the corporate AI can detect raw-material distress and choose
        // furlough over permanent layoffs.
        building.last_fulfillment_ratio = fulfillment_ratio;

        // Consume inputs (skip fixed-asset commodities — they're not consumed).
        let mut inputs_consumed: HashMap<Commodity, f64> = HashMap::default();
        for (&commodity, &qty_per_1k) in &method.inputs {
            if commodity.is_fixed_asset() {
                continue;
            }
            // Phase 94: Skip local utility commodities (Energy, Heat, Water).
            // Same rationale as the fulfillment loop above: grids express
            // shortage via efficiency_penalties, not inventory depletion.
            if commodity.is_local_utility() {
                continue;
            }
            // Food is worker sustenance (wage-funded B2C), not consumed
            // from building inventory — mirrors the fulfillment-gate skip.
            if commodity == Commodity::Food {
                continue;
            }
            // W12: thermal fuel slots are burned via the allocated mix
            // below — deducting every listed fuel at the fixed rate would
            // triple-charge plants for fuels they never combust.
            if method.thermal_efficiency > 0.0 && commodity.is_fuel() {
                continue;
            }
            let required = qty_per_1k * production_scale * fulfillment_ratio;
            if required > 0.0 {
                let available = building.inventory.get(&commodity).copied().unwrap_or(0.0);
                let consumed = required.min(available);
                let remaining = (available - consumed).max(0.0);
                if remaining > 0.0 {
                    building.inventory.insert(commodity, remaining);
                } else {
                    building.inventory.remove(&commodity);
                }
                inputs_consumed.insert(commodity, consumed);
            }
        }

        // W12: physical fuel burn for thermal methods — allocate the
        // required MJ (scaled by fulfillment) across the plant's fuel
        // slots, highest calorific density first. Mirrors
        // process_building_cycle's cheapest-first allocation; without
        // price access here, calorific density is the deterministic
        // efficiency ordering. Each slot contributes up to its listed
        // per-cycle capacity.
        if method.thermal_efficiency > 0.0 {
            let target_energy = method
                .outputs
                .get(&Commodity::Energy)
                .copied()
                .unwrap_or(0.0)
                * production_scale;
            let mut remaining_mj =
                target_energy / method.thermal_efficiency * fulfillment_ratio;
            if remaining_mj > 0.0 {
                let mut slots: Vec<(Commodity, f64, f64)> = method
                    .inputs
                    .iter()
                    .filter(|(c, _)| c.is_fuel())
                    .map(|(c, q)| {
                        (*c, *q * production_scale, c.calorific_value_mj_per_unit())
                    })
                    .collect();
                slots.sort_by(|a, b| {
                    b.2.partial_cmp(&a.2)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(a.0.cmp(&b.0))
                });
                for (fuel, slot_cap, cv) in slots {
                    if remaining_mj <= 0.0 {
                        break;
                    }
                    let on_hand = building.inventory.get(&fuel).copied().unwrap_or(0.0);
                    let burn = on_hand.min(slot_cap).min(remaining_mj / cv);
                    if burn <= 0.0 {
                        continue;
                    }
                    let leftover = on_hand - burn;
                    if leftover > 0.0 {
                        building.inventory.insert(fuel, leftover);
                    } else {
                        building.inventory.remove(&fuel);
                    }
                    *inputs_consumed.entry(fuel).or_insert(0.0) += burn;
                    remaining_mj -= burn * cv;
                }
            }
        }

        // Produce outputs (multiplied by the Phase 19B machinery capacity factor).
        let mut outputs_produced: HashMap<Commodity, f64> = HashMap::default();
        for (&commodity, &qty_per_1k) in &method.outputs {
            let produced =
                qty_per_1k * production_scale * fulfillment_ratio * efficiency * machinery_factor;
            if produced > 0.0 {
                *building.inventory.entry(commodity).or_insert(0.0) += produced;
                outputs_produced.insert(commodity, produced);
            }
        }

        // Fix 1.21: Track the financial value of destroyed overflow inventory.
        let unit_cost_for_writeoff = calculate_unit_cost(building, &HashMap::default(), 0.0);
        let mut inventory_write_down: f64 = 0.0;
        // Phase 29: Track total overflow costs (storage fees + write-downs)
        // for ROI-driven warehouse construction decisions.
        let mut overflow_costs_this_turn: f64 = 0.0;

        // W7: Overflow must not eat the production buffer. Proportional
        // shed/destroy treats every commodity alike — a producer whose
        // output floods past capacity loses its seeded INPUTS in the same
        // ratio, then starves next turn despite posting huge asks. Method
        // inputs are NEVER shed by overflow: spoilage applies to outputs and
        // surplus stock only; whether to sell inputs is the ask loop's
        // decision, not the rot mechanic's.
        let movable_share = |commodity: Commodity, qty: f64, share: f64| {
            // Food is not an effective input (sustenance is wage-funded),
            // so seeded food stock may be shed like surplus.
            if method.inputs.contains_key(&commodity)
                && !commodity.is_local_utility()
                && commodity != Commodity::Food
            {
                return 0.0;
            }
            qty * share
        };

        // Handle inventory overflow
        let total_inventory: f64 = building.inventory.values().sum();
        if total_inventory > building.inventory_capacity {
            let overflow = total_inventory - building.inventory_capacity;

            // Try to route overflow to warehouse
            let warehouse_capacity =
                find_warehouse_capacity(commercial_buildings, &building.region_id);

            if warehouse_capacity >= overflow {
                // Route to warehouse — deduct storage fees from company, credit warehouse owner
                let storage_fee = overflow * config.warehouse_storage_fee_per_ton;
                overflow_costs_this_turn += storage_fee;
                let warehouse_owner =
                    find_warehouse_owner(commercial_buildings, &building.region_id);
                let _debited = debit_company_by_id(companies, &building.owner_id, storage_fee);
                if let Some(ref owner_id) = warehouse_owner {
                    if !owner_id.is_empty() {
                        credit_company_by_id(companies, owner_id, _debited);
                    }
                }
                // Move overflow commodities to warehouse (proportionally,
                // protected by the input reserve).
                let ratio = overflow / total_inventory;
                for (&commodity, &qty) in &building.inventory.clone() {
                    let moved = movable_share(commodity, qty, ratio);
                    if moved > 0.0 {
                        let remaining = (qty - moved).max(0.0);
                        if remaining > 0.0 {
                            building.inventory.insert(commodity, remaining);
                        } else {
                            building.inventory.remove(&commodity);
                        }
                        // Add to warehouse
                        deposit_to_warehouse(
                            commercial_buildings,
                            &building.region_id,
                            commodity,
                            moved,
                        );
                    }
                }
            } else if warehouse_capacity > 0.0 {
                // Partial warehouse storage, rest perishes
                let storable = warehouse_capacity;
                let _ratio = storable / overflow;
                let storage_fee = storable * config.warehouse_storage_fee_per_ton;
                overflow_costs_this_turn += storage_fee;
                let warehouse_owner =
                    find_warehouse_owner(commercial_buildings, &building.region_id);
                let _debited = debit_company_by_id(companies, &building.owner_id, storage_fee);
                if let Some(ref owner_id) = warehouse_owner {
                    if !owner_id.is_empty() {
                        credit_company_by_id(companies, owner_id, _debited);
                    }
                }
                let inv_ratio = storable / total_inventory;
                for (&commodity, &qty) in &building.inventory.clone() {
                    let moved = movable_share(commodity, qty, inv_ratio);
                    if moved > 0.0 {
                        let remaining = (qty - moved).max(0.0);
                        if remaining > 0.0 {
                            building.inventory.insert(commodity, remaining);
                        } else {
                            building.inventory.remove(&commodity);
                        }
                        deposit_to_warehouse(
                            commercial_buildings,
                            &building.region_id,
                            commodity,
                            moved,
                        );
                    }
                }
                // Remaining overflow perishes — destroy excess (surplus first;
                // input reserve is touched only if unprotected stock cannot
                // cover the excess — a hard physical capacity limit).
                let new_total: f64 = building.inventory.values().sum();
                if new_total > building.inventory_capacity {
                    let excess = new_total - building.inventory_capacity;
                    let destroy_ratio = excess / new_total;
                    // Fix 1.21: Record the financial write-down for destroyed inventory
                    inventory_write_down += excess * unit_cost_for_writeoff;
                    overflow_costs_this_turn += excess * unit_cost_for_writeoff;
                    for (&commodity, &qty) in &building.inventory.clone() {
                        let destroyed = movable_share(commodity, qty, destroy_ratio);
                        let remaining = (qty - destroyed).max(0.0);
                        if remaining > 0.0 {
                            building.inventory.insert(commodity, remaining);
                        } else {
                            building.inventory.remove(&commodity);
                        }
                    }
                }
            } else {
                // No warehouse capacity — all overflow perishes immediately,
                // surplus first (input reserve protected; hard-cap fallback below).
                let destroy_ratio = overflow / total_inventory;
                // Fix 1.21: Record the financial write-down for destroyed inventory
                inventory_write_down += overflow * unit_cost_for_writeoff;
                overflow_costs_this_turn += overflow * unit_cost_for_writeoff;
                for (&commodity, &qty) in &building.inventory.clone() {
                    let destroyed = movable_share(commodity, qty, destroy_ratio);
                    let remaining = (qty - destroyed).max(0.0);
                    if remaining > 0.0 {
                        building.inventory.insert(commodity, remaining);
                    } else {
                        building.inventory.remove(&commodity);
                    }
                }
            }

            // W7: Hard-cap fallback — if the input reserve itself exceeds
            // physical capacity, the surplus-first passes cannot shed enough.
            // Destroy proportionally across everything that remains.
            let post_total: f64 = building.inventory.values().sum();
            if post_total > building.inventory_capacity {
                let excess = post_total - building.inventory_capacity;
                let destroy_ratio = excess / post_total;
                inventory_write_down += excess * unit_cost_for_writeoff;
                overflow_costs_this_turn += excess * unit_cost_for_writeoff;
                for (&commodity, &qty) in &building.inventory.clone() {
                    let destroyed = qty * destroy_ratio;
                    let remaining = (qty - destroyed).max(0.0);
                    if remaining > 0.0 {
                        building.inventory.insert(commodity, remaining);
                    } else {
                        building.inventory.remove(&commodity);
                    }
                }
            }
        }

        // Phase 29: Store overflow costs on building for regional aggregation.
        if overflow_costs_this_turn > 0.0 {
            building.extra.insert(
                "overflow_costs_this_turn".to_string(),
                serde_json::Value::from(overflow_costs_this_turn),
            );
        }

        // Phase 80: Clear last_production BEFORE inserting actual production.
        // process_building_cycle_with_geology (called earlier in the turn) sets
        // last_production with FULL output quantities (no inventory check). If we
        // don't clear, commodities with 0 actual production retain their phantom
        // values from the market-order generation step — causing the UI to show
        // e.g. 800 Trains "produced" despite zero Steel being available.
        building.last_production.clear();
        for (&commodity, &qty) in &outputs_produced {
            building.last_production.insert(commodity, qty);
        }

        // Fix 1.20 (revised): Calculate actual financial metrics using a real
        // per-commodity price oracle. The previous implementation called
        // `calculate_unit_cost(building, &HashMap::default(), 0.0)` — an empty
        // reference-price map plus a zero base wage yields unit_cost = 0 for
        // every building, so output_revenue = 0 and input_costs = 0. With
        // gross_profit = -inventory_write_down <= 0, EVERY building reports a
        // non-positive last_profit regardless of real production. This made
        // the "0.00 Income" diagnostic structural (manager.rs writes record
        // revenue = total_profit + overhead = 0 for all companies).
        //
        // The fix values outputs and inputs at `estimated_base_price`, the
        // same per-commodity base-price table that seeds
        // `market_history.global_base_prices` at world generation (see
        // generator/mod.rs Phase 80). It is the canonical stable price
        // anchor used throughout the clearing engine, so using it here keeps
        // the ProductionResult consistent with the rest of the accounting path.
        //
        // Inputs are valued at 0.5× their base price. Rationale: inputs were
        // already valued at full base price as outputs by their upstream
        // producers; valuing them at full price again in the downstream
        // producer's cost base would double-count the upstream output value
        // and drive gross_profit negative for genuine value-added
        // transformation. The 0.5 factor approximates a value-added margin:
        // gross_profit = Σ(output_qty × base_price) − 0.5 × Σ(input_qty ×
        // base_price) − write_down, which is positive for any transformation
        // whose outputs are at least half as valuable (in aggregate) as its
        // inputs — the normal case for a functioning BOM.
        let value_price = |c: Commodity| {
            crate::engine::generator::corporate::anchored_base_price(c)
        };
        let input_costs: f64 = {
            let mut terms: Vec<f64> = inputs_consumed
                .iter()
                .map(|(&commodity, &qty)| qty * value_price(commodity) * 0.5)
                .collect();
            terms.sort_by(|a, b| a.total_cmp(b));
            terms.into_iter().sum()
        };
        let output_revenue: f64 = {
            let mut terms: Vec<f64> = outputs_produced
                .iter()
                .map(|(&commodity, &qty)| qty * value_price(commodity))
                .collect();
            terms.sort_by(|a, b| a.total_cmp(b));
            terms.into_iter().sum()
        };

        // Macro-Remediation: subtract the wage bill so `last_profit` reflects
        // true insolvency. `process_building_cycle*` (earlier this turn) stored
        // the estimated wage bill in `building.extra["last_wage_bill"]`; the
        // previous wage-exclusive profit here masked unprofitable buildings
        // and delayed furloughs by several turns.
        let wage_bill = building
            .extra
            .get("last_wage_bill")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        // Macro-Remediation: persist the physical-cycle output value so
        // `process_company` can record true revenue (the history's
        // "revenue" field was previously total_profit + overhead — a
        // margin figure that goes negative whenever a company is
        // unprofitable, making real sales invisible to diagnostics).
        building.extra.insert(
            "last_output_value".to_string(),
            serde_json::Value::from(output_revenue),
        );

        // 24-turn fix: persist consumed-input cost so process_company can
        // compute realized (settled-sales) profit instead of oracle accrual.
        building.extra.insert(
            "last_input_cost".to_string(),
            serde_json::Value::from(input_costs),
        );

        // Fix 1.21: Record inventory write-down loss from overflow destruction.
        // The write_down value was accumulated during overflow handling above.
        let gross_profit = output_revenue - input_costs - inventory_write_down - wage_bill;

        // Update building.last_profit for corporate/tax processing
        building.last_profit = gross_profit;

        results.push(ProductionResult {
            inputs_consumed,
            outputs_produced,
            wages_paid: wage_bill,
            input_costs,
            output_revenue,
            gross_profit,
            blueprint_id: None,
            quality: None,
            durability: None,
        });
    }

    results
}

/// Phase 95: Consume inventory cohorts for a commodity (FIFO — oldest first).
///
/// # Rules
/// * Sorts cohorts by `produced_turn` (ascending) and consumes from oldest first.
/// * Removes depleted cohorts (quantity ≤ 0).
/// * Does not sell more physical goods than exist — clamps to available.
fn consume_inventory_cohorts(
    cohorts: &mut Vec<crate::economy::fixed_assets::InventoryCohort>,
    commodity: Commodity,
    quantity: f64,
) {
    // Filter to cohorts of this commodity and sort by produced_turn (FIFO).
    let mut remaining = quantity;
    let mut to_remove: Vec<usize> = Vec::new();

    // Sort indices by produced_turn (ascending) for FIFO consumption.
    let mut indices: Vec<(usize, u32)> = cohorts
        .iter()
        .enumerate()
        .filter(|(_, c)| c.commodity == commodity)
        .map(|(i, c)| (i, c.produced_turn))
        .collect();
    indices.sort_by_key(|&(_, t)| t);

    for (i, _) in indices {
        if remaining <= 0.0 {
            break;
        }
        let available = cohorts[i].quantity;
        let consumed = remaining.min(available);
        cohorts[i].quantity -= consumed;
        remaining -= consumed;
        if cohorts[i].quantity <= 0.0 {
            to_remove.push(i);
        }
    }

    // Remove depleted cohorts (in reverse order to preserve indices).
    to_remove.sort();
    to_remove.dedup();
    for &i in to_remove.iter().rev() {
        cohorts.remove(i);
    }
}

/// Find available warehouse capacity in a region.
fn find_warehouse_capacity(
    commercial_buildings: &[crate::society::housing::CommercialBuilding],
    region_id: &str,
) -> f64 {
    commercial_buildings
        .iter()
        .filter(|b| {
            b.building_type == crate::society::housing::CommercialBuildingType::Warehouse
                && b.micro_region_id == region_id
        })
        .map(|b| {
            let total_stored: f64 = b
                .current_inventory
                .values()
                .flat_map(|batches| batches.iter().map(|batch| batch.quantity))
                .sum();
            (b.storage_capacity - total_stored).max(0.0)
        })
        .sum()
}

/// Find the owner company ID of the first warehouse with available capacity in a region.
fn find_warehouse_owner(
    commercial_buildings: &[crate::society::housing::CommercialBuilding],
    region_id: &str,
) -> Option<String> {
    commercial_buildings
        .iter()
        .filter(|b| {
            b.building_type == crate::society::housing::CommercialBuildingType::Warehouse
                && b.micro_region_id == region_id
        })
        .find(|b| {
            let total_stored: f64 = b
                .current_inventory
                .values()
                .flat_map(|batches| batches.iter().map(|batch| batch.quantity))
                .sum();
            (b.storage_capacity - total_stored) > 0.0
        })
        .map(|b| b.owner_id.clone())
}

/// Deposit a commodity quantity into the first available warehouse in a region.
fn deposit_to_warehouse(
    commercial_buildings: &mut [crate::society::housing::CommercialBuilding],
    region_id: &str,
    commodity: Commodity,
    quantity: f64,
) {
    let commodity_key: String = commodity.into();

    for building in commercial_buildings.iter_mut() {
        if building.building_type != crate::society::housing::CommercialBuildingType::Warehouse
            || building.micro_region_id != region_id
        {
            continue;
        }

        let total_stored: f64 = building
            .current_inventory
            .values()
            .flat_map(|batches| batches.iter().map(|batch| batch.quantity))
            .sum();

        let available = (building.storage_capacity - total_stored).max(0.0);
        if available <= 0.0 {
            continue;
        }

        let to_store = quantity.min(available);
        if to_store <= 0.0 {
            continue;
        }

        let batch = crate::society::housing::InventoryBatch {
            quantity: to_store,
            storage_turn: 0, // Will be set by caller in production
            owner_id: String::new(),
            accumulated_fees: 0.0,
            warehouse_id: building.id.clone(),
            fire_sale_discount: 0.0,
            acquisition_cost_per_unit: 0.0,
        };

        building
            .current_inventory
            .entry(commodity_key.clone())
            .or_insert_with(Vec::new)
            .push(batch);

        if to_store >= quantity {
            return; // All stored
        }
    }
}

// ── Phase 19B: Maintenance Service B2B Market ─────────────────────────────

/// Submit Buy Bids for `Commodity::MaintenanceServices` on behalf of factories
/// with degraded fixed-asset cohorts.
///
/// # Rules
/// * For each building with `fixed_assets`, compute `maintenance_services_needed`.
/// * If needed > 0, submit a Buy Bid for `MaintenanceServices` at a reference
///   price (from `market_history` or a fallback).
/// * Cash is encumbered exactly like normal B2B bids.
/// * MaintenanceServices Sell Asks are generated naturally by MaintenanceWorkshops
///   buildings via the normal output-ask loop in `submit_company_b2b_orders`
///   (MaintenanceServices is in their `method.outputs`).
pub fn submit_maintenance_service_bids(
    companies: &mut [Company],
    buildings: &[Building],
    order_book: &mut OrderBook,
    market_history: &MarketHistory,
    b2b_config: &B2bOrderConfig,
    gen_config: &crate::economy::generative_goods_config::GenerativeGoodsConfig,
    owner_to_buildings: &rustc_hash::FxHashMap<String, Vec<usize>>,
) -> Vec<String> {
    use crate::economy::fixed_assets::maintenance_services_needed;
    use crate::registries::enums::Commodity;

    let messages = Vec::new();
    for company in companies.iter_mut() {
        let liquid = company
            .brokerage_account
            .as_ref()
            .map(|ba| ba.cash.max(0.0))
            .unwrap_or_else(|| company.available_cash.max(0.0));
        // Phase 94: Do NOT overwrite available_cash (M0 duplication for unbanked companies)
        // Maintenance/capital procurement is an operating necessity, not
        // discretionary spend — use the same working-capital facility as
        // the primary B2B loop (liquidity plus net worth, payroll reserved
        // first) so cash-poor factories still post demand instead of
        // starving MaintenanceWorkshops and machinery sellers into the
        // zero-FTE absorbing state.
        let working_capital_facility = procurement_working_capital_facility(company, liquid);
        let payroll_reserve = (company.wages_paid_this_turn * 2.0
            + company.wage_arrears
            + company.severance_arrears)
            .min(working_capital_facility.max(0.0));
        let max_encumber = (working_capital_facility - payroll_reserve).max(0.0)
            * b2b_config.max_cash_encumbrance_ratio;
        let mut total_encumbered = 0.0;

        // Phase 94: Use pre-built index map instead of O(B) filter scan.
        let company_building_indices = match owner_to_buildings.get(&company.id) {
            Some(indices) => indices.as_slice(),
            None => continue,
        };

        for &i in company_building_indices {
            let building = &buildings[i];
            if building.fixed_assets.is_empty() {
                continue;
            }
            let needed = maintenance_services_needed(&building.fixed_assets, gen_config);
            if needed <= 0.0 {
                continue;
            }
            let ref_price =
                get_reference_price(&Commodity::MaintenanceServices, market_history).unwrap_or(1.0);
            let limit_price = ref_price * (1.0 + b2b_config.buy_premium_ratio);
            let encumbrance = needed * limit_price;
            if total_encumbered + encumbrance > max_encumber {
                let remaining = max_encumber - total_encumbered;
                if remaining <= 0.0 || limit_price <= 0.0 {
                    continue;
                }
                let affordable_qty = remaining / limit_price;
                if affordable_qty <= 0.0 {
                    continue;
                }
                encumber_cash(company, affordable_qty * limit_price);
                total_encumbered += affordable_qty * limit_price;
                order_book
                    .bids
                    .entry(Commodity::MaintenanceServices)
                    .or_default()
                    .push(Bid {
                        buyer_id: company.id.clone(),
                        commodity: Commodity::MaintenanceServices,
                        quantity: affordable_qty,
                        limit_price,
                        blueprint_id: None,
                        min_quality: None,
                    });
            } else {
                encumber_cash(company, encumbrance);
                total_encumbered += encumbrance;
                order_book
                    .bids
                    .entry(Commodity::MaintenanceServices)
                    .or_default()
                    .push(Bid {
                        buyer_id: company.id.clone(),
                        commodity: Commodity::MaintenanceServices,
                        quantity: needed,
                        limit_price,
                        blueprint_id: None,
                        min_quality: None,
                    });
            }
        }
    }
    messages
}

/// Phase 19C: Submit Buy Bids for fixed-asset commodities (machinery/vehicles)
/// with **cash-bottlenecked** willingness-to-pay.
///
/// # Rules (affordability segmentation)
/// * Companies derive a `desired_wtp` from the reference price scaled by a
///   quality/durability premium (premium assets cost more but last longer).
/// * The bid QUANTITY is the missing complement of the building's installed
///   machine fleet: `target = method inputs × worker_capacity`, and installed
///   (non-scrapped) cohorts already count against it. Previously the full
///   complement was re-bid every turn — each cash-flush turn repurchased the
///   entire fleet (the sparse multi-billion `I` spikes) while installed
///   machinery went uncounted.
/// * The fleet target is scaled by `worker_capacity`, not
///   `current_employment`: a furloughed plant still needs its machine
///   complement to restart, and demand that collapses with the crew is what
///   produced the permanent-furlough ratchet (same basis the seeded input
///   buffer and construction reserve use).
/// * Cash-poor companies buy a partial fleet: `bid_qty` shrinks to what the
///   remaining encumbrance covers at the junk-price floor
///   (`ref × asset_purchase_starvation_ratio`), and `limit_price` degrades
///   smoothly from `desired_wtp` toward that floor. The old all-or-nothing
///   gate skipped the entire fleet whenever `remaining/desired` fell below
///   the floor — zero bid instead of a small one.
/// * Bids carry `blueprint_id: None` and `min_quality: None` — the matching
///   engine fills the cheapest compatible ask first (price-time priority), so
///   cash-poor buyers naturally receive lower-quality assets.
pub fn submit_fixed_asset_purchase_bids(
    companies: &mut [Company],
    buildings: &[Building],
    order_book: &mut OrderBook,
    market_history: &MarketHistory,
    b2b_config: &B2bOrderConfig,
    gen_config: &crate::economy::generative_goods_config::GenerativeGoodsConfig,
    owner_to_buildings: &rustc_hash::FxHashMap<String, Vec<usize>>,
) -> Vec<String> {
    let messages = Vec::new();
    for company in companies.iter_mut() {
        let liquid = company
            .brokerage_account
            .as_ref()
            .map(|ba| ba.cash.max(0.0))
            .unwrap_or_else(|| company.available_cash.max(0.0));
        // Phase 94: Do NOT overwrite available_cash (M0 duplication for unbanked companies)
        // Maintenance/capital procurement is an operating necessity, not
        // discretionary spend — use the same working-capital facility as
        // the primary B2B loop (liquidity plus net worth, payroll reserved
        // first) so cash-poor factories still post demand instead of
        // starving MaintenanceWorkshops and machinery sellers into the
        // zero-FTE absorbing state.
        // v4.19 FS-7: excludes receivable collateral — see
        // `procurement_working_capital_facility`.
        let working_capital_facility = procurement_working_capital_facility(company, liquid);
        let payroll_reserve = (company.wages_paid_this_turn * 2.0
            + company.wage_arrears
            + company.severance_arrears)
            .min(working_capital_facility.max(0.0));
        let max_encumber = (working_capital_facility - payroll_reserve).max(0.0)
            * b2b_config.max_cash_encumbrance_ratio;
        let mut total_encumbered = 0.0;

        // Phase 94: Use pre-built index map instead of O(B) filter scan.
        let company_building_indices = match owner_to_buildings.get(&company.id) {
            Some(indices) => indices.as_slice(),
            None => continue,
        };

        for &i in company_building_indices {
            let building = &buildings[i];
            let method = &building.active_method;
            // Fleet demand scales with installed capacity (`worker_capacity`),
            // not this turn's crew: a furloughed plant still needs its machine
            // complement to restart. Keying on `current_employment` made CapEx
            // collapse exactly when the economy needs it (the furlough ratchet).
            let production_scale = building.worker_capacity as f64
                * building.scale_factor.max(1) as f64
                / 1000.0;

            let mut sorted_inputs: Vec<(Commodity, f64)> = method
                .inputs
                .iter()
                .map(|(&c, &q)| (c, q))
                .collect();
            sorted_inputs.sort_by_key(|(c, _)| *c);
            for &(commodity, qty_per_1k) in &sorted_inputs {
                // Only fixed-asset commodities (skipped by the normal bid loop).
                if !commodity.is_fixed_asset() {
                    continue;
                }
                let target_qty = qty_per_1k * production_scale;
                // Buy only the missing complement — installed non-scrapped
                // cohorts already supply this commodity's capacity.
                let installed: f64 = building
                    .fixed_assets
                    .iter()
                    .filter(|c| c.commodity == commodity && !c.is_scrapped())
                    .map(|c| c.count)
                    .sum();
                let desired_qty = (target_qty - installed).max(0.0);
                if desired_qty <= 0.0 {
                    continue;
                }

                let ref_price = match get_reference_price(&commodity, market_history) {
                    Some(p) => p,
                    None => continue,
                };

                // Desired willingness-to-pay: reference price scaled by a
                // quality/durability premium. Companies *want* premium assets
                // but can only afford what their cash allows.
                let quality_premium = gen_config.asset_quality_wtp_multiplier;
                let desired_wtp = ref_price * quality_premium;
                // Below the starvation floor the asset is junk not worth buying.
                let floor_price = ref_price * gen_config.asset_purchase_starvation_ratio;
                if floor_price <= 0.0 {
                    continue;
                }

                // Cash-bottlenecked quantity: a cash-tight company buys as many
                // units as its remaining encumbrance covers at the floor price
                // instead of skipping the whole fleet (old all-or-nothing gate).
                let remaining_encumbrance = (max_encumber - total_encumbered).max(0.0);
                if remaining_encumbrance <= 0.0 {
                    break; // No budget left for asset purchases.
                }
                let bid_qty = desired_qty.min(remaining_encumbrance / floor_price);
                // Skip dust lots: a company that can't fund even one machine
                // (or its whole small deficit) goes without this turn.
                let min_lot = desired_qty.min(1.0);
                if bid_qty < min_lot {
                    continue;
                }
                // WTP: the full premium while the deficit is fully affordable at
                // it; otherwise the per-unit price the headroom supports,
                // floored at the junk threshold.
                let limit_price = desired_wtp
                    .min(remaining_encumbrance / bid_qty)
                    .max(floor_price);

                let encumbrance = bid_qty * limit_price;
                encumber_cash(company, encumbrance);
                total_encumbered += encumbrance;

                order_book.bids.entry(commodity).or_default().push(Bid {
                    buyer_id: company.id.clone(),
                    commodity,
                    quantity: bid_qty,
                    limit_price,
                    blueprint_id: None,
                    min_quality: None,
                });
            }

            // Phase 45: Replacement demand from degraded fixed-asset cohorts.
            // For each cohort, compute the condition deficit:
            //   replacement_demand = count * (1.0 - condition)
            // This represents the quantity of new machinery needed to restore
            // the building's production capacity to full.
            let mut replacement_needed: std::collections::HashMap<Commodity, f64> =
                std::collections::HashMap::default();
            for cohort in &building.fixed_assets {
                if cohort.is_scrapped() {
                    continue;
                }
                let deficit = cohort.count * (1.0 - cohort.condition);
                if deficit > 0.0 {
                    *replacement_needed.entry(cohort.commodity).or_insert(0.0) += deficit;
                }
            }

            let mut sorted_replacements: Vec<(Commodity, f64)> =
                replacement_needed.iter().map(|(&c, &q)| (c, q)).collect();
            sorted_replacements.sort_by_key(|(c, _)| *c);
            for &(commodity, qty) in &sorted_replacements {
                if qty <= 0.0 {
                    continue;
                }
                let ref_price = match get_reference_price(&commodity, market_history) {
                    Some(p) => p,
                    None => continue,
                };
                let remaining_encumbrance = (max_encumber - total_encumbered).max(0.0);
                if remaining_encumbrance <= 0.0 {
                    break;
                }
                let quality_premium = gen_config.asset_quality_wtp_multiplier;
                let desired_wtp = ref_price * quality_premium;
                let floor_price = ref_price * gen_config.asset_purchase_starvation_ratio;
                if floor_price <= 0.0 {
                    continue;
                }
                // Same partial-fill affordability as the complement loop:
                // shrink the quantity to what headroom covers at the floor
                // price rather than skipping the purchase entirely.
                let bid_qty = qty.min(remaining_encumbrance / floor_price);
                let min_lot = qty.min(1.0);
                if bid_qty < min_lot {
                    continue;
                }
                let limit_price = desired_wtp
                    .min(remaining_encumbrance / bid_qty)
                    .max(floor_price);
                let encumbrance = bid_qty * limit_price;
                encumber_cash(company, encumbrance);
                total_encumbered += encumbrance;
                order_book.bids.entry(commodity).or_default().push(Bid {
                    buyer_id: company.id.clone(),
                    commodity,
                    quantity: bid_qty,
                    limit_price,
                    blueprint_id: None,
                    min_quality: None,
                });
            }
        }
    }
    messages
}

/// Apply the service leg of MaintenanceServices trades: condition
/// restoration on the buyer's fixed-asset cohorts.
///
/// # Rules
/// * Filters `trades` for `Commodity::MaintenanceServices` only.
/// * Cash leg: NONE — `settle_trades` already settled the cash for every
///   trade (encumbrance release + seller credit + bank sync). Crediting the
///   seller here a second time paid double and created fiat (W3 fix).
/// * Physical leg: NONE — MaintenanceServices is a service, consumed on delivery.
///   The buyer's cohort condition is restored proportionally to the quantity
///   bought.
/// * Returns the list of settled trades (for audit/logging).
pub fn settle_maintenance_service_trades(
    trades: &[Trade],
    _companies: &mut [Company],
    buildings: &mut [Building],
    gen_config: &crate::economy::generative_goods_config::GenerativeGoodsConfig,
) -> Vec<String> {
    use crate::economy::fixed_assets::restore_cohort_condition;
    use crate::registries::enums::Commodity;

    let mut messages = Vec::new();
    for trade in trades
        .iter()
        .filter(|t| t.commodity == Commodity::MaintenanceServices)
    {
        // W3: NO cash legs here — `settle_trades` (called by
        // `settle_trades_with_tariffs` before this function) already performs
        // the full cash settlement for every trade, including
        // MaintenanceServices: encumbrance release + seller credit via
        // settle_transfer_mapped / credit_company_by_id. Crediting the
        // seller AGAIN paid it twice per trade — creating bank reserves
        // (M0) from nothing (FiatCreation). This function's job is the
        // service leg only: restoring fixed-asset cohort condition.

        // Find all buildings owned by the buyer with fixed assets and distribute
        // the service quantity proportionally (restore_cohort_condition handles
        // the proportional distribution across cohorts within each building).
        let buyer_buildings: Vec<usize> = buildings
            .iter()
            .enumerate()
            .filter(|(_, b)| b.owner_id == trade.buyer_id && !b.fixed_assets.is_empty())
            .map(|(i, _)| i)
            .collect();

        if buyer_buildings.is_empty() {
            continue;
        }
        // Distribute the service quantity equally across the buyer's buildings
        // that have fixed assets (a simple, deterministic split).
        let per_building = trade.quantity / buyer_buildings.len() as f64;
        for &idx in &buyer_buildings {
            let building = &mut buildings[idx];
            restore_cohort_condition(&mut building.fixed_assets, per_building, gen_config);
        }
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::legal_form::{JointStockData, LegalForm};
    use crate::registries::enums::Sector;
    use crate::state::banking::BankBalanceSheet;
    use crate::state::{Country, Treasury};

    fn make_test_company(id: &str, cash: f64) -> Company {
        Company::new(
            id.to_string(),
            id.to_string(),
            Sector::LightIndustry,
            LegalForm::JointStockCompany(JointStockData::default()),
            100_000.0,
            cash,
            10,
        )
    }

    fn make_test_bank(id: &str, reserves: f64, deposits: f64) -> Company {
        let mut bank = Company::new(
            id.to_string(),
            id.to_string(),
            Sector::Banking,
            LegalForm::JointStockCompany(JointStockData::default()),
            1_000_000.0,
            0.0,
            5,
        );
        bank.balance_sheet = Some(BankBalanceSheet {
            reserves_at_central_bank: reserves,
            deposits,
            ..Default::default()
        });
        bank.bank_type = Some(crate::state::banking::BankType::Commercial);
        bank
    }

    fn make_test_country() -> Country {
        let mut country = Country::default();
        country.budget = Treasury {
            gdp: 1_000_000.0,
            population: 1000,
            nominal_budget: 500_000.0,
            liquid_reserves: 100_000.0,
            ..Default::default()
        };
        country
    }

    fn make_defense_trade(seller_id: &str, quantity: f64, price: f64) -> Trade {
        Trade {
            buyer_id: "MIN-DEF".to_string(),
            seller_id: seller_id.to_string(),
            commodity: Commodity::Ammunition,
            quantity,
            execution_price: price,
            bid_limit_price: price,
            blueprint_id: None,
            quality: None,
            durability: None,
        }
    }

    fn make_defense_bid(quantity: f64, price: f64) -> Bid {
        Bid {
            buyer_id: "MIN-DEF".to_string(),
            commodity: Commodity::Ammunition,
            quantity,
            limit_price: price,
            blueprint_id: None,
            min_quality: None,
        }
    }

    // --- settle_defense_trades tests ---

    #[test]
    fn test_settle_defense_trades_credits_seller_and_syncs_bank() {
        let mut companies = vec![
            make_test_company("seller_0", 1_000.0),
            make_test_bank("bank_0", 500_000.0, 400_000.0),
        ];
        companies[0].primary_bank_id = Some("bank_0".to_string());

        let initial_cash = companies[0].brokerage_account.as_ref().unwrap().cash;
        let initial_deposits = companies[1].balance_sheet.as_ref().unwrap().deposits;
        let initial_reserves = companies[1]
            .balance_sheet
            .as_ref()
            .unwrap()
            .reserves_at_central_bank;

        let trades = vec![make_defense_trade("seller_0", 100.0, 50.0)];
        let mut country = make_test_country();

        settle_defense_trades(&trades, &mut companies, &mut country);

        // Seller cash increased by trade value (100 * 50 = 5000)
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_cash + 5_000.0
        );
        // Bank deposits increased
        assert_eq!(
            companies[1].balance_sheet.as_ref().unwrap().deposits,
            initial_deposits + 5_000.0
        );
        // Bank reserves increased
        assert_eq!(
            companies[1]
                .balance_sheet
                .as_ref()
                .unwrap()
                .reserves_at_central_bank,
            initial_reserves + 5_000.0
        );
    }

    #[test]
    fn test_settle_defense_trades_skips_non_defense() {
        let mut companies = vec![
            make_test_company("seller_0", 1_000.0),
            make_test_bank("bank_0", 500_000.0, 400_000.0),
        ];
        companies[0].primary_bank_id = Some("bank_0".to_string());

        let initial_cash = companies[0].brokerage_account.as_ref().unwrap().cash;
        let initial_deposits = companies[1].balance_sheet.as_ref().unwrap().deposits;

        let trades = vec![Trade {
            buyer_id: "buyer_0".to_string(),
            seller_id: "seller_0".to_string(),
            commodity: Commodity::Ammunition,
            quantity: 100.0,
            execution_price: 50.0,
            bid_limit_price: 50.0,
            blueprint_id: None,
            quality: None,
            durability: None,
        }];
        let mut country = make_test_country();

        settle_defense_trades(&trades, &mut companies, &mut country);

        // Seller cash unchanged (non-defense trade skipped)
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_cash
        );
        // Bank deposits unchanged
        assert_eq!(
            companies[1].balance_sheet.as_ref().unwrap().deposits,
            initial_deposits
        );
    }

    #[test]
    fn test_settle_defense_trades_no_seller_bank() {
        let mut companies = vec![make_test_company("seller_0", 1_000.0)];

        let initial_cash = companies[0].brokerage_account.as_ref().unwrap().cash;

        let trades = vec![make_defense_trade("seller_0", 100.0, 50.0)];
        let mut country = make_test_country();

        settle_defense_trades(&trades, &mut companies, &mut country);

        // Seller cash still increased even without a bank
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_cash + 5_000.0
        );
    }

    #[test]
    fn test_settle_defense_trades_zero_value_skipped() {
        let mut companies = vec![make_test_company("seller_0", 1_000.0)];

        let initial_cash = companies[0].brokerage_account.as_ref().unwrap().cash;

        let trades = vec![make_defense_trade("seller_0", 0.0, 50.0)];
        let mut country = make_test_country();

        settle_defense_trades(&trades, &mut companies, &mut country);

        // No credit for zero-value trade
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_cash
        );
    }

    #[test]
    fn test_settle_defense_trades_seller_not_found() {
        let mut companies = vec![
            make_test_company("seller_0", 1_000.0),
            make_test_bank("bank_0", 500_000.0, 400_000.0),
        ];
        companies[0].primary_bank_id = Some("bank_0".to_string());

        let initial_deposits = companies[1].balance_sheet.as_ref().unwrap().deposits;

        let trades = vec![make_defense_trade("nonexistent_seller", 100.0, 50.0)];
        let mut country = make_test_country();

        settle_defense_trades(&trades, &mut companies, &mut country);

        // Bank unchanged — seller not found
        assert_eq!(
            companies[1].balance_sheet.as_ref().unwrap().deposits,
            initial_deposits
        );
    }

    // --- refund_unfilled_defense_bids_per_country tests ---

    #[test]
    fn test_refund_unfilled_defense_bids_full_refund() {
        let mut country = make_test_country();
        let initial_reserves = country.budget.liquid_reserves;

        let bids = vec![make_defense_bid(100.0, 50.0)];
        let trades: Vec<Trade> = vec![];
        let companies: Vec<Company> = vec![];

        refund_unfilled_defense_bids_per_country(&bids, &trades, &companies, &mut country);

        // Phase 94: No refund — cash is no longer encumbered at bid submission.
        // Treasury is debited only for executed trades in settle_defense_trades.
        assert_eq!(country.budget.liquid_reserves, initial_reserves);
    }

    #[test]
    fn test_refund_unfilled_defense_bids_partial_fill() {
        let mut country = make_test_country();
        let initial_reserves = country.budget.liquid_reserves;

        let bids = vec![make_defense_bid(100.0, 50.0)];
        let trades = vec![make_defense_trade("seller_0", 60.0, 40.0)];
        let companies = vec![make_test_company("seller_0", 1_000.0)];

        refund_unfilled_defense_bids_per_country(&bids, &trades, &companies, &mut country);

        // Phase 94: No refund — cash is no longer encumbered at bid submission.
        assert_eq!(country.budget.liquid_reserves, initial_reserves);
    }

    #[test]
    fn test_refund_unfilled_defense_bids_fully_filled() {
        let mut country = make_test_country();
        let initial_reserves = country.budget.liquid_reserves;

        let bids = vec![make_defense_bid(100.0, 50.0)];
        let trades = vec![make_defense_trade("seller_0", 100.0, 50.0)];
        let companies = vec![make_test_company("seller_0", 1_000.0)];

        refund_unfilled_defense_bids_per_country(&bids, &trades, &companies, &mut country);

        // Fully filled at limit price — no refund
        assert_eq!(country.budget.liquid_reserves, initial_reserves);
    }

    #[test]
    fn test_refund_unfilled_defense_bids_no_bids() {
        let mut country = make_test_country();
        let initial_reserves = country.budget.liquid_reserves;

        let bids: Vec<Bid> = vec![];
        let trades = vec![make_defense_trade("seller_0", 100.0, 50.0)];
        let companies = vec![make_test_company("seller_0", 1_000.0)];

        refund_unfilled_defense_bids_per_country(&bids, &trades, &companies, &mut country);

        // No bids — no refund
        assert_eq!(country.budget.liquid_reserves, initial_reserves);
    }

    #[test]
    fn test_refund_unfilled_defense_bids_seller_not_in_country() {
        let mut country = make_test_country();
        let initial_reserves = country.budget.liquid_reserves;

        let bids = vec![make_defense_bid(100.0, 50.0)];
        let trades = vec![make_defense_trade("foreign_seller", 100.0, 50.0)];
        let companies: Vec<Company> = vec![]; // seller not in this country

        refund_unfilled_defense_bids_per_country(&bids, &trades, &companies, &mut country);

        // Phase 94: No refund — cash is no longer encumbered at bid submission.
        assert_eq!(country.budget.liquid_reserves, initial_reserves);
    }

    // --- settle_trades defense skip test ---

    #[test]
    fn test_settle_trades_skips_defense_cash_credit() {
        let mut companies = vec![
            make_test_company("seller_0", 1_000.0),
            make_test_bank("bank_0", 500_000.0, 400_000.0),
        ];
        companies[0].primary_bank_id = Some("bank_0".to_string());

        let initial_cash = companies[0].brokerage_account.as_ref().unwrap().cash;
        let initial_available = companies[0].available_cash;

        let trades = vec![make_defense_trade("seller_0", 100.0, 50.0)];
        let mut buildings: Vec<Building> = vec![];
        let mut test_country = crate::state::Country::default();

        settle_trades(
            &trades,
            &mut companies,
            &mut buildings,
            &mut test_country,
            0,
            &GenerativeGoodsConfig::default(),
            &std::collections::HashMap::new(),
        );

        // Seller cash NOT credited by settle_trades for defense trades
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_cash
        );
        assert_eq!(companies[0].available_cash, initial_available);
    }

    // --- v4.18 FS-7: open_bid_credit lifecycle tests ---

    /// Unfilled credit-backed bids must discharge the provisional
    /// `procurement_credit` they booked — no goods arrived, so no payable
    /// may survive the refund. Regression: before the split ledger the
    /// credit ratcheted every unfilled turn (contractors carrying billions
    /// of never-delivered "payables", payroll capacity pinned at zero).
    #[test]
    fn test_unfilled_credit_bid_discharges_procurement_credit() {
        let mut company = make_test_company("buyer_0", 100.0);
        // Encumber beyond the pocket: 250 encumbrance = 100 cash + 150 credit.
        encumber_cash(&mut company, 250.0);
        assert_eq!(company.debit_cash, 250.0);
        assert_eq!(company.procurement_credit, 150.0);
        assert_eq!(company.open_bid_credit, 150.0);
        assert_eq!(company.operational_cash(), 0.0);

        let mut book = OrderBook::default();
        book.bids
            .entry(Commodity::Steel)
            .or_default()
            .push(Bid {
                buyer_id: "buyer_0".to_string(),
                commodity: Commodity::Steel,
                quantity: 25.0,
                limit_price: 10.0,
                blueprint_id: None,
                min_quality: None,
            });
        let mut companies = vec![company];
        let idx: rustc_hash::FxHashMap<String, usize> =
            [("buyer_0".to_string(), 0usize)].into_iter().collect();

        refund_unfilled_bids(&book, &mut companies, &idx, 0.0);

        let c = &companies[0];
        assert_eq!(c.debit_cash, 0.0);
        assert_eq!(
            c.procurement_credit, 0.0,
            "phantom payable must discharge with the unfilled bid"
        );
        assert_eq!(c.open_bid_credit, 0.0);
        assert_eq!(
            c.operational_cash(),
            100.0,
            "only the cash-backed share returns — crediting the credit share would mint fiat"
        );
    }

    /// A filled bid converts the consumed credit share into a real
    /// supplier payable: goods were delivered, so the obligation survives.
    #[test]
    fn test_filled_credit_bid_converts_to_payable() {
        let mut company = make_test_company("buyer_0", 100.0);
        encumber_cash(&mut company, 250.0); // 100 cash + 150 credit

        // Fill of 180 consumes the whole 100 cash share + 80 credit share.
        company.consume_bid_encumbrance(180.0);
        assert_eq!(company.debit_cash, 70.0);
        assert_eq!(
            company.procurement_credit, 150.0,
            "delivered-goods credit stays a payable"
        );
        assert_eq!(
            company.open_bid_credit, 70.0,
            "filled credit leaves the open-bid ledger (80 now payable)"
        );

        // The 70 residual unfills: all of it is open credit → discharges.
        let cash_back = company.release_bid_encumbrance(70.0);
        assert_eq!(cash_back, 0.0);
        assert_eq!(company.debit_cash, 0.0);
        assert_eq!(company.open_bid_credit, 0.0);
        assert_eq!(
            company.procurement_credit, 80.0,
            "only the delivered-goods payable survives"
        );
    }

    /// A filled bid releases the `(limit − exec) × qty` slack back to the
    /// buyer: the encumbrance locked `qty × limit`, the seller was paid
    /// `qty × exec`, and the residual was never owed to anyone. Regression:
    /// fully-filled bids left the slack locked in `debit_cash` forever (with
    /// its credit share ratcheting `procurement_credit`), ballooning the
    /// encumbrance pool and suppressing payroll capacity.
    #[test]
    fn test_filled_bid_releases_limit_exec_slack() {
        let mut company = make_test_company("buyer_0", 100.0);
        // Bid qty=10 limit=20 → 200 encumbrance (100 cash + 100 credit).
        encumber_cash(&mut company, 200.0);
        assert_eq!(company.debit_cash, 200.0);
        assert_eq!(company.open_bid_credit, 100.0);

        // Fill at exec=10: pay the seller 100, then release the slack.
        company.consume_bid_encumbrance(100.0);
        release_fill_slack(&mut company, 10.0, 20.0, 10.0);

        // Escrow fully reconciled: 100 paid + 100 slack released.
        assert_eq!(company.debit_cash, 0.0);
        // Slack was the remaining cash share after the consume — all of it
        // is open credit (consume takes cash first) → phantom discharge,
        // zero cash back to the pocket.
        assert_eq!(company.open_bid_credit, 0.0);
        assert_eq!(
            company.procurement_credit, 0.0,
            "slack credit discharges — the bid encumbrance funded goods that cost less than the limit"
        );
        assert_eq!(
            company.operational_cash(),
            0.0,
            "cash share already consumed by the fill — nothing returns"
        );
    }
}

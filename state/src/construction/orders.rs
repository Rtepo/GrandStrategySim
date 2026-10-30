//! Construction B2B order submission and project advancement.
//!
//! This module integrates construction projects with the B2B OrderBook:
//! * `submit_construction_b2b_orders` — submits buy bids for missing materials.
//! * `advance_construction_projects` — consumes delivered materials and checks completion.
//! * `release_construction_tranches` — Phase 22A: pays contractor tranches on milestones.

use crate::construction::ConstructionProject;
use crate::economy::b2b_config::B2bOrderConfig;
use crate::economy::market_history::{get_reference_price, MarketHistory};
use crate::economy::order_book::{Bid, OrderBook};
use crate::economy::transfer_settler::{
    release_escrow_company_to_contractor, release_escrow_treasury_to_contractor,
    settle_company_to_company,
};
use crate::entities::{Building, Company};
use crate::registries::enums::Commodity;
use crate::state::Country;
use std::collections::BTreeMap;

/// Submit B2B buy bids for construction materials needed by active projects.
///
/// # Arguments
/// * `companies` - Mutable slice of companies (cash is encumbered).
/// * `buildings` - Slice of buildings (read-only; checked for `active_project`).
/// * `order_book` - Mutable order book where bids are inserted.
/// * `market_history` - Historical price data for reference pricing.
/// * `config` - B2B order configuration parameters.
///
/// # Returns
/// A vector of diagnostic messages.
///
/// # Rules
/// * For each building with `active_project`, compute remaining materials.
/// * Phase 22A: If `main_contractor_id` is set, the contractor submits bids.
///   Otherwise, the building owner submits bids (legacy self-build behavior).
/// * Cash is encumbered: `available_cash -= encumbrance`, `debit_cash += encumbrance`.
/// * Bids are clamped to affordable quantities.
/// * Unfilled bids are automatically refunded by the existing `refund_unfilled_bids`.
pub fn submit_construction_b2b_orders(
    companies: &mut [Company],
    buildings: &[Building],
    order_book: &mut OrderBook,
    market_history: &MarketHistory,
    config: &B2bOrderConfig,
) -> Vec<String> {
    let mut messages = Vec::new();

    // Build a map: buyer_company_id → Vec<(building_id, commodity, remaining_needed)>
    // Phase 22A: contractor submits bids if main_contractor_id is set.
    let mut buyer_requests: BTreeMap<String, Vec<(String, Commodity, f64)>> = BTreeMap::new();
    // Phase 36: Track pending tranche value per contractor for State-backed projects.
    // Contractors can use 50% of pending tranche value as additional bidding capacity.
    let mut pending_tranche_value: BTreeMap<String, f64> = BTreeMap::new();

    for building in buildings.iter() {
        let project = match &building.active_project {
            Some(p) => p,
            None => continue,
        };

        // v4.18: stop-work on unpaid certified work. If a tranche's trigger
        // progress has been reached but the release pass could not pay it
        // (escrow leg exhausted + investor cash leg deferred), the contract
        // is past due — a real contractor stops mobilizing materials until
        // the invoice clears. With no new deliveries the project stalls on
        // the existing material-shortage hold path and eventually cancels,
        // freeing the escrow — rather than the contractor fronting
        // unbillable work (the stranded-COGS bleed behind the multi-billion
        // windowed losses). Tranche-less internal projects (self-funded)
        // have no due tranches and pass through untouched.
        let invoice_past_due = project
            .tranches
            .iter()
            .any(|t| !t.released && project.progress >= t.trigger_progress);
        if invoice_past_due {
            #[cfg(feature = "diagnostic")]
            eprintln!(
                "CONST_DBG[STOPWORK]: project={} site={} contractor={} investor={} progress={:.2} — tranche due but unpaid; deployment halted",
                project.id, building.id, project.main_contractor_id, project.investor_id, project.progress,
            );
            continue;
        }

        // Phase 22A: determine who submits bids
        let buyer_id = if !project.main_contractor_id.is_empty() {
            project.main_contractor_id.clone()
        } else {
            building.owner_id.clone()
        };

        // Phase 36: Accumulate pending tranche value for escrow-backed
        // projects so the contractor can bid against guaranteed escrow.
        // Extended beyond `STATE:` investors — corporate expansion projects
        // escrow the investor's cash identically (`investor_cash_debited`),
        // so their pending tranches are just as reliable a capacity source.
        // Unescrowed projects (unfunded tenders) contribute nothing.
        // v4.17 FS-7: near-term pipeline only. Ordering materials against
        // ALL unreleased tranches let a contractor buy the whole BOM on
        // credit in one turn; supplier-credit retirement then garnished
        // every inbound tranche before wages — the payroll starvation
        // behind the hire→arrears→furlough churn. A contractor orders for
        // the work the NEXT milestone can fund, like payroll pacing does.
        if project.investor_cash_debited > 0.0 {
            if let Some(next) = project.tranches.iter().find(|t| !t.released) {
                *pending_tranche_value.entry(buyer_id.clone()).or_insert(0.0) +=
                    next.amount;
            }
        }

        // Phase v4.17: subtract material already STAGED at the site —
        // goods sitting in the site inventory that the project has not yet
        // installed count against the requirement. Without this the same
        // tonnes are re-bid every turn and the buyer burns cash on
        // duplicate deliveries. The site's own production-method reserve
        // (4 turns of input, same convention as
        // `consume_delivered_materials`) stays out of the staged figure.
        let site_scale = building.worker_capacity as f64
            * building.scale_factor.max(1) as f64
            / 1000.0;
        let staged_at_site = |commodity: &Commodity| -> f64 {
            let reserve = building
                .active_method
                .inputs
                .get(commodity)
                .copied()
                .unwrap_or(0.0)
                * site_scale
                * 4.0;
            (building
                .inventory
                .get(commodity)
                .copied()
                .unwrap_or(0.0)
                - reserve)
                .max(0.0)
        };

        // Phase 6 fix (M2): Also collect bid requests for subcontractors.
        // Each subcontractor submits bids for their assigned task_materials.
        for sub in &project.subcontractors {
            if sub.completed {
                continue; // Task done, no more materials needed
            }
            for (&commodity, &required) in &sub.task_materials {
                if required <= 0.0 {
                    continue;
                }
                // Check remaining based on project-wide delivery (materials
                // are delivered to the building inventory, not per-subcontractor)
                let already_delivered = project
                    .delivered_materials
                    .get(&commodity)
                    .copied()
                    .unwrap_or(0.0);
                let remaining_needed =
                    (required - already_delivered - staged_at_site(&commodity)).max(0.0);
                if remaining_needed <= 0.0 {
                    continue;
                }
                buyer_requests
                    .entry(sub.subcontractor_id.clone())
                    .or_default()
                    .push((building.id.clone(), commodity, remaining_needed));
            }
        }

        for (&commodity, &required) in &project.required_materials {
            if required <= 0.0 {
                continue;
            }
            let already_delivered = project
                .delivered_materials
                .get(&commodity)
                .copied()
                .unwrap_or(0.0);
            let remaining_needed =
                (required - already_delivered - staged_at_site(&commodity)).max(0.0);
            if remaining_needed <= 0.0 {
                continue;
            }
            buyer_requests.entry(buyer_id.clone()).or_default().push((
                building.id.clone(),
                commodity,
                remaining_needed,
            ));
        }
    }

    // Canonicalize per-buyer request order — the inner Vecs were pushed in
    // `task_materials`/`required_materials` HashMap iteration order, which is
    // per-process; the tail/bulk partition and cumulative-encumbrance drain
    // below are order-sensitive.
    for requests in buyer_requests.values_mut() {
        requests.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    }

    for company in companies.iter_mut() {
        // `computed_liquid_capital` returns only brokerage cash — 0.0 for
        // unbanked contractors, so they could never bid for materials even on
        // fully escrowed projects. `operational_cash()` is the actual cash
        // source (brokerage + available_cash), the same basis payroll and
        // distress checks use.
        // Phase 94: Do NOT overwrite available_cash — that would duplicate
        // brokerage_account.cash into available_cash, creating M0 for unbanked
        // companies (both fields are in the M0 walk). The liquid figure below
        // only sizes bid capacity; the encumbrance itself debits whichever
        // pocket actually holds the money.
        let liquid = company.operational_cash();
        let pending_raw = pending_tranche_value
            .get(&company.id)
            .copied()
            .unwrap_or(0.0);
        // v4.17.1 FS-7: the tranche pipeline only converts to cash at the
        // crew's install pace — tranche releases are progress-gated. A
        // contractor cannot honestly claim next-milestone funding beyond
        // the work its demonstrated crew (fielded + recallable bench) can
        // deliver within the milestone window; the unbounded sum let a
        // 3,000-project contractor encumber $10B of materials on $2k of
        // cash, booking supplier debt that became a deadweight liability
        // on every stall-cancel and drove negative-equity extinction.
        // Bonded mobilization cash — the actual funding for the next
        // milestone — stays fully spendable through `liquid`.
        let crew_fte = company.fulfilled_fte as f64
            + company.furloughed_workers_count.max(0.0);
        let install_per_cycle = crew_fte
            * 10.0
            * company.offered_wage_per_fte.max(1.0)
            * crate::construction::tender_market::MILESTONE_SPACING_TURNS;
        let pending = pending_raw.min(install_per_cycle);
        // v4.17 FS-7: payroll reserve. Wages are senior to materials — a
        // contractor that bids its whole cash pile into the supply market
        // cannot meet payroll at wage clearing, so the crew it hired goes
        // into arrears and furloughs (the hire→furlough→stall churn).
        // Material capacity is sized on cash NET of the wage bill through
        // the next milestone — tranches land every ~4 turns, so a 1-turn
        // reserve still starves payroll mid-cycle.
        let payroll_reserve = company.fulfilled_fte as f64
            * company.offered_wage_per_fte.max(1.0)
            * crate::construction::tender_market::MILESTONE_SPACING_TURNS;
        let effective_liquid =
            (liquid - payroll_reserve).max(0.0) + pending * 0.5;
        let max_encumber = effective_liquid * config.max_cash_encumbrance_ratio;
        let mut total_encumbered = 0.0;

        let requests = match buyer_requests.get(&company.id) {
            Some(r) => r,
            None => continue,
        };

        #[cfg(feature = "diagnostic")]
        {
            let req_qty: f64 = requests.iter().map(|(_, _, q)| *q).sum();
            eprintln!(
                "CONST_DBG[BIDBATCH]: company={} requests={} qty={:.0} liquid={:.0} pending_tranche={:.0} max_encumber={:.0}",
                company.id,
                requests.len(),
                req_qty,
                liquid,
                pending,
                max_encumber
            );
        }

        // v4.17 FS-7: two-pass proportional allocation. A single shared
        // encumbrance budget consumed in request order deterministically
        // starved the tail — companies with 87-175 request lines funded the
        // first ~20% and posted ZERO bids for the rest, so those sites
        // never saw one delivery and died at progress 0.00 (the 1,456
        // stalled-cancellation mass). Scale every request by the same
        // factor when wants exceed the budget so all active sites advance
        // each turn; a partially-supplied site still resets its hold clock
        // on any consumption.
        let mut priced: Vec<(Commodity, f64, f64)> = Vec::with_capacity(requests.len());
        let mut total_want = 0.0_f64;
        for (_building_id, commodity, remaining_needed) in requests {
            let ref_price = match get_reference_price(commodity, market_history) {
                Some(p) => p,
                None => {
                    messages.push(format!(
                        "Construction: No reference price for {:?} (company {}), skipping bid",
                        commodity, company.id
                    ));
                    continue;
                }
            };
            // Same unfilled-bid ratchet the generic B2B path uses: a bid
            // capped at ref×(1+premium) forever loses to sellers floored at
            // unit_cost above that cap (observed: Cement asks never crossed,
            // every site starved at progress 0.00 → mass cancellations).
            // Escalate the limit 20% per unfilled turn so converging
            // spreads actually cross — settlement still executes at the
            // ask price, so the premium is a ceiling, not the price paid.
            // v4.18: the escalation is bounded by the seller ask ceiling
            // (1+max_markup_ratio)×reference. Unbounded, a never-filled BOM
            // input (extinct ConstructionServices/Machinery) compounded its
            // limit to ~10^8×ref, exploded `total_want`, and collapsed the
            // proportional fill_scale to ~0 — starving every OTHER line's
            // bid volume too. The cap keeps the limit inside what the book
            // can legitimately charge.
            let last_unfilled = company
                .unfilled_bid_prices
                .get(commodity)
                .copied()
                .unwrap_or(0.0);
            let limit_cap = ref_price * (1.0 + config.max_markup_ratio);
            let limit_price = if last_unfilled > 0.0 {
                (last_unfilled * 1.20)
                    .min(limit_cap)
                    .max(ref_price * (1.0 + config.buy_premium_ratio))
            } else {
                ref_price * (1.0 + config.buy_premium_ratio)
            };
            if limit_price <= 0.0 || *remaining_needed <= 0.0 {
                continue;
            }
            priced.push((*commodity, *remaining_needed, limit_price));
            total_want += *remaining_needed * limit_price;
        }
        // v4.18 WS2: last-mile exception. The proportional scale-down
        // protects the shared encumbrance budget for bulk wants, but it
        // must not shave the final units of a nearly-complete line —
        // bidding `remaining × s` every turn converges on `required`
        // asymptotically and never lets a site finish (observed: every BOM
        // line parked ~2% short for 50+ turns, zero completions).
        //
        // Lines whose FULL residual encumbrance is a small share of the
        // budget are bid in full and funded FIRST (a site near completion
        // is closest to its next tranche release — the contractor's own
        // cash-flow incentive); bulk wants then split the remaining budget
        // proportionally, preserving the anti-tail-starvation intent.
        let last_mile_cost = (max_encumber * 0.05).max(0.0);
        let (tail, bulk): (Vec<(Commodity, f64, f64)>, Vec<(Commodity, f64, f64)>) =
            priced
                .into_iter()
                .partition(|&(_, qty, price)| qty * price <= last_mile_cost);
        let tail_cost: f64 = tail.iter().map(|&(_, q, p)| q * p).sum();
        let bulk_budget = (max_encumber - tail_cost.min(max_encumber)).max(0.0);
        let bulk_want: f64 = bulk.iter().map(|&(_, q, p)| q * p).sum();
        let fill_scale = if bulk_want > bulk_budget && bulk_want > 0.0 {
            bulk_budget / bulk_want
        } else {
            1.0
        };

        // Tail residuals first (bounded by the cumulative-encumbrance
        // guard below), then scaled bulk wants.
        let ordered = tail
            .into_iter()
            .map(|(c, q, p)| (c, q, p, false))
            .chain(bulk.into_iter().map(|(c, q, p)| (c, q, p, true)));
        for (commodity, remaining_needed, limit_price, scaled) in ordered {
            let bid_qty = if scaled {
                remaining_needed * fill_scale
            } else {
                remaining_needed
            };
            if bid_qty <= 0.0 {
                continue;
            }

            let encumbrance = bid_qty * limit_price;
            if total_encumbered + encumbrance > max_encumber {
                continue;
            }

            // Encumber cash (double-entry): debit the pocket that actually
            // holds the company's money (brokerage for banked firms, not
            // `available_cash` — debiting it unconditionally drove banked
            // buyers to −5B), floor it at 0, and book the uncovered share
            // as explicit procurement_credit.
            let pocket = if let Some(ba) = &mut company.brokerage_account {
                &mut ba.cash
            } else {
                &mut company.available_cash
            };
            let paid = encumbrance.min((*pocket).max(0.0));
            *pocket -= paid;
            let credit = encumbrance - paid;
            if credit > 0.0 {
                company.procurement_credit += credit;
                company.open_bid_credit += credit;
            }
            company.debit_cash += encumbrance;
            total_encumbered += encumbrance;

            // Submit bid with company.id as buyer_id
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

    messages
}

/// v4.17 mechanics fix — deploy contractor-purchased materials to project
/// sites.
///
/// `settle_trades` delivers purchased commodities to buildings OWNED BY THE
/// BID BUYER. When the project buyer is `main_contractor_id` the goods
/// therefore pool in the contractor's own plants while the investor-owned
/// site starves. This pass moves each site's outstanding BOM needs out of
/// contractor-owned stock into the site inventory.
///
/// # Rules
/// * Only projects with a `main_contractor_id` deploy (legacy self-builds
///   already deliver to the site owner).
/// * A donor building never ships below its own restart reserve — the
///   active method's seeded input buffer (4 turns, same convention as
///   `consume_delivered_materials`) PLUS its own project's outstanding
///   BOM need for that commodity.
/// * Pure physical move: total quantity per commodity is conserved.
fn deploy_project_materials(buildings: &mut [Building]) {
    // owner → building indices (contractor stock lookup)
    let mut owner_to_idx: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, b) in buildings.iter().enumerate() {
        if !b.owner_id.is_empty() {
            owner_to_idx.entry(b.owner_id.clone()).or_default().push(i);
        }
    }

    // (site_idx, commodity, pull_total, [(donor_idx, give_qty)])
    let mut moves: Vec<(usize, Commodity, f64, Vec<(usize, f64)>)> = Vec::new();

    for site_idx in 0..buildings.len() {
        let site = &buildings[site_idx];
        let project = match &site.active_project {
            Some(p) if !p.main_contractor_id.is_empty() => p,
            _ => continue,
        };
        let donor_idxs = match owner_to_idx.get(&project.main_contractor_id) {
            Some(v) => v.clone(),
            None => continue,
        };
        let site_scale =
            site.worker_capacity as f64 * site.scale_factor.max(1) as f64 / 1000.0;

        for (&commodity, &required) in &project.required_materials {
            if required <= 0.0 {
                continue;
            }
            let delivered = project
                .delivered_materials
                .get(&commodity)
                .copied()
                .unwrap_or(0.0);
            let site_reserve = site
                .active_method
                .inputs
                .get(&commodity)
                .copied()
                .unwrap_or(0.0)
                * site_scale
                * 4.0;
            let staged = (site
                .inventory
                .get(&commodity)
                .copied()
                .unwrap_or(0.0)
                - site_reserve)
                .max(0.0);
            let mut want = (required - delivered - staged).max(0.0);
            if want <= 0.0 {
                continue;
            }

            let mut gives: Vec<(usize, f64)> = Vec::new();
            let mut pulled = 0.0;
            for &di in &donor_idxs {
                if di == site_idx || want <= 0.0 {
                    continue;
                }
                let donor = &buildings[di];
                let dscale = donor.worker_capacity as f64
                    * donor.scale_factor.max(1) as f64
                    / 1000.0;
                // The restart reserve only protects a production run the
                // yard can actually perform. When any other input is
                // exhausted the method cannot fire this turn regardless —
                // stock held "in reserve" is stranded inventory the site
                // needs while the yard starves (observed: Steel pulled=0
                // from donors whose own Cement was 0). A starved yard's
                // stock flows to the site; the reserve re-binds the turn
                // the full input set is back on hand.
                let method_can_run = donor.active_method.inputs.iter().all(|(c, q)| {
                    donor
                        .inventory
                        .get(c)
                        .copied()
                        .unwrap_or(0.0)
                        >= *q * dscale
                });
                let method_reserve = if method_can_run {
                    donor
                        .active_method
                        .inputs
                        .get(&commodity)
                        .copied()
                        .unwrap_or(0.0)
                        * dscale
                        * 4.0
                } else {
                    0.0
                };
                let project_reserve = donor
                    .active_project
                    .as_ref()
                    .map(|p| {
                        let req =
                            p.required_materials.get(&commodity).copied().unwrap_or(0.0);
                        let del =
                            p.delivered_materials.get(&commodity).copied().unwrap_or(0.0);
                        (req - del).max(0.0)
                    })
                    .unwrap_or(0.0);
                let held = donor.inventory.get(&commodity).copied().unwrap_or(0.0);
                let spare = (held - method_reserve - project_reserve).max(0.0);
                let give = spare.min(want);
                if give > 0.0 {
                    gives.push((di, give));
                    pulled += give;
                    want -= give;
                }
            }
            if pulled > 0.0 {
                moves.push((site_idx, commodity, pulled, gives));
            }
        }
    }

    // Apply the moves: credit the site, debit each donor.
    for (site_idx, commodity, pulled, gives) in moves {
        *buildings[site_idx]
            .inventory
            .entry(commodity)
            .or_insert(0.0) += pulled;
        // v4.18 WS2: deployed units carry a cost basis into the project's
        // staged pool — valued at the cost-plus anchor (registry-derived
        // acquisition proxy; contractor stock was bought in earlier turns,
        // so the spot reference chain would overstate at ratcheted prices).
        if let Some(p) = buildings[site_idx].active_project.as_mut() {
            let basis_price =
                crate::engine::generator::corporate::anchored_base_price(commodity);
            *p.staging_cost_basis.entry(commodity).or_insert(0.0) += pulled * basis_price;
            *p.staging_qty.entry(commodity).or_insert(0.0) += pulled;
        }
        for (di, give) in &gives {
            let donor = &mut buildings[*di];
            let held = donor.inventory.get(&commodity).copied().unwrap_or(0.0);
            let new_qty = (held - give).max(0.0);
            if new_qty > 0.0 {
                donor.inventory.insert(commodity, new_qty);
            } else {
                donor.inventory.remove(&commodity);
            }
        }
        #[cfg(feature = "diagnostic")]
        eprintln!(
            "CONST_DBG[DEPLOY]: site={} commodity={:?} pulled={:.0} donors={}",
            buildings[site_idx].id,
            commodity,
            pulled,
            gives.len()
        );
    }
}

/// Advance all construction projects: consume delivered materials from
/// building inventories, update progress, and complete finished projects.
///
/// # Arguments
/// * `buildings` - Mutable slice of buildings (inventories are consumed).
/// * `companies` - Mutable slice of companies (fixed_capital updated on completion).
/// * `unit_costs` - Map of commodity to unit cost for tracking `cost_spent`.
///
/// # Returns
/// A vector of diagnostic messages.
///
/// # Rules
/// * Must run BEFORE `execute_production_cycle` so construction materials
///   are consumed from inventory before production logic sees them.
/// * For each building with `active_project`:
///   1. Consume available materials from `building.inventory` into the project.
///   2. Update progress = min(delivered/required) across all materials.
///   3. If complete: add capacity/capital, clear `active_project`.
///   4. If no materials consumed: set `on_hold`.
/// * Only buildings where `active_project.is_some() && worker_capacity == 0`
///   are brand-new sites; expanding buildings continue production normally.
pub fn advance_construction_projects(
    buildings: &mut [Building],
    companies: &mut [Company],
    unit_costs: &BTreeMap<Commodity, f64>,
    country: &mut Country,
) -> (Vec<String>, f64, BTreeMap<String, f64>) {
    let mut messages = Vec::new();
    // Phase 34: Track total materials consumed (cost_spent delta) as investment.
    // I is recorded exclusively from materials consumed, NOT tranche payments
    // (which are merely cash transfers from Investor to Contractor).
    let mut total_investment = 0.0;
    // v4.17: same material-consumption delta tagged by site region so the
    // regional GDP reconciler can place construction capital formation
    // where the building actually stands.
    let mut investment_by_region: BTreeMap<String, f64> = BTreeMap::new();

    // v4.17 mechanics fix — SITE MATERIAL DEPLOYMENT.
    // `settle_trades` routes physical deliveries to buildings OWNED BY THE
    // BID BUYER (economy/trade/b2b_orders.rs: `owner_to_buildings[buyer]`).
    // For contractor-driven projects the buyer is `main_contractor_id`, but
    // the project site is owned by the INVESTOR — purchased Steel/Cement/
    // Planks pooled in the contractor's own plants while the site starved
    // at progress 0.0 until the stall-cancel fired. That is why contractors
    // never consumed materials, never progressed, and never got paid.
    // Before advancing, move each site's outstanding BOM needs out of
    // contractor-owned stock (above each donor's own production + project
    // reserves) into the site inventory. Physical move only — quantity is
    // conserved, no money or goods are created.
    deploy_project_materials(buildings);

    // v4.17: Material cost of work performed, booked per contractor so the
    // P&L expenses project inputs (matched against tranche revenue).
    let mut contractor_cogs: BTreeMap<String, f64> = BTreeMap::new();
    let mut sites_consumed = 0u32;
    let mut sites_completed = 0u32;
    let mut sites_cancelled = 0u32;

    for building in buildings.iter_mut() {
        let project = match building.active_project.as_mut() {
            Some(p) => p,
            None => continue,
        };

        // v4.18 WS2: a physically-complete project lingers only as the
        // contractor's receivable claim — the completion tranche may have
        // deferred on investor cash. Clear it once every tranche has
        // settled or the contractor no longer exists to collect. The
        // escrow/commitment remainder goes back to the investor at close.
        if project.work_complete {
            let contractor_gone = !project.main_contractor_id.is_empty()
                && !companies.iter().any(|c| c.id == project.main_contractor_id);
            // A liquidated corporate investor can never fund the cash leg —
            // the claim is unpayable bad debt; close the record rather than
            // let the site linger as a permanent zombie. STATE investors are
            // the Treasury itself and cannot disappear.
            let is_state = project.investor_id.starts_with("STATE:");
            let investor_gone = !is_state
                && !project.investor_id.is_empty()
                && !companies.iter().any(|c| c.id == project.investor_id);
            if project.tranches.is_empty()
                || project.tranches.iter().all(|t| t.released)
                || contractor_gone
                || investor_gone
            {
                let escrow_left =
                    (project.investor_cash_debited - project.tranches_paid).max(0.0);
                let commitment_release =
                    (project.investor_capital_committed - project.tranches_paid).max(0.0);
                let investor_id = project.investor_id.clone();
                building.active_project = None;
                refund_unspent_escrow(
                    &investor_id,
                    escrow_left,
                    commitment_release,
                    companies,
                    country,
                );
            }
            continue;
        }

        // Phase 3 fix (C5): Apply weather productivity multiplier.
        // Severe weather (storms, floods, frost) reduces construction
        // productivity, slowing material consumption.
        let region_id = building.region_id.clone();
        let weather_mod = crate::economy::production::weather::get_region_weather_modifier(
            &country.weather_state,
            &region_id,
        );
        let productivity = weather_mod.construction_multiplier.max(0.0).min(1.0);
        project.weather_productivity = productivity;

        if productivity < 0.5 && productivity > 0.0 {
            messages.push(format!(
                "Construction slowed by weather: building {} productivity {:.0}%",
                building.id,
                productivity * 100.0
            ));
        } else if productivity == 0.0 {
            messages.push(format!(
                "Construction halted by severe weather: building {}",
                building.id
            ));
        }

        // Phase 34: Capture cost_spent before consumption to compute the delta.
        let cost_before = project.cost_spent;

        // Consume delivered materials from building inventory. Reserve the
        // active method's seeded input buffer first — an expansion tender
        // must not cannibalize the stock production needs over the genesis
        // horizon (4 turns of input, matching SEED_INVENTORY_TURNS). Stock
        // above that is surplus the project may legitimately install.
        // Reserve scales with worker_capacity (the genesis seed basis), not
        // current_employment — a furloughed building must keep its restart
        // buffer out of reach of its own construction project.
        // worker_capacity is per-virtual-plant — the entity's restart buffer
        // covers all scale_factor plants.
        let capacity_scale = building.worker_capacity as f64
            * building.scale_factor.max(1) as f64
            / 1000.0;
        let input_reserve: BTreeMap<Commodity, f64> = building
            .active_method
            .inputs
            .iter()
            .map(|(&c, &q)| (c, q * capacity_scale * 4.0))
            .collect();
        // Effective unit costs: prefer the caller-supplied map — the turn
        // loop now fills it with `get_reference_price` (VWAP → last trade →
        // global base), i.e. the actual clearing prices contractors pay.
        // Fallback is the flat cost-plus numéraire price (the same table
        // `global_base_prices` seeds from). It is deliberately NOT scaled by
        // `ppi_index`: that "index" is a weighted-average absolute price
        // level (~currency units), not normalized to 100 — multiplying a
        // price by price/100 produced unit costs ~30-100× too high, the
        // source of multi-hundred-billion phantom investment and phantom
        // contractor COGS (MACRO-3).
        // Fixed-asset materials are deliberately left unvalued: their purchase
        // already counted as investment in the fixed-asset trade leg —
        // valuing them again here would double-count.
        let mut effective_unit_costs: BTreeMap<Commodity, f64> = BTreeMap::new();
        for &commodity in project.required_materials.keys() {
            if let Some(&price) = unit_costs.get(&commodity) {
                effective_unit_costs.insert(commodity, price);
                continue;
            }
            if commodity.is_fixed_asset() {
                continue;
            }
            let price = crate::engine::generator::corporate::anchored_base_price(commodity);
            if price > 0.0 {
                effective_unit_costs.insert(commodity, price);
            }
        }
        // v4.17 FS-7: construction is labor-gated — materials only install
        // as fast as the crew can place them. Productivity scales by the
        // builder's fill ratio (fulfilled ÷ demanded FTE); a contractor
        // with no crew installs nothing, closing the loop between the
        // labor market and physical progress instead of leaving staffing
        // decorative.
        let builder_id = if !project.main_contractor_id.is_empty() {
            project.main_contractor_id.as_str()
        } else {
            building.owner_id.as_str()
        };
        let labor_frac = companies
            .iter()
            .find(|c| c.id == builder_id)
            .map(|c| {
                if c.target_fte_demand > 0 {
                    (c.fulfilled_fte as f64 / c.target_fte_demand as f64)
                        .clamp(0.0, 1.0)
                } else {
                    1.0
                }
            })
            .unwrap_or(1.0);
        // v4.18: certified-work stop — when a tranche's trigger progress
        // has been met but `release_due_tranches` could not pay it (escrow
        // leg exhausted, investor cash leg deferred), the contract is past
        // due. A real crew stops placing material until the invoice clears;
        // the hold/cancel path below winds the dead project down the same
        // way a material shortage does. Delivered-but-uninstalled staging
        // stays physical on site — no phantom refund of consumed goods.
        let invoice_past_due = project
            .tranches
            .iter()
            .any(|t| !t.released && project.progress >= t.trigger_progress);
        let labor_productivity = productivity * labor_frac;
        let consumed = if invoice_past_due {
            false
        } else {
            project.consume_delivered_materials(
                &mut building.inventory,
                &effective_unit_costs,
                labor_productivity,
                &input_reserve,
            )
        };
        project.turns_elapsed += 1;

        // Phase 34: Accumulate the cost_spent delta (materials consumed value).
        let turn_investment = project.cost_spent - cost_before;
        total_investment += turn_investment;
        if turn_investment > 0.0 {
            *investment_by_region.entry(region_id.clone()).or_insert(0.0) +=
                turn_investment;
        }
        if consumed {
            sites_consumed += 1;
        }
        // v4.17: Book the material cost of work performed on the MAIN
        // CONTRACTOR — the firm that purchased the inputs. Recognized as
        // COGS in `process_companies` (matched against tranche revenue).
        // Reporting-only; the cash already moved at B2B settlement.
        if turn_investment > 0.0 && !project.main_contractor_id.is_empty() {
            *contractor_cogs
                .entry(project.main_contractor_id.clone())
                .or_insert(0.0) += turn_investment;
        }

        if consumed {
            if project.on_hold {
                project.resume();
            }
        } else if !project.is_complete() {
            project.consecutive_hold_turns += 1;
            project.put_on_hold(if invoice_past_due {
                "Investor funding unavailable — tranche past due".to_string()
            } else {
                "Material shortage on B2B market".to_string()
            });
        }

        // Phase 29: Cancel permanently stalled projects (on hold for > 5 turns
        // with no material consumption). Release remaining escrow to investor.
        // Phase 1 fix (C1): The refund is now `investor_cash_debited -
        // tranches_paid` — the actual unspent escrow, not
        // `total_cost - cost_spent` which included the contractor's margin
        // (money never paid by the investor).
        if project.consecutive_hold_turns > 5 && !project.is_complete() {
            let refund = (project.investor_cash_debited - project.tranches_paid).max(0.0);
            let commitment_release =
                (project.investor_capital_committed - project.tranches_paid).max(0.0);
            let investor_id = project.investor_id.clone();
            let building_id = building.id.clone();
            let is_state = investor_id.starts_with("STATE:");
            #[cfg(feature = "diagnostic")]
            {
                let project_id = project.id.clone();
                let contractor_id = project.main_contractor_id.clone();
                let progress = project.progress;
                let missing: Vec<String> = project
                    .required_materials
                    .iter()
                    .filter(|(c, req)| {
                        **req
                            - project
                                .delivered_materials
                                .get(*c)
                                .copied()
                                .unwrap_or(0.0)
                            > 0.0
                    })
                    .map(|(c, req)| {
                        format!(
                            "{:?}:{:.0}/{:.0} stg={:.0}",
                            c,
                            project
                                .delivered_materials
                                .get(c)
                                .copied()
                                .unwrap_or(0.0),
                            req,
                            building.inventory.get(c).copied().unwrap_or(0.0)
                        )
                    })
                    .collect();
                eprintln!(
                    "CONST_DBG[CANCELLED]: project={} site={} contractor={} investor={} refund={:.0} progress={:.2} missing=[{}]",
                    project_id, building_id, contractor_id, investor_id, refund, progress,
                    missing.join(",")
                );
            }
            // v4.17.1 FS-7: termination pay — quantum meruit. A stall
            // cancel transfers the contractor's installed materials and
            // labor to the investor's site while the supplier credit that
            // bought them stays on the contractor's book — the deadweight
            // loss pattern behind the sector's negative-equity extinction
            // (observed: a 3,000-project contractor holding $10B supplier
            // debt with every tranche still progress-gated). Certified
            // work performed but unpaid (`cost_spent` net of tranches)
            // settles from the remaining escrow ahead of the investor's
            // refund — the standard termination-for-convenience settlement.
            let mut contractor_paid = 0.0_f64;
            let earned_unpaid =
                (project.cost_spent - project.tranches_paid).max(0.0);
            let mut termination_due = earned_unpaid.min(refund);
            if !is_state && !investor_id.is_empty() {
                // The corporate escrow leg pays only out of the investor's
                // dedicated `construction_escrow` pocket — clamp here so
                // the booked payment equals what actually moves.
                if let Some(iidx) =
                    companies.iter().position(|c| c.id == investor_id)
                {
                    termination_due =
                        termination_due.min(companies[iidx].construction_escrow.max(0.0));
                }
            }
            if termination_due > 0.0 && !project.main_contractor_id.is_empty() {
                let contractor_id = project.main_contractor_id.clone();
                if let Some(cidx) = companies.iter().position(|c| c.id == contractor_id)
                {
                    let leg_ok = if is_state {
                        crate::economy::trade::transfer_settler::release_escrow_treasury_to_contractor(
                            companies, cidx, termination_due, country,
                        )
                        .is_ok()
                    } else if let Some(iidx) =
                        companies.iter().position(|c| c.id == investor_id)
                    {
                        crate::economy::trade::transfer_settler::release_escrow_company_to_contractor(
                            companies, iidx, cidx, termination_due, country,
                        )
                        .is_ok()
                    } else {
                        false
                    };
                    if leg_ok {
                        contractor_paid = termination_due;
                        // Book as revenue — the same `turn_settled_sales`
                        // convention tranche releases use.
                        companies[cidx].turn_settled_sales += contractor_paid;
                        project.tranches_paid += contractor_paid;
                    }
                }
            }
            // Return unspent escrow to the investor
            let investor_refund = refund - contractor_paid;
            if refund > 0.0 {
                if is_state {
                    country.budget.liquid_reserves += investor_refund;
                } else if !investor_id.is_empty() {
                    // v4.18 FS-7: the escrow stayed a deposit liability at
                    // the investor's bank (earmark convention — the deposit
                    // never left the book), so the return leg is a pure
                    // pocket credit, mirroring `refund_unfilled_bids`. A
                    // bank-synced credit here would mint reserves with no
                    // counterparty debit. The release clamps to the pocket
                    // balance so the credit never exceeds what is
                    // actually encumbered.
                    if let Some(company) = companies.iter_mut().find(|c| c.id == investor_id) {
                        let released =
                            investor_refund.min(company.construction_escrow.max(0.0));
                        if let Some(ref mut ba) = company.brokerage_account {
                            ba.cash += released;
                        } else {
                            company.available_cash += released;
                        }
                        company.construction_escrow -= released;
                        // v4.17: release the unspent share of the committed
                        // investment back to the capital book — the full
                        // commitment minus what tranches actually drew.
                        // Under progress financing `refund` is only the
                        // undrawn escrow (~30%), so restoring `refund` here
                        // leaked the never-drawn commitment; restoring it
                        // for non-expansion investors fabricated capital
                        // they never committed.
                        company.liquid_capital +=
                            (commitment_release - contractor_paid).max(0.0);
                    }
                }
            }
            building.active_project = None;
            sites_cancelled += 1;
            messages.push(format!(
                "Construction cancelled (stalled): building {} refund {:.0}",
                building_id, investor_refund
            ));
            continue;
        }

        // Phase 6 fix (M2): Check subcontractor task completion.
        // A subcontractor's task is complete when all their assigned
        // task_materials have been delivered (project-wide delivery covers
        // their subset).
        for sub in project.subcontractors.iter_mut() {
            if sub.completed {
                continue;
            }
            let all_delivered = sub.task_materials.iter().all(|(&commodity, &required)| {
                let delivered = project
                    .delivered_materials
                    .get(&commodity)
                    .copied()
                    .unwrap_or(0.0);
                delivered >= required
            });
            if all_delivered {
                sub.completed = true;
            }
        }

        // Check completion
        if project.is_complete() {
            let project_id = project.id.clone();
            let contractor_id = project.main_contractor_id.clone();
            let project_type = project.project_type;
            let cap_increase = project.target_capacity_increase;
            let capital_increase = project.target_capital_increase;
            let building_id = building.id.clone();
            let owner_id = building.owner_id.clone();
            // v4.18 WS2: release every due tranche — including the final
            // `trigger_progress == 1.0` completion tranche — BEFORE the
            // project is cleared and before the escrow refund is computed.
            // The post-advance `release_construction_tranches` sweep can
            // never see this tranche: `active_project` is `None` by then.
            // Releasing here pays the contractor the completion milestone
            // through the real escrow+cash legs, and the refund below then
            // returns only what is genuinely unspent.
            // Recompute progress first: `is_complete()` reads it live, so an
            // edge case (e.g. an empty-BOM project) may have a stale stored
            // value below the final tranche's trigger.
            project.progress = project.compute_progress();
            release_due_tranches(project, &building_id, companies, country);
            // v4.17: unspent escrow (contract priced below the encumbered
            // estimate) must go back to the investor on completion —
            // previously it stayed debited in `debit_cash`/Treasury limbo
            // forever (M0 never restored).
            let completion_escrow_left =
                (project.investor_cash_debited - project.tranches_paid).max(0.0);
            let refund_investor = project.investor_id.clone();
            let refund_commitment =
                (project.investor_capital_committed - project.tranches_paid).max(0.0);
            // v4.18 WS2: physical work is finished — mark it once so this
            // block cannot re-fire (capacity/capital are one-shot credits).
            // If every tranche settled, close out now. If the completion
            // tranche deferred on investor cash, the record stays attached
            // to `active_project` as the contractor's open receivable: the
            // tranche sweep retries each turn and the `work_complete` branch
            // above clears it on settlement.
            project.work_complete = true;
            let all_settled = project.tranches.is_empty()
                || project.tranches.iter().all(|t| t.released);

            // Phase 23B: Transport network projects install a NetworkLink
            // instead of adding building capacity.
            if project_type
                == crate::construction::projects::ConstructionProjectType::TransportNetwork
            {
                if let (Some((region_a, region_b)), Some(level)) =
                    (&project.network_link_target, project.network_target_level)
                {
                    country.transport_networks.install_link(
                        region_a,
                        region_b,
                        level,
                        project.turns_elapsed,
                    );
                    messages.push(format!(
                        "Transport network complete: {:?} link {} ↔ {}",
                        level, region_a, region_b
                    ));
                }
                sites_completed += 1;
                #[cfg(feature = "diagnostic")]
                eprintln!(
                    "CONST_DBG[COMPLETE]: project={} site={} contractor={} type={:?} capacity=+0 escrow_refund={:.0}",
                    project_id, building_id, contractor_id, project_type, completion_escrow_left
                );
                // v4.18 WS2: close out only when every tranche settled —
                // otherwise the record stays as the contractor's claim.
                if all_settled {
                    building.active_project = None;
                    refund_unspent_escrow(
                        &refund_investor,
                        completion_escrow_left,
                        refund_commitment,
                        companies,
                        country,
                    );
                }
                continue;
            }

            building.worker_capacity += cap_increase;
            // v4.17: A spawned site building completes into its target —
            // adopt the contracted building kind/sector so production-method
            // resolution and GDP attribution see the real structure.
            if let Some(ref proj) = building.active_project {
                if proj.is_new_building && !proj.target_building_type.is_empty() {
                    building.name = proj.target_building_type.clone();
                    building.sector =
                        crate::construction::bom::sector_from_building_kind(
                            &proj.target_building_type,
                        );
                }
            }

            // Update company's fixed_capital
            if let Some(company) = companies.iter_mut().find(|c| c.id == owner_id) {
                company.fixed_capital += capital_increase;
            }

            sites_completed += 1;
            #[cfg(feature = "diagnostic")]
            eprintln!(
                "CONST_DBG[COMPLETE]: project={} site={} contractor={} type={:?} capacity=+{} escrow_refund={:.0}",
                project_id, building_id, contractor_id, project_type, cap_increase,
                completion_escrow_left
            );
            // v4.18 WS2: close out only when every tranche settled —
            // otherwise the record stays attached as the contractor's
            // open receivable until the tranche sweep pays it.
            if all_settled {
                building.active_project = None;
                refund_unspent_escrow(
                    &refund_investor,
                    completion_escrow_left,
                    refund_commitment,
                    companies,
                    country,
                );
            }
            messages.push(format!(
                "Construction complete: building {} capacity +{}",
                building_id, cap_increase
            ));
        }
    }

    // v4.17: Book project material COGS on the contractors that purchased
    // the inputs (matched against tranche revenue in `process_companies`).
    for (contractor_id, cogs) in contractor_cogs {
        if let Some(c) = companies.iter_mut().find(|c| c.id == contractor_id) {
            let prev = c
                .extra
                .get("construction_project_cogs")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            c.extra.insert(
                "construction_project_cogs".to_string(),
                serde_json::Value::from(prev + cogs),
            );
        }
    }

    // Non-diagnostic builds don't read the counters — touch them so the
    // increments above don't warn `unused_assignments`.
    let _site_counters = (sites_consumed, sites_completed, sites_cancelled);
    #[cfg(feature = "diagnostic")]
    eprintln!(
        "CONST_DBG[ADVANCE]: sites_consumed={} completed={} cancelled={} investment={:.0}",
        _site_counters.0, _site_counters.1, _site_counters.2, total_investment
    );

    (messages, total_investment, investment_by_region)
}

/// Phase 22A: Release pending tranches based on project progress.
///
/// For each building with an active project that has tranches, check if any
/// unreleased tranche's `trigger_progress` has been reached. If so, pay the
/// contractor by releasing escrowed funds.
///
/// Phase 1 fix (C2/C3): Tranche payments now use escrow release instead of
/// direct settlement. The investor's cash was already debited at tender
/// publication time (escrow). This function releases the encumbrance and
/// credits the contractor WITHOUT debiting the investor again.
/// `cost_spent` is no longer incremented by tranche payments — it only
/// tracks material costs. Tranche payments are tracked in `tranches_paid`.
///
/// # Arguments
/// * `buildings` - Mutable slice of buildings (projects are read).
/// * `companies` - Mutable slice of companies (contractor credited).
/// * `country` - Mutable country state (for bank sync).
///
/// # Returns
/// Number of tranches released this turn.
pub fn release_construction_tranches(
    buildings: &mut [Building],
    companies: &mut [Company],
    country: &mut Country,
) -> u32 {
    let mut released_count = 0u32;

    for building in buildings.iter_mut() {
        let project = match building.active_project.as_mut() {
            Some(p) => p,
            None => continue,
        };

        // Skip legacy projects without tranches
        if project.tranches.is_empty() || project.main_contractor_id.is_empty() {
            continue;
        }

        let building_id = building.id.clone();
        released_count +=
            release_due_tranches(project, &building_id, companies, country);
    }

    released_count
}

/// v4.18 WS2: Release every eligible unreleased tranche on one project.
///
/// Shared by the milestone sweep ([`release_construction_tranches`]) and by
/// the completion path in `advance_construction_projects`. The final tranche
/// (`trigger_progress == 1.0`) can only fire once the project is complete —
/// and completion clears `active_project` before the post-advance sweep sees
/// it, so without this helper the completion payment could never land.
///
/// Returns the number of tranches released this call.
fn release_due_tranches(
    project: &mut ConstructionProject,
    building_id: &str,
    companies: &mut [Company],
    country: &mut Country,
) -> u32 {
    let mut released_count = 0u32;

    {
        let contractor_id = project.main_contractor_id.clone();
        let investor_id = project.investor_id.clone();
        let progress = project.progress;

        // Determine investor type from investor_id prefix
        let is_state_investor = investor_id.starts_with("STATE:");

        // Find contractor index
        let contractor_idx = match companies.iter().position(|c| c.id == contractor_id) {
            Some(idx) => idx,
            None => return released_count,
        };

        // Release eligible tranches
        for tranche in project.tranches.iter_mut() {
            if tranche.released {
                continue;
            }
            if progress < tranche.trigger_progress {
                continue;
            }

            // v4.17 FS-7: progress financing. Tenders now escrow only the
            // mobilization share at publication (a ministry's cash pocket is
            // per-period budget that cannot escrow a multi-period capital
            // project up front — the old full-escrow model capped every
            // state contract at ~$112k, ~20× under its real BOM+labor cost,
            // which is why no contractor could ever meet payroll).
            // Each tranche therefore draws in two legs: the remaining escrow
            // first, then the investor's CURRENT cash (treasury reserves for
            // the State, company cash for corporates) — a real progress
            // payment against certified work. If the investor can't cover
            // the cash leg, the tranche defers to next turn instead of
            // paying partway; the contract stalls on appropriation the way
            // real public works do.
            let escrow_remaining =
                (project.investor_cash_debited - project.tranches_paid).max(0.0);
            let from_escrow = tranche.amount.min(escrow_remaining);
            let from_cash = tranche.amount - from_escrow;

            let investor_idx = if is_state_investor {
                if country.budget.liquid_reserves < from_cash {
                    continue; // appropriation not yet available
                }
                usize::MAX
            } else {
                match companies.iter().position(|c| c.id == investor_id) {
                    Some(idx) => {
                        let inv_cash = companies[idx]
                            .brokerage_account
                            .as_ref()
                            .map(|b| b.cash)
                            .unwrap_or(0.0)
                            + companies[idx].available_cash;
                        if inv_cash < from_cash {
                            continue;
                        }
                        idx
                    }
                    None => continue,
                }
            };

            let mut paid = 0.0;
            let mut cash_drawn = 0.0;
            if from_escrow > 0.0 {
                // v4.18 FS-7: the corporate escrow helper draws from the
                // dedicated `construction_escrow` pocket (B2B bid
                // encumbrances and the wage trust live in `debit_cash` and
                // can no longer consume funded tranches). Mirror the clamp
                // here so `paid`/`tranches_paid` record what actually
                // moved — booking the requested leg while the pocket is
                // short would overstate payments and leak the difference
                // from the escrow ledger.
                let escrow_leg = if is_state_investor {
                    from_escrow
                } else {
                    from_escrow.min(companies[investor_idx].construction_escrow.max(0.0))
                };
                if escrow_leg > 0.0 {
                    let leg_ok = if is_state_investor {
                        release_escrow_treasury_to_contractor(
                            companies,
                            contractor_idx,
                            escrow_leg,
                            country,
                        )
                        .is_ok()
                    } else {
                        release_escrow_company_to_contractor(
                            companies,
                            investor_idx,
                            contractor_idx,
                            escrow_leg,
                            country,
                        )
                        .is_ok()
                    };
                    if leg_ok {
                        paid += escrow_leg;
                    }
                }
            }
            if from_cash > 0.0 {
                if is_state_investor {
                    country.budget.liquid_reserves -= from_cash;
                    if release_escrow_treasury_to_contractor(
                        companies,
                        contractor_idx,
                        from_cash,
                        country,
                    )
                    .is_ok()
                    {
                        paid += from_cash;
                        cash_drawn += from_cash;
                    } else {
                        country.budget.liquid_reserves += from_cash;
                    }
                } else {
                    let debited = crate::economy::trade::transfer_settler::debit_company_by_id(
                        companies,
                        &investor_id,
                        from_cash,
                    );
                    if debited > 0.0 {
                        crate::economy::trade::transfer_settler::credit_company_by_id(
                            companies,
                            &contractor_id,
                            debited,
                        );
                        paid += debited;
                        cash_drawn += debited;
                    }
                }
            }
            if paid <= 0.0 {
                continue;
            }
            project.investor_cash_debited += cash_drawn;
            project.tranches_paid += paid;
            let amount = paid;
            if paid >= tranche.amount - 1e-6 {
                tranche.released = true;
                tranche.released_turn = 0; // caller can set actual turn
                project.paid_tranches += 1;
                released_count += 1;
            } else {
                // Shortfall on the cash leg mid-flight — the remainder is a
                // deferred claim the investor still owes; it retries next
                // pass rather than evaporating.
                tranche.amount -= paid;
            }
            // v4.17 mechanics fix: book the receipt as settled sales so
            // `process_companies` recognizes construction revenue (the
            // same convention `settle_transfer` uses for B2C sales and
            // `settle_trades` uses for B2B sellers). The cash leg is the
            // escrow release above — M0-neutral double-entry; this field
            // is pure revenue recognition and creates no money.
            companies[contractor_idx].turn_settled_sales += amount;
            #[cfg(feature = "diagnostic")]
            eprintln!(
                "CONST_DBG[TCRELEASE]: project={} site={} contractor={} investor={} amount={:.0} progress={:.2} tranches_paid={:.0} escrow_left={:.0}",
                project.id,
                building_id,
                contractor_id,
                investor_id,
                amount,
                progress,
                project.tranches_paid,
                (project.investor_cash_debited - project.tranches_paid).max(0.0)
            );
        }
    }

    released_count
}

/// Phase 22A: Pay subcontractors for completed tasks.
///
/// For each building with an active project that has subcontractors, pay any
/// subcontractor whose task is completed but not yet paid.
///
/// # Arguments
/// * `buildings` - Mutable slice of buildings (projects are read).
/// * `companies` - Mutable slice of companies (contractor debited, subcontractor credited).
/// * `country` - Mutable country state.
///
/// # Returns
/// Number of subcontractor payments made this turn.
pub fn pay_subcontractors(
    buildings: &mut [Building],
    companies: &mut [Company],
    country: &mut Country,
) -> u32 {
    let mut paid_count = 0u32;

    for building in buildings.iter_mut() {
        let project = match building.active_project.as_mut() {
            Some(p) => p,
            None => continue,
        };

        if project.subcontractors.is_empty() || project.main_contractor_id.is_empty() {
            continue;
        }

        let contractor_id = project.main_contractor_id.clone();
        let contractor_idx = match companies.iter().position(|c| c.id == contractor_id) {
            Some(idx) => idx,
            None => continue,
        };

        // Collect subcontractors that need payment (to avoid borrow issues)
        let to_pay: Vec<(usize, usize, f64)> = project
            .subcontractors
            .iter()
            .enumerate()
            .filter(|(_, s)| s.completed && !s.paid)
            .filter_map(|(i, s)| {
                let sub_idx = companies.iter().position(|c| c.id == s.subcontractor_id)?;
                Some((i, sub_idx, s.tranche_payment))
            })
            .collect();

        for (sub_idx_in_proj, sub_company_idx, amount) in to_pay {
            if settle_company_to_company(
                companies,
                contractor_idx,
                sub_company_idx,
                amount,
                country,
            )
            .is_ok()
            {
                project.subcontractors[sub_idx_in_proj].paid = true;
                paid_count += 1;
                // v4.17: the subcontractor's receipt is realized contract
                // revenue — book it so its financial history is non-zero
                // (same convention as the main-contractor tranche leg).
                companies[sub_company_idx].turn_settled_sales += amount;
                #[cfg(feature = "diagnostic")]
                eprintln!(
                    "CONST_DBG[SUBPAY]: project={} site={} sub={} amount={:.0}",
                    project.id,
                    building.id,
                    project.subcontractors[sub_idx_in_proj].subcontractor_id,
                    amount
                );
            }
        }
    }

    paid_count
}

/// Phase 4 fix (C6): Voluntarily abandon a construction project.
///
/// Called when a company decides to abandon a stalled or unprofitable
/// construction project (via `CorporateAction::AbandonProject`).
///
/// # Rules
/// * Refunds only the actual unspent escrow: `investor_cash_debited -
///   tranches_paid`. Does NOT refund consumed materials or contractor
///   margins (money never paid).
/// * Delivered-but-unconsumed materials remain in the building inventory
///   (they are physical assets that stay where they were delivered).
/// * Contractor keeps any tranches already paid (work performed).
/// * Updates project status consistently (clears `active_project`).
/// * Does NOT create synthetic cash or refund consumed materials.
///
/// # Arguments
/// * `building_id` - The building whose project should be abandoned.
/// * `buildings` - Mutable slice of buildings.
/// * `companies` - Mutable slice of companies (for corporate refund).
/// * `country` - Mutable country state (for State Treasury refund).
///
/// # Returns
/// `Ok(refund_amount)` on success, `Err(reason)` if the building has no
/// active project or the project is already complete.
pub fn abandon_project(
    building_id: &str,
    buildings: &mut [Building],
    companies: &mut [Company],
    country: &mut Country,
) -> Result<f64, String> {
    let building = buildings
        .iter_mut()
        .find(|b| b.id == building_id)
        .ok_or_else(|| format!("Building {} not found", building_id))?;

    let project = building
        .active_project
        .as_ref()
        .ok_or_else(|| format!("Building {} has no active project", building_id))?;

    if project.is_complete() {
        return Err(format!(
            "Building {} project is already complete — cannot abandon",
            building_id
        ));
    }

    let refund = (project.investor_cash_debited - project.tranches_paid).max(0.0);
    // v4.17: the expansion investor's unspent capital commitment releases
    // even when the cash escrow is already exhausted — the commitment
    // (not the escrow balance) is what `apply_action` debited.
    let commitment_release =
        (project.investor_capital_committed - project.tranches_paid).max(0.0);
    let investor_id = project.investor_id.clone();
    let is_state = investor_id.starts_with("STATE:");

    // Clear the active project — delivered-but-unconsumed materials stay
    // in building.inventory (they are physical assets).
    building.active_project = None;

    // Refund unspent escrow to the investor
    if refund > 0.0 || commitment_release > 0.0 {
        if is_state {
            country.budget.liquid_reserves += refund;
        } else if !investor_id.is_empty() {
            // v4.18 FS-7: pure pocket credit + encumbrance release — the
            // escrowed deposit never left the investor's bank book, so a
            // bank-synced credit would mint reserves without a
            // counterparty (same convention as `refund_unfilled_bids`).
            if let Some(company) = companies.iter_mut().find(|c| c.id == investor_id) {
                let released = refund.min(company.construction_escrow.max(0.0));
                if let Some(ref mut ba) = company.brokerage_account {
                    ba.cash += released;
                } else {
                    company.available_cash += released;
                }
                company.construction_escrow -= released;
                company.liquid_capital += commitment_release;
            }
        }
    }

    Ok(refund)
}

/// Phase 5 fix (M1): Compute construction labor demand from active projects.
///
/// Returns the total FTE needed across all active projects for a given
/// construction company (as main contractor or subcontractor).
///
/// # Rules
/// * Each active project requires labor proportional to the remaining
///   material requirements (more work = more workers).
/// * The base labor per project is `remaining_material_units / labor_per_unit`,
///   where `labor_per_unit` is derived from `average_wage` to avoid magic
///   numbers (Rule 2).
/// * A project with zero remaining materials needs 0 workers.
/// * A company with zero active projects gets 0 demand.
/// * The demand is clamped to a reasonable maximum based on the company's
///   existing workforce to prevent sudden massive hiring.
///
/// # Arguments
/// * `contractor_id` - The construction company ID.
/// * `buildings` - Slice of buildings (checked for `active_project`).
/// * `average_wage` - Current average wage (for labor-per-unit derivation).
///
/// # Returns
/// The total FTE demand for the company.
pub fn compute_construction_labor_demand(
    contractor_id: &str,
    buildings: &[Building],
    average_wage: f64,
) -> u32 {
    let avg_wage = average_wage.max(1.0);
    // Labor required per unit of material remaining.
    // Derived from average_wage: a worker installs ~10 wage-equivalents of
    // materials per turn (physical productivity, not a magic number).
    let labor_per_unit = 10.0 * avg_wage;

    let mut total_demand: f64 = 0.0;

    for building in buildings {
        let project = match &building.active_project {
            Some(p) => p,
            None => continue,
        };

        // Check if this company is the main contractor or a subcontractor
        let is_contractor = project.main_contractor_id == contractor_id;
        let is_subcontractor = project
            .subcontractors
            .iter()
            .any(|s| s.subcontractor_id == contractor_id);

        if !is_contractor && !is_subcontractor {
            continue;
        }

        // Compute remaining work as VALUE (not raw quantity): the BOM holds
        // physical units while `labor_per_unit` is a wage-denominated
        // throughput. The old code divided ~1,300 units by ~5,000 value and
        // demanded ~1 FTE per project — the second half of the
        // zero-construction-employment defect. Price remaining units at the
        // economy's own numéraire (`anchored_base_price`, the same basis
        // `cost_spent` valuation uses above).
        let remaining_value: f64 = project
            .required_materials
            .iter()
            .map(|(&commodity, &required)| {
                let delivered = project
                    .delivered_materials
                    .get(&commodity)
                    .copied()
                    .unwrap_or(0.0);
                let qty = (required - delivered).max(0.0);
                qty * crate::engine::generator::corporate::anchored_base_price(commodity)
                    .max(1.0)
            })
            .sum();

        if remaining_value <= 0.0 {
            continue;
        }

        // Labor demand = remaining work / (labor_per_unit × remaining
        // schedule). Each worker installs `labor_per_unit` worth of
        // materials per turn, so remaining/labor_per_unit is the crew
        // needed to finish in ONE turn — ~8× more than any contract's
        // labor budget can pay (observed: 6,669 FTE demanded against a
        // ~$70k/turn income rate). A real site paces the crew over the
        // remaining scheduled duration (`duration_turns − turns_elapsed`);
        // an overdue project compresses toward all-hands crash pace.
        // Capped per-site (200 workers) so mega-BOMs can't hire a city.
        let remaining_schedule = if project.duration_turns == 0 {
            12.0 // legacy projects carry no schedule
        } else {
            project
                .duration_turns
                .saturating_sub(project.turns_elapsed)
                .max(1) as f64
        };
        // v4.25.0 WS5: per-site crew cap raised 200→400 — a major
        // infrastructure/rail project legitimately crews several hundred.
        let project_demand = (remaining_value / (labor_per_unit * remaining_schedule))
            .min(400.0)
            .max(1.0);
        total_demand += project_demand;
    }

    total_demand.round() as u32
}

/// Phase 5 fix (M1): Update construction companies' labor demand based on
/// active project volume. Companies with no active projects furlough their
/// workforce; companies with active projects scale demand accordingly.
///
/// # Arguments
/// * `companies` - Mutable slice of companies (construction companies updated).
/// * `buildings` - Slice of buildings (checked for `active_project`).
/// * `average_wage` - Current average wage.
pub fn update_construction_labor_demand(
    companies: &mut [Company],
    buildings: &[Building],
    average_wage: f64,
) {
    for company in companies.iter_mut() {
        if company.sector != crate::registries::enums::Sector::Construction {
            continue;
        }

        let new_demand = compute_construction_labor_demand(&company.id, buildings, average_wage);

        // v4.17 FS-7: Contract-backed working capital. The labor market's
        // affordability gate only sees liquid cash, so a contractor that
        // wins an escrowed tender cannot staff the work until milestone
        // payments arrive — the working-capital deadlock behind
        // fulfilled≪demand. Unreleased tranches on escrowed projects are
        // cash already debited from the investor and contractually owed on
        // completion; unpaid subcontractor tasks are owed by the main
        // contractor. Both are real receivables — publish them for the
        // affordability gate, the same basis the material-bid path uses.
        let mut receivables = 0.0_f64;
        // v4.17 FS-7: payroll pacing. `new_demand` from the remaining BOM
        // counts ALL uninstallable work — a contractor could book a crew
        // its contract cash flow can never pay (observed: demand 6,669,
        // revenue $285k/turn, $974k wage bill → arrears → mass furlough →
        // stall → cancellation). Real contractors crew to the work the
        // current cash + the next milestone can pay for. `imminent` is the
        // next unreleased tranche per project — the cash actually arrivable
        // within one milestone — and supplier credit already committed to
        // materials is deducted: the same escrowed dollar cannot fund both
        // the material bid and the payroll.
        let mut imminent = 0.0_f64;
        let mut active_projects = 0u32;
        for building in buildings {
            let project = match &building.active_project {
                Some(p) => p,
                None => continue,
            };
            if project.main_contractor_id == company.id {
                active_projects += 1;
                if project.investor_cash_debited > 0.0 {
                    receivables += project
                        .tranches
                        .iter()
                        .filter(|t| !t.released)
                        .map(|t| t.amount)
                        .sum::<f64>();
                    if let Some(next) = project.tranches.iter().find(|t| !t.released) {
                        imminent += next.amount;
                    }
                }
            }
            for sub in &project.subcontractors {
                if sub.subcontractor_id == company.id && !sub.paid {
                    receivables += sub.tranche_payment;
                    imminent += sub.tranche_payment;
                }
            }
        }
        company.extra.insert(
            "contract_receivables".to_string(),
            serde_json::Value::from(receivables),
        );
        // Uncapped committed workload (FTE) — the backlog figure the tender
        // market reads for bonding capacity, before payroll pacing trims it
        // to what the company can pay this turn.
        company.extra.insert(
            "committed_demand_fte".to_string(),
            serde_json::Value::from(new_demand as f64),
        );

        // Crew size bounded by payroll capacity through the next milestone:
        // (cash on hand + half the next tranche, haircut for cancellation
        // risk − supplier credit already committed to materials) spread
        // over the ~4-turn pay cycle until it lands. A crew payable for one
        // turn but not through the milestone goes into arrears — the
        // hire→furlough→stall churn observed at Dacia. Between tranches a
        // site-keeping skeleton (~20 per live project) stays on so the
        // contract isn't abandoned the turn a milestone clears.
        // v4.17.1: the receivable side is capped at the crew's own install
        // throughput for the window — tranche releases are progress-gated,
        // so cash that cannot physically arrive this cycle is not payroll
        // capacity. Unbounded `imminent` let a mega-backlog contractor
        // claim next-milestone funding on thousands of stalled sites at
        // once.
        let wage = company
            .offered_wage_per_fte
            .max(average_wage)
            .max(1.0);
        let crew_fte = company.fulfilled_fte as f64
            + company.furloughed_workers_count.max(0.0);
        let install_per_cycle =
            crew_fte * 10.0 * wage * crate::construction::tender_market::MILESTONE_SPACING_TURNS;
        let imminent_capped = (imminent * 0.5).min(install_per_cycle);
        // v4.20 WS3b: only the AP actually collectible THIS cycle
        // competes with payroll. Suppliers cannot extract cash the
        // company does not hold — the retirement pass already debits
        // exactly `pocket − payroll_reserve` (transfer_settler), so
        // netting the full `procurement_credit` stock here punishes the
        // same obligation twice: the credit-bought materials sit
        // undelivered-to-site because the liability that bought them
        // also zeroes the crew needed to install them, which stalls the
        // tranche revenue that would retire it — the FS-7 death spiral
        // that pinned solvent-backlog contractors at skeleton crews.
        let payroll_reserve =
            company.fulfilled_fte as f64 * company.offered_wage_per_fte.max(1.0);
        let pocket = company
            .brokerage_account
            .as_ref()
            .map(|ba| ba.cash.max(0.0))
            .unwrap_or(company.available_cash.max(0.0));
        let collectible_credit = company
            .procurement_credit
            .min((pocket - payroll_reserve).max(0.0));
        let payroll_capacity_fte = ((company.operational_cash()
            + imminent_capped
            - collectible_credit)
            .max(0.0)
            / (wage * crate::construction::tender_market::MILESTONE_SPACING_TURNS))
            as u32;
        let skeleton = (active_projects * 20).min(new_demand);
        let raw_demand = new_demand;
        let new_demand = new_demand.min(payroll_capacity_fte.max(skeleton));

        let old_demand = company.physical_fte_demand;

        #[cfg(feature = "diagnostic")]
        if new_demand != old_demand || company.fulfilled_fte > 0 {
            eprintln!(
                "CONST_DBG[LABORDEMAND]: company={} demand={} prev={} fulfilled={} furloughed={:.0} wage_offer={:.0} pc={} cred={:.0} cft={} imm={:.0} cash={:.0} sites={}",
                company.id,
                new_demand,
                old_demand,
                company.fulfilled_fte,
                company.furloughed_workers_count,
                company.offered_wage_per_fte,
                payroll_capacity_fte,
                company.procurement_credit,
                raw_demand,
                imminent,
                company.operational_cash(),
                active_projects,
            );
        }

        if new_demand == 0 && old_demand > 0 {
            // No active projects — furlough all workers
            let excess = company.fulfilled_fte as f64;
            company.furloughed_workers_count += excess;
            company.fulfilled_fte = 0;
            company.physical_fte_demand = 0;
            company.target_fte_demand = 0;
        } else if new_demand > 0 {
            // Active projects — set demand and re-instate furloughed workers
            // if needed
            if company.furloughed_workers_count > 0.0 && new_demand > company.fulfilled_fte {
                let needed = (new_demand as f64 - company.fulfilled_fte as f64).max(0.0);
                let reinstate = needed.min(company.furloughed_workers_count);
                company.fulfilled_fte += reinstate.round() as u32;
                company.furloughed_workers_count =
                    (company.furloughed_workers_count - reinstate).max(0.0);
            }
            company.physical_fte_demand = new_demand;
            company.target_fte_demand = new_demand;
        }
    }
}

/// Phase 8 fix (M4): Find an undeveloped State-owned parcel in the target
/// region for a State construction project.
///
/// # Rules
/// * The parcel MUST be `owner_type: State` and `owner_id: "TREASURY"`.
/// * The parcel MUST NOT already have a building on it (checked via
///   `buildings` slice — no parcel_id matches any building's parcel).
/// * The parcel MUST be in the target region.
/// * The parcel MUST NOT be frozen.
/// * Returns the first matching parcel ID, or `None` if none available.
///
/// # Arguments
/// * `cadastre` - The cadastre registry.
/// * `region_id` - Target region for the project.
/// * `buildings` - Buildings slice (to check which parcels are developed).
///
/// # Returns
/// `Some(ParcelId)` if an undeveloped State parcel is found, `None` otherwise.
pub fn find_undeveloped_state_parcel(
    cadastre: &crate::society::cadastre::Cadastre,
    region_id: &str,
    buildings: &[Building],
) -> Option<crate::society::cadastre::ParcelId> {
    use crate::society::cadastre::ParcelOwnerType;

    // Collect parcel IDs that already have buildings on them
    let developed_parcels: std::collections::HashSet<String> = buildings
        .iter()
        .filter_map(|b| b.parcel_id.clone().filter(|s| !s.is_empty()))
        .collect();

    for (parcel_id, parcel) in cadastre.parcels.iter() {
        if parcel.owner_type != ParcelOwnerType::State {
            continue;
        }
        if parcel.owner_id != "TREASURY" {
            continue;
        }
        if parcel.region_id != region_id {
            continue;
        }
        if parcel.is_frozen {
            continue;
        }
        // Check if this parcel is already developed
        let parcel_id_str = format!("{:?}", parcel_id);
        if developed_parcels.contains(&parcel_id_str) {
            continue;
        }
        return Some(parcel_id);
    }
    None
}

/// Phase 8 fix (M4): Reallocate a State-owned parcel to a construction
/// project WITHOUT purchasing it from the Treasury.
///
/// # Rules
/// * The parcel's `owner_type` remains `State`.
/// * The parcel's `owner_id` remains `"TREASURY"`.
/// * The parcel is marked as reserved for the project via `land_use_tag`.
/// * No money changes hands — the State already owns this land.
///
/// # Arguments
/// * `cadastre` - Mutable cadastre registry.
/// * `parcel_id` - The parcel to reallocate.
/// * `project_id` - The construction project ID (for tracking).
pub fn reallocate_state_parcel(
    cadastre: &mut crate::society::cadastre::Cadastre,
    parcel_id: crate::society::cadastre::ParcelId,
    project_id: &str,
) -> bool {
    if let Some(parcel) = cadastre.parcels.get_mut(parcel_id) {
        // Preserve owner_type: State and owner_id: TREASURY
        // Mark as reserved for this project
        parcel.land_use_tag = format!("StateProject:{}", project_id);
        return true;
    }
    false
}

/// Phase 8 fix (M4): Ensure a construction project has land before it can
/// proceed. For State projects, reallocate a Treasury parcel. For corporate
/// projects, the project should already have acquired land via the cadastre
/// market.
///
/// # Arguments
/// * `project` - The construction project (mutated to set `parcel_id`).
/// * `cadastre` - Mutable cadastre registry.
/// * `buildings` - Buildings slice (to check developed parcels).
/// * `region_id` - Target region.
///
/// # Returns
/// `true` if the project has land (or acquired it), `false` if it still
/// needs land and cannot proceed.
pub fn ensure_project_land(
    project: &mut ConstructionProject,
    cadastre: &mut crate::society::cadastre::Cadastre,
    buildings: &[Building],
    region_id: &str,
) -> bool {
    // Already has a parcel
    if !project.parcel_id.is_empty() {
        return true;
    }

    // State projects: reallocate a Treasury parcel (no purchase)
    if project.investor_id.starts_with("STATE:") {
        if let Some(parcel_id) = find_undeveloped_state_parcel(cadastre, region_id, buildings) {
            let project_id = format!("{}_{:?}", region_id, project.project_type);
            if reallocate_state_parcel(cadastre, parcel_id, &project_id) {
                project.parcel_id = format!("{:?}", parcel_id);
                return true;
            }
        }
        return false; // No State parcel available
    }

    // Corporate projects: must have acquired land via the cadastre market
    // before construction starts. If no parcel_id is set, the project
    // cannot proceed.
    false
}

/// Phase 8 fix (M4): Ensure all active construction projects have land.
/// Must be called BEFORE `advance_construction_projects` and with separate
/// borrows to avoid borrow-checker violations.
///
/// For State projects: reallocates Treasury parcels (no purchase).
/// For corporate projects: skips projects that already have `parcel_id`;
/// projects without land are put on hold (cannot advance).
///
/// # Arguments
/// * `buildings` - Mutable buildings (projects checked/updated).
/// * `cadastre` - Mutable cadastre (State parcels reallocated).
pub fn ensure_all_projects_have_land(
    buildings: &mut [Building],
    cadastre: &mut crate::society::cadastre::Cadastre,
) {
    for building in buildings.iter_mut() {
        let project = match building.active_project.as_mut() {
            Some(p) => p,
            None => continue,
        };

        let region_id = building.region_id.clone();
        // Temporarily remove the project to avoid double borrow of `building`
        let has_land = ensure_project_land(project, cadastre, &[], &region_id);
        if !has_land && project.parcel_id.is_empty() {
            // Project has no land — put on hold to stall it
            project.put_on_hold("No land parcel acquired".to_string());
        }
    }
}

/// Refund the unspent escrow of an awarded project that could not be
/// attached to any building. Returns the refund amount paid.
///
/// # Rules
/// * STATE investors are refunded into `country.budget.liquid_reserves`
///   (their escrow debited the Treasury at publication).
/// * Corporate investors are refunded through `credit_company_by_id`
///   (proper bank-reserve sync, M0-neutral) and the `debit_cash`
///   encumbrance is released. Expansion projects additionally release the
///   unspent share of their committed `liquid_capital` (debited by
///   `apply_action` at decision time) — `committed − tranches_paid`.
/// * Never creates money: the cash refund is exactly the unspent escrow —
///   `investor_cash_debited − tranches_paid`.
fn refund_project_escrow(
    project: &ConstructionProject,
    companies: &mut [Company],
    country: &mut Country,
) -> f64 {
    refund_unspent_escrow(
        &project.investor_id,
        (project.investor_cash_debited - project.tranches_paid).max(0.0),
        (project.investor_capital_committed - project.tranches_paid).max(0.0),
        companies,
        country,
    )
}

/// Field-level escrow refund — shared by attach-failure, stall-cancel, and
/// project-completion paths. `unspent_commitment` is the expansion
/// investor's `liquid_capital` still tied up in this project
/// (`investor_capital_committed − tranches_paid`, 0 for other investors).
fn refund_unspent_escrow(
    investor_id: &str,
    refund: f64,
    unspent_commitment: f64,
    companies: &mut [Company],
    country: &mut Country,
) -> f64 {
    if refund <= 0.0 && unspent_commitment <= 0.0 {
        return 0.0;
    }
    if investor_id.starts_with("STATE:") {
        country.budget.liquid_reserves += refund;
    } else if !investor_id.is_empty() {
        if let Some(c) = companies.iter_mut().find(|c| c.id == investor_id) {
            // Pure pocket credit + encumbrance release — the escrowed
            // deposit never left the investor's bank book, so no bank
            // sync applies (same convention as `refund_unfilled_bids`).
            // Clamp the release to the pocket balance: crediting more
            // than remains encumbered would mint money.
            let released = refund.min(c.construction_escrow.max(0.0));
            if let Some(ref mut ba) = c.brokerage_account {
                ba.cash += released;
            } else {
                c.available_cash += released;
            }
            c.construction_escrow -= released;
            c.liquid_capital += unspent_commitment;
        }
    }
    refund
}

/// v4.17 mechanics fix — attach newly-awarded projects to buildings.
///
/// Replaces the silent-drop attachment that lived inline in `engine/turn.rs`:
/// previously an awarded project that found no matching building was simply
/// dropped — its escrow stayed debited forever (an M0 leak), no contractor
/// was ever attached, and construction never happened. State tenders could
/// NEVER match (`building.owner_id == "State"` vs `investor_id "STATE:…"`),
/// so every state-funded project evaporated on award.
///
/// # Rules
/// * Expansion tenders attach to `expansion_target_building_id`. If the
///   target vanished or already hosts a project, the award is cancelled and
///   escrow refunded.
/// * New-build tenders first try the legacy match — an investor-owned,
///   project-free building of the target kind already standing in the
///   region (keeps upgrade-in-place semantics).
/// * Otherwise a dedicated SITE BUILDING is spawned, owned by the investor
///   ("State" for `STATE:` investors). It starts with `worker_capacity = 0`;
///   on completion `advance_construction_projects` adds the contracted
///   capacity and stamps the target kind/sector.
/// * The site is registered on the investor's `building_ids` so ownership
///   lookups, fixed-capital accounting, and delivery routing see it.
///
/// # Returns
/// Diagnostic messages (one per award).
pub fn attach_awarded_projects(
    awarded: Vec<(String, ConstructionProject, Option<String>)>,
    buildings: &mut Vec<Building>,
    companies: &mut [Company],
    country: &mut Country,
    current_year: u32,
) -> Vec<String> {
    let mut messages = Vec::new();

    for (award_seq, (tender_id, project, expansion_target)) in
        awarded.into_iter().enumerate()
    {
        let mut project = project;

        // v4.18 FS-7: award-time escrow top-up. Publication escrowed 30% of
        // the estimate; the signed contract is the winning bid price. The
        // mobilization security owed at signing is
        // `contract_price × MOBILIZATION_ESCROW_RATIO` — draw the difference
        // from the investor's spendable cash into the escrow pocket now.
        // An investor that cannot cover the top-up cannot honor tranche 1
        // (20% of contract): refusing the award refunds the escrow instead
        // of spawning a zombie site that stop-works at progress 0.00.
        if !project.investor_id.is_empty()
            && !project.investor_id.starts_with("STATE:")
            && project.investor_cash_debited
                < project.contract_price
                    * crate::construction::tender_market::MOBILIZATION_ESCROW_RATIO
        {
            let required = project.contract_price
                * crate::construction::tender_market::MOBILIZATION_ESCROW_RATIO;
            let shortfall = required - project.investor_cash_debited;
            let investor_idx =
                companies.iter().position(|c| c.id == project.investor_id);
            let paid = if let Some(iidx) = investor_idx {
                let investor = &mut companies[iidx];
                let pocket = if let Some(ref mut ba) = investor.brokerage_account {
                    &mut ba.cash
                } else {
                    &mut investor.available_cash
                };
                let drawn = shortfall.min((*pocket).max(0.0));
                *pocket -= drawn;
                investor.construction_escrow += drawn;
                project.investor_cash_debited += drawn;
                drawn
            } else {
                0.0
            };
            if paid < shortfall - 1e-6 {
                // Mobilization security unfundable — cancel the award and
                // return whatever escrow exists rather than attach a
                // guaranteed stall.
                let refund = refund_project_escrow(&project, companies, country);
                #[cfg(feature = "diagnostic")]
                eprintln!(
                    "CONST_DBG[ATTACH]: tender={} project={} mode=refuse reason=mobilization_escrow_unfunded investor={} required={:.0} escrowed={:.0} refund={:.0}",
                    tender_id,
                    project.id,
                    project.investor_id,
                    required,
                    project.investor_cash_debited,
                    refund
                );
                messages.push(format!(
                    "Tender {} award refused: investor cannot fund mobilization escrow ({:.0}/{:.0}) — escrow refunded",
                    tender_id,
                    project.investor_cash_debited,
                    required
                ));
                continue;
            }
            #[cfg(feature = "diagnostic")]
            if paid > 0.0 {
                eprintln!(
                    "CONST_DBG[ESCROWTOPUP]: tender={} investor={} contract={:.0} topup={:.0} escrow_total={:.0}",
                    tender_id,
                    project.investor_id,
                    project.contract_price,
                    paid,
                    project.investor_cash_debited
                );
            }
        }

        // ── Expansion tender: attach to the building being expanded ──
        if let Some(target_bldg_id) = expansion_target {
            match buildings
                .iter_mut()
                .find(|b| b.id == target_bldg_id && b.active_project.is_none())
            {
                Some(building) => {
                    #[cfg(feature = "diagnostic")]
                    eprintln!(
                        "CONST_DBG[ATTACH]: tender={} project={} mode=expansion site={} contractor={}",
                        tender_id,
                        project.id,
                        building.id,
                        project.main_contractor_id
                    );
                    messages.push(format!(
                        "Tender {} awarded: expansion attached to building {}",
                        tender_id, building.id
                    ));
                    building.active_project = Some(project);
                }
                None => {
                    let refund = refund_project_escrow(&project, companies, country);
                    #[cfg(feature = "diagnostic")]
                    eprintln!(
                        "CONST_DBG[ATTACH]: tender={} project={} mode=refund reason=expansion_target_unavailable refund={:.0}",
                        tender_id, project.id, refund
                    );
                    messages.push(format!(
                        "Tender {} cancelled at attach: expansion target unavailable — escrow {:.0} refunded",
                        tender_id, refund
                    ));
                }
            }
            continue;
        }

        // ── New build: legacy match — an existing investor-owned,
        //    project-free building of the target kind in the region ──
        let region_id = project.micro_region_id.clone();
        let investor_id = project.investor_id.clone();
        let target_bt = project.target_building_type.clone();
        let legacy_site = buildings.iter_mut().find(|b| {
            b.region_id == region_id
                && (b.owner_id == investor_id || investor_id.is_empty())
                && b.active_project.is_none()
                && (b.name == target_bt || target_bt.is_empty())
        });
        if let Some(building) = legacy_site {
            #[cfg(feature = "diagnostic")]
            eprintln!(
                "CONST_DBG[ATTACH]: tender={} project={} mode=match site={} contractor={}",
                tender_id,
                project.id,
                building.id,
                project.main_contractor_id
            );
            messages.push(format!(
                "Tender {} awarded: project attached to existing building {}",
                tender_id, building.id
            ));
            building.active_project = Some(project);
            continue;
        }

        // ── No standing structure matches: break ground on a dedicated
        //    site building owned by the investor ──
        let site_id = format!("site_{}_{}", project.id, award_seq);
        let owner_id = if investor_id.is_empty() || investor_id.starts_with("STATE:") {
            "State".to_string()
        } else {
            investor_id.clone()
        };
        let sector = crate::construction::bom::sector_from_building_kind(&target_bt);
        let mut site = Building::new(site_id.clone(), owner_id.clone(), sector, 0);
        site.name = if target_bt.is_empty() {
            format!("{:?}", project.project_type)
        } else {
            target_bt.clone()
        };
        site.region_id = region_id.clone();
        site.year_built = current_year;

        // A spawned corporate site IS its own land record — the cadastre-
        // market purchase flow belongs to the real-estate domain; marking
        // the project avoids the "No land parcel acquired" hold oscillation.
        // STATE sites are left unmarked so `ensure_project_land` performs
        // the real Treasury-parcel reallocation.
        if !investor_id.starts_with("STATE:") {
            project.parcel_id = format!("site:{}", site_id);
        }

        let project_id = project.id.clone();
        let contractor_id = project.main_contractor_id.clone();
        site.active_project = Some(project);
        buildings.push(site);

        // Register the site on the investor's building_ids so ownership
        // lookups (fixed_capital on completion, delivery routing) see it.
        if owner_id != "State" {
            if let Some(c) = companies.iter_mut().find(|c| c.id == owner_id) {
                if !c.building_ids.contains(&site_id) {
                    c.building_ids.push(site_id.clone());
                }
            }
        }

        #[cfg(feature = "diagnostic")]
        eprintln!(
            "CONST_DBG[ATTACH]: tender={} project={} mode=site_spawn site={} owner={} contractor={} region={}",
            tender_id, project_id, site_id, owner_id, contractor_id, region_id
        );
        messages.push(format!(
            "Tender {} awarded: new site {} broke ground",
            tender_id, site_id
        ));
    }

    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::tenders::default_tranches;
    use crate::entities::legal_form::{JointStockData, LegalForm};
    use crate::entities::Company;
    use crate::registries::enums::Sector;
    use crate::state::Country;

    fn make_company(id: &str, sector: Sector, cash: f64) -> Company {
        Company::new(
            id.to_string(),
            id.to_string(),
            sector,
            LegalForm::JointStockCompany(JointStockData::default()),
            100_000.0,
            cash,
            10,
        )
    }

    fn make_project(
        investor: &str,
        contractor: &str,
        escrowed: f64,
        contract_price: f64,
    ) -> ConstructionProject {
        ConstructionProject {
            id: "proj_test".to_string(),
            investor_id: investor.to_string(),
            main_contractor_id: contractor.to_string(),
            micro_region_id: "R1".to_string(),
            target_building_type: "Warehouse".to_string(),
            is_new_building: true,
            investor_cash_debited: escrowed,
            contract_price,
            tranches: default_tranches(contract_price),
            ..Default::default()
        }
    }

    /// An awarded STATE project must spawn a site building — the old
    /// inline turn-loop attach could never match `"STATE:…"` to
    /// `building.owner_id == "State"`, so every state project vanished.
    #[test]
    fn attach_spawns_site_for_state_project() {
        let mut buildings: Vec<Building> = Vec::new();
        let mut companies = vec![make_company("dev_co", Sector::Construction, 10_000.0)];
        let mut country = Country::default();
        let project = make_project("STATE:R1", "dev_co", 50_000.0, 50_000.0);

        let msgs = attach_awarded_projects(
            vec![("tender_1".to_string(), project, None)],
            &mut buildings,
            &mut companies,
            &mut country,
            1960,
        );

        assert_eq!(buildings.len(), 1, "a site building must be spawned");
        let site = &buildings[0];
        assert_eq!(site.owner_id, "State");
        assert!(site.active_project.is_some());
        assert_eq!(site.worker_capacity, 0);
        assert!(msgs.iter().any(|m| m.contains("broke ground")));
    }

    /// An expansion award whose target vanished must refund escrow to the
    /// investor instead of dropping the project (escrow stays debited).
    #[test]
    fn attach_refunds_escrow_when_expansion_target_missing() {
        let mut buildings: Vec<Building> = Vec::new();
        let mut investor = make_company("inv_co", Sector::PublicServices, 0.0);
        investor.construction_escrow = 8_000.0; // encumbered at publish
        let mut companies = vec![investor];
        let mut country = Country::default();
        let mut project = make_project("inv_co", "dev_co", 8_000.0, 8_000.0);
        project.is_new_building = false;
        // Expansion tenders commit the investor's liquid_capital at decision
        // time — `award_tender` records it as `investor_capital_committed`.
        project.investor_capital_committed = 8_000.0;

        let _ = attach_awarded_projects(
            vec![(
                "tender_2".to_string(),
                project,
                Some("nonexistent_bldg".to_string()),
            )],
            &mut buildings,
            &mut companies,
            &mut country,
            1960,
        );

        assert!(buildings.is_empty(), "no site should spawn for expansions");
        let inv = &companies[0];
        assert_eq!(inv.construction_escrow, 0.0, "encumbrance must be released");
        assert!(
            inv.available_cash > 0.0
                || inv.brokerage_account.as_ref().map(|b| b.cash).unwrap_or(0.0) > 0.0,
            "refund must restore investor cash"
        );
        assert!(
            inv.liquid_capital >= 8_000.0,
            "liquid_capital must be restored by the refunded escrow, got {}",
            inv.liquid_capital
        );
    }

    /// Contractor-purchased goods must move from contractor-owned buildings
    /// to the project site (delivery routing sends them to the buyer).
    #[test]
    fn deploy_project_materials_stages_site() {
        let mut contractor_site = Building::new(
            "crew_plant".to_string(),
            "con_co".to_string(),
            Sector::Construction,
            50,
        );
        contractor_site.inventory.insert(Commodity::Steel, 100.0);

        let mut site = Building::new(
            "site_1".to_string(),
            "State".to_string(),
            Sector::Construction,
            0,
        );
        let mut project = make_project("STATE:R1", "con_co", 10_000.0, 10_000.0);
        project
            .required_materials
            .insert(Commodity::Steel, 40.0);
        site.active_project = Some(project);

        let mut buildings = vec![contractor_site, site];
        deploy_project_materials(&mut buildings);

        let site_inv = buildings[1]
            .inventory
            .get(&Commodity::Steel)
            .copied()
            .unwrap_or(0.0);
        assert_eq!(site_inv, 40.0, "site should receive BOM need");
        let donor_left = buildings[0]
            .inventory
            .get(&Commodity::Steel)
            .copied()
            .unwrap_or(0.0);
        assert_eq!(donor_left, 60.0, "donor keeps surplus above need");
    }

    /// Tranche release must book the contractor's receipt as settled sales
    /// so `process_companies` reports construction revenue.
    #[test]
    fn tranche_release_books_settled_sales() {
        let mut contractor = make_company("con_co", Sector::Construction, 0.0);
        contractor.available_cash = 1.0; // non-empty pocket for credit
        let mut companies = vec![contractor];
        let mut country = Country::default();

        let mut site = Building::new(
            "site_1".to_string(),
            "State".to_string(),
            Sector::Construction,
            0,
        );
        // progress 0.0 → the trigger-0.0 mobilization tranche releases.
        let project = make_project("STATE:R1", "con_co", 10_000.0, 10_000.0);
        site.active_project = Some(project);
        let mut buildings = vec![site];

        let released =
            release_construction_tranches(&mut buildings, &mut companies, &mut country);
        assert_eq!(released, 1, "mobilization tranche should release");
        assert!(
            companies[0].turn_settled_sales > 0.0,
            "contractor receipt must be booked as settled sales, got {}",
            companies[0].turn_settled_sales
        );
        assert_eq!(companies[0].turn_settled_sales, 2_000.0); // 20% of 10k
    }

    /// v4.18 FS-7 regression: tender escrow must live in the dedicated
    /// `construction_escrow` pocket, not the shared `debit_cash` pool —
    /// B2B settlement and the wage trust drain `debit_cash`, so a funded
    /// tranche would clamp to zero and stall the project even though the
    /// investor escrowed at award.
    #[test]
    fn tranche_release_ignores_shared_debit_pool() {
        let mut investor = make_company("inv_co", Sector::PublicServices, 0.0);
        investor.construction_escrow = 2_000.0; // escrow funded at award
        investor.debit_cash = 0.0; // B2B/wage drains consumed the shared pool
        investor.available_cash = 0.0;
        let mut contractor = make_company("con_co", Sector::Construction, 0.0);
        contractor.available_cash = 1.0;
        let mut companies = vec![investor, contractor];
        let mut country = Country::default();

        let mut site = Building::new(
            "site_1".to_string(),
            "inv_co".to_string(),
            Sector::Construction,
            0,
        );
        // Contract 10k; escrow claim 2k covers the mobilization tranche.
        let project = make_project("inv_co", "con_co", 2_000.0, 10_000.0);
        site.active_project = Some(project);
        let mut buildings = vec![site];

        let released =
            release_construction_tranches(&mut buildings, &mut companies, &mut country);
        assert_eq!(
            released, 1,
            "mobilization tranche must release from the escrow pocket even \
             when the shared debit pool is empty"
        );
        assert_eq!(companies[0].construction_escrow, 0.0);
        assert_eq!(companies[1].turn_settled_sales, 2_000.0);
    }

    /// v4.18 FS-7 regression: at award the mobilization escrow must be
    /// topped up to 30% of the signed CONTRACT price (the publish-time
    /// estimate can undershoot the real bid). An investor that cannot
    /// cover the top-up gets the award refused and the escrow refunded —
    /// no zombie site.
    #[test]
    fn attach_tops_up_escrow_to_contract_price() {
        let mut buildings: Vec<Building> = Vec::new();
        let mut investor = make_company("inv_co", Sector::PublicServices, 0.0);
        investor.construction_escrow = 3_000.0; // 30% of a 10k estimate
        investor.available_cash = 9_000.0;
        let mut companies = vec![investor];
        let mut country = Country::default();
        // Contract signed at 20k — escrow must reach 6k (30%).
        let project = make_project("inv_co", "con_co", 3_000.0, 20_000.0);

        let _ = attach_awarded_projects(
            vec![("tender_topup".to_string(), project, None)],
            &mut buildings,
            &mut companies,
            &mut country,
            1960,
        );

        assert_eq!(buildings.len(), 1, "fundable award must attach a site");
        assert_eq!(
            companies[0].construction_escrow, 6_000.0,
            "escrow must be topped to 30% of contract, got {}",
            companies[0].construction_escrow
        );
        assert_eq!(companies[0].available_cash, 6_000.0);
        let escrow_claim = buildings[0]
            .active_project
            .as_ref()
            .unwrap()
            .investor_cash_debited;
        assert_eq!(escrow_claim, 6_000.0);
    }

    /// The mirror case: investor cannot fund 30% of the signed contract —
    /// the award is refused and the already-escrowed funds return.
    #[test]
    fn attach_refuses_unfundable_contract() {
        let mut buildings: Vec<Building> = Vec::new();
        let mut investor = make_company("inv_co", Sector::PublicServices, 0.0);
        investor.construction_escrow = 3_000.0;
        investor.available_cash = 0.0; // cannot top up
        let mut companies = vec![investor];
        let mut country = Country::default();
        let project = make_project("inv_co", "con_co", 3_000.0, 20_000.0);

        let _ = attach_awarded_projects(
            vec![("tender_refuse".to_string(), project, None)],
            &mut buildings,
            &mut companies,
            &mut country,
            1960,
        );

        assert!(
            buildings.is_empty(),
            "an unfundable award must not spawn a zombie site"
        );
        assert_eq!(companies[0].construction_escrow, 0.0);
        assert_eq!(
            companies[0].available_cash, 3_000.0,
            "refused award must refund the escrow to spendable cash"
        );
    }

    /// v4.18 WS2: a project whose last material installs must reach
    /// progress 1.0, release EVERY tranche including the completion
    /// milestone (trigger 1.0), pay the contractor the full contract, and
    /// clear the site record. Two pre-fix defects broke this: consumption
    /// asymptoted below 1.0 (`installable = available × p` never closed the
    /// last gap), and the completion branch cleared `active_project` before
    /// the tranche sweep ran, so the final 20% could never be paid.
    #[test]
    fn completion_releases_final_tranche_and_clears() {
        let mut contractor = make_company("con_co", Sector::Construction, 0.0);
        contractor.available_cash = 1.0; // non-empty pocket for credits
        // Construction is labor-gated: productivity scales with the
        // contractor's crew fill ratio. Staff it fully.
        contractor.fulfilled_fte = 10;
        contractor.target_fte_demand = 10;
        let mut companies = vec![contractor];
        let mut country = Country::default();
        country.budget.liquid_reserves = 10_000_000.0;

        let mut site = Building::new(
            "site_1".to_string(),
            "State".to_string(),
            Sector::Construction,
            0,
        );
        let mut project = make_project("STATE:R1", "con_co", 10_000.0, 10_000.0);
        project.required_materials.insert(Commodity::Timber, 10.0);
        // One unit short of complete; the last unit is staged in the site
        // laydown so `advance_construction_projects` performs the final
        // consume and crosses 1.0 inside the completion path.
        project
            .delivered_materials
            .insert(Commodity::Timber, 9.0);
        project.target_capacity_increase = 40;
        site.inventory.insert(Commodity::Timber, 1.0);
        site.active_project = Some(project);
        let mut buildings = vec![site];

        let unit_costs: BTreeMap<Commodity, f64> =
            [(Commodity::Timber, 1.0)].into_iter().collect();
        // Real turn order: the tranche sweep releases the mobilization
        // advance before the advance pass consumes staged materials.
        release_construction_tranches(&mut buildings, &mut companies, &mut country);
        let (msgs, _spent, _by_region) = advance_construction_projects(
            &mut buildings,
            &mut companies,
            &unit_costs,
            &mut country,
        );

        assert!(
            buildings[0].active_project.is_none(),
            "a fully-settled completed project must clear the site record"
        );
        assert_eq!(
            buildings[0].worker_capacity, 40,
            "completion must credit the contracted capacity once"
        );
        assert!(
            msgs.iter().any(|m| m.contains("Construction complete")),
            "completion message expected, got {:?}",
            msgs
        );
        assert!(
            (companies[0].turn_settled_sales - 10_000.0).abs() < 1e-6,
            "contractor must book the full contract incl. the completion \
             tranche, got {}",
            companies[0].turn_settled_sales
        );
    }

    /// v4.18 WS2: when the completion tranche's cash leg defers (investor
    /// cannot cover it), the finished site must keep its project record as
    /// the contractor's open claim — clearing it would silently erase the
    /// unpaid receivable. The tranche sweep retries each turn; once funded
    /// and paid, the record clears.
    #[test]
    fn completion_claim_survives_deferred_tranche() {
        // Corporate investor: escrowed 2k at publish (held in
        // construction_escrow), contract 10k — tranches are 2k/3k/3k/2k, so
        // after the escrow leg pays tranche_1 the remaining 8k needs
        // investor cash. Investor starts with none → deferral.
        let mut investor = make_company("inv_co", Sector::PublicServices, 0.0);
        investor.construction_escrow = 2_000.0; // escrow pool from publication
        investor.available_cash = 0.0;
        investor.liquid_capital = 0.0;
        let mut contractor = make_company("con_co", Sector::Construction, 0.0);
        contractor.available_cash = 1.0;
        contractor.fulfilled_fte = 10;
        contractor.target_fte_demand = 10;
        let mut companies = vec![investor, contractor];
        let mut country = Country::default();

        let mut site = Building::new(
            "site_1".to_string(),
            "inv_co".to_string(),
            Sector::Construction,
            0,
        );
        let mut project = make_project("inv_co", "con_co", 2_000.0, 10_000.0);
        project.required_materials.insert(Commodity::Timber, 10.0);
        project
            .delivered_materials
            .insert(Commodity::Timber, 9.0);
        site.inventory.insert(Commodity::Timber, 1.0);
        site.active_project = Some(project);
        let mut buildings = vec![site];

        let unit_costs: BTreeMap<Commodity, f64> =
            [(Commodity::Timber, 1.0)].into_iter().collect();
        // Real turn order: the tranche sweep pays the mobilization advance
        // (escrow leg) before the advance pass runs — otherwise the
        // invoice-past-due gate correctly stops work on an unpaid contract.
        release_construction_tranches(&mut buildings, &mut companies, &mut country);
        advance_construction_projects(&mut buildings, &mut companies, &unit_costs, &mut country);

        // Physical work is done (capacity grant is one-shot) but the
        // record survives: tranche_1 settled from escrow, tranches 2-4
        // are a live claim on the unfunded investor.
        assert!(
            buildings[0].active_project.is_some(),
            "an unpaid completion tranche must keep the claim open"
        );
        assert!(
            buildings[0]
                .active_project
                .as_ref()
                .unwrap()
                .work_complete,
            "physical completion is marked even while payment pends"
        );
        assert_eq!(companies[1].turn_settled_sales, 2_000.0);

        // Fund the investor — the normal tranche sweep can now settle the
        // deferred milestones against the still-attached record.
        companies[0].available_cash = 20_000.0;
        let released =
            release_construction_tranches(&mut buildings, &mut companies, &mut country);
        assert_eq!(released, 3, "tranches 2-4 should now release");

        // Next advance pass sees all tranches settled and clears the record.
        advance_construction_projects(&mut buildings, &mut companies, &unit_costs, &mut country);
        assert!(
            buildings[0].active_project.is_none(),
            "once every tranche settles the receivable record must clear"
        );
        assert!(
            (companies[1].turn_settled_sales - 10_000.0).abs() < 1e-6,
            "contractor must eventually book the full contract, got {}",
            companies[1].turn_settled_sales
        );
    }
}

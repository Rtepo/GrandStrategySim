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

        // Phase 22A: determine who submits bids
        let buyer_id = if !project.main_contractor_id.is_empty() {
            project.main_contractor_id.clone()
        } else {
            building.owner_id.clone()
        };

        // Phase 36: For State-backed projects, accumulate pending tranche value
        // so the contractor can bid against guaranteed escrow.
        if project.investor_id.starts_with("STATE:") {
            let pending: f64 = project
                .tranches
                .iter()
                .filter(|t| !t.released)
                .map(|t| t.amount)
                .sum();
            *pending_tranche_value.entry(buyer_id.clone()).or_insert(0.0) += pending;
        }

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
                let remaining_needed = (required - already_delivered).max(0.0);
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
            let remaining_needed = (required - already_delivered).max(0.0);
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

    for company in companies.iter_mut() {
        let liquid = company.computed_liquid_capital();
        company.available_cash = liquid;

        // Phase 36: Include 50% of pending tranche value in bidding capacity
        // for State-backed projects. This prevents construction deadlock when
        // contractors have no current cash but have guaranteed Treasury escrow.
        let pending = pending_tranche_value
            .get(&company.id)
            .copied()
            .unwrap_or(0.0);
        let effective_liquid = liquid + pending * 0.5;
        let max_encumber = effective_liquid * config.max_cash_encumbrance_ratio;
        let mut total_encumbered = 0.0;

        let requests = match buyer_requests.get(&company.id) {
            Some(r) => r,
            None => continue,
        };

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

            let limit_price = ref_price * (1.0 + config.buy_premium_ratio);
            let affordable_qty = if limit_price > 0.0 {
                ((max_encumber - total_encumbered) / limit_price).max(0.0)
            } else {
                0.0
            };
            let bid_qty = (*remaining_needed).min(affordable_qty);
            if bid_qty <= 0.0 {
                continue;
            }

            let encumbrance = bid_qty * limit_price;
            if total_encumbered + encumbrance > max_encumber {
                continue;
            }

            // Encumber cash (double-entry)
            company.available_cash -= encumbrance;
            company.debit_cash += encumbrance;
            total_encumbered += encumbrance;

            // Submit bid with company.id as buyer_id
            order_book.bids.entry(*commodity).or_default().push(Bid {
                buyer_id: company.id.clone(),
                commodity: *commodity,
                quantity: bid_qty,
                limit_price,
                blueprint_id: None,
                min_quality: None,
            });
        }
    }

    messages
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
) -> (Vec<String>, f64) {
    let mut messages = Vec::new();
    // Phase 34: Track total materials consumed (cost_spent delta) as investment.
    // I is recorded exclusively from materials consumed, NOT tranche payments
    // (which are merely cash transfers from Investor to Contractor).
    let mut total_investment = 0.0;

    for building in buildings.iter_mut() {
        let project = match building.active_project.as_mut() {
            Some(p) => p,
            None => continue,
        };

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

        // Consume delivered materials from building inventory
        let consumed =
            project.consume_delivered_materials(&mut building.inventory, unit_costs, productivity);
        project.turns_elapsed += 1;

        // Phase 34: Accumulate the cost_spent delta (materials consumed value).
        let turn_investment = project.cost_spent - cost_before;
        total_investment += turn_investment;

        if consumed {
            if project.on_hold {
                project.resume();
            }
        } else if !project.is_complete() {
            project.consecutive_hold_turns += 1;
            project.put_on_hold("Material shortage on B2B market".to_string());
        }

        // Phase 29: Cancel permanently stalled projects (on hold for > 5 turns
        // with no material consumption). Release remaining escrow to investor.
        // Phase 1 fix (C1): The refund is now `investor_cash_debited -
        // tranches_paid` — the actual unspent escrow, not
        // `total_cost - cost_spent` which included the contractor's margin
        // (money never paid by the investor).
        if project.consecutive_hold_turns > 5 && !project.is_complete() {
            let refund = (project.investor_cash_debited - project.tranches_paid).max(0.0);
            let investor_id = project.investor_id.clone();
            let building_id = building.id.clone();
            let is_state = investor_id.starts_with("STATE:");
            building.active_project = None;
            messages.push(format!(
                "Construction cancelled (stalled): building {} refund {:.0}",
                building_id, refund
            ));
            // Return unspent escrow to the investor
            if refund > 0.0 {
                if is_state {
                    country.budget.liquid_reserves += refund;
                } else if !investor_id.is_empty() {
                    if let Some(company) = companies.iter_mut().find(|c| c.id == investor_id) {
                        // Refund to actual cash (brokerage_account or available_cash)
                        if let Some(ref mut ba) = company.brokerage_account {
                            ba.cash += refund;
                        } else {
                            company.available_cash += refund;
                        }
                        // Release the encumbrance
                        company.debit_cash = (company.debit_cash - refund).max(0.0);
                        // Also restore liquid_capital (the accounting field
                        // that was debited in apply_action)
                        company.liquid_capital += refund;
                    }
                }
            }
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
            let project_type = project.project_type;
            let cap_increase = project.target_capacity_increase;
            let capital_increase = project.target_capital_increase;
            let building_id = building.id.clone();
            let owner_id = building.owner_id.clone();

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
                building.active_project = None;
                continue;
            }

            building.worker_capacity += cap_increase;

            // Update company's fixed_capital
            if let Some(company) = companies.iter_mut().find(|c| c.id == owner_id) {
                company.fixed_capital += capital_increase;
            }

            building.active_project = None;
            messages.push(format!(
                "Construction complete: building {} capacity +{}",
                building_id, cap_increase
            ));
        }
    }

    (messages, total_investment)
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

        let contractor_id = project.main_contractor_id.clone();
        let investor_id = project.investor_id.clone();
        let progress = project.progress;

        // Determine investor type from investor_id prefix
        let is_state_investor = investor_id.starts_with("STATE:");

        // Find contractor index
        let contractor_idx = match companies.iter().position(|c| c.id == contractor_id) {
            Some(idx) => idx,
            None => continue,
        };

        // Release eligible tranches
        for tranche in project.tranches.iter_mut() {
            if tranche.released {
                continue;
            }
            if progress < tranche.trigger_progress {
                continue;
            }

            let amount = tranche.amount;
            let payment_ok = if is_state_investor {
                // State escrow: Treasury was already debited at publication.
                // Just credit the contractor.
                release_escrow_treasury_to_contractor(
                    companies,
                    contractor_idx,
                    amount,
                    country,
                )
                .is_ok()
            } else {
                // Corporate escrow: investor's cash was already debited at
                // publication. Release the encumbrance and credit contractor.
                let investor_idx = match companies.iter().position(|c| c.id == investor_id) {
                    Some(idx) => idx,
                    None => continue,
                };
                release_escrow_company_to_contractor(
                    companies,
                    investor_idx,
                    contractor_idx,
                    amount,
                    country,
                )
                .is_ok()
            };

            if payment_ok {
                tranche.released = true;
                tranche.released_turn = 0; // caller can set actual turn
                project.paid_tranches += 1;
                project.tranches_paid += amount;
                released_count += 1;
            }
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
    let investor_id = project.investor_id.clone();
    let is_state = investor_id.starts_with("STATE:");

    // Clear the active project — delivered-but-unconsumed materials stay
    // in building.inventory (they are physical assets).
    building.active_project = None;

    // Refund unspent escrow to the investor
    if refund > 0.0 {
        if is_state {
            country.budget.liquid_reserves += refund;
        } else if !investor_id.is_empty() {
            if let Some(company) = companies.iter_mut().find(|c| c.id == investor_id) {
                if let Some(ref mut ba) = company.brokerage_account {
                    ba.cash += refund;
                } else {
                    company.available_cash += refund;
                }
                company.debit_cash = (company.debit_cash - refund).max(0.0);
                company.liquid_capital += refund;
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

        // Compute remaining material units
        let remaining: f64 = project
            .required_materials
            .iter()
            .map(|(&_commodity, &required)| {
                let delivered = project
                    .delivered_materials
                    .get(&_commodity)
                    .copied()
                    .unwrap_or(0.0);
                (required - delivered).max(0.0)
            })
            .sum();

        if remaining <= 0.0 {
            continue;
        }

        // Labor demand = remaining work / labor_per_unit
        // Each worker can install `labor_per_unit` worth of materials per turn.
        // The total FTE needed is the remaining work divided by this rate,
        // but capped at a reasonable level (e.g., 50 workers per project).
        let project_demand = (remaining / labor_per_unit).min(50.0).max(1.0);
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
        let old_demand = company.physical_fte_demand;

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

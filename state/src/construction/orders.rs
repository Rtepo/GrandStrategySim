//! Construction B2B order submission and project advancement.
//!
//! This module integrates construction projects with the B2B OrderBook:
//! * `submit_construction_b2b_orders` — submits buy bids for missing materials.
//! * `advance_construction_projects` — consumes delivered materials and checks completion.
//! * `release_construction_tranches` — Phase 22A: pays contractor tranches on milestones.

use crate::economy::b2b_config::B2bOrderConfig;
use crate::economy::market_history::{get_reference_price, MarketHistory};
use crate::economy::order_book::{Bid, OrderBook};
use crate::economy::transfer_settler::{
    settle_company_to_company, settle_treasury_to_company,
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
            let pending: f64 = project.tranches.iter()
                .filter(|t| !t.released)
                .map(|t| t.amount)
                .sum();
            *pending_tranche_value.entry(buyer_id.clone()).or_insert(0.0) += pending;
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
            buyer_requests
                .entry(buyer_id.clone())
                .or_default()
                .push((building.id.clone(), commodity, remaining_needed));
        }
    }

    for company in companies.iter_mut() {
        let liquid = company.computed_liquid_capital();
        company.available_cash = liquid;

        // Phase 36: Include 50% of pending tranche value in bidding capacity
        // for State-backed projects. This prevents construction deadlock when
        // contractors have no current cash but have guaranteed Treasury escrow.
        let pending = pending_tranche_value.get(&company.id).copied().unwrap_or(0.0);
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
            order_book.bids
                .entry(*commodity)
                .or_default()
                .push(Bid {
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

        // Phase 34: Capture cost_spent before consumption to compute the delta.
        let cost_before = project.cost_spent;

        // Consume delivered materials from building inventory
        let consumed = project.consume_delivered_materials(&mut building.inventory, unit_costs);
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
        // with no material consumption). Release remaining capital to investor.
        if project.consecutive_hold_turns > 5 && !project.is_complete() {
            let refund = project.total_cost - project.cost_spent;
            let investor_id = project.investor_id.clone();
            let building_id = building.id.clone();
            building.active_project = None;
            messages.push(format!(
                "Construction cancelled (stalled): building {} refund {:.0}",
                building_id, refund
            ));
            // Return unspent capital to the investor
            if !investor_id.is_empty() {
                if let Some(company) = companies.iter_mut().find(|c| c.id == investor_id) {
                    company.liquid_capital += refund;
                }
            }
            continue;
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
            if project_type == crate::construction::projects::ConstructionProjectType::TransportNetwork {
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
/// contractor via `settle_company_to_company` (corporate investor) or
/// `settle_treasury_to_company` (state investor).
///
/// # Arguments
/// * `buildings` - Mutable slice of buildings (projects are read).
/// * `companies` - Mutable slice of companies (investor debited, contractor credited).
/// * `country` - Mutable country state (for Treasury payments).
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
                settle_treasury_to_company(companies, contractor_idx, amount, country).is_ok()
            } else {
                // Find investor index
                let investor_idx = match companies.iter().position(|c| c.id == investor_id) {
                    Some(idx) => idx,
                    None => continue,
                };
                settle_company_to_company(companies, investor_idx, contractor_idx, amount, country)
                    .is_ok()
            };

            if payment_ok {
                tranche.released = true;
                tranche.released_turn = 0; // caller can set actual turn
                project.paid_tranches += 1;
                project.cost_spent += amount;
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

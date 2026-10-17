//! Phase 22A: Construction tender market logic.
//!
//! Implements tender publication, bid submission, and tender award.
//! All cash encumbrance uses the existing `available_cash` / `debit_cash`
//! pattern. Tranche payments route through `TransferSettler`.

use crate::construction::bom::get_construction_bom;
use crate::construction::projects::{ConstructionProject, ConstructionProjectType};
use crate::construction::tenders::{
    default_tranches, Bid, ConstructionTender, TenderInvestorType, TenderStatus,
};
use crate::entities::Company;
use crate::registries::enums::Sector;
use crate::state::Country;
use rand::Rng;
use std::collections::BTreeMap;

/// Minimum reputation score to be eligible for tender awards (KIO blacklist).
pub const BLACKLIST_THRESHOLD: f64 = 20.0;

/// Dumping floor: bids with cost below this fraction of estimated_cost are rejected.
pub const DUMPING_FLOOR_RATIO: f64 = 0.5;

/// Publish a new construction tender.
///
/// # Arguments
/// * `investor_id` - Company ID or "STATE:{region_id}".
/// * `investor_type` - State or Corporation.
/// * `project_type` - Residential, Commercial, Factory, etc.
/// * `micro_region_id` - Region where construction occurs.
/// * `target_building_type` - Building name (for BOM lookup).
/// * `target_capacity_increase` - Worker capacity to add.
/// * `target_capital_increase` - Fixed capital to add.
/// * `estimated_cost` - Investor's budget ceiling.
/// * `deadline_turns` - Bidding window length.
/// * `current_turn` - Current turn number.
///
/// # Returns
/// A new `ConstructionTender` with status `Open`.
pub fn publish_tender(
    investor_id: String,
    investor_type: TenderInvestorType,
    project_type: ConstructionProjectType,
    micro_region_id: String,
    target_building_type: String,
    target_capacity_increase: u32,
    target_capital_increase: f64,
    estimated_cost: f64,
    deadline_turns: u32,
    current_turn: u32,
    sector: crate::registries::enums::Sector,
    start_year: u32,
) -> ConstructionTender {
    let required_materials = get_construction_bom(sector, start_year, target_capacity_increase);

    let tender_id = format!(
        "tender_{}_{}_{}",
        investor_id,
        target_building_type.replace(' ', "_"),
        current_turn
    );

    let tender_name = generate_tender_name(&project_type, current_turn);

    let tender = ConstructionTender {
        id: tender_id,
        tender_name,
        investor_id,
        investor_type,
        project_type,
        micro_region_id,
        target_building_type,
        required_materials,
        target_capacity_increase,
        target_capital_increase,
        estimated_cost,
        escrowed_amount: 0.0,
        deadline_turns,
        published_turn: current_turn,
        status: TenderStatus::Open,
        bids: Vec::new(),
        awarded_bid: None,
        expansion_target_building_id: None,
    };
    #[cfg(feature = "diagnostic")]
    eprintln!(
        "CONST_DBG[PUBLISH]: turn={} id={} investor={} type={:?} region={} est_cost={:.0} bom_items={} capacity=+{}",
        current_turn,
        tender.id,
        tender.investor_id,
        tender.project_type,
        tender.micro_region_id,
        tender.estimated_cost,
        tender.required_materials.len(),
        tender.target_capacity_increase
    );
    tender
}

/// v4.17 FS-7: share of the contract price escrowed at publication.
/// Progress financing — the investor commits the mobilization advance plus
/// a secured first-milestone margin (30%), and pays later tranches from its
/// ongoing cash at release (`release_construction_tranches`). Requiring the
/// full price up front capped every state contract at one period of
/// ministry budget (~$112k vs ~$2M real cost) because `ministry_cash` is a
/// per-period pocket that can never accumulate — a multi-period capital
/// project is funded across allocations, not escrowed whole.
pub const MOBILIZATION_ESCROW_RATIO: f64 = 0.30;

/// v4.17 FS-7: turns between milestone releases, used to size pay-cycle
/// working capital. Tranches default to 4-5 milestones over ~10-16 turn
/// builds, so a crew's wages must be covered ~4 turns at a time, not one
/// turn. Shared by the bid payroll reserve, the labor-demand capacity cap,
/// and the bonding gate so all three agree on what "affordable" means.
pub const MILESTONE_SPACING_TURNS: f64 = 4.0;

/// v4.17 FS-7: bonding headroom beyond payroll capacity, in FTE. Surety
/// practice lets a contractor carry backlog beyond liquid working capital
/// — each project's mobilization advance partly funds its own crew — so a
/// company is not shut out of its next contract just because its current
/// backlog is at payroll capacity. Sized at ~1.5 small sites.
pub const MOBILIZATION_HEADROOM_FTE: f64 = 300.0;

/// v4.17 FS-7: surety backlog cap relative to the crew the contractor has
/// actually fielded. The cash-based term alone is circular — every award
/// adds its unreleased tranches to `contract_receivables`, so the winner's
/// capacity grows with the very backlog being gated (observed: one bidder
/// holding ~$570M of concurrent work, demanding 3,854 FTE while hiring
/// friction let it field ~1,500 — sites stalled and cancelled while the
/// rest of the sector sat idle). Real sureties underwrite backlog against
/// demonstrated organization size: a firm cannot bond work it could only
/// crew by hiring several multiples of its entire workforce inside one
/// build schedule. Furloughed workers count — they are a recallable bench,
/// not capacity the firm never had.
pub const BACKLOG_CREW_MULTIPLE: f64 = 3.0;

/// v4.17 FS-7: payroll-funded bonding capacity (FTE) for a contractor —
/// cash on hand plus half the contract receivables (the 50% haircut the
/// bid and labor-affordability paths both apply), minus supplier credit
/// already committed to materials, spread over the pay cycle until the
/// next milestone lands, then capped by demonstrated crew capacity.
/// Shared by `construction_bid_decision` (bid gate) and `award_tender`
/// (award gate) so both enforce the same surety rule.
pub fn bonding_capacity_fte(company: &Company) -> f64 {
    let wage = company.offered_wage_per_fte.max(1.0);
    let receivables = company
        .extra
        .get("contract_receivables")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
        .max(0.0);
    let payroll_capacity = (company.operational_cash()
        + receivables * 0.5
        - company.procurement_credit)
        .max(0.0)
        / (wage * MILESTONE_SPACING_TURNS)
        + MOBILIZATION_HEADROOM_FTE;
    let demonstrated = (company.fulfilled_fte as f64
        + company.furloughed_workers_count.max(0.0))
        * BACKLOG_CREW_MULTIPLE
        + MOBILIZATION_HEADROOM_FTE;
    payroll_capacity.min(demonstrated)
}

/// v4.17 FS-7: the contract's implied FTE demand, mirroring
/// `compute_construction_labor_demand` — remaining BOM value priced at the
/// economy's numéraire, spread over the scheduled duration, capped per
/// site. Used by `award_tender` to charge each award against the winner's
/// bonding capacity.
pub fn tender_labor_fte(tender: &ConstructionTender, wage: f64) -> f64 {
    // Same `.max(average_wage)` floor `update_construction_labor_demand`
    // applies — a bidder with no posted wage (idle/new entrant) would
    // otherwise divide by ~1 and read every tender as a mega-crew.
    let labor_per_unit = 10.0 * wage.max(1.0);
    let remaining_value: f64 = tender
        .required_materials
        .iter()
        .map(|(&commodity, &qty)| {
            qty * crate::engine::generator::corporate::anchored_base_price(commodity)
                .max(1.0)
        })
        .sum();
    let schedule =
        crate::construction::projects::project_type_duration_turns(tender.project_type).max(1)
            as f64;
    (remaining_value / (labor_per_unit * schedule))
        .min(200.0)
        .max(1.0)
}

/// Phase 1 fix (C3): Encumber `estimated_cost` from the investor's cash
/// at tender publication time. This creates a real escrow that ensures
/// the investor has the funds to pay the contractor.
///
/// For Corporate investors: debits `brokerage_account.cash` (or
/// `available_cash`), credits `debit_cash`.
/// For State investors: debits `country.budget.liquid_reserves`.
///
/// v4.17: the encumbered amount is `estimated_cost ×
/// [`MOBILIZATION_ESCROW_RATIO`] — the secured advance. Unsecured
/// tranches are progress payments drawn on the investor's cash at release.
///
/// # Arguments
/// * `tender` - The tender to encumber funds for (mutated: `escrowed_amount` set).
/// * `companies` - Mutable slice of companies (for Corporate investor lookup).
/// * `country` - Mutable country state (for State Treasury debit).
///
/// # Returns
/// `true` if escrow was successful, `false` if insufficient funds.
pub fn encumber_tender_escrow(
    tender: &mut ConstructionTender,
    companies: &mut [Company],
    country: &mut Country,
) -> bool {
    let cost = tender.estimated_cost * MOBILIZATION_ESCROW_RATIO;
    if cost <= 0.0 {
        return true; // Nothing to escrow
    }

    let is_state = tender.investor_id.starts_with("STATE:");

    if is_state {
        // State escrow: debit Treasury reserves
        if country.budget.liquid_reserves < cost {
            #[cfg(feature = "diagnostic")]
            eprintln!(
                "CONST_DBG[ESCROW]: tender={} investor={} cost={:.0} funded=false reason=treasury_shortfall reserves={:.0}",
                tender.id, tender.investor_id, cost, country.budget.liquid_reserves
            );
            return false;
        }
        country.budget.liquid_reserves -= cost;
        tender.escrowed_amount = cost;
        #[cfg(feature = "diagnostic")]
        eprintln!(
            "CONST_DBG[ESCROW]: tender={} investor={} cost={:.0} funded=true source=treasury",
            tender.id, tender.investor_id, cost
        );
        true
    } else {
        // Corporate escrow: debit company cash, credit debit_cash
        let investor_idx = match companies.iter().position(|c| c.id == tender.investor_id) {
            Some(idx) => idx,
            None => {
                #[cfg(feature = "diagnostic")]
                eprintln!(
                    "CONST_DBG[ESCROW]: tender={} investor={} cost={:.0} funded=false reason=investor_not_found",
                    tender.id, tender.investor_id, cost
                );
                return false;
            }
        };

        let available = if let Some(ref ba) = companies[investor_idx].brokerage_account {
            ba.cash
        } else {
            companies[investor_idx].available_cash
        };

        if available < cost {
            #[cfg(feature = "diagnostic")]
            eprintln!(
                "CONST_DBG[ESCROW]: tender={} investor={} cost={:.0} funded=false reason=insufficient_cash available={:.0}",
                tender.id, tender.investor_id, cost, available
            );
            return false;
        }

        if let Some(ref mut ba) = companies[investor_idx].brokerage_account {
            ba.cash -= cost;
        } else {
            companies[investor_idx].available_cash -= cost;
        }
        companies[investor_idx].debit_cash += cost;
        tender.escrowed_amount = cost;
        #[cfg(feature = "diagnostic")]
        eprintln!(
            "CONST_DBG[ESCROW]: tender={} investor={} cost={:.0} funded=true source=corporate",
            tender.id, tender.investor_id, cost
        );
        true
    }
}

/// Phase 1 fix (C3): Refund the escrowed amount to the investor when a
/// tender is cancelled (no eligible bids) or abandoned.
///
/// For Corporate investors: credits `brokerage_account.cash` (or
/// `available_cash`), debits `debit_cash`.
/// For State investors: credits `country.budget.liquid_reserves`.
///
/// # Arguments
/// * `tender` - The cancelled tender (reads `escrowed_amount`).
/// * `companies` - Mutable slice of companies (for Corporate investor refund).
/// * `country` - Mutable country state (for State Treasury refund).
pub fn refund_tender_escrow(
    tender: &ConstructionTender,
    companies: &mut [Company],
    country: &mut Country,
) {
    // Expansion tenders only: `apply_action` debited the investor's
    // `liquid_capital` by the committed investment up front; the manager
    // then restored the unfunded share, leaving `estimated_cost` committed
    // against this tender. On cancellation the commitment is void — restore
    // the accounting capital so the firm's capital book isn't permanently
    // shrunk by a tender that never produced a project. Under progress
    // financing `escrowed_amount` is only the mobilization share (30%) —
    // restoring it alone would leak the other ~70% of the committed
    // capital (v4.17). State tenders and developer tenders do not debit
    // liquid_capital, so they are untouched.
    if tender.expansion_target_building_id.is_some()
        && !tender.investor_id.starts_with("STATE:")
    {
        if let Some(idx) = companies.iter().position(|c| c.id == tender.investor_id) {
            companies[idx].liquid_capital += tender.estimated_cost;
        }
    }

    let refund = tender.escrowed_amount;
    if refund <= 0.0 {
        return;
    }

    let is_state = tender.investor_id.starts_with("STATE:");

    if is_state {
        country.budget.liquid_reserves += refund;
    } else {
        if let Some(investor_idx) = companies.iter().position(|c| c.id == tender.investor_id) {
            if let Some(ref mut ba) = companies[investor_idx].brokerage_account {
                ba.cash += refund;
            } else {
                companies[investor_idx].available_cash += refund;
            }
            companies[investor_idx].debit_cash =
                (companies[investor_idx].debit_cash - refund).max(0.0);
        }
    }
}

/// Phase 41: Global monotonic tender counter for unique names.
static TENDER_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

/// Phase 40/41: Generate a human-readable tender name from the project type
/// and a monotonic sequence number.
///
/// Phase 41 fix: The old code used `current_turn % 26` which produced duplicate
/// names when multiple tenders were published on the same turn. Now uses a
/// global `AtomicU32` counter that increments on every call, guaranteeing
/// uniqueness across the entire simulation.
pub fn generate_tender_name(project_type: &ConstructionProjectType, current_turn: u32) -> String {
    let seq_num = TENDER_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let year_label = format!("Y{}", current_turn);
    match project_type {
        ConstructionProjectType::Residential => {
            format!("Housing Estate #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::Commercial => {
            format!("Commercial Plaza #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::UtilityNetwork => {
            format!("Utility Network #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::Infrastructure => {
            format!("Infrastructure Project #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::SocialHousing => {
            format!("Social Housing #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::Factory => {
            format!("Industrial Park #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::TransportNetwork => {
            format!("Transport Network #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::Court => {
            format!("Regional Courthouse #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::CustomsOffice => {
            format!("Customs Office #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::Embassy => {
            format!("Embassy Complex #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::ResearchInstitute => {
            format!("Research Institute #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::LaborInspectorate => {
            format!("Labor Inspectorate #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::PublicWorksSite => {
            format!("Public Works Site #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::NationalTheater => {
            format!("National Theater #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::NationalLibrary => {
            format!("National Library #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::TransportDepot => {
            format!("Transport Depot #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::ThermalGridPipe => {
            format!("Thermal Grid Pipe #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::ThermalHeatingPlant => {
            format!("Heating Plant #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::CHPRetrofit => {
            format!("CHP Retrofit #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::WaterMainPipe => {
            format!("Water Main Pipe #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::SewerMainPipe => {
            format!("Sewer Main Pipe #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::WaterTreatmentPlant => {
            format!("Water Treatment Plant #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::WastewaterTreatmentPlant => {
            format!("Wastewater Treatment Plant #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::Landfill => format!("Landfill #{} ({})", seq_num, year_label),
        ConstructionProjectType::WasteSeparationPlant => {
            format!("Waste Separation Plant #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::RecyclingFacility => {
            format!("Recycling Facility #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::WasteToEnergyPlant => {
            format!("Waste-to-Energy Plant #{} ({})", seq_num, year_label)
        }
        ConstructionProjectType::CivicAmenitySite => {
            format!("Civic Amenity Site (PSZOK) #{} ({})", seq_num, year_label)
        }
    }
}

/// Phase 29: Publish a tender for expanding an existing building.
///
/// Same as `publish_tender` but sets `expansion_target_building_id` so the
/// awarded project is attached to the specific building being expanded.
pub fn publish_expansion_tender(
    investor_id: String,
    investor_type: TenderInvestorType,
    project_type: ConstructionProjectType,
    micro_region_id: String,
    target_building_type: String,
    target_capacity_increase: u32,
    target_capital_increase: f64,
    estimated_cost: f64,
    deadline_turns: u32,
    current_turn: u32,
    expansion_target_building_id: String,
    sector: crate::registries::enums::Sector,
    start_year: u32,
) -> ConstructionTender {
    let mut tender = publish_tender(
        investor_id,
        investor_type,
        project_type,
        micro_region_id,
        target_building_type,
        target_capacity_increase,
        target_capital_increase,
        estimated_cost,
        deadline_turns,
        current_turn,
        sector,
        start_year,
    );
    tender.expansion_target_building_id = Some(expansion_target_building_id);
    tender
}

/// Submit a bid on a tender.
///
/// # Arguments
/// * `tender` - The tender to bid on (mutated: bid is appended).
/// * `bidder` - The construction company submitting the bid.
/// * `bid_cost` - Contractor's cost estimate.
/// * `bid_margin` - Profit margin (0.0–1.0 of cost).
/// * `consortium_members` - Subcontractor IDs (empty if solo).
/// * `current_turn` - Current turn number.
///
/// # Returns
/// `Ok(())` if the bid was accepted, `Err(reason)` if rejected.
///
/// # Rules
/// * Only `Sector::Construction` companies can bid.
/// * Bids below the dumping floor are rejected.
/// * Blacklisted companies (reputation < threshold) can submit bids but
///   will be excluded from award.
pub fn submit_bid(
    tender: &mut ConstructionTender,
    bidder: &Company,
    bid_cost: f64,
    bid_margin: f64,
    consortium_members: Vec<String>,
    current_turn: u32,
) -> Result<(), String> {
    if bidder.sector != Sector::Construction {
        return Err("Only construction companies can bid on tenders".to_string());
    }

    let dumping_floor = tender.estimated_cost * DUMPING_FLOOR_RATIO;
    if bid_cost < dumping_floor {
        return Err(format!(
            "Bid cost {} below dumping floor {}",
            bid_cost, dumping_floor
        ));
    }

    let bid_price = bid_cost * (1.0 + bid_margin);
    let is_consortium = !consortium_members.is_empty();
    let bidder_rep = bidder_extra_reputation(bidder);

    let bid = Bid {
        id: format!("bid_{}_{}", bidder.id, current_turn),
        tender_id: tender.id.clone(),
        bidder_id: bidder.id.clone(),
        bid_cost,
        bid_margin,
        bid_price,
        is_consortium,
        consortium_members,
        submitted_turn: current_turn,
        reputation_score: bidder_rep,
    };

    tender.bids.push(bid);
    #[cfg(feature = "diagnostic")]
    eprintln!(
        "CONST_DBG[BID]: turn={} tender={} bidder={} cost={:.0} margin={:.3} price={:.0} rep={:.0} bids_total={}",
        current_turn, tender.id, bidder.id, bid_cost, bid_margin, bid_price, bidder_rep,
        tender.bids.len()
    );
    Ok(())
}

/// Award a tender to the best eligible bid.
///
/// # Rules
/// * Excludes blacklisted bidders (reputation < `BLACKLIST_THRESHOLD`).
/// * Selects the lowest `bid_price` among eligible bidders.
/// * Creates a `ConstructionProject` with contractor linkage and tranches.
///
/// # Returns
/// `Some(ConstructionProject)` if a winner was found, `None` if no eligible bids.
pub fn award_tender(
    tender: &mut ConstructionTender,
    current_turn: u32,
    companies: &[Company],
    newly_committed_fte: &mut rustc_hash::FxHashMap<String, f64>,
    average_wage: f64,
) -> Option<ConstructionProject> {
    if tender.status != TenderStatus::Open {
        return None;
    }

    // A tender whose escrow never funded can never pay a tranche —
    // `release_escrow_*` clamps to the encumbered balance, which is 0.
    // Awarding it would create a zombie project that starves the contractor
    // and dies stalled after 5 hold turns. Treat it as unawardable so the
    // caller cancels (and refunds) it instead.
    if tender.estimated_cost > 0.0 && tender.escrowed_amount <= 0.0 {
        return None;
    }

    // Filter eligible bids (above blacklist threshold)
    let eligible: Vec<&Bid> = tender
        .bids
        .iter()
        .filter(|b| b.reputation_score >= BLACKLIST_THRESHOLD)
        .collect();

    if eligible.is_empty() {
        return None;
    }
    let eligible_count = eligible.len();

    // v4.17 FS-7: award-time bonding gate. `construction_bid_decision`
    // gates the BID, but every same-turn tender awards against the same
    // stale `committed_demand_fte` — one persistent low bidder was taking
    // ~8 contracts in a single turn (observed: NEW_Dacia_1928_78_3), far
    // beyond what it could crew or supply; the sites stalled at progress
    // 0.0 and mass-cancelled, dragging the company into a
    // distressed→merged cycle that depopulated the sector. Here the award
    // walks the eligible list lowest-price-first and takes the first
    // bidder whose committed FTE — including awards already made this
    // pass — fits inside its bonding capacity. Real surety behavior: a
    // contractor can't be awarded work its bonding line can't cover, so
    // work spreads across the sector instead of piling onto the cheapest
    // name.
    let mut candidates: Vec<&&Bid> = eligible.iter().collect();
    candidates.sort_by(|a, b| {
        a.bid_price
            .partial_cmp(&b.bid_price)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut winner: Option<&Bid> = None;
    let mut winner_fte = 0.0_f64;
    for cand in candidates {
        let Some(co) = companies.iter().find(|c| c.id == cand.bidder_id) else {
            continue;
        };
        let wage = co.offered_wage_per_fte.max(average_wage).max(1.0);
        let tender_fte = tender_labor_fte(tender, wage);
        let committed = co
            .extra
            .get("committed_demand_fte")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
            .max(0.0)
            + newly_committed_fte
                .get(&cand.bidder_id)
                .copied()
                .unwrap_or(0.0);
        if committed + tender_fte <= bonding_capacity_fte(co) {
            winner = Some(*cand);
            winner_fte = tender_fte;
            break;
        }
    }
    let winner = winner?;
    *newly_committed_fte
        .entry(winner.bidder_id.clone())
        .or_insert(0.0) += winner_fte;

    let winner_bid_id = winner.id.clone();
    let contractor_id = winner.bidder_id.clone();
    let contract_price = winner.bid_price;
    let contractor_margin = winner.bid_margin;

    // NOTE: the first tranche (trigger_progress 0.0) is the mobilization
    // advance — it is paid by `release_construction_tranches` on the next
    // tranche pass (same turn as the award, post-progress stage), which
    // performs the real escrow→contractor transfer. The old code pre-marked
    // it `released` here WITHOUT moving cash: the release pass then skipped
    // it forever, so the mobilization never arrived and the tranche was also
    // excluded from the contractor's pending-tranche bidding capacity.
    let tranches = default_tranches(contract_price);

    let project = ConstructionProject {
        id: format!("proj_{}", tender.id),
        project_type: tender.project_type,
        micro_region_id: tender.micro_region_id.clone(),
        target_building_type: tender.target_building_type.clone(),
        required_materials: tender.required_materials.clone(),
        delivered_materials: BTreeMap::new(),
        target_capacity_increase: tender.target_capacity_increase,
        target_capital_increase: tender.target_capital_increase,
        is_new_building: tender.expansion_target_building_id.is_none(),
        total_cost: contract_price,
        cost_spent: 0.0,
        investor_cash_debited: tender.escrowed_amount,
        tranches_paid: 0.0,
        investor_capital_committed: if tender.expansion_target_building_id.is_some()
        {
            tender.estimated_cost
        } else {
            0.0
        },
        duration_turns:
            crate::construction::projects::project_type_duration_turns(
                tender.project_type,
            ),
        turns_elapsed: 0,
        progress: 0.0,
        on_hold: false,
        consecutive_hold_turns: 0,
        work_complete: false,
        hold_reason: None,
        investor_id: tender.investor_id.clone(),
        main_contractor_id: contractor_id.clone(),
        subcontractors: Vec::new(),
        tranches,
        paid_tranches: 0,
        contract_price,
        contractor_margin,
        structural_defect: 0.0,
        ohs_health_required: 0.0,
        ohs_education_required: 0.0,
        ohs_health_delivered: 0.0,
        ohs_education_delivered: 0.0,
        ohs_coverage_ratio: 1.0,
        ohs_accidents: 0,
        network_link_target: None,
        network_target_level: None,
        weather_productivity: 1.0,
        disaster_material_loss: 0.0,
        parcel_id: String::new(),
    };

    tender.status = TenderStatus::Awarded;
    tender.awarded_bid = Some(winner_bid_id);

    #[cfg(feature = "diagnostic")]
    eprintln!(
        "CONST_DBG[AWARD]: turn={} tender={} contractor={} price={:.0} bids={} eligible={} escrowed={:.0}",
        current_turn,
        tender.id,
        contractor_id,
        contract_price,
        tender.bids.len(),
        eligible_count,
        tender.escrowed_amount
    );
    Some(project)
}

/// Check if a tender's bidding window has expired.
pub fn is_tender_expired(tender: &ConstructionTender, current_turn: u32) -> bool {
    tender.status == TenderStatus::Open
        && current_turn >= tender.published_turn + tender.deadline_turns
}

/// Read a company's reputation score (Phase 22D).
///
/// The authoritative source is `company.reputation_score` — the field
/// written by KIO blacklist rulings, civil lawsuits, and construction-fraud
/// compensation. The previous implementation read `extra["reputation_score"]`,
/// a key that is never populated anywhere in the codebase, so every bidder
/// silently scored the 50.0 fallback and blacklisting never applied.
fn bidder_extra_reputation(company: &Company) -> f64 {
    company.reputation_score
}

/// Process the tender market for one turn: award expired tenders.
///
/// # Arguments
/// * `tenders` - All active tenders (mutated: expired ones are awarded).
/// * `current_turn` - Current turn number.
///
/// # Returns
/// Tuple of:
/// * Vector of (tender_id, project, expansion_target_building_id) tuples for
///   newly awarded tenders.
/// * Vector of cancelled tenders (no eligible bids) — caller must refund
///   their escrow via `refund_tender_escrow`.
pub fn process_tender_awards(
    tenders: &mut Vec<ConstructionTender>,
    current_turn: u32,
    companies: &[Company],
    average_wage: f64,
) -> (
    Vec<(String, ConstructionProject, Option<String>)>,
    Vec<ConstructionTender>,
) {
    let mut awarded = Vec::new();
    let mut cancelled = Vec::new();

    // v4.17 FS-7: FTE committed by awards already made this pass — the
    // same-turn backlog the bid-time gate cannot see.
    let mut newly_committed_fte: rustc_hash::FxHashMap<String, f64> =
        rustc_hash::FxHashMap::default();

    let mut to_remove = Vec::new();
    for (idx, tender) in tenders.iter_mut().enumerate() {
        // Phase 40: Immediate award if the tender has ≥3 eligible bids,
        // regardless of deadline. This prevents tenders from sitting open
        // for many turns when there's already enough competition.
        let eligible_bids = tender
            .bids
            .iter()
            .filter(|b| b.reputation_score >= BLACKLIST_THRESHOLD)
            .count();
        let should_award = is_tender_expired(tender, current_turn)
            || (tender.status == TenderStatus::Open && eligible_bids >= 3);

        if !should_award {
            continue;
        }
        let expansion_target = tender.expansion_target_building_id.clone();
        if let Some(project) = award_tender(
            tender,
            current_turn,
            companies,
            &mut newly_committed_fte,
            average_wage,
        ) {
            awarded.push((tender.id.clone(), project, expansion_target));
        } else {
            // No eligible bids — cancel
            tender.status = TenderStatus::Cancelled;
            #[cfg(feature = "diagnostic")]
            eprintln!(
                "CONST_DBG[CANCEL]: turn={} tender={} investor={} bids={} eligible={} escrowed={:.0}",
                current_turn,
                tender.id,
                tender.investor_id,
                tender.bids.len(),
                eligible_bids,
                tender.escrowed_amount
            );
        }
        to_remove.push(idx);
    }

    // Remove awarded/cancelled tenders from the active list
    for idx in to_remove.iter().rev() {
        let removed = tenders.remove(*idx);
        if removed.status == TenderStatus::Cancelled {
            cancelled.push(removed);
        }
    }

    (awarded, cancelled)
}

/// AI helper: a construction company decides whether to bid on a tender
/// and at what margin.
///
/// # Rules
/// * Companies with low reputation bid more aggressively (lower margin).
/// * Companies with high `safety_level` bid higher cost (better OHS).
/// * Returns `None` if the company decides not to bid.
pub fn construction_bid_decision(
    company: &Company,
    tender: &ConstructionTender,
    _rng: &mut impl Rng,
    market_history: &crate::economy::market::market_history::MarketHistory,
) -> Option<(f64, f64, Vec<String>)> {
    if company.sector != Sector::Construction {
        return None;
    }

    let reputation = bidder_extra_reputation(company);

    // Blacklisted companies don't bid
    if reputation < BLACKLIST_THRESHOLD {
        return None;
    }

    // v4.17 FS-7: bonding capacity. `award_tender` picks the lowest bid,
    // and the deterministic jitter below makes the same few companies the
    // persistent low bidders — so without a backlog check one contractor
    // accumulates unlimited projects (observed: demand 5,770 FTE against a
    // payroll it could fund for ~500 → arrears → mass furlough → stall →
    // cancellation). Surety practice lets a contractor carry backlog beyond
    // liquid working capital — each project's mobilization advance partly
    // funds its own crew — but not unboundedly: committed FTE beyond
    // payroll capacity + ~1.5 sites of headroom stops the bid, spreading
    // work across the sector.
    let committed_fte = company
        .extra
        .get("committed_demand_fte")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
        .max(0.0);
    if committed_fte > bonding_capacity_fte(company) {
        return None;
    }

    // Estimate cost: base on tender's estimated_cost, adjusted for safety level
    let safety_factor = 1.0 - company.safety_level * 0.1; // safer = higher cost
    // Deterministic per-(bidder, tender) aggressiveness jitter in [0.9, 1.1).
    // The old `rng.gen::<f64>()` factor drew from a seeded stream shared
    // across tenders inside `par_iter_mut`, so award outcomes depended on
    // rayon scheduling — the same world produced different winners. A
    // DefaultHasher (fixed-key SipHash) yields a stable jitter from
    // identity alone and consumes no RNG state.
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    company.id.hash(&mut hasher);
    tender.id.hash(&mut hasher);
    let jitter = (hasher.finish() % 200) as f64 / 1000.0; // [0.0, 0.2)
    let mut bid_cost = tender.estimated_cost * (0.8 + safety_factor * 0.2) * (0.9 + jitter);
    // v4.17 MACRO-3: cost floor at CURRENT market procurement prices.
    // `estimated_cost` is the publish-time anchor estimate; when materials
    // trade above it (scarcity-priced inputs), a contractor that bids at
    // the anchor estimate buys inputs at 2-3× contract value and books
    // structural losses on every project (observed: −$2.5B windowed P&L
    // against $12M revenue). A real bidder prices its cost estimate at
    // what the BOM actually costs today — reference chain is VWAP → last
    // trade → base, the same prices procurement settles at.
    let market_bom_cost: f64 = tender
        .required_materials
        .iter()
        .map(|(c, q)| {
            *q * crate::economy::market::market_history::get_reference_price(
                c,
                market_history,
            )
            .unwrap_or(0.0)
        })
        .sum();
    if market_bom_cost > 0.0 {
        let market_cost = market_bom_cost * (0.8 + safety_factor * 0.2);
        bid_cost = bid_cost.max(market_cost);
    }

    // Margin: lower for low-reputation companies (desperate for work)
    let base_margin = 0.15;
    let reputation_adjustment = (reputation - 50.0) / 500.0; // ±10% around base
    let bid_margin = (base_margin + reputation_adjustment).max(0.05).min(0.50);

    // Solo bid (consortium formation is a future enhancement)
    Some((bid_cost, bid_margin, Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::legal_form::LegalForm;
    use crate::entities::Company;
    use crate::registries::enums::Sector;

    fn make_construction_company(id: &str) -> Company {
        Company::new(
            id.to_string(),
            "Test Construction".to_string(),
            Sector::Construction,
            LegalForm::JointStockCompany(Default::default()),
            100_000.0,
            50_000.0,
            100,
        )
    }

    #[test]
    fn test_publish_tender() {
        let tender = publish_tender(
            "STATE:r1".to_string(),
            TenderInvestorType::State,
            ConstructionProjectType::Factory,
            "r1".to_string(),
            "Cement Plant".to_string(),
            50,
            1_000_000.0,
            500_000.0,
            3,
            10,
            crate::registries::enums::Sector::HeavyIndustry,
            1900,
        );
        assert_eq!(tender.status, TenderStatus::Open);
        assert!(!tender.required_materials.is_empty());
    }

    #[test]
    fn test_submit_bid_rejects_non_construction() {
        let mut tender = publish_tender(
            "STATE:r1".to_string(),
            TenderInvestorType::State,
            ConstructionProjectType::Factory,
            "r1".to_string(),
            "Cement Plant".to_string(),
            50,
            1_000_000.0,
            500_000.0,
            3,
            10,
            crate::registries::enums::Sector::HeavyIndustry,
            1900,
        );
        let mut company = make_construction_company("c1");
        company.sector = Sector::Agriculture;
        let result = submit_bid(&mut tender, &company, 400_000.0, 0.1, Vec::new(), 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_submit_bid_rejects_dumping() {
        let mut tender = publish_tender(
            "STATE:r1".to_string(),
            TenderInvestorType::State,
            ConstructionProjectType::Factory,
            "r1".to_string(),
            "Cement Plant".to_string(),
            50,
            1_000_000.0,
            500_000.0,
            3,
            10,
            crate::registries::enums::Sector::HeavyIndustry,
            1900,
        );
        let company = make_construction_company("c1");
        // Cost 100k when estimated is 500k → below 50% floor
        let result = submit_bid(&mut tender, &company, 100_000.0, 0.1, Vec::new(), 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_award_tender_selects_lowest() {
        let mut tender = publish_tender(
            "STATE:r1".to_string(),
            TenderInvestorType::State,
            ConstructionProjectType::Factory,
            "r1".to_string(),
            "Cement Plant".to_string(),
            50,
            1_000_000.0,
            500_000.0,
            3,
            10,
            crate::registries::enums::Sector::HeavyIndustry,
            1900,
        );
        // Model the real pipeline: the escrow encumbrance runs before award
        // (turn.rs), so a fundable tender carries escrowed_amount > 0. An
        // unfunded tender is now unawardable (zombie-project guard).
        tender.escrowed_amount = tender.estimated_cost;
        let c1 = make_construction_company("c1");
        let c2 = make_construction_company("c2");
        submit_bid(&mut tender, &c1, 400_000.0, 0.15, Vec::new(), 10).unwrap();
        submit_bid(&mut tender, &c2, 350_000.0, 0.10, Vec::new(), 10).unwrap();

        let cos = vec![c1.clone(), c2.clone()];
        let mut committed = rustc_hash::FxHashMap::default();
        let project =
            award_tender(&mut tender, 13, &cos, &mut committed, 100.0).unwrap();
        assert_eq!(tender.status, TenderStatus::Awarded);
        // c2 has lower bid_price (350k * 1.10 = 385k vs 400k * 1.15 = 460k)
        assert_eq!(project.main_contractor_id, "c2");
        assert!(!project.tranches.is_empty());
    }
}

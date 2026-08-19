//! Phase 22A: Construction tender market logic.
//!
//! Implements tender publication, bid submission, and tender award.
//! All cash encumbrance uses the existing `available_cash` / `debit_cash`
//! pattern. Tranche payments route through `TransferSettler`.

use crate::construction::projects::{ConstructionProject, ConstructionProjectType};
use crate::construction::tenders::{
    default_tranches, Bid, ConstructionTender, TenderInvestorType, TenderStatus,
};
use crate::construction::bom::get_construction_bom;
use crate::entities::Company;
use crate::registries::enums::Sector;
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
    let required_materials =
        get_construction_bom(sector, start_year);

    let tender_id = format!(
        "tender_{}_{}_{}",
        investor_id,
        target_building_type.replace(' ', "_"),
        current_turn
    );

    let tender_name = generate_tender_name(&project_type, current_turn);

    ConstructionTender {
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
        deadline_turns,
        published_turn: current_turn,
        status: TenderStatus::Open,
        bids: Vec::new(),
        awarded_bid: None,
        expansion_target_building_id: None,
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
        ConstructionProjectType::Residential => format!("Housing Estate #{} ({})", seq_num, year_label),
        ConstructionProjectType::Commercial => format!("Commercial Plaza #{} ({})", seq_num, year_label),
        ConstructionProjectType::UtilityNetwork => format!("Utility Network #{} ({})", seq_num, year_label),
        ConstructionProjectType::Infrastructure => format!("Infrastructure Project #{} ({})", seq_num, year_label),
        ConstructionProjectType::SocialHousing => format!("Social Housing #{} ({})", seq_num, year_label),
        ConstructionProjectType::Factory => format!("Industrial Park #{} ({})", seq_num, year_label),
        ConstructionProjectType::TransportNetwork => format!("Transport Network #{} ({})", seq_num, year_label),
        ConstructionProjectType::Court => format!("Regional Courthouse #{} ({})", seq_num, year_label),
        ConstructionProjectType::CustomsOffice => format!("Customs Office #{} ({})", seq_num, year_label),
        ConstructionProjectType::Embassy => format!("Embassy Complex #{} ({})", seq_num, year_label),
        ConstructionProjectType::ResearchInstitute => format!("Research Institute #{} ({})", seq_num, year_label),
        ConstructionProjectType::LaborInspectorate => format!("Labor Inspectorate #{} ({})", seq_num, year_label),
        ConstructionProjectType::PublicWorksSite => format!("Public Works Site #{} ({})", seq_num, year_label),
        ConstructionProjectType::NationalTheater => format!("National Theater #{} ({})", seq_num, year_label),
        ConstructionProjectType::NationalLibrary => format!("National Library #{} ({})", seq_num, year_label),
        ConstructionProjectType::TransportDepot => format!("Transport Depot #{} ({})", seq_num, year_label),
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
        reputation_score: bidder_extra_reputation(bidder),
    };

    tender.bids.push(bid);
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
) -> Option<ConstructionProject> {
    if tender.status != TenderStatus::Open {
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

    // Select lowest bid_price
    let winner = eligible
        .iter()
        .min_by(|a, b| {
            a.bid_price
                .partial_cmp(&b.bid_price)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()?;

    let winner_bid_id = winner.id.clone();
    let contractor_id = winner.bidder_id.clone();
    let contract_price = winner.bid_price;
    let contractor_margin = winner.bid_margin;

    let mut tranches = default_tranches(contract_price);

    // Phase 36: State-backed projects receive a mobilization advance at award time.
    // The first tranche (trigger_progress 0.0) is released immediately so the
    // contractor has cash to bid for materials. Without this, the contractor
    // has no cash and cannot bid, deadlocking the project.
    let is_state_backed = tender.investor_id.starts_with("STATE:");
    if is_state_backed {
        if let Some(first_tranche) = tranches.first_mut() {
            first_tranche.released = true;
            first_tranche.released_turn = current_turn;
        }
    }

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
        duration_turns: 0,
        turns_elapsed: 0,
        progress: 0.0,
        on_hold: false,
        consecutive_hold_turns: 0,
        hold_reason: None,
        investor_id: tender.investor_id.clone(),
        main_contractor_id: contractor_id,
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
    };

    tender.status = TenderStatus::Awarded;
    tender.awarded_bid = Some(winner_bid_id);

    let _ = current_turn; // reserved for future logging
    Some(project)
}

/// Check if a tender's bidding window has expired.
pub fn is_tender_expired(tender: &ConstructionTender, current_turn: u32) -> bool {
    tender.status == TenderStatus::Open
        && current_turn >= tender.published_turn + tender.deadline_turns
}

/// Read a company's reputation score from its `extra` field (Phase 22D).
/// Returns 50.0 (neutral) if not set.
fn bidder_extra_reputation(company: &Company) -> f64 {
    company
        .extra
        .get("reputation_score")
        .and_then(|v| v.as_f64())
        .unwrap_or(50.0)
}

/// Process the tender market for one turn: award expired tenders.
///
/// # Arguments
/// * `tenders` - All active tenders (mutated: expired ones are awarded).
/// * `current_turn` - Current turn number.
///
/// # Returns
/// Vector of (tender_id, project, expansion_target_building_id) tuples for
/// newly awarded tenders. The expansion target is `Some(building_id)` for
/// expansion tenders, `None` for new-building tenders.
pub fn process_tender_awards(
    tenders: &mut Vec<ConstructionTender>,
    current_turn: u32,
) -> Vec<(String, ConstructionProject, Option<String>)> {
    let mut awarded = Vec::new();

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
        if let Some(project) = award_tender(tender, current_turn) {
            awarded.push((tender.id.clone(), project, expansion_target));
        } else {
            // No eligible bids — cancel
            tender.status = TenderStatus::Cancelled;
        }
        to_remove.push(idx);
    }

    // Remove awarded/cancelled tenders from the active list
    for idx in to_remove.iter().rev() {
        tenders.remove(*idx);
    }

    awarded
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
    rng: &mut impl Rng,
) -> Option<(f64, f64, Vec<String>)> {
    if company.sector != Sector::Construction {
        return None;
    }

    let reputation = bidder_extra_reputation(company);

    // Blacklisted companies don't bid
    if reputation < BLACKLIST_THRESHOLD {
        return None;
    }

    // Estimate cost: base on tender's estimated_cost, adjusted for safety level
    let safety_factor = 1.0 - company.safety_level * 0.1; // safer = higher cost
    let bid_cost = tender.estimated_cost * (0.8 + safety_factor * 0.2) * (0.9 + rng.gen::<f64>() * 0.2);

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
    use crate::entities::Company;
    use crate::entities::legal_form::LegalForm;
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
            "Cementownia".to_string(),
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
            "Cementownia".to_string(),
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
            "Cementownia".to_string(),
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
            "Cementownia".to_string(),
            50,
            1_000_000.0,
            500_000.0,
            3,
            10,
            crate::registries::enums::Sector::HeavyIndustry,
            1900,
        );
        let c1 = make_construction_company("c1");
        let c2 = make_construction_company("c2");
        submit_bid(&mut tender, &c1, 400_000.0, 0.15, Vec::new(), 10).unwrap();
        submit_bid(&mut tender, &c2, 350_000.0, 0.10, Vec::new(), 10).unwrap();

        let project = award_tender(&mut tender, 13).unwrap();
        assert_eq!(tender.status, TenderStatus::Awarded);
        // c2 has lower bid_price (350k * 1.10 = 385k vs 400k * 1.15 = 460k)
        assert_eq!(project.main_contractor_id, "c2");
        assert!(!project.tranches.is_empty());
    }
}

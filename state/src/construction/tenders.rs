//! Phase 22A: Construction tender market data structures.
//!
//! Defines the B2B tender market entities: `ConstructionTender`, `Bid`,
//! `Tranche`, and `SubcontractorAssignment`. These structures separate the
//! Investor (who funds the project) from the Main Contractor (who builds it),
//! enabling milestone payments, consortia, and domino bankruptcy cascades.

use crate::construction::projects::ConstructionProjectType;
use crate::registries::enums::Commodity;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Identifies whether the tender investor is the State or a private corporation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TenderInvestorType {
    /// State-funded tender (Treasury pays tranches).
    #[default]
    State,
    /// Corporate-funded tender (investor company pays tranches).
    Corporation,
}

/// Lifecycle status of a construction tender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TenderStatus {
    /// Open for bid submission.
    #[default]
    Open,
    /// Awarded to a winning bid; project created.
    Awarded,
    /// Cancelled by the investor before award.
    Cancelled,
    /// Investor or contractor bankrupted mid-construction; re-tendering.
    Distressed,
}

/// A published construction tender (Investor seeking a contractor).
///
/// # Rules
/// * The investor encumbers `estimated_cost` on publication (escrow).
/// * Bids are collected during `deadline_turns`.
/// * On award, a `ConstructionProject` is created with contractor linkage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ConstructionTender {
    /// Unique tender ID.
    #[serde(default)]
    pub id: String,
    /// Phase 40: Human-readable tender name (e.g., "State Highway A1").
    #[serde(default)]
    pub tender_name: String,
    /// Investor entity: company ID or `"STATE:{region_id}"`.
    #[serde(default)]
    pub investor_id: String,
    /// Whether the investor is State or Corporation.
    #[serde(default)]
    pub investor_type: TenderInvestorType,
    /// Project type (residential, commercial, factory, etc.).
    #[serde(default)]
    pub project_type: ConstructionProjectType,
    /// Micro-region where construction occurs.
    #[serde(default)]
    pub micro_region_id: String,
    /// Target building type name (e.g. "Cement Plant").
    #[serde(default)]
    pub target_building_type: String,
    /// Required construction materials (BOM from `get_construction_bom`).
    #[serde(default)]
    pub required_materials: BTreeMap<Commodity, f64>,
    /// Worker capacity to add on completion.
    #[serde(default)]
    pub target_capacity_increase: u32,
    /// Fixed capital to add on completion.
    #[serde(default)]
    pub target_capital_increase: f64,
    /// Investor's budget ceiling (total price the investor will pay).
    #[serde(default)]
    pub estimated_cost: f64,
    /// Bidding window in turns.
    #[serde(default)]
    pub deadline_turns: u32,
    /// Turn the tender was published.
    #[serde(default)]
    pub published_turn: u32,
    /// Current lifecycle status.
    #[serde(default)]
    pub status: TenderStatus,
    /// All submitted bids.
    #[serde(default)]
    pub bids: Vec<Bid>,
    /// ID of the awarded bid (None until awarded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub awarded_bid: Option<String>,
    /// Phase 29: If set, this tender is for expanding an existing building
    /// (not constructing a new one). The project will be attached to this
    /// specific building instead of searching for a matching one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expansion_target_building_id: Option<String>,
}

/// A contractor's bid on a tender.
///
/// # Rules
/// * `bid_price = bid_cost + bid_margin * bid_cost`.
/// * Bids below the dumping floor (`cost < 0.5 * estimated_cost`) are rejected.
/// * `reputation_score` is snapshotted at submission time for blacklist checks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Bid {
    /// Unique bid ID.
    #[serde(default)]
    pub id: String,
    /// Tender this bid responds to.
    #[serde(default)]
    pub tender_id: String,
    /// Main contractor company ID.
    #[serde(default)]
    pub bidder_id: String,
    /// Contractor's cost estimate (materials + labor + OHS).
    #[serde(default)]
    pub bid_cost: f64,
    /// Profit margin (0.0–1.0 of cost).
    #[serde(default)]
    pub bid_margin: f64,
    /// Total price the investor pays (`cost + margin * cost`).
    #[serde(default)]
    pub bid_price: f64,
    /// True if this is a consortium bid (multiple contractors).
    #[serde(default)]
    pub is_consortium: bool,
    /// Subcontractor company IDs (empty if solo bid).
    #[serde(default)]
    pub consortium_members: Vec<String>,
    /// Turn the bid was submitted.
    #[serde(default)]
    pub submitted_turn: u32,
    /// Snapshot of bidder's reputation at bid time (0.0–100.0).
    #[serde(default)]
    pub reputation_score: f64,
}

/// A milestone payment tranche released on progress threshold.
///
/// # Rules
/// * Released when `project.progress >= trigger_progress`.
/// * Payment routes through `settle_company_to_company` (corporate investor)
///   or `settle_treasury_to_company` (state investor).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Tranche {
    /// Unique tranche ID.
    #[serde(default)]
    pub tranche_id: String,
    /// Progress threshold at which this tranche is released (0.0–1.0).
    #[serde(default)]
    pub trigger_progress: f64,
    /// Cash amount paid to the main contractor on release.
    #[serde(default)]
    pub amount: f64,
    /// Whether this tranche has been released.
    #[serde(default)]
    pub released: bool,
    /// Turn the tranche was released (0 if not yet released).
    #[serde(default)]
    pub released_turn: u32,
}

/// A scoped task assigned to a subcontractor by the main contractor.
///
/// # Rules
/// * The subcontractor is responsible for delivering `task_materials`.
/// * `tranche_payment` is released when the task is completed.
/// * Payment routes through `settle_company_to_company`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SubcontractorAssignment {
    /// Subcontractor company ID.
    #[serde(default)]
    pub subcontractor_id: String,
    /// Subset of the project BOM assigned to this subcontractor.
    #[serde(default)]
    pub task_materials: BTreeMap<Commodity, f64>,
    /// Cash the subcontractor receives on task completion.
    #[serde(default)]
    pub tranche_payment: f64,
    /// Whether the subcontractor's task is complete.
    #[serde(default)]
    pub completed: bool,
    /// Whether the subcontractor has been paid.
    #[serde(default)]
    pub paid: bool,
}

/// Creates default tranches from a contract price.
///
/// Splits the price into 4 milestones: 20% / 30% / 30% / 20%
/// at progress thresholds 0.0 / 0.33 / 0.66 / 1.0.
pub fn default_tranches(contract_price: f64) -> Vec<Tranche> {
    vec![
        Tranche {
            tranche_id: "tranche_1".to_string(),
            trigger_progress: 0.0,
            amount: contract_price * 0.20,
            released: false,
            released_turn: 0,
        },
        Tranche {
            tranche_id: "tranche_2".to_string(),
            trigger_progress: 0.33,
            amount: contract_price * 0.30,
            released: false,
            released_turn: 0,
        },
        Tranche {
            tranche_id: "tranche_3".to_string(),
            trigger_progress: 0.66,
            amount: contract_price * 0.30,
            released: false,
            released_turn: 0,
        },
        Tranche {
            tranche_id: "tranche_4".to_string(),
            trigger_progress: 1.0,
            amount: contract_price * 0.20,
            released: false,
            released_turn: 0,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_tranches_sum() {
        let tranches = default_tranches(1000.0);
        let total: f64 = tranches.iter().map(|t| t.amount).sum();
        assert!((total - 1000.0).abs() < 0.01);
        assert_eq!(tranches.len(), 4);
    }

    #[test]
    fn test_tender_default() {
        let tender = ConstructionTender::default();
        assert_eq!(tender.status, TenderStatus::Open);
        assert_eq!(tender.investor_type, TenderInvestorType::State);
        assert!(tender.bids.is_empty());
    }

    #[test]
    fn test_bid_default() {
        let bid = Bid::default();
        assert!(!bid.is_consortium);
        assert!(bid.consortium_members.is_empty());
        assert_eq!(bid.reputation_score, 0.0);
    }
}

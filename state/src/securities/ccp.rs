//! Central Counterparty Clearinghouse (CCP) module.
//!
//! This module implements Phase D.5 CCP structures:
//! - CentralCounterparty struct
//! - CcpMember with margin accounts
//! - MarginRequirements for enforcement

use serde::{Deserialize, Serialize};
use serde_json::Map;
use std::collections::BTreeMap;

/// Member status in CCP.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Copy)]

#[derive(Default)]
pub enum MemberStatus {
    /// Active member - can trade.

    #[default]
    Active,
    /// Suspended member - cannot trade temporarily.

    Suspended,
    /// Defaulted member - in resolution process.

    Defaulted,
}


/// CCP member with margin account.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct CcpMember {
    /// Member ID.
    #[serde(default)]
    pub id: String,
    
    /// Posted margin (collateral).
    #[serde(default)]
    pub posted_margin: f64,
    
    /// Current margin deficit (if below requirements).
    #[serde(default)]
    pub margin_deficit: f64,
    
    /// Member status (active, suspended, defaulted).
    #[serde(default)]
    pub status: MemberStatus,
    
    /// Any additional member fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

/// Strict margin requirements enforced by CCP.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct MarginRequirements {
    /// Initial margin ratio (e.g., 10%).
    #[serde(default)]
    pub initial_margin_ratio: f64,
    
    /// Maintenance margin ratio (e.g., 5%).
    #[serde(default)]
    pub maintenance_margin_ratio: f64,
    
    /// Any additional requirements fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

/// Central Counterparty Clearinghouse - Guarantees derivative trades.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct CentralCounterparty {
    /// CCP ID.
    #[serde(default)]
    pub id: String,
    
    /// Member banks/funds cleared by CCP.
    #[serde(default)]
    pub members: BTreeMap<String, CcpMember>,
    
    /// Strict margin requirements (enforced by engine).
    #[serde(default)]
    pub margin_requirements: MarginRequirements,
    
    /// Default fund (buffer for member defaults).
    #[serde(default)]
    pub default_fund: f64,
    
    /// Cleared derivatives positions.
    #[serde(default)]
    pub cleared_positions: Vec<String>,
    
    /// Any additional CCP fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

/// Process CCP margin requirements: check and collect variation margins from members.
///
/// # Arguments
/// * `ccp` - Mutable CCP
/// * `companies` - Mutable slice of all companies (CCP members)
/// * `futures_contracts` - Slice of all futures contracts (for exposure calculation)
/// * `config` - Securities market config with margin ratios
/// * `current_turn` - Current turn number
///
/// # Rules
/// * For each CCP member: calculate total exposure from cleared futures
/// * Required margin = exposure * initial_margin_ratio
/// * If posted_margin < required: issue margin call
/// * Margin call debits from member's brokerage cash, credits to CCP posted_margin
/// * If member cannot meet margin call: suspend and set margin_deficit
/// * NO MAGIC CASH: margin transferred from member brokerage to CCP
pub fn process_ccp_margins(
    ccp: &mut CentralCounterparty,
    companies: &mut [crate::entities::Company],
    futures_contracts: &[crate::securities::derivatives::FuturesContract],
    config: &crate::securities::config::SecuritiesMarketConfig,
    _current_turn: u32,
) {
    // Update margin requirements from config
    ccp.margin_requirements.initial_margin_ratio = config.ccp_initial_margin_ratio;
    ccp.margin_requirements.maintenance_margin_ratio = config.ccp_maintenance_margin_ratio;

    for (member_id, member) in ccp.members.iter_mut() {
        if member.status == MemberStatus::Defaulted {
            continue;
        }

        // Calculate total exposure from cleared futures
        let total_exposure: f64 = futures_contracts.iter()
            .filter(|f| f.clearing_method == crate::securities::derivatives::ClearingMethod::CCP
                && (f.owner_id == *member_id || f.counterparty_id == *member_id))
            .map(|f| f.contract_size * f.current_price)
            .sum();

        let required_margin = total_exposure * ccp.margin_requirements.initial_margin_ratio;

        if member.posted_margin < required_margin {
            let margin_call = required_margin - member.posted_margin;

            // Attempt to collect from member's brokerage account
            let mut collected = 0.0;
            if let Some(company) = companies.iter_mut().find(|c| c.id == *member_id) {
                if let Some(ref mut acct) = company.brokerage_account {
                    let available = acct.cash;
                    collected = margin_call.min(available);
                    acct.cash -= collected;
                    acct.frozen_cash += collected;
                }
            }

            member.posted_margin += collected;

            if collected < margin_call {
                member.margin_deficit = margin_call - collected;
                if member.margin_deficit > required_margin * 0.5 {
                    member.status = MemberStatus::Suspended;
                }
            } else {
                member.margin_deficit = 0.0;
            }
        } else {
            member.margin_deficit = 0.0;
        }
    }
}

/// Process CCP default waterfall when a member defaults.
///
/// # Arguments
/// * `ccp` - Mutable CCP
/// * `companies` - Mutable slice of all companies
/// * `defaulted_member_id` - ID of the member that defaulted
/// * `current_turn` - Current turn number
///
/// # Returns
/// Total loss absorbed by the CCP waterfall
///
/// # Rules
/// * Default waterfall order:
///   1. Defaulting member's posted margin
///   2. Default fund contribution
///   3. Remaining members' default fund contributions (mutualization)
/// * Any remaining loss: CCP cannot cover (systemic risk event)
/// * NO MAGIC CASH: all funds come from existing posted margins and default fund
pub fn process_ccp_default_waterfall(
    ccp: &mut CentralCounterparty,
    _companies: &mut [crate::entities::Company],
    defaulted_member_id: &str,
    _current_turn: u32,
) -> f64 {
    let member = match ccp.members.get_mut(defaulted_member_id) {
        Some(m) => m,
        None => return 0.0,
    };

    member.status = MemberStatus::Defaulted;
    let mut remaining_loss = member.margin_deficit;

    // Step 1: Use defaulting member's posted margin
    let posted = member.posted_margin;
    let absorbed_1 = posted.min(remaining_loss);
    member.posted_margin -= absorbed_1;
    remaining_loss -= absorbed_1;

    // Step 2: Use CCP default fund
    if remaining_loss > 0.0 {
        let absorbed_2 = ccp.default_fund.min(remaining_loss);
        ccp.default_fund -= absorbed_2;
        remaining_loss -= absorbed_2;
    }

    // Step 3: Mutualize remaining loss across surviving members
    if remaining_loss > 0.0 {
        let surviving_ids: Vec<String> = ccp.members.iter()
            .filter(|(_, m)| m.status == MemberStatus::Active)
            .map(|(id, _)| id.clone())
            .collect();

        if !surviving_ids.is_empty() {
            let per_member = remaining_loss / surviving_ids.len() as f64;
            for survivor_id in &surviving_ids {
                if let Some(survivor) = ccp.members.get_mut(survivor_id) {
                    let contribution = survivor.posted_margin.min(per_member);
                    survivor.posted_margin -= contribution;
                    remaining_loss -= contribution;
                }
            }
        }
    }

    // Any remaining loss is systemic (CCP cannot cover)
    remaining_loss
}

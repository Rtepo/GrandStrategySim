//! Institutional lobbying and special economic zones for Phase 4 Economic Bridge.
//!
//! This module defines the institutional layer connecting corporate entities and
//! political parties through LobbyingGroups and SpecialEconomicZones, establishing
//! strict double-entry transactional flows for all economic influence operations.

#![allow(missing_docs)]

use crate::registries::enums::Sector;
use crate::securities::BrokerageAccount;
use crate::politics::campaign::BlackMoneySource;
use crate::politics::legislation::Bill;
use crate::politics::local_council::Councilor;
use crate::politics::system::Party;
use crate::entities::Company;
use crate::society::geography::Region;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, BTreeMap};

/// Institutional lobbying group (Chamber of Commerce, Industry Association, etc.)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LobbyingGroup {
    /// Unique identifier (e.g., "[LOB-IND-001]")
    #[serde(default)]
    pub id: String,
    
    /// Group name (e.g., "Polish Chamber of Commerce")
    #[serde(default)]
    pub name: String,
    
    /// Group type (sectoral, regional, ideological)
    #[serde(default)]
    pub group_type: LobbyingGroupType,
    
    /// Brokerage account for pooled capital
    #[serde(default)]
    pub brokerage_account: Option<BrokerageAccount>,
    
    /// Member companies (by company_id)
    #[serde(default)]
    pub member_companies: Vec<String>,
    
    /// Membership dues structure (percentage of company liquid capital)
    #[serde(default)]
    pub membership_dues_rate: f64,
    
    /// Target sectors for influence (empty = all sectors)
    #[serde(default)]
    pub target_sectors: Vec<Sector>,
    
    /// Target regions for influence (empty = national)
    #[serde(default)]
    pub target_regions: Vec<String>,
    
    /// Political alignment (ideology vector)
    #[serde(default)]
    pub political_alignment: HashMap<String, f64>,
    
    /// Influence power (derived from pooled capital + member count)
    #[serde(default)]
    pub influence_power: f64,
    
    /// Active lobbying operations
    #[serde(default)]
    pub active_lobbies: Vec<LobbyingOperation>,
    
    /// Turn when group was founded
    #[serde(default)]
    pub founding_turn: u32,
    
    /// Any additional fields
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum LobbyingGroupType {
    #[default]

    Sectoral,  // Industry association (e.g., Mining Association)
    

    Regional,  // Regional chamber of commerce
    

    Ideological,  // Think tank / advocacy group
    

    Professional,  // Professional association (e.g., Medical Association)
}

/// Lobbying operation targeting legislation or individuals
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LobbyingOperation {
    /// Operation ID
    #[serde(default)]
    pub id: String,
    
    /// Initiating lobbying group
    #[serde(default)]
    pub lobbying_group_id: String,
    
    /// Target type (Bill, Councilor, Party)
    #[serde(default)]
    pub target_type: LobbyingTarget,
    
    /// Target identifier (bill_id, councilor_id, or party_id)
    #[serde(default)]
    pub target_id: String,
    
    /// Operation type (legal lobbying, illicit bribery)
    #[serde(default)]
    pub operation_type: LobbyingOperationType,
    
    /// Amount spent
    #[serde(default)]
    pub amount: f64,
    
    /// Expected influence modifier (-0.5 to +0.5)
    #[serde(default)]
    pub influence_modifier: f64,
    
    /// Turn when operation was initiated
    #[serde(default)]
    pub initiation_turn: u32,
    
    /// Operation status
    #[serde(default)]
    pub status: LobbyingStatus,
    
    /// Discovery risk (0-1, for illicit operations)
    #[serde(default)]
    pub discovery_risk: f64,
    
    /// Any additional fields
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum LobbyingTarget {
    #[default]

    Bill,  // Target a specific Bill in parliament
    

    Councilor,  // Target a specific LocalCouncil councilor
    

    Party,  // Target a specific Party (campaign contribution)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum LobbyingOperationType {
    #[default]

    LegalLobbying,  // Legal campaign contribution / advocacy
    

    Bribery,  // Illicit direct payment to individual
    

    BlackMoneyFinancing,  // Illicit party funding (triggers Phase 3 mechanics)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum LobbyingStatus {
    #[default]

    InProgress,
    

    Success,
    

    Failed,
    

    Discovered,  // Illicit operation exposed
}

#[derive(Debug, Clone, PartialEq)]
pub enum LobbyingError {
    InsufficientFunds,
    CompanyNotEligible,
}

/// Collect membership dues from a company to a lobbying group
pub fn collect_membership_dues(
    company: &mut Company,
    lobbying_group: &mut LobbyingGroup,
    dues_rate: f64,  // e.g., 0.01 = 1% of liquid capital
) -> Result<(), LobbyingError> {
    let company_cash = company.available_cash;
    
    if company_cash <= 0.0 {
        return Err(LobbyingError::InsufficientFunds);
    }
    
    let dues_amount = company_cash * dues_rate;
    
    // Deduct from company operational cash (NOT brokerage account — that's for securities)
    company.available_cash -= dues_amount;
    
    // Ensure lobbying group has brokerage account
    if lobbying_group.brokerage_account.is_none() {
        lobbying_group.brokerage_account = Some(BrokerageAccount {
            cash: 0.0,
            fx_balances: HashMap::new(),
            portfolio: BTreeMap::new(),
            pending_orders: BTreeMap::new(),
            frozen_cash: 0.0,
            is_frozen: false,
            margin_account: None,
            extra: HashMap::new(),
        });
    }
    
    // Credit to lobbying group
    lobbying_group.brokerage_account.as_mut().unwrap().cash += dues_amount;
    
    // Update influence power
    lobbying_group.influence_power = calculate_influence_power(lobbying_group);
    
    Ok(())
}

fn calculate_influence_power(group: &LobbyingGroup) -> f64 {
    let pooled_capital = group.brokerage_account.as_ref()
        .map(|a| a.cash)
        .unwrap_or(0.0);
    
    let member_count = group.member_companies.len() as f64;
    
    // Influence = sqrt(pooled_capital) * log(member_count + 1)
    pooled_capital.sqrt() * (member_count + 1.0).log10()
}

/// Execute legal lobbying (campaign contribution to party)
pub fn execute_legal_lobbying(
    lobbying_group: &mut LobbyingGroup,
    party: &mut Party,
    bill: &mut Bill,
    amount: f64,
    influence_modifier: f64,
) -> Result<(), LobbyingError> {
    let group_cash = lobbying_group.brokerage_account.as_ref()
        .map(|a| a.cash)
        .unwrap_or(0.0);
    
    if group_cash < amount {
        return Err(LobbyingError::InsufficientFunds);
    }
    
    // Deduct from lobbying group
    lobbying_group.brokerage_account.as_mut().unwrap().cash -= amount;
    
    // Credit to party (campaign war chest)
    if party.brokerage_account.is_none() {
        party.brokerage_account = Some(BrokerageAccount {
            cash: 0.0,
            fx_balances: HashMap::new(),
            portfolio: BTreeMap::new(),
            pending_orders: BTreeMap::new(),
            frozen_cash: 0.0,
            is_frozen: false,
            margin_account: None,
            extra: HashMap::new(),
        });
    }
    
    party.brokerage_account.as_mut().unwrap().cash += amount;
    party.annual_donations += amount;
    
    // Apply influence modifier to bill
    bill.committee_modifier += influence_modifier;
    
    // Record operation
    lobbying_group.active_lobbies.push(LobbyingOperation {
        id: format!("[LOB-OPR-{}]", lobbying_group.active_lobbies.len()),
        lobbying_group_id: lobbying_group.id.clone(),
        target_type: LobbyingTarget::Bill,
        target_id: bill.id.clone(),
        operation_type: LobbyingOperationType::LegalLobbying,
        amount,
        influence_modifier,
        initiation_turn: 0,  // Set by caller
        status: LobbyingStatus::Success,
        discovery_risk: 0.0,
        extra: Map::new(),
    });
    
    Ok(())
}

/// Execute councilor bribery (routes to demographic savings to prevent black hole)
pub fn execute_councilor_bribery(
    lobbying_group: &mut LobbyingGroup,
    councilor: &Councilor,
    home_region: &mut Region,
    amount: f64,
    influence_modifier: f64,
    discovery_risk: f64,
) -> Result<(), LobbyingError> {
    let group_cash = lobbying_group.brokerage_account.as_ref()
        .map(|a| a.cash)
        .unwrap_or(0.0);
    
    if group_cash < amount {
        return Err(LobbyingError::InsufficientFunds);
    }
    
    // Deduct from lobbying group
    lobbying_group.brokerage_account.as_mut().unwrap().cash -= amount;
    
    // Credit to wealthiest demographic class in councilor's home region (prevents black hole)
    // Dynamically select class with highest savings_per_capita (or total savings if population is 0)
    // CRITICAL: Chain both rural_classes and urban_classes to seal Urban Loophole
    let target_class_key = home_region.class_demographics.rural_classes
        .iter()
        .chain(home_region.class_demographics.urban_classes.iter())
        .max_by(|a, b| {
            let a_savings_per_capita = if a.1.population > 0 {
                a.1.savings / a.1.population as f64
            } else {
                a.1.savings
            };
            let b_savings_per_capita = if b.1.population > 0 {
                b.1.savings / b.1.population as f64
            } else {
                b.1.savings
            };
            a_savings_per_capita.partial_cmp(&b_savings_per_capita).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(key, _)| key.clone())
        .unwrap_or_default();
    
    // Try rural_classes first, then urban_classes
    if let Some(class_demographics) = home_region.class_demographics.rural_classes.get_mut(&target_class_key) {
        class_demographics.savings += amount;
    } else if let Some(class_demographics) = home_region.class_demographics.urban_classes.get_mut(&target_class_key) {
        class_demographics.savings += amount;
    }
    
    // Increase councilor corruption risk (tracked separately, not as savings)
    // Note: Councilor.corruption_risk must be added to Councilor struct for tracking
    
    // Record operation
    lobbying_group.active_lobbies.push(LobbyingOperation {
        id: format!("[LOB-OPR-{}]", lobbying_group.active_lobbies.len()),
        lobbying_group_id: lobbying_group.id.clone(),
        target_type: LobbyingTarget::Councilor,
        target_id: councilor.id.clone(),
        operation_type: LobbyingOperationType::Bribery,
        amount,
        influence_modifier,
        initiation_turn: 0,
        status: LobbyingStatus::InProgress,
        discovery_risk,
        extra: Map::new(),
    });
    
    Ok(())
}

/// Execute black money financing to party (triggers Phase 3 mechanics)
pub fn execute_black_money_financing(
    lobbying_group: &mut LobbyingGroup,
    party: &mut Party,
    amount: f64,
    discovery_risk: f64,
) -> Result<(), LobbyingError> {
    let group_cash = lobbying_group.brokerage_account.as_ref()
        .map(|a| a.cash)
        .unwrap_or(0.0);
    
    if group_cash < amount {
        return Err(LobbyingError::InsufficientFunds);
    }
    
    // Deduct from lobbying group
    lobbying_group.brokerage_account.as_mut().unwrap().cash -= amount;
    
    // Credit to party black money pool (Phase 3 mechanics)
    if party.black_money_pool.is_none() {
        party.black_money_pool = Some(crate::politics::campaign::BlackMoneyPool {
            illicit_funds: 0.0,
            source: BlackMoneySource::None,
            discovery_risk: 0.0,
        });
    }
    
    let pool = party.black_money_pool.as_mut().unwrap();
    pool.illicit_funds += amount;
    pool.source = BlackMoneySource::CorporateLobbying {
        company_id: lobbying_group.id.clone(),
        amount,
    };
    pool.discovery_risk = discovery_risk;
    
    // Record operation
    lobbying_group.active_lobbies.push(LobbyingOperation {
        id: format!("[LOB-OPR-{}]", lobbying_group.active_lobbies.len()),
        lobbying_group_id: lobbying_group.id.clone(),
        target_type: LobbyingTarget::Party,
        target_id: party.id.clone(),
        operation_type: LobbyingOperationType::BlackMoneyFinancing,
        amount,
        influence_modifier: 0.5,  // High influence for black money
        initiation_turn: 0,
        status: LobbyingStatus::InProgress,
        discovery_risk,
        extra: Map::new(),
    });
    
    Ok(())
}

/// Process lobbying for one turn — collect dues, execute lobbying operations.
///
/// # Arguments
/// * `country` - Mutable country (for lobbying groups, active operations)
/// * `companies` - Mutable companies (for dues collection — debits available_cash)
/// * `parties` - Mutable parties (for receiving lobbying funds)
/// * `bills` - Mutable bills (for influence modifiers)
/// * `current_turn` - Current game turn
///
/// # Returns
/// Vector of diagnostic messages
///
/// # Rules
/// * Dues: Debit company.available_cash → credit lobbying group brokerage.
/// * Legal lobbying: Debit lobbying group brokerage → credit party brokerage + bill influence.
/// * Bribery: Debit lobbying group brokerage → credit party black_money_pool.
/// * Double-entry: All flows are traced from company operational cash to political actors.
pub fn process_lobbying_turn(
    country: &mut crate::state::Country,
    companies: &mut [crate::entities::Company],
    parties: &mut std::collections::HashMap<String, crate::politics::system::Party>,
    bills: &mut [crate::politics::legislation::Bill],
    current_turn: u32,
) -> Vec<String> {
    let mut messages = Vec::new();

    // 1. Collect membership dues from companies to lobbying groups
    for group in &mut country.politics.lobbying_groups {
        for company in companies.iter_mut() {
            if group.member_companies.contains(&company.id) {
                if let Err(_e) = collect_membership_dues(company, group, 0.01) {
                    // Insufficient funds is common, don't log
                }
            }
        }
    }

    // 2. Execute legal lobbying on active bills
    for group in &mut country.politics.lobbying_groups {
        let group_cash = group.brokerage_account.as_ref().map(|a| a.cash).unwrap_or(0.0);
        if group_cash < 100.0 {
            continue;
        }

        let lobby_amount = group_cash * 0.05; // 5% of group cash per turn
        let influence = lobby_amount / 1000.0;

        // Find a bill to lobby on
        if let Some(bill) = bills.iter_mut().find(|b| b.stage == crate::politics::legislation::LegislativeStage::Committee) {
            // Find the target party (ruling party or bill initiator's party)
            let target_party_id = if !country.politics.ruling_party.is_empty() {
                country.politics.ruling_party.clone()
            } else {
                bill.initiator.clone()
            };

            if !target_party_id.is_empty() {
                if let Some(party) = parties.get_mut(&target_party_id) {
                    if let Err(_e) = execute_legal_lobbying(group, party, bill, lobby_amount, influence) {
                        // Insufficient funds or other error — skip
                    } else {
                        messages.push(format!(
                            "[LOBBY] {} lobbied {} for {:.0}",
                            group.name, target_party_id, lobby_amount
                        ));
                    }
                }
            }
        }
    }

    let _ = current_turn;
    messages
}

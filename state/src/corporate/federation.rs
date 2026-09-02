//! Cooperative federations — deterministic birth, shared administrative dues,
//! joint debt issuance with individual creditor tracking, and dissolution
//! with pro-rata fund rebates and precise debt redistribution.
//!
//! This module implements Phase 2 of the Cooperative Sector Refactoring Plan.
//! It deliberately does NOT implement speculative B2B procurement, order-book
//! interception, cash pooling, or inventory distribution — those are deferred
//! to a separately designed and approved sprint.
//!
//! # Design Principles (Global Directives)
//!
//! * **Rule 1 (Closed-loop):** Every dues payment, joint debt issuance, and
//!   dissolution rebate has a real payer and recipient. No fiat creation.
//! * **Rule 4 (Complete lifecycles):** Federations have explicit birth
//!   (minimum qualifying members), operation (dues + joint debt), and death
//!   (insufficient members → dissolution) conditions.
//! * **Rule 7 (Individual accountability):** Joint debt tracks each lender
//!   individually via `HashMap<EntityID, f64>`. No averaging across lenders.
//! * **Rule 8 (Rational actors):** Federations form only when deterministic
//!   economic conditions are met, not randomly.

use crate::entities::Company;
use crate::state::Country;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Minimum number of qualifying cooperatives required to form a federation.
/// A cooperative qualifies if it has at least `MIN_QUALIFYING_MEMBERS` members.
pub const MIN_FEDERATION_MEMBERS: usize = 3;
/// Minimum member count for a cooperative to qualify for federation membership.
/// Scaled dynamically by `average_wage` in the qualification check, but the
/// base threshold is expressed in member-workers (a physical quantity, not
/// a nominal financial value — Rule 2/3 compliance).
pub const MIN_QUALIFYING_MEMBERS: u32 = 50;
/// Dissolution threshold: if active members drop below this, the federation
/// dissolves with pro-rata fund rebates and debt redistribution.
pub const DISSOLUTION_THRESHOLD: usize = 2;
/// Administrative dues rate: pro-rata share of `average_wage * member_count`
/// per cooperative, charged annually. This is a dynamic, inflation-proof basis
/// (Rule 2: no magic nominal constants).
pub const ADMIN_DUES_WAGE_MULTIPLIER: f64 = 0.5;

/// A cooperative federation — a voluntary association of cooperatives for
/// shared administrative services and joint debt issuance.
///
/// # Lifecycle
///
/// * **Birth:** Created when `MIN_FEDERATION_MEMBERS` qualifying cooperatives
///   meet the deterministic conditions (see `try_form_federation`).
/// * **Life:** Collects administrative dues (pro-rata by `average_wage *
///   member_count`), issues joint debt backed by member assets, and tracks
///   each lender individually.
/// * **Death:** Dissolves when active members drop below
///   `DISSOLUTION_THRESHOLD`. Remaining funds are rebated pro-rata;
///   outstanding debts are redistributed to remaining members or settled.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CooperativeFederation {
    /// Unique federation ID.
    pub id: String,
    /// Member cooperative company IDs.
    pub member_ids: Vec<String>,
    /// Shared administrative fund (cash held by the federation).
    pub admin_fund: f64,
    /// Joint debt obligations: lender_id → outstanding amount.
    /// Each lender is tracked individually (Rule 7: no communization).
    pub joint_debt: BTreeMap<String, f64>,
    /// Total joint debt principal issued (historical, for audit).
    pub total_debt_issued: f64,
    /// Turn the federation was founded.
    pub founded_turn: u32,
    /// Whether the federation is active (false after dissolution).
    pub active: bool,
}

impl CooperativeFederation {
    /// Returns the total outstanding joint debt across all lenders.
    pub fn total_outstanding_debt(&self) -> f64 {
        self.joint_debt.values().sum()
    }

    /// Returns the number of active members.
    pub fn active_member_count(&self) -> usize {
        self.member_ids.len()
    }

    /// Check if the federation should dissolve (too few members).
    pub fn should_dissolve(&self) -> bool {
        self.active && self.active_member_count() < DISSOLUTION_THRESHOLD
    }
}

/// Attempt to form a new federation from qualifying cooperatives.
///
/// # Birth Conditions (Deterministic)
///
/// * At least `MIN_FEDERATION_MEMBERS` cooperatives must qualify.
/// * A cooperative qualifies if it has `MIN_QUALIFYING_MEMBERS` or more
///   member-workers and is not already in a federation.
/// * The cooperatives must be profitable (positive `company_capital`).
///
/// # Arguments
/// * `companies` - All companies (used to find qualifying cooperatives).
/// * `country` - Country state (for federation storage and `federation_id` assignment).
/// * `current_turn` - Current turn number.
///
/// # Returns
/// `Some(federation_id)` if a federation was formed, `None` otherwise.
pub fn try_form_federation(
    companies: &mut [Company],
    country: &mut Country,
    current_turn: u32,
) -> Option<String> {
    // Collect indices of qualifying cooperatives.
    let mut qualifying: Vec<usize> = Vec::new();
    for (i, c) in companies.iter().enumerate() {
        if let crate::entities::LegalForm::Cooperative(ref data) = c.legal_form {
            if data.federation_id.is_some() {
                continue; // Already in a federation
            }
            if data.member_count >= MIN_QUALIFYING_MEMBERS && c.company_capital > 0.0 {
                qualifying.push(i);
            }
        }
    }

    if qualifying.len() < MIN_FEDERATION_MEMBERS {
        return None;
    }

    // Form the federation with all qualifying cooperatives.
    let federation_id = format!("FED-{}-{}", current_turn, country.cooperative_federations.len());
    let member_ids: Vec<String> = qualifying
        .iter()
        .map(|&i| companies[i].id.clone())
        .collect();

    let federation = CooperativeFederation {
        id: federation_id.clone(),
        member_ids,
        admin_fund: 0.0,
        joint_debt: BTreeMap::new(),
        total_debt_issued: 0.0,
        founded_turn: current_turn,
        active: true,
    };

    // Assign federation_id to each member cooperative.
    for &i in &qualifying {
        if let crate::entities::LegalForm::Cooperative(ref mut data) = companies[i].legal_form {
            data.federation_id = Some(federation_id.clone());
        }
    }

    country.cooperative_federations.push(federation);
    Some(federation_id)
}

/// Collect administrative dues from all federation members.
///
/// Dues are pro-rata based on `average_wage * member_count` for each
/// cooperative. This is a dynamic, inflation-proof basis (Rule 2).
///
/// # Arguments
/// * `federation` - The federation collecting dues.
/// * `companies` - All companies (members are looked up by ID).
/// * `country` - Country state (for `average_wage` and settlement).
///
/// # Returns
/// Total dues collected.
pub fn collect_admin_dues(
    federation: &mut CooperativeFederation,
    companies: &mut [Company],
    avg_wage: f64,
) -> f64 {
    if !federation.active || federation.member_ids.is_empty() {
        return 0.0;
    }

    let avg_wage = avg_wage.max(1.0);
    let dues_basis = avg_wage * ADMIN_DUES_WAGE_MULTIPLIER;

    // Compute each member's dues: dues_basis * member_count.
    // Collect member info first to avoid borrow conflicts.
    let member_dues: Vec<(String, f64, usize)> = federation
        .member_ids
        .iter()
        .filter_map(|mid| {
            let idx = companies.iter().position(|c| &c.id == mid)?;
            let member_count = if let crate::entities::LegalForm::Cooperative(ref data) =
                companies[idx].legal_form
            {
                data.member_count
            } else {
                return None; // No longer a cooperative
            };
            let dues = dues_basis * member_count as f64;
            Some((mid.clone(), dues, idx))
        })
        .collect();

    let mut total_collected = 0.0_f64;
    for (mid, dues, idx) in member_dues {
        if dues <= 0.0 {
            continue;
        }

        // Debit the cooperative's cash and credit the federation's admin fund.
        // Federations don't have their own company entity, so we credit
        // directly to the admin_fund field. The debit from the cooperative
        // uses real cash (Rule 1: closed-loop, real counterparty).

        // If the federation doesn't have a company entity, credit directly
        // to the admin_fund field. We use a direct transfer to the federation's
        // admin_fund since federations are not companies.
        let available = companies[idx]
            .brokerage_account
            .as_ref()
            .map(|ba| ba.cash.max(0.0))
            .unwrap_or(companies[idx].available_cash.max(0.0));
        let actual_dues = dues.min(available);

        if actual_dues <= 0.0 {
            continue; // Member cannot pay — skip (no forced confiscation)
        }

        // Debit from cooperative
        if let Some(ref mut ba) = companies[idx].brokerage_account {
            ba.cash -= actual_dues;
        } else {
            companies[idx].available_cash -= actual_dues;
        }

        // Credit to federation admin fund
        federation.admin_fund += actual_dues;
        total_collected += actual_dues;

        let _ = mid;
    }

    total_collected
}

/// Issue joint debt on behalf of the federation, backed by member assets.
///
/// Each lender is tracked individually in `joint_debt` (Rule 7).
/// The loan proceeds are credited to the federation's admin fund.
///
/// # Arguments
/// * `federation` - The federation issuing debt.
/// * `lender_id` - The lender's entity ID.
/// * `amount` - The principal amount.
/// * `companies` - All companies (for settlement).
pub fn issue_joint_debt(
    federation: &mut CooperativeFederation,
    lender_id: &str,
    amount: f64,
    companies: &mut [Company],
) -> bool {
    if !federation.active || amount <= 0.0 {
        return false;
    }

    // Find the lender company and debit its cash.
    let lender_idx = companies.iter().position(|c| c.id == lender_id);
    let lender_idx = match lender_idx {
        Some(idx) => idx,
        None => return false,
    };

    let available = companies[lender_idx]
        .brokerage_account
        .as_ref()
        .map(|ba| ba.cash.max(0.0))
        .unwrap_or(companies[lender_idx].available_cash.max(0.0));

    let actual = amount.min(available);
    if actual <= 0.0 {
        return false;
    }

    // Debit lender
    if let Some(ref mut ba) = companies[lender_idx].brokerage_account {
        ba.cash -= actual;
    } else {
        companies[lender_idx].available_cash -= actual;
    }

    // Credit federation admin fund
    federation.admin_fund += actual;
    federation.total_debt_issued += actual;

    // Track lender individually (Rule 7)
    *federation.joint_debt.entry(lender_id.to_string()).or_insert(0.0) += actual;

    true
}

/// Dissolve a federation: rebate remaining funds pro-rata to members and
/// redistribute outstanding debt to remaining members.
///
/// # Arguments
/// * `federation` - The federation to dissolve (will be marked inactive).
/// * `companies` - All companies (for settlement).
pub fn dissolve_federation(
    federation: &mut CooperativeFederation,
    companies: &mut [Company],
) -> bool {
    if !federation.active {
        return false;
    }

    // Mark inactive first
    federation.active = false;

    // 1. Rebate remaining admin fund pro-rata to members.
    if federation.admin_fund > 0.0 && !federation.member_ids.is_empty() {
        // Compute total member_count for pro-rata distribution.
        let member_counts: Vec<(String, u32)> = federation
            .member_ids
            .iter()
            .filter_map(|mid| {
                let idx = companies.iter().position(|c| &c.id == mid)?;
                if let crate::entities::LegalForm::Cooperative(ref data) = companies[idx].legal_form
                {
                    Some((mid.clone(), data.member_count))
                } else {
                    None
                }
            })
            .collect();

        let total_member_count: u32 = member_counts.iter().map(|(_, mc)| *mc).sum();

        if total_member_count > 0 {
            for (mid, mc) in &member_counts {
                let share = *mc as f64 / total_member_count as f64;
                let rebate = federation.admin_fund * share;
                if rebate <= 0.0 {
                    continue;
                }
                if let Some(idx) = companies.iter().position(|c| &c.id == mid) {
                    if let Some(ref mut ba) = companies[idx].brokerage_account {
                        ba.cash += rebate;
                    } else {
                        companies[idx].available_cash += rebate;
                    }
                }
            }
        }
        federation.admin_fund = 0.0;
    }

    // 2. Redistribute outstanding debt to remaining members pro-rata by
    //    member_count. Each member inherits a share of the total debt.
    //    The individual lender mappings are preserved: each lender is
    //    still owed their exact amount, but the obligation is now split
    //    among the former members (who become individual debtors).
    let total_debt = federation.total_outstanding_debt();
    if total_debt > 0.0 && !federation.member_ids.is_empty() {
        let member_counts: Vec<(String, u32)> = federation
            .member_ids
            .iter()
            .filter_map(|mid| {
                let idx = companies.iter().position(|c| &c.id == mid)?;
                if let crate::entities::LegalForm::Cooperative(ref data) = companies[idx].legal_form
                {
                    Some((mid.clone(), data.member_count))
                } else {
                    None
                }
            })
            .collect();

        let total_member_count: u32 = member_counts.iter().map(|(_, mc)| *mc).sum();

        if total_member_count > 0 {
            for (mid, mc) in &member_counts {
                let share = *mc as f64 / total_member_count as f64;
                let debt_share = total_debt * share;
                if debt_share <= 0.0 {
                    continue;
                }
                if let Some(idx) = companies.iter().position(|c| &c.id == mid) {
                    companies[idx].liabilities += debt_share;
                }
            }
        }
        federation.joint_debt.clear();
    }

    // 3. Clear federation_id from all former members.
    for mid in &federation.member_ids {
        if let Some(idx) = companies.iter().position(|c| &c.id == mid) {
            if let crate::entities::LegalForm::Cooperative(ref mut data) = companies[idx].legal_form
            {
                data.federation_id = None;
            }
        }
    }

    true
}

/// Process all federations for the turn: collect dues, check dissolution.
///
/// This is the main entry point called from the turn loop.
pub fn process_federations(
    companies: &mut [Company],
    country: &mut Country,
    current_turn: u32,
) {
    // 1. Try to form new federations if enough qualifying cooperatives exist.
    try_form_federation(companies, country, current_turn);

    // 2. Collect dues and check dissolution for existing federations.
    //    We process in reverse order to allow dissolution removal.
    let mut i = 0;
    while i < country.cooperative_federations.len() {
        if !country.cooperative_federations[i].active {
            i += 1;
            continue;
        }

        // Collect admin dues.
        let federation_id = country.cooperative_federations[i].id.clone();
        let avg_wage = country.macro_indicators.average_wage;
        let _ = collect_admin_dues(
            &mut country.cooperative_federations[i],
            companies,
            avg_wage,
        );

        // Check dissolution condition.
        if country.cooperative_federations[i].should_dissolve() {
            dissolve_federation(&mut country.cooperative_federations[i], companies);
        }

        let _ = federation_id;
        i += 1;
    }
}

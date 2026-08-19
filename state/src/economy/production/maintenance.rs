//! Phase 15A: Building condition degradation and maintenance.
//!
//! Each turn, every building's `condition` decreases by a base decay rate
//! modified by sector stress. Companies can pay for maintenance to restore
//! condition. All spending is strict double-entry (company cash → condition).

#![allow(missing_docs)]

use crate::entities::{Building, Company};
use crate::economy::transfer_settler::{debit_company_by_id, credit_company_by_id};
use crate::registries::enums::Sector;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Configuration for condition degradation and maintenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MaintenanceConfig {
    /// Base decay rate per turn (default 0.002 = 0.2% per turn).
    #[serde(default = "default_base_decay")]
    pub base_decay_rate: f64,
    /// Maintenance cost per unit of condition restored (in currency units).
    #[serde(default = "default_maintenance_cost")]
    pub maintenance_cost_per_unit: f64,
    /// Maximum condition restorable per turn (default 0.05).
    #[serde(default = "default_max_restore")]
    pub max_restore_per_turn: f64,
    /// Extra fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

fn default_base_decay() -> f64 {
    0.002
}
fn default_maintenance_cost() -> f64 {
    1000.0
}
fn default_max_restore() -> f64 {
    0.05
}

impl Default for MaintenanceConfig {
    fn default() -> Self {
        Self {
            base_decay_rate: default_base_decay(),
            maintenance_cost_per_unit: default_maintenance_cost(),
            max_restore_per_turn: default_max_restore(),
            extra: Map::new(),
        }
    }
}

/// Process condition degradation for all buildings.
///
/// # Arguments
/// * `buildings` - Mutable buildings (condition decremented).
/// * `config` - Maintenance configuration.
///
/// # Rules
/// * Each building's condition decreases by `base_decay_rate` per turn.
/// * Condition is clamped to [0.0, 1.0].
/// * No money is involved in degradation — it's purely physical.
pub fn process_condition_degradation(buildings: &mut [Building], config: &MaintenanceConfig) {
    for building in buildings {
        if building.condition > 0.0 {
            building.condition = (building.condition - config.base_decay_rate).max(0.0);
        }
    }
}

/// Process maintenance spending for buildings owned by companies.
///
/// # Arguments
/// * `buildings` - Mutable buildings (condition incremented).
/// * `companies` - Mutable companies (owner debited, contractor credited).
/// * `config` - Maintenance configuration.
///
/// # Double-Entry
/// * Debit building owner via `debit_company_by_id` (syncs bank balances).
/// * Credit a Construction-sector company in the same region via `credit_company_by_id`.
/// * Physical restoration applied to `Building.condition`.
///
/// # Rules
/// * Buildings with `condition < 1.0` are candidates for maintenance.
/// * The owning company pays `maintenance_cost_per_unit * condition_deficit`.
/// * If the company cannot afford full maintenance, partial maintenance is applied.
/// * Condition restoration is capped at `max_restore_per_turn` per building.
/// * If no Construction-sector company exists in the region, the debited funds are
///   still removed from the owner but not credited (leakage fallback).
pub fn process_maintenance_spending(
    buildings: &mut [Building],
    companies: &mut [Company],
    config: &MaintenanceConfig,
) {
    // Pre-compute construction company IDs per region for contractor lookup.
    let region_contractors: std::collections::HashMap<String, String> = {
        let mut map = std::collections::HashMap::new();
        for c in companies.iter() {
            if c.sector == Sector::Construction && !c.region_id.is_empty() {
                map.entry(c.region_id.clone()).or_insert_with(|| c.id.clone());
            }
        }
        map
    };

    for building in buildings {
        if building.condition >= 1.0 {
            continue;
        }
        if building.owner_id.is_empty() {
            continue;
        }
        let owner_id = &building.owner_id;

        // Phase 25: Strict realism — if no Construction-sector company exists
        // in this region, maintenance CANNOT happen. Do NOT debit the owner.
        // The building's condition is NOT restored — it continues to degrade
        // at the base rate from process_condition_degradation. The owner
        // suffers the physical consequences of neglected maintenance.
        let has_contractor = region_contractors.contains_key(&building.region_id);
        if !has_contractor {
            continue;
        }

        // Find the owner's liquid cash to determine affordability.
        let owner_cash = companies
            .iter()
            .find(|c| &c.id == owner_id)
            .map(|c| {
                c.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0)
                    + c.available_cash
            })
            .unwrap_or(0.0);

        let deficit = 1.0 - building.condition;
        let restore_amount = deficit.min(config.max_restore_per_turn);
        let cost = restore_amount * config.maintenance_cost_per_unit;

        if owner_cash < cost {
            // Partial maintenance based on available cash.
            let affordable_restore =
                (owner_cash / config.maintenance_cost_per_unit).min(restore_amount);
            let affordable_cost = affordable_restore * config.maintenance_cost_per_unit;
            let debited = debit_company_by_id(companies, owner_id, affordable_cost);
            if debited > 0.0 {
                if let Some(contractor_id) = region_contractors.get(&building.region_id) {
                    credit_company_by_id(companies, contractor_id, debited);
                }
            }
            building.condition = (building.condition + affordable_restore).min(1.0);
        } else {
            let debited = debit_company_by_id(companies, owner_id, cost);
            if debited > 0.0 {
                if let Some(contractor_id) = region_contractors.get(&building.region_id) {
                    credit_company_by_id(companies, contractor_id, debited);
                }
            }
            building.condition = (building.condition + restore_amount).min(1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_degradation() {
        let mut buildings = vec![Building {
            condition: 1.0,
            ..Building::default()
        }];
        let config = MaintenanceConfig::default();
        process_condition_degradation(&mut buildings, &config);
        assert!((buildings[0].condition - (1.0 - 0.002)).abs() < 0.0001);
    }

    #[test]
    fn test_degradation_floor() {
        let mut buildings = vec![Building {
            condition: 0.001,
            ..Building::default()
        }];
        let config = MaintenanceConfig::default();
        process_condition_degradation(&mut buildings, &config);
        assert_eq!(buildings[0].condition, 0.0);
    }
}

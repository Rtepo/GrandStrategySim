//! Phase 72: Proxy Wars — Real Costs for Sponsor States (Rule 1 compliance).
//!
//! When the AI executes `ArmRebels` or `FundSeparatists`, it CANNOT magically
//! spawn rebels. The sponsoring state must:
//!
//! - **ArmRebels**: Physically purchase `Rifles` and `Ammunition` from its
//!   military stockpile and transfer them to the rebel `PeasantBattalion`
//!   units. No magic weapon spawning.
//!
//! - **FundSeparatists**: Cash flows from the sponsor's treasury to a
//!   `RebellionState` in the target country. This increases `unrest_level`
//!   in the target region.
//!
//! - **PropagandaCampaigns**: Strictly debit the state treasury and credit
//!   the Media/Entertainment sector (handled in `propaganda.rs`).
//!
//! All transfers are double-entry. No resources or money are created.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::registries::enums::Commodity;

// ============================================================================
// PROXY WAR ACTION
// ============================================================================

/// Action a country can take to fund proxy wars in another country.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ProxyWarAction {
    /// Fund separatist movement in a target country's region.
    /// Cash flows from sponsor treasury to the rebellion state.
    FundSeparatists {
        /// Country funding the separatists.
        sponsor_country: String,
        /// Country where separatists are being funded.
        target_country: String,
        /// Region where the separatist movement is active.
        target_region: String,
        /// Amount of funding (debited from sponsor treasury).
        amount: f64,
    },
    /// Arm rebels with physical weapons from the sponsor's stockpile.
    /// Rifles and Ammunition are physically transferred — no magic spawning.
    ArmRebels {
        /// Country arming the rebels.
        sponsor_country: String,
        /// Country where rebels are being armed.
        target_country: String,
        /// Number of rifles to transfer (from sponsor's military_stockpile).
        rifles_quantity: f64,
        /// Amount of ammunition to transfer (from sponsor's military_stockpile).
        ammunition_quantity: f64,
    },
}

// ============================================================================
// PROXY WAR RESULT
// ============================================================================

/// Result of executing a proxy war action.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProxyWarResult {
    /// Whether the action was successfully executed.
    pub executed: bool,
    /// Amount debited from sponsor treasury (for FundSeparatists).
    pub treasury_debited: f64,
    /// Amount credited to rebellion state.
    pub rebellion_credited: f64,
    /// Physical commodities transferred (for ArmRebels).
    pub commodities_transferred: HashMap<Commodity, f64>,
    /// Unrest increase in the target region.
    pub unrest_increase: f64,
    /// Rebel units spawned (for ArmRebels).
    pub rebel_units_spawned: u32,
    /// Log messages.
    pub messages: Vec<String>,
}

// ============================================================================
// PROXY WAR CONFIG
// ============================================================================

/// Configuration for proxy war actions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProxyWarConfig {
    /// Unrest increase per unit of funding (scaled by amount).
    pub unrest_per_funding_unit: f64,
    /// Maximum unrest increase from a single funding action.
    pub max_unrest_per_action: f64,
    /// Multiplier for unrest when the target region is an Autonomous Republic.
    pub autonomous_republic_multiplier: f64,
    /// Manpower per rifle transferred (how many rebels can be armed per rifle).
    pub manpower_per_rifle: f64,
    /// Ammunition required per armed rebel.
    pub ammunition_per_rebel: f64,
}

impl Default for ProxyWarConfig {
    fn default() -> Self {
        Self {
            unrest_per_funding_unit: 0.001,
            max_unrest_per_action: 0.3,
            autonomous_republic_multiplier: 2.0,
            manpower_per_rifle: 1.0,
            ammunition_per_rebel: 50.0,
        }
    }
}

// ============================================================================
// FUND SEPARATISTS
// ============================================================================

/// Executes a FundSeparatists action.
///
/// # Cash Flow (Rule 1 — double-entry)
/// 1. Sponsor treasury is debited: `treasury.liquid_reserves -= amount`
/// 2. Rebellion state is credited: `rebellion_funds += amount`
/// 3. No money is created or destroyed.
///
/// # Unrest Impact
/// The funding increases unrest in the target region. If the region is an
/// Autonomous Republic, the unrest increase is multiplied.
///
/// # Arguments
/// * `sponsor_treasury` - Mutable sponsor treasury (will be debited).
/// * `rebellion_funds` - Mutable rebellion funds (will be credited).
/// * `is_autonomous_republic` - Whether the target region is an autonomous republic.
/// * `config` - Proxy war configuration.
/// * `action` - The fund separatists action.
///
/// # Returns
/// `ProxyWarResult` with the execution details.
pub fn fund_separatists(
    sponsor_treasury: &mut f64,
    rebellion_funds: &mut f64,
    is_autonomous_republic: bool,
    config: &ProxyWarConfig,
    action: &ProxyWarAction,
) -> ProxyWarResult {
    let mut result = ProxyWarResult::default();

    let (amount, sponsor, target, region) = match action {
        ProxyWarAction::FundSeparatists {
            sponsor_country,
            target_country,
            target_region,
            amount,
        } => (*amount, sponsor_country, target_country, target_region),
        _ => {
            result
                .messages
                .push("[PROXY] Invalid action type for fund_separatists".to_string());
            return result;
        }
    };

    // Check if sponsor has sufficient funds
    if *sponsor_treasury < amount {
        result.messages.push(format!(
            "[PROXY] {} lacks funds to fund separatists in {} (have {:.2}, need {:.2})",
            sponsor, target, *sponsor_treasury, amount
        ));
        return result;
    }

    // Debit sponsor treasury
    *sponsor_treasury -= amount;
    result.treasury_debited = amount;

    // Credit rebellion funds
    *rebellion_funds += amount;
    result.rebellion_credited = amount;

    // Calculate unrest increase
    let mut unrest = (amount * config.unrest_per_funding_unit).min(config.max_unrest_per_action);
    if is_autonomous_republic {
        unrest *= config.autonomous_republic_multiplier;
    }
    result.unrest_increase = unrest;

    result.executed = true;
    result.messages.push(format!(
        "[PROXY] {} funds separatists in {} (region {}) with {:.2}: unrest +{:.4}{}",
        sponsor,
        target,
        region,
        amount,
        unrest,
        if is_autonomous_republic {
            " (autonomous republic)"
        } else {
            ""
        }
    ));

    result
}

// ============================================================================
// ARM REBELS
// ============================================================================

/// Executes an ArmRebels action.
///
/// # Physical Commodity Transfer (Rule 1 — no magic spawning)
/// 1. Rifles and Ammunition are REMOVED from the sponsor's military stockpile.
/// 2. The transferred weapons are used to arm rebel PeasantBattalion units.
/// 3. If the sponsor lacks sufficient weapons, the action is partially executed
///    or aborted.
///
/// # Rebel Spawning
/// The number of rebels that can be armed is limited by the MINIMUM of:
/// - Rifles available / manpower_per_rifle
/// - Ammunition available / ammunition_per_rebel
///
/// # Arguments
/// * `sponsor_stockpile` - Mutable sponsor military stockpile (will be debited).
/// * `config` - Proxy war configuration.
/// * `action` - The arm rebels action.
///
/// # Returns
/// `ProxyWarResult` with the execution details.
pub fn arm_rebels(
    sponsor_stockpile: &mut HashMap<Commodity, f64>,
    config: &ProxyWarConfig,
    action: &ProxyWarAction,
) -> ProxyWarResult {
    let mut result = ProxyWarResult::default();

    let (rifles_qty, ammo_qty, sponsor, target) = match action {
        ProxyWarAction::ArmRebels {
            sponsor_country,
            target_country,
            rifles_quantity,
            ammunition_quantity,
        } => (
            *rifles_quantity,
            *ammunition_quantity,
            sponsor_country,
            target_country,
        ),
        _ => {
            result
                .messages
                .push("[PROXY] Invalid action type for arm_rebels".to_string());
            return result;
        }
    };

    // Check if sponsor has sufficient rifles
    let available_rifles = sponsor_stockpile
        .get(&Commodity::Rifles)
        .copied()
        .unwrap_or(0.0);
    if available_rifles < rifles_qty {
        result.messages.push(format!(
            "[PROXY] {} lacks rifles: have {:.0}, need {:.0}",
            sponsor, available_rifles, rifles_qty
        ));
        // Partial execution: use what's available
        if available_rifles <= 0.0 {
            return result;
        }
    }
    let rifles_to_transfer = available_rifles.min(rifles_qty);

    // Check if sponsor has sufficient ammunition
    let available_ammo = sponsor_stockpile
        .get(&Commodity::Ammunition)
        .copied()
        .unwrap_or(0.0);
    if available_ammo < ammo_qty {
        result.messages.push(format!(
            "[PROXY] {} lacks ammunition: have {:.0}, need {:.0}",
            sponsor, available_ammo, ammo_qty
        ));
        if available_ammo <= 0.0 {
            return result;
        }
    }
    let ammo_to_transfer = available_ammo.min(ammo_qty);

    // Debit physical commodities from sponsor stockpile (Rule 1 — no spawning)
    // Phase 94: Rule 20 clamp — stockpile cannot go negative.
    {
        let rifles = sponsor_stockpile.entry(Commodity::Rifles).or_insert(0.0);
        *rifles = (*rifles - rifles_to_transfer).max(0.0);
    }
    {
        let ammo = sponsor_stockpile
            .entry(Commodity::Ammunition)
            .or_insert(0.0);
        *ammo = (*ammo - ammo_to_transfer).max(0.0);
    }

    // Record transferred commodities
    result
        .commodities_transferred
        .insert(Commodity::Rifles, rifles_to_transfer);
    result
        .commodities_transferred
        .insert(Commodity::Ammunition, ammo_to_transfer);

    // Calculate how many rebels can be armed
    let rebels_from_rifles = (rifles_to_transfer / config.manpower_per_rifle) as i64;
    let rebels_from_ammo = (ammo_to_transfer / config.ammunition_per_rebel) as i64;
    let armed_rebels = rebels_from_rifles.min(rebels_from_ammo).max(0);

    result.rebel_units_spawned = if armed_rebels > 0 {
        ((armed_rebels as f64 / 1000.0).ceil() as u32).max(1) // One PeasantBattalion per ~1000 rebels
    } else {
        0
    };

    result.executed = true;
    result.messages.push(format!(
        "[PROXY] {} arms rebels in {}: {:.0} rifles, {:.0} ammo → {} armed rebels, {} battalions",
        sponsor,
        target,
        rifles_to_transfer,
        ammo_to_transfer,
        armed_rebels,
        result.rebel_units_spawned
    ));

    result
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stockpile(rifles: f64, ammo: f64) -> HashMap<Commodity, f64> {
        let mut s = HashMap::new();
        s.insert(Commodity::Rifles, rifles);
        s.insert(Commodity::Ammunition, ammo);
        s
    }

    #[test]
    fn test_fund_separatists_executes() {
        let mut treasury = 10_000.0;
        let mut rebellion_funds = 0.0;
        let config = ProxyWarConfig::default();
        let action = ProxyWarAction::FundSeparatists {
            sponsor_country: "Sponsor".to_string(),
            target_country: "target".to_string(),
            target_region: "region_1".to_string(),
            amount: 1000.0,
        };

        let result = fund_separatists(&mut treasury, &mut rebellion_funds, false, &config, &action);

        assert!(result.executed);
        assert_eq!(result.treasury_debited, 1000.0);
        assert_eq!(result.rebellion_credited, 1000.0);
        assert_eq!(treasury, 9000.0);
        assert_eq!(rebellion_funds, 1000.0);
    }

    #[test]
    fn test_fund_separatists_insufficient_funds() {
        let mut treasury = 100.0;
        let mut rebellion_funds = 0.0;
        let config = ProxyWarConfig::default();
        let action = ProxyWarAction::FundSeparatists {
            sponsor_country: "Sponsor".to_string(),
            target_country: "target".to_string(),
            target_region: "region_1".to_string(),
            amount: 1000.0,
        };

        let result = fund_separatists(&mut treasury, &mut rebellion_funds, false, &config, &action);

        assert!(!result.executed);
        assert_eq!(treasury, 100.0, "Treasury must not be debited on failure");
    }

    #[test]
    fn test_fund_separatists_autonomous_republic_multiplier() {
        let treasury = 10_000.0;
        let rebellion_funds = 0.0;
        let config = ProxyWarConfig::default();
        let action = ProxyWarAction::FundSeparatists {
            sponsor_country: "Sponsor".to_string(),
            target_country: "target".to_string(),
            target_region: "region_1".to_string(),
            amount: 1000.0,
        };

        let result_normal = fund_separatists(
            &mut treasury.clone(),
            &mut rebellion_funds.clone(),
            false,
            &config,
            &action,
        );
        let result_autonomous = fund_separatists(
            &mut treasury.clone(),
            &mut rebellion_funds.clone(),
            true,
            &config,
            &action,
        );

        assert!(
            result_autonomous.unrest_increase > result_normal.unrest_increase,
            "Autonomous republic must have higher unrest multiplier"
        );
    }

    #[test]
    fn test_fund_separatists_double_entry() {
        let mut treasury = 10_000.0;
        let mut rebellion_funds = 0.0;
        let config = ProxyWarConfig::default();
        let action = ProxyWarAction::FundSeparatists {
            sponsor_country: "Sponsor".to_string(),
            target_country: "target".to_string(),
            target_region: "region_1".to_string(),
            amount: 1000.0,
        };

        let result = fund_separatists(&mut treasury, &mut rebellion_funds, false, &config, &action);

        // Double-entry: treasury debit must equal rebellion credit
        assert_eq!(result.treasury_debited, result.rebellion_credited);
    }

    #[test]
    fn test_arm_rebels_transfers_physical_commodities() {
        let mut stockpile = make_stockpile(5000.0, 100_000.0);
        let config = ProxyWarConfig::default();
        let action = ProxyWarAction::ArmRebels {
            sponsor_country: "Sponsor".to_string(),
            target_country: "target".to_string(),
            rifles_quantity: 1000.0,
            ammunition_quantity: 50_000.0,
        };

        let result = arm_rebels(&mut stockpile, &config, &action);

        assert!(result.executed);
        // Rifles must be removed from stockpile (no magic spawning)
        assert!(
            stockpile.get(&Commodity::Rifles).unwrap() < &5000.0,
            "Rifles must be debited from sponsor stockpile"
        );
        assert!(
            stockpile.get(&Commodity::Ammunition).unwrap() < &100_000.0,
            "Ammunition must be debited from sponsor stockpile"
        );
        // Transferred commodities must be recorded
        assert!(
            result
                .commodities_transferred
                .get(&Commodity::Rifles)
                .unwrap()
                > &0.0
        );
        assert!(
            result
                .commodities_transferred
                .get(&Commodity::Ammunition)
                .unwrap()
                > &0.0
        );
    }

    #[test]
    fn test_arm_rebels_insufficient_rifles() {
        let mut stockpile = make_stockpile(100.0, 100_000.0); // Only 100 rifles
        let config = ProxyWarConfig::default();
        let action = ProxyWarAction::ArmRebels {
            sponsor_country: "Sponsor".to_string(),
            target_country: "target".to_string(),
            rifles_quantity: 1000.0,
            ammunition_quantity: 50_000.0,
        };

        let result = arm_rebels(&mut stockpile, &config, &action);

        // Should partially execute with available rifles
        assert!(result.executed);
        let transferred_rifles = result
            .commodities_transferred
            .get(&Commodity::Rifles)
            .unwrap();
        assert!(
            *transferred_rifles <= 100.0,
            "Cannot transfer more rifles than available"
        );
    }

    #[test]
    fn test_arm_rebels_no_rifles_aborts() {
        let mut stockpile = make_stockpile(0.0, 100_000.0); // No rifles
        let config = ProxyWarConfig::default();
        let action = ProxyWarAction::ArmRebels {
            sponsor_country: "Sponsor".to_string(),
            target_country: "target".to_string(),
            rifles_quantity: 1000.0,
            ammunition_quantity: 50_000.0,
        };

        let result = arm_rebels(&mut stockpile, &config, &action);

        assert!(!result.executed, "Must abort when no rifles available");
        assert_eq!(result.commodities_transferred.len(), 0);
    }

    #[test]
    fn test_arm_rebels_no_ammo_aborts() {
        let mut stockpile = make_stockpile(5000.0, 0.0); // No ammo
        let config = ProxyWarConfig::default();
        let action = ProxyWarAction::ArmRebels {
            sponsor_country: "Sponsor".to_string(),
            target_country: "target".to_string(),
            rifles_quantity: 1000.0,
            ammunition_quantity: 50_000.0,
        };

        let result = arm_rebels(&mut stockpile, &config, &action);

        assert!(!result.executed, "Must abort when no ammunition available");
    }

    #[test]
    fn test_arm_rebels_rebel_count_limited_by_min() {
        let mut stockpile = make_stockpile(1000.0, 10_000.0); // 1000 rifles, but only 10k ammo
        let config = ProxyWarConfig::default();
        // ammunition_per_rebel = 50.0, so 10k ammo → 200 rebels
        // manpower_per_rifle = 1.0, so 1000 rifles → 1000 rebels
        // Min = 200 rebels
        let action = ProxyWarAction::ArmRebels {
            sponsor_country: "Sponsor".to_string(),
            target_country: "target".to_string(),
            rifles_quantity: 1000.0,
            ammunition_quantity: 10_000.0,
        };

        let result = arm_rebels(&mut stockpile, &config, &action);

        assert!(result.executed);
        assert!(result.rebel_units_spawned > 0, "Must spawn rebel units");
        // With 200 rebels, we should get 1 battalion (ceil(200/1000) = 1)
        assert_eq!(result.rebel_units_spawned, 1);
    }

    #[test]
    fn test_arm_rebels_double_entry_physical() {
        let mut stockpile = make_stockpile(5000.0, 100_000.0);
        let config = ProxyWarConfig::default();
        let action = ProxyWarAction::ArmRebels {
            sponsor_country: "Sponsor".to_string(),
            target_country: "target".to_string(),
            rifles_quantity: 1000.0,
            ammunition_quantity: 50_000.0,
        };

        let initial_rifles = *stockpile.get(&Commodity::Rifles).unwrap();
        let initial_ammo = *stockpile.get(&Commodity::Ammunition).unwrap();

        let result = arm_rebels(&mut stockpile, &config, &action);

        let final_rifles = *stockpile.get(&Commodity::Rifles).unwrap();
        let final_ammo = *stockpile.get(&Commodity::Ammunition).unwrap();

        // Physical conservation: stockpile decrease must equal transferred amount
        let rifles_decrease = initial_rifles - final_rifles;
        let ammo_decrease = initial_ammo - final_ammo;
        let rifles_transferred = result
            .commodities_transferred
            .get(&Commodity::Rifles)
            .copied()
            .unwrap_or(0.0);
        let ammo_transferred = result
            .commodities_transferred
            .get(&Commodity::Ammunition)
            .copied()
            .unwrap_or(0.0);
        assert!(
            (rifles_decrease - rifles_transferred).abs() < 0.01,
            "Physical conservation: rifles decrease must equal transferred"
        );
        assert!(
            (ammo_decrease - ammo_transferred).abs() < 0.01,
            "Physical conservation: ammo decrease must equal transferred"
        );
    }
}

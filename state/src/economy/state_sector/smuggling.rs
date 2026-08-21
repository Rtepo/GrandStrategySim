//! Smuggling and grey economy mechanics (Phase 15B).
//!
//! Smuggling represents the grey economy: goods crossing borders without
//! paying tariffs. Border enforcement capacity (from border_guard buildings)
//! determines how much smuggling is intercepted. Confiscated goods are added
//! to the state treasury inventory; recovered tariffs go to the treasury.
//!
//! # Double-Entry Accounting
//!
//! * Smuggled goods that slip through: remain in the seller's inventory
//!   (no tariff paid — this is the "loss" to the state).
//! * Confiscated goods: removed from smuggler, added to state inventory.
//! * Recovered tariffs: debited from smuggler's cash, credited to treasury.

#![allow(missing_docs)]

use crate::entities::Building;
use crate::registries::enums::Commodity;
use crate::state::Country;
use crate::economy::sum_border_enforcement_capacity;
use std::collections::HashMap;

/// Maximum fraction of trade that can be smuggled (bypassing tariffs).
const MAX_SMUGGLING_RATE: f64 = 0.15;
/// Value of goods that one unit of border enforcement can intercept.
const ENFORCEMENT_INTERCEPT_VALUE: f64 = 50_000.0;

/// Sum CustomsCapacity from all buildings' last_production.
///
/// # Arguments
/// * `buildings` - Slice of buildings to scan.
///
/// # Returns
/// Total customs capacity.
pub fn sum_customs_capacity(buildings: &[Building]) -> f64 {
    buildings
        .iter()
        .map(|b| {
            *b.last_production
                .get(&Commodity::CustomsCapacity)
                .unwrap_or(&0.0)
        })
        .sum()
}

/// Result of a smuggling turn for one country.
#[derive(Debug, Clone, Default)]
pub struct SmugglingTurnResult {
    /// Total value of goods smuggled (before interception).
    pub smuggling_value: f64,
    /// Value of smuggling intercepted by border enforcement.
    pub intercepted_value: f64,
    /// Value of goods confiscated (added to state inventory).
    pub confiscated_value: f64,
    /// Tariff revenue recovered from intercepted smuggling.
    pub recovered_tariffs: f64,
    /// Effective tariff loss (tariffs not collected due to smuggling).
    pub tariff_loss: f64,
}

/// Process smuggling for one country.
///
/// # Arguments
/// * `country` - Mutable country (for tariff rates, treasury, border state).
/// * `buildings` - Buildings for border enforcement capacity.
/// * `trade_volume` - Total trade volume this turn (imports + exports).
///
/// # Returns
/// `SmugglingTurnResult` with diagnostics.
///
/// # Rules
/// * Smuggling rate is proportional to trade volume, capped at `MAX_SMUGGLING_RATE`.
/// * Border enforcement intercepts smuggling up to its capacity.
/// * Confiscated goods value is credited to treasury as recovered revenue.
/// * Recovered tariffs = intercepted_value * average_tariff_rate.
/// * Tariff loss = un-intercepted smuggling value * average_tariff_rate.
pub fn process_smuggling_turn(
    country: &mut Country,
    buildings: &[Building],
    trade_volume: f64,
) -> SmugglingTurnResult {
    let mut result = SmugglingTurnResult::default();

    if trade_volume <= 0.0 {
        return result;
    }

    // Get border enforcement capacity
    let border_cap = sum_border_enforcement_capacity(buildings);

    // Calculate smuggling attempt: fraction of trade that bypasses tariffs
    let smuggling_attempt = trade_volume * MAX_SMUGGLING_RATE;
    result.smuggling_value = smuggling_attempt;

    // Calculate interception: enforcement capacity limits how much can be caught
    let max_intercept = border_cap * ENFORCEMENT_INTERCEPT_VALUE;
    let intercepted = smuggling_attempt.min(max_intercept);
    result.intercepted_value = intercepted;

    // Average tariff rate from trade policy
    let avg_tariff_rate = calculate_average_tariff_rate(country);

    // Confiscated goods: value goes to treasury
    result.confiscated_value = intercepted;
    country.budget.liquid_reserves += intercepted;

    // Recovered tariffs on intercepted smuggling
    result.recovered_tariffs = intercepted * avg_tariff_rate;
    country.budget.liquid_reserves += result.recovered_tariffs;

    // Tariff loss from un-intercepted smuggling
    let unintercepted = smuggling_attempt - intercepted;
    result.tariff_loss = unintercepted * avg_tariff_rate;

    // Update border state
    if let Some(border_state) = &mut country.politics.border_state {
        border_state.smuggling_intercepted = intercepted;
        border_state.smuggling_value = smuggling_attempt;
    }

    result
}

/// Calculate the average tariff rate from a country's trade policy.
///
/// # Arguments
/// * `country` - Country with trade policy.
///
/// # Returns
/// Average tariff rate (0.0 to 1.0).
fn calculate_average_tariff_rate(country: &Country) -> f64 {
    // Average across all import tariff rates; default to 0.05 if none set
    let tariffs: Vec<f64> = country.trade_policy.import_tariffs.values().copied().collect();
    if tariffs.is_empty() {
        return 0.05;
    }
    let avg = tariffs.iter().sum::<f64>() / tariffs.len() as f64;
    avg.clamp(0.0, 1.0)
}

/// Process customs-based tax evasion recovery.
///
/// Customs capacity affects how much evaded tax can be recovered.
/// This scales the existing tax evasion calculation: higher customs capacity
/// means more evaded taxes are detected and recovered.
///
/// # Arguments
/// * `country` - Mutable country (for customs state, treasury).
/// * `buildings` - Buildings for customs capacity.
/// * `taxes_evaded` - Total taxes evaded this turn (from tax collection).
///
/// # Returns
/// Amount of evaded taxes recovered.
///
/// # Rules
/// * Recovery rate = customs_capacity / (customs_capacity + evasion_base).
/// * Recovered taxes are credited to treasury.
/// * Customs state is updated with detection and recovery amounts.
pub fn process_customs_evasion_recovery(
    country: &mut Country,
    buildings: &[Building],
    taxes_evaded: f64,
) -> f64 {
    if taxes_evaded <= 0.0 {
        return 0.0;
    }

    let customs_cap = sum_customs_capacity(buildings);

    // Recovery rate: diminishing returns from customs capacity
    // 50% recovery when customs_cap equals the evasion base
    let evasion_base = 100.0; // Base difficulty of detecting evasion
    let recovery_rate = (customs_cap / (customs_cap + evasion_base)).clamp(0.0, 0.9);

    let recovered = taxes_evaded * recovery_rate;

    // Double-entry: recovered taxes go to treasury
    country.budget.liquid_reserves += recovered;

    // Update customs state
    if let Some(customs_state) = &mut country.politics.customs_state {
        customs_state.customs_capacity = customs_cap;
        customs_state.evasion_detected = taxes_evaded;
        customs_state.evasion_recovered = recovered;
        customs_state.inspections_conducted = customs_cap as u32;
    }

    recovered
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Building;
    use crate::state::Country;

    #[test]
    fn test_sum_customs_capacity() {
        let mut b1 = Building::default();
        b1.last_production
            .insert(Commodity::CustomsCapacity, 10.0);
        let mut b2 = Building::default();
        b2.last_production
            .insert(Commodity::CustomsCapacity, 5.0);
        assert_eq!(sum_customs_capacity(&[b1, b2]), 15.0);
    }

    #[test]
    fn test_smuggling_zero_trade() {
        let mut country = Country::mock_for_tests();
        let buildings = vec![];
        let result = process_smuggling_turn(&mut country, &buildings, 0.0);
        assert_eq!(result.smuggling_value, 0.0);
    }

    #[test]
    fn test_smuggling_with_enforcement() {
        let mut country = Country::mock_for_tests();
        let mut b = Building::default();
        b.last_production
            .insert(Commodity::BorderEnforcementCapacity, 100.0);
        let buildings = vec![b];

        let result = process_smuggling_turn(&mut country, &buildings, 1_000_000.0);
        assert!(result.smuggling_value > 0.0);
        assert!(result.intercepted_value > 0.0);
        assert!(result.intercepted_value <= result.smuggling_value);
    }

    #[test]
    fn test_smuggling_no_enforcement() {
        let mut country = Country::mock_for_tests();
        let buildings = vec![];

        let result = process_smuggling_turn(&mut country, &buildings, 1_000_000.0);
        assert_eq!(result.intercepted_value, 0.0);
        assert!(result.tariff_loss > 0.0);
    }

    #[test]
    fn test_customs_evasion_recovery() {
        let mut country = Country::mock_for_tests();
        let mut b = Building::default();
        b.last_production
            .insert(Commodity::CustomsCapacity, 100.0);
        let buildings = vec![b];

        let initial_reserves = country.budget.liquid_reserves;
        let recovered = process_customs_evasion_recovery(&mut country, &buildings, 100_000.0);
        assert!(recovered > 0.0);
        assert!(country.budget.liquid_reserves > initial_reserves);
    }

    #[test]
    fn test_customs_evasion_zero_evaded() {
        let mut country = Country::mock_for_tests();
        let buildings = vec![];
        let recovered = process_customs_evasion_recovery(&mut country, &buildings, 0.0);
        assert_eq!(recovered, 0.0);
    }
}

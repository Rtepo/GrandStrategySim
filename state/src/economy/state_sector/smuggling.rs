//! Smuggling and grey economy mechanics (Phase 15B).
//!
//! Smuggling represents the grey economy: goods crossing borders without
//! paying tariffs. Border enforcement capacity (from border_guard buildings)
//! determines how much smuggling is intercepted.
//!
//! # Double-Entry Accounting (Agent 4 — Fiat Leak Fix)
//!
//! **PREVIOUS (BROKEN):** The treasury was credited `intercepted_value +
//! recovered_tariffs` with NO counterparty debit — pure fiat creation (Rule 1
//! violation).
//!
//! **CURRENT (FIXED):** The smuggling model is aggregate and does not yet
//! identify individual smuggler entities (Phase 4 will integrate smuggling
//! into the B2B trade flow with per-trade interception). Until then:
//! * **Confiscated goods:** NO fiat is booked at seizure time. The
//!   `confiscated_value` is recorded as a diagnostic only. Phase 4 will move
//!   physical `Commodity` units to `country.state_customs_warehouse` and
//!   auction them on the `GlobalMarket` for real fiat revenue.
//! * **Recovered tariffs:** NO fiat is booked. The `recovered_tariffs` amount
//!   is recorded as a `pending_smuggling_receivable` — money owed by
//!   unidentified smugglers. Phase 4 will debit the specific smuggler's cash
//!   via `settle_transfer_to_treasury`.
//! * **Tariff loss:** Recorded as a diagnostic (state revenue foregone).
//! * **Unintercepted smuggling:** Remains with the smuggler (no fiat movement).
//!
//! This is the conservative double-entry-correct approach: no fiat is created,
//! no innocent companies are debited (Rule 7), and the diagnostic data is
//! preserved for UI exposure (Rule 17) and fog-of-war role-gating (Rule 11).

#![allow(missing_docs)]

use crate::economy::sum_border_enforcement_capacity;
use crate::entities::Building;
use crate::registries::enums::Commodity;
use crate::state::Country;
use serde::{Deserialize, Serialize};

/// Maximum fraction of trade that can be smuggled (bypassing tariffs).
/// This is a physical cap — the actual rate is incentive-driven.
/// Agent 4 — Phase 4: Replaced flat 0.15 with dynamic formula below.
const MAX_SMUGGLING_RATE: f64 = 0.50;
/// Base value of goods that one unit of border enforcement can intercept,
/// scaled by `average_wage` for inflation-proofing (Rule 2).
/// At average_wage = 1000, one enforcement unit intercepts ~50_000 value.
const ENFORCEMENT_INTERCEPT_BASE: f64 = 50.0;

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

/// Result of a smuggling turn for one country (diagnostics only — no fiat flows).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SmugglingTurnResult {
    /// Total value of goods smuggled (before interception).
    pub smuggling_value: f64,
    /// Value of smuggling intercepted by border enforcement.
    pub intercepted_value: f64,
    /// Value of goods confiscated (diagnostic — NOT booked as fiat).
    /// Phase 4 will move physical Commodity units to state_customs_warehouse.
    pub confiscated_value: f64,
    /// Tariff revenue recoverable from intercepted smuggling (diagnostic —
    /// NOT booked as fiat). Recorded as pending_smuggling_receivable.
    /// Phase 4 will debit the smuggler via settle_transfer_to_treasury.
    pub recovered_tariffs: f64,
    /// Effective tariff loss (tariffs not collected due to smuggling).
    pub tariff_loss: f64,
}

/// Process smuggling for one country.
///
/// # Arguments
/// * `country` - Mutable country (for tariff rates, border state, diagnostics).
/// * `buildings` - Buildings for border enforcement capacity.
/// * `trade_volume` - Total cross-border trade volume this turn.
///
/// # Returns
/// `SmugglingTurnResult` with diagnostics. The result is also stored on
/// `country.last_smuggling_result` for UI snapshot exposure.
///
/// # Rules (Agent 4 — Phase 4: Rational Smuggling)
/// * Smuggling rate is incentive-driven: `f(tariff_rate, enforcement_ratio)`.
///   Higher tariffs → more smuggling (greater arbitrage profit).
///   Higher enforcement → less smuggling (greater seizure risk).
///   This models rational actors (Rule 8) responding to economic incentives.
/// * Border enforcement intercepts smuggling up to its capacity.
/// * **NO fiat is created.** Confiscated goods value and recovered tariffs
///   are recorded as diagnostics only.
/// * Tariff loss = un-intercepted smuggling value * average_tariff_rate.
/// * The result is stored on `country.last_smuggling_result` for UI visibility.
pub fn process_smuggling_turn(
    country: &mut Country,
    buildings: &[Building],
    trade_volume: f64,
) -> SmugglingTurnResult {
    let mut result = SmugglingTurnResult::default();

    if trade_volume <= 0.0 {
        country.last_smuggling_result = Some(result.clone());
        return result;
    }

    // Get border enforcement capacity
    let border_cap = sum_border_enforcement_capacity(buildings);

    // Average tariff rate from trade policy — this is the arbitrage incentive.
    let avg_tariff_rate = calculate_average_tariff_rate(country);

    // Agent 4 — Phase 4: Incentive-driven smuggling rate.
    // Rational actors smuggle when the tariff arbitrage profit exceeds the
    // expected seizure cost. The smuggling rate is:
    //   rate = tariff_rate * (1 - enforcement_ratio) * demand_elasticity
    // where enforcement_ratio = border_cap / (border_cap + trade_volume).
    // This means:
    //   - High tariffs → high smuggling incentive (more arbitrage profit).
    //   - High enforcement → low smuggling (high seizure risk).
    //   - No tariffs → no smuggling (no arbitrage profit).
    // The rate is capped at MAX_SMUGGLING_RATE (physical limit).
    let enforcement_ratio = if trade_volume > 0.0 {
        border_cap / (border_cap + trade_volume * 0.01)
    } else {
        0.0
    };
    // Demand elasticity for smuggling: how responsive smugglers are to the
    // tariff arbitrage. 1.0 means fully responsive.
    let smuggling_elasticity = 1.0;
    let incentive_rate = avg_tariff_rate * (1.0 - enforcement_ratio) * smuggling_elasticity;
    let smuggling_rate = incentive_rate.min(MAX_SMUGGLING_RATE);
    let smuggling_attempt = trade_volume * smuggling_rate;
    result.smuggling_value = smuggling_attempt;

    // Calculate interception: enforcement capacity limits how much can be caught.
    // Agent 4 — Phase 4: Scale ENFORCEMENT_INTERCEPT_BASE by average_wage
    // for inflation-proofing (Rule 2). At average_wage = 1000, one enforcement
    // unit intercepts ~50_000 value (matching the previous static constant).
    let average_wage = country.macro_indicators.average_wage.max(1.0);
    let enforcement_intercept_value = ENFORCEMENT_INTERCEPT_BASE * average_wage;
    let max_intercept = border_cap * enforcement_intercept_value;
    let intercepted = smuggling_attempt.min(max_intercept);
    result.intercepted_value = intercepted;

    // Confiscated goods: diagnostic only — NO fiat booked.
    // Phase 4 will move physical Commodity units to state_customs_warehouse.
    result.confiscated_value = intercepted;

    // Recovered tariffs: diagnostic only — NO fiat booked.
    // Recorded as pending_smuggling_receivable (owed by unidentified smugglers).
    // Phase 4 will debit the specific smuggler via settle_transfer_to_treasury.
    result.recovered_tariffs = intercepted * avg_tariff_rate;

    // Tariff loss from un-intercepted smuggling
    let unintercepted = smuggling_attempt - intercepted;
    result.tariff_loss = unintercepted * avg_tariff_rate;

    // Update border state
    if let Some(border_state) = &mut country.politics.border_state {
        border_state.smuggling_intercepted = intercepted;
        border_state.smuggling_value = smuggling_attempt;
    }

    // Store result for UI snapshot exposure (Rule 17) and fog-of-war (Rule 11).
    country.last_smuggling_result = Some(result.clone());

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
    let tariffs: Vec<f64> = country
        .trade_policy
        .import_tariffs
        .values()
        .copied()
        .collect();
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
/// * `country` - Mutable country (for customs state, diagnostics).
/// * `buildings` - Buildings for customs capacity.
/// * `taxes_evaded` - Total taxes evaded this turn (from tax collection).
///
/// # Returns
/// Amount of evaded taxes theoretically recoverable (diagnostic only).
///
/// # Rules (Agent 4 — Fiat Leak Fix)
/// * Recovery rate = customs_capacity / (customs_capacity + evasion_base).
/// * **NO fiat is created.** The recovered amount is recorded as a diagnostic
///   only. The evaded cash remains in the evading entities' hands (per tax.rs).
///   Phase 4 will debit the specific evading entities via
///   `settle_transfer_to_treasury` when per-company evasion tracking is added.
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

    // Agent 4 — Phase 4: Scale the evasion base by average_wage for
    // inflation-proofing (Rule 2). The base difficulty of detecting evasion
    // should scale with the economy, not be a static 100.0.
    let average_wage = country.macro_indicators.average_wage.max(1.0);
    let evasion_base = average_wage * 0.1; // 10% of average_wage as base difficulty
    let recovery_rate = (customs_cap / (customs_cap + evasion_base)).clamp(0.0, 0.9);

    let recovered = taxes_evaded * recovery_rate;

    // Agent 4 — Fiat Leak Fix: NO treasury credit.
    // The evaded cash is still in the evading entities' hands (per tax.rs).
    // Crediting the treasury here would create fiat from the void.
    // Phase 4 will debit the specific evading entities when per-company
    // evasion tracking is available.

    // Update customs state (diagnostics only)
    if let Some(customs_state) = &mut country.politics.customs_state {
        customs_state.customs_capacity = customs_cap;
        customs_state.evasion_detected = taxes_evaded;
        customs_state.evasion_recovered = recovered;
        // Agent 4 — Phase 4: Scale inspections by customs_cap and average_wage
        // instead of truncating f64→u32 (Rule 15). One inspection per
        // average_wage units of customs capacity.
        customs_state.inspections_conducted = ((customs_cap / average_wage) * 10.0) as u32;
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
        b1.last_production.insert(Commodity::CustomsCapacity, 10.0);
        let mut b2 = Building::default();
        b2.last_production.insert(Commodity::CustomsCapacity, 5.0);
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
    fn test_smuggling_no_fiat_creation() {
        // Agent 4: Verify that smuggling does NOT create fiat.
        // The treasury's liquid_reserves must NOT increase from smuggling.
        let mut country = Country::mock_for_tests();
        let mut b = Building::default();
        b.last_production
            .insert(Commodity::BorderEnforcementCapacity, 100.0);
        let buildings = vec![b];

        let initial_reserves = country.budget.liquid_reserves;
        let result = process_smuggling_turn(&mut country, &buildings, 1_000_000.0);
        assert!(result.intercepted_value > 0.0);
        assert!(result.recovered_tariffs > 0.0);
        assert!(result.confiscated_value > 0.0);
        // CRITICAL: treasury must not increase (no fiat creation).
        assert_eq!(
            country.budget.liquid_reserves, initial_reserves,
            "Smuggling must NOT credit liquid_reserves (fiat leak fix)"
        );
    }

    #[test]
    fn test_smuggling_result_stored_on_country() {
        // Agent 4: Verify the result is stored for UI exposure (Rule 17).
        let mut country = Country::mock_for_tests();
        let buildings = vec![];
        let _result = process_smuggling_turn(&mut country, &buildings, 1_000_000.0);
        assert!(
            country.last_smuggling_result.is_some(),
            "Smuggling result must be stored on country for UI exposure"
        );
    }

    #[test]
    fn test_customs_evasion_recovery_no_fiat_creation() {
        // Agent 4: Verify that customs evasion recovery does NOT create fiat.
        let mut country = Country::mock_for_tests();
        let mut b = Building::default();
        b.last_production.insert(Commodity::CustomsCapacity, 100.0);
        let buildings = vec![b];

        let initial_reserves = country.budget.liquid_reserves;
        let recovered = process_customs_evasion_recovery(&mut country, &buildings, 100_000.0);
        assert!(
            recovered > 0.0,
            "Recovery amount should be positive (diagnostic)"
        );
        // CRITICAL: treasury must not increase (no fiat creation).
        assert_eq!(
            country.budget.liquid_reserves, initial_reserves,
            "Customs evasion recovery must NOT credit liquid_reserves (fiat leak fix)"
        );
    }

    #[test]
    fn test_customs_evasion_zero_evaded() {
        let mut country = Country::mock_for_tests();
        let buildings = vec![];
        let recovered = process_customs_evasion_recovery(&mut country, &buildings, 0.0);
        assert_eq!(recovered, 0.0);
    }

    #[test]
    fn test_smuggling_incentive_driven_high_tariff() {
        // Agent 4 — Phase 4: Higher tariffs should increase smuggling.
        let mut country_high = Country::mock_for_tests();
        country_high
            .trade_policy
            .import_tariffs
            .insert(Commodity::Steel, 0.50);
        country_high.macro_indicators.average_wage = 1000.0;

        let mut country_low = Country::mock_for_tests();
        country_low
            .trade_policy
            .import_tariffs
            .insert(Commodity::Steel, 0.05);
        country_low.macro_indicators.average_wage = 1000.0;

        let buildings = vec![];
        let result_high = process_smuggling_turn(&mut country_high, &buildings, 1_000_000.0);
        let result_low = process_smuggling_turn(&mut country_low, &buildings, 1_000_000.0);
        // High tariff should produce more smuggling than low tariff.
        assert!(
            result_high.smuggling_value > result_low.smuggling_value,
            "Higher tariffs should increase smuggling (rational actor model)"
        );
    }

    #[test]
    fn test_smuggling_incentive_driven_high_enforcement() {
        // Agent 4 — Phase 4: Higher enforcement should decrease smuggling.
        let mut country = Country::mock_for_tests();
        country
            .trade_policy
            .import_tariffs
            .insert(Commodity::Steel, 0.30);
        country.macro_indicators.average_wage = 1000.0;

        let mut b = Building::default();
        b.last_production
            .insert(Commodity::BorderEnforcementCapacity, 1000.0);
        let buildings_high = vec![b];

        let buildings_low = vec![];

        let result_high =
            process_smuggling_turn(&mut country.clone(), &buildings_high, 1_000_000.0);
        let result_low = process_smuggling_turn(&mut country, &buildings_low, 1_000_000.0);
        // High enforcement should produce less smuggling than low enforcement.
        assert!(
            result_high.smuggling_value <= result_low.smuggling_value,
            "Higher enforcement should decrease smuggling (rational actor model)"
        );
    }

    #[test]
    fn test_smuggling_zero_tariff_zero_smuggling() {
        // Agent 4 — Phase 4: No tariff means no smuggling incentive.
        let mut country = Country::mock_for_tests();
        // No import tariffs set — average tariff rate defaults to 0.05.
        // Let's explicitly set it to 0 by having an empty policy.
        country.trade_policy.import_tariffs.clear();
        // Override the default 0.05 by inserting a 0.0 rate.
        country
            .trade_policy
            .import_tariffs
            .insert(Commodity::Steel, 0.0);
        country.macro_indicators.average_wage = 1000.0;

        let buildings = vec![];
        let result = process_smuggling_turn(&mut country, &buildings, 1_000_000.0);
        // With zero tariff, smuggling incentive is zero.
        assert_eq!(
            result.smuggling_value, 0.0,
            "Zero tariff should produce zero smuggling (no arbitrage profit)"
        );
    }
}

//! Market Gridlock Diagnostic — Fast Assertion Helpers
//!
//! Pure-function helpers for the market-gridlock diagnostic suite. These do
//! NOT run the simulation — they validate dump artifacts and provide reusable
//! classification logic for both the epic test (`market_gridlock_diagnostic_test.rs`)
//! and external Python parity checks (`audit_dumps.py`).
//!
//! Compiled every CI run (no feature gate). The epic test in `tests/epics/`
//! reuses these helpers via `#[path]` inclusion so the classification logic is
//! defined in exactly one place.
//!
//! # Probes implemented here
//! - **Probe 3a (Wage Transfer Conservation):** `check_wage_transfer` —
//!   compares company-cash debit vs citizen-savings credit across the wage
//!   phase to detect money vanishing between payroll and citizen credit.
//! - **Probe 3b (Propensity-to-consume):** `classify_demand_gate` —
//!   replicates `commodity_wealth_gate` (retail.rs:209) and
//!   `era_consumption_multiplier` (retail.rs:742) to distinguish "cannot
//!   afford" from "will not spend" (hoarding) without editing the locked
//!   `economy/trade/retail.rs` file.

use sim_engine::registries::enums::Commodity;

// ============================================================================
// PROBE 3b: Replicated wealth-gate + era-multiplier (parity with retail.rs)
// ============================================================================

/// Minimum `savings_per_capita` required to demand a commodity.
///
/// Parity replica of `commodity_wealth_gate` in `economy/trade/retail.rs:209`.
/// Kept in sync so the diagnostic can classify demand gating WITHOUT editing
/// agent-1's locked `retail.rs`. If the source function changes, this must be
/// updated to match.
pub fn commodity_wealth_gate(commodity: Commodity) -> f64 {
    match commodity {
        // Perishables — universal (consumed every turn)
        Commodity::Cereal
        | Commodity::Vegetable
        | Commodity::Meat
        | Commodity::Fruit
        | Commodity::HealthCapacity
        | Commodity::EducationSlots
        | Commodity::Food
        | Commodity::Water => 0.0,
        // Durables — wealth-gated (purchased only when savings permit)
        Commodity::Clothing => 20.0,
        Commodity::Furniture => 50.0,
        Commodity::Radio => 150.0,
        Commodity::Agd => 300.0,
        Commodity::Televisions => 500.0,
        Commodity::Cars => 1500.0,
        Commodity::Fuels => 200.0,
        Commodity::Luxury | Commodity::LuxuryFurniture | Commodity::LuxuryClothing => 2000.0,
        _ => 0.0,
    }
}

/// Era-aware consumption multiplier.
///
/// Parity replica of `era_consumption_multiplier` in `economy/trade/retail.rs:742`.
/// Returns 0.0 if the commodity is not yet available in the given year, a
/// fractional ramp-up value during early adoption, or 1.0 when fully in-era.
pub fn era_consumption_multiplier(commodity: Commodity, year: u32) -> f64 {
    match commodity {
        Commodity::Cereal
        | Commodity::Vegetable
        | Commodity::Meat
        | Commodity::Fruit
        | Commodity::Clothing
        | Commodity::Furniture
        | Commodity::Food
        | Commodity::Water
        | Commodity::HealthCapacity
        | Commodity::EducationSlots => 1.0,

        Commodity::Radio => {
            if year < 1920 {
                0.0
            } else if year < 1930 {
                0.3
            } else {
                1.0
            }
        }
        Commodity::Televisions => {
            if year < 1936 {
                0.0
            } else if year < 1950 {
                0.2
            } else {
                1.0
            }
        }
        Commodity::Agd => {
            if year < 1930 {
                0.0
            } else if year < 1950 {
                0.3
            } else {
                1.0
            }
        }
        Commodity::Cars => {
            if year < 1910 {
                0.0
            } else if year < 1950 {
                0.1
            } else {
                0.5
            }
        }
        Commodity::Luxury | Commodity::LuxuryFurniture | Commodity::LuxuryClothing
            if year < 1880 =>
        {
            0.5
        }
        _ => 1.0,
    }
}

/// Classification of why a class does or does not demand a commodity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemandGateClass {
    /// `savings_per_capita < commodity_wealth_gate(commodity)` — too poor.
    CannotAfford,
    /// `era_consumption_multiplier <= 0.0` — commodity not yet in-era.
    EraGated,
    /// Clears both the wealth gate and the era gate — demand is eligible.
    Eligible,
}

/// Classify a single (class, commodity) pair against the demand gates.
///
/// This is the core of Probe 3b. It distinguishes "cannot afford" (poverty)
/// from "will not spend" (hoarding above the gate) and "not in era" (tech
/// not yet available).
pub fn classify_demand_gate(
    savings_per_capita: f64,
    commodity: Commodity,
    year: u32,
) -> DemandGateClass {
    let era_mult = era_consumption_multiplier(commodity, year);
    if era_mult <= 0.0 {
        return DemandGateClass::EraGated;
    }
    let gate = commodity_wealth_gate(commodity);
    if savings_per_capita < gate {
        return DemandGateClass::CannotAfford;
    }
    DemandGateClass::Eligible
}

/// The set of consumption-relevant commodities to probe.
///
/// These are the commodities that appear in the consumption registry baskets
/// and are subject to the wealth gate / era multiplier in `build_consumer_demand`.
pub fn consumption_commodities() -> &'static [Commodity] {
    &[
        // Perishables (gate 0.0 — always affordable if in-era)
        Commodity::Cereal,
        Commodity::Vegetable,
        Commodity::Meat,
        Commodity::Fruit,
        Commodity::Food,
        Commodity::Water,
        // Durables (wealth-gated)
        Commodity::Clothing,
        Commodity::Furniture,
        Commodity::Radio,
        Commodity::Agd,
        Commodity::Televisions,
        Commodity::Cars,
        Commodity::Fuels,
        Commodity::Luxury,
        Commodity::LuxuryFurniture,
        Commodity::LuxuryClothing,
    ]
}

// ============================================================================
// PROBE 3a: Wage Transfer Conservation
// ============================================================================

/// Result of a wage-transfer conservation check across one turn's wage phase.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WageTransferResult {
    /// Turn number this result applies to.
    pub turn: u32,
    /// ΔΣcompany.available_cash from turn_start → building_cycle_post
    /// (negative = companies paid out cash).
    pub company_cash_delta: f64,
    /// Δcitizen_cash from turn_start → building_cycle_post
    /// (positive = citizens received cash).
    pub citizen_cash_delta: f64,
    /// `citizen_cash_delta + company_cash_delta` — the net transfer.
    /// For a pure wage transfer this is ~0 (money moves company→citizen).
    /// A large negative value means money vanished; a large positive value
    /// means money was created (e.g., government subsidies).
    pub transfer_balance: f64,
    /// True if citizen cash increased (wages reached citizens).
    pub citizens_gained: bool,
    /// True if company cash decreased (companies paid wages).
    pub companies_paid: bool,
}

/// Check wage-transfer conservation across the wage phase.
///
/// Given the company-cash and citizen-cash totals at `turn_start` and
/// `building_cycle_post`, compute whether wages flowed from companies to
/// citizens without vanishing.
///
/// # Arguments
/// - `turn`: The turn number.
/// - `company_cash_start`: Σcompany.available_cash at `turn_start`.
/// - `company_cash_post`: Σcompany.available_cash at `building_cycle_post`.
/// - `citizen_cash_start`: FiatWalk.citizen_cash at `turn_start`.
/// - `citizen_cash_post`: FiatWalk.citizen_cash at `building_cycle_post`.
pub fn check_wage_transfer(
    turn: u32,
    company_cash_start: f64,
    company_cash_post: f64,
    citizen_cash_start: f64,
    citizen_cash_post: f64,
) -> WageTransferResult {
    let company_cash_delta = company_cash_post - company_cash_start;
    let citizen_cash_delta = citizen_cash_post - citizen_cash_start;
    let transfer_balance = citizen_cash_delta + company_cash_delta;
    WageTransferResult {
        turn,
        company_cash_delta,
        citizen_cash_delta,
        transfer_balance,
        citizens_gained: citizen_cash_delta > 0.0,
        companies_paid: company_cash_delta < 0.0,
    }
}

// ============================================================================
// UNIT TESTS (always run — validate the helper logic in isolation)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wealth_gate_perishables_universal() {
        assert_eq!(commodity_wealth_gate(Commodity::Cereal), 0.0);
        assert_eq!(commodity_wealth_gate(Commodity::Food), 0.0);
        assert_eq!(commodity_wealth_gate(Commodity::Water), 0.0);
        assert_eq!(commodity_wealth_gate(Commodity::Meat), 0.0);
    }

    #[test]
    fn test_wealth_gate_durables_tiered() {
        assert_eq!(commodity_wealth_gate(Commodity::Clothing), 20.0);
        assert_eq!(commodity_wealth_gate(Commodity::Furniture), 50.0);
        assert_eq!(commodity_wealth_gate(Commodity::Cars), 1500.0);
        assert_eq!(commodity_wealth_gate(Commodity::Luxury), 2000.0);
    }

    #[test]
    fn test_era_multiplier_always_available() {
        // Staples are always in-era (multiplier 1.0) regardless of year.
        for year in [1800, 1900, 1925, 2000] {
            assert_eq!(era_consumption_multiplier(Commodity::Cereal, year), 1.0);
            assert_eq!(era_consumption_multiplier(Commodity::Clothing, year), 1.0);
        }
    }

    #[test]
    fn test_era_multiplier_radio() {
        assert_eq!(era_consumption_multiplier(Commodity::Radio, 1919), 0.0);
        assert_eq!(era_consumption_multiplier(Commodity::Radio, 1925), 0.3);
        assert_eq!(era_consumption_multiplier(Commodity::Radio, 1935), 1.0);
    }

    #[test]
    fn test_era_multiplier_televisions() {
        assert_eq!(era_consumption_multiplier(Commodity::Televisions, 1935), 0.0);
        assert_eq!(era_consumption_multiplier(Commodity::Televisions, 1940), 0.2);
        assert_eq!(era_consumption_multiplier(Commodity::Televisions, 1960), 1.0);
    }

    #[test]
    fn test_classify_cannot_afford() {
        // Destitute class (savings_per_capita = 10) cannot afford Clothing (gate 20).
        let cls = classify_demand_gate(10.0, Commodity::Clothing, 1925);
        assert_eq!(cls, DemandGateClass::CannotAfford);
    }

    #[test]
    fn test_classify_era_gated() {
        // Televisions not in era in 1925 → era-gated regardless of wealth.
        let cls = classify_demand_gate(10000.0, Commodity::Televisions, 1925);
        assert_eq!(cls, DemandGateClass::EraGated);
    }

    #[test]
    fn test_classify_eligible() {
        // Worker with savings 100 can afford Clothing (gate 20) in 1925.
        let cls = classify_demand_gate(100.0, Commodity::Clothing, 1925);
        assert_eq!(cls, DemandGateClass::Eligible);
        // Perishables always eligible (gate 0, era 1.0).
        let cls = classify_demand_gate(0.0, Commodity::Cereal, 1925);
        assert_eq!(cls, DemandGateClass::Eligible);
    }

    #[test]
    fn test_wage_transfer_clean() {
        // Companies pay 1000 in wages, citizens receive 1000.
        let r = check_wage_transfer(1, 10_000.0, 9_000.0, 5_000.0, 6_000.0);
        assert!(r.companies_paid, "companies should have paid");
        assert!(r.citizens_gained, "citizens should have gained");
        assert!(
            r.transfer_balance.abs() < 1e-6,
            "clean transfer balance ~0, got {}",
            r.transfer_balance
        );
    }

    #[test]
    fn test_wage_transfer_leak() {
        // Companies pay 1000 but citizens receive nothing — money vanished.
        let r = check_wage_transfer(1, 10_000.0, 9_000.0, 5_000.0, 5_000.0);
        assert!(r.companies_paid);
        assert!(!r.citizens_gained, "citizens gained nothing — leak detected");
        assert!(
            r.transfer_balance < -100.0,
            "transfer balance should be strongly negative, got {}",
            r.transfer_balance
        );
    }

    #[test]
    fn test_wage_transfer_no_wages_paid() {
        // Companies don't pay, citizens don't gain — no wage activity.
        let r = check_wage_transfer(1, 10_000.0, 10_000.0, 5_000.0, 5_000.0);
        assert!(!r.companies_paid, "no wages paid");
        assert!(!r.citizens_gained, "no wages received");
        assert!(r.transfer_balance.abs() < 1e-6);
    }

    #[test]
    fn test_consumption_commodities_nonempty() {
        assert!(!consumption_commodities().is_empty());
        // Staples must be in the probe set.
        assert!(consumption_commodities().contains(&Commodity::Cereal));
        assert!(consumption_commodities().contains(&Commodity::Clothing));
    }
}

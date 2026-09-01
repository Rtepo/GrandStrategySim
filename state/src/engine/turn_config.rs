//! Turn-level configuration parameters (Phase 86.5A).
//!
//! Extracts CRITICAL magic numbers from `engine/turn.rs` into a serializable
//! config struct. All nominal fiat costs are scaled by `average_wage` with a
//! dynamic subsistence floor derived from `MarketHistory` VWAP.

use crate::economy::market::market_history::{get_reference_price, MarketHistory};
use crate::registries::enums::Commodity;
use serde::{Deserialize, Serialize};

/// Phase 86.5A: Computes the minimum subsistence wage from market prices.
///
/// This is NOT a static float — it is dynamically derived from the current
/// VWAP of a minimal survival basket (food + clothing). This ensures the
/// wage clamp is inflation-proof at Turn 1 and Turn 1000.
///
/// The subsistence basket quantities are physical constants (kg of food,
/// units of clothing), NOT fiat values. Only the prices are dynamic.
pub fn compute_minimum_subsistence_wage(
    food_price: f64,
    clothing_price: f64,
    config: &TurnConfig,
) -> f64 {
    (food_price * config.subsistence_food_kg_per_turn)
        + (clothing_price * config.subsistence_clothing_units_per_turn)
}

/// Phase 86.5A: Computes the minimum subsistence GDP per capita.
///
/// Derived from the subsistence wage to ensure GDP-based scaling also has
/// an inflation-proof floor.
pub fn compute_minimum_subsistence_gdp_per_capita(
    subsistence_wage: f64,
    config: &TurnConfig,
) -> f64 {
    // Minimum GDP per capita is the subsistence wage scaled by a minimal
    // labor force participation rate (e.g., 50% of population works).
    subsistence_wage / config.subsistence_min_labor_participation.max(0.01)
}

/// Phase 86.5A: Gets the current VWAP price for a commodity from MarketHistory.
/// Falls back to `base_price_fallback` when no history exists (Turn 0/1).
pub fn get_commodity_vwap(
    market_history: &MarketHistory,
    commodity: &Commodity,
    config: &TurnConfig,
) -> f64 {
    // Try to get VWAP from market history; fall back to a nominal price
    // only when no history exists (early game bootstrap).
    get_reference_price(commodity, market_history)
        .filter(|p| p.is_finite() && *p > 0.0)
        .unwrap_or(config.base_price_fallback)
}

/// Phase 86.5A: Computes the effective wage with a dynamic subsistence floor.
///
/// This replaces the old `.max(1.0)` and `.max(1000.0)` static floors with
/// a market-derived subsistence wage. The effective wage is:
///   `average_wage.max(minimum_subsistence_wage)`
///
/// This ensures that:
/// - Turn 0/1 costs are nonzero and economically meaningful.
/// - The system remains meaningful during hyperinflation and deflation.
/// - No static fiat fallback is used as a minimum.
pub fn effective_wage(
    average_wage: f64,
    market_history: &MarketHistory,
    config: &TurnConfig,
) -> f64 {
    let food_price = get_commodity_vwap(market_history, &Commodity::Food, config);
    let clothing_price = get_commodity_vwap(market_history, &Commodity::Clothing, config);
    let subsistence = compute_minimum_subsistence_wage(food_price, clothing_price, config);
    average_wage.max(subsistence).max(0.01)
}

/// Phase 86.5A: Computes the effective GDP per capita with a dynamic floor.
pub fn effective_gdp_per_capita(
    gdp_per_capita: f64,
    market_history: &MarketHistory,
    config: &TurnConfig,
) -> f64 {
    let food_price = get_commodity_vwap(market_history, &Commodity::Food, config);
    let clothing_price = get_commodity_vwap(market_history, &Commodity::Clothing, config);
    let subsistence_wage = compute_minimum_subsistence_wage(food_price, clothing_price, config);
    let min_gdp = compute_minimum_subsistence_gdp_per_capita(subsistence_wage, config);
    gdp_per_capita.max(min_gdp).max(0.01)
}

/// Configuration for turn-level economic parameters (Phase 86.5A).
///
/// Replaces hardcoded magic numbers in `engine/turn.rs` with configurable,
/// serializable values. Nominal fiat costs are expressed as multipliers of
/// `average_wage` (clamped to `minimum_subsistence_wage`) to ensure
/// inflation-proof scaling.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TurnConfig {
    // ── Retail / Market ──
    /// Retail ask markup multiplier (e.g., 1.1 for 10% markup).
    #[serde(default = "default_retail_ask_markup")]
    pub retail_ask_markup: f64,

    /// Maximum ask quantity a company can post to the market.
    /// Scaled by physical capacity, not a flat cap.
    #[serde(default = "default_company_ask_quantity_cap")]
    pub company_ask_quantity_cap: f64,

    /// Retail surplus threshold above which restocking is reduced.
    #[serde(default = "default_retail_surplus_threshold")]
    pub retail_surplus_threshold: f64,

    /// Fraction of retail demand that is restocked per turn.
    #[serde(default = "default_retail_restock_fraction")]
    pub retail_restock_fraction: f64,

    // ── OHS (Occupational Health & Safety) ──
    /// Fraction of OHS casualties that result in death (rest are injuries).
    #[serde(default = "default_ohs_casualty_dead_share")]
    pub ohs_casualty_dead_share: f64,

    /// Defect rate threshold above which OHS fines are issued.
    #[serde(default = "default_ohs_defect_threshold")]
    pub ohs_defect_threshold: f64,

    /// OHS compliance ratio threshold (below which fines escalate).
    #[serde(default = "default_ohs_ratio_threshold")]
    pub ohs_ratio_threshold: f64,

    /// OHS fine for defects, as a multiple of `effective_wage`.
    /// Fine = defect_count * effective_wage * this_multiplier.
    #[serde(default = "default_ohs_defect_fine_wage_multiple")]
    pub ohs_defect_fine_wage_multiple: f64,

    /// OHS fine for low compliance ratio, as a multiple of `effective_wage`.
    /// Fine = (1.0 - ohs_ratio) * effective_wage * this_multiplier.
    #[serde(default = "default_ohs_ratio_fine_wage_multiple")]
    pub ohs_ratio_fine_wage_multiple: f64,

    /// Minimum OHS fine, as a multiple of `effective_wage`.
    #[serde(default = "default_ohs_min_fine_wage_multiple")]
    pub ohs_min_fine_wage_multiple: f64,

    /// Reputation penalty per OHS violation (scaled 0-100).
    #[serde(default = "default_ohs_reputation_penalty")]
    pub ohs_reputation_penalty: f64,

    // ── Transport ──
    /// Transport infrastructure degradation rate per turn (fraction).
    #[serde(default = "default_transport_degradation_rate")]
    pub transport_degradation_rate: f64,

    /// Transport repair cost per point of degradation, as a multiple of `effective_wage`.
    #[serde(default = "default_transport_repair_cost_wage_multiple")]
    pub transport_repair_cost_wage_multiple: f64,

    // ── Labor / Civil Service ──
    /// Civil service wage as a fraction of average private-sector wage.
    #[serde(default = "default_civil_service_wage_ratio")]
    pub civil_service_wage_ratio: f64,

    /// Commuter inflow coefficient (fraction of neighboring population that commutes).
    #[serde(default = "default_commuter_inflow_coefficient")]
    pub commuter_inflow_coefficient: f64,

    // ── Military Procurement ──
    /// Fraction of military budget reserved for procurement (rest is upkeep).
    #[serde(default = "default_mod_procurement_reserve_ratio")]
    pub mod_procurement_reserve_ratio: f64,

    // ── Political Capital ──
    /// Political capital regenerated per turn.
    #[serde(default = "default_political_capital_regen_per_turn")]
    pub political_capital_regen_per_turn: f64,

    /// Maximum political capital that can be accumulated.
    #[serde(default = "default_political_capital_cap")]
    pub political_capital_cap: f64,

    // ── Price Smoothing ──
    /// Weight of old price in EMA smoothing (e.g., 0.7 means 70% old + 30% new).
    #[serde(default = "default_base_price_ema_old_weight")]
    pub base_price_ema_old_weight: f64,

    /// Weight of new price in EMA smoothing (e.g., 0.3).
    #[serde(default = "default_base_price_ema_new_weight")]
    pub base_price_ema_new_weight: f64,

    // ── Parliament ──
    /// MP salary as a fraction of average wage.
    #[serde(default = "default_parliament_mp_salary_wage_ratio")]
    pub parliament_mp_salary_wage_ratio: f64,

    /// MP salary multiplier (additional multiplier on top of wage ratio).
    #[serde(default = "default_parliament_mp_salary_multiplier")]
    pub parliament_mp_salary_multiplier: f64,

    /// Parliamentary staff salary as a fraction of MP salary.
    #[serde(default = "default_parliament_staff_salary_ratio")]
    pub parliament_staff_salary_ratio: f64,

    // ── Faction Tension ──
    /// Faction tension gain per political event.
    #[serde(default = "default_faction_tension_gain_per_event")]
    pub faction_tension_gain_per_event: f64,

    // ── Subsistence Basket (for dynamic wage clamp) ──
    /// Physical kg of food in the subsistence basket per turn.
    /// This is a physical constant, NOT a fiat value.
    #[serde(default = "default_subsistence_food_kg_per_turn")]
    pub subsistence_food_kg_per_turn: f64,

    /// Physical units of clothing in the subsistence basket per turn.
    /// This is a physical constant, NOT a fiat value.
    #[serde(default = "default_subsistence_clothing_units_per_turn")]
    pub subsistence_clothing_units_per_turn: f64,

    /// Minimum labor force participation rate for GDP per capita subsistence calc.
    #[serde(default = "default_subsistence_min_labor_participation")]
    pub subsistence_min_labor_participation: f64,

    /// Base price fallback when market history has no data for a commodity.
    #[serde(default = "default_base_price_fallback")]
    pub base_price_fallback: f64,
}

// ── Default value functions ──

fn default_retail_ask_markup() -> f64 {
    1.1
}
fn default_company_ask_quantity_cap() -> f64 {
    1000.0
}
fn default_retail_surplus_threshold() -> f64 {
    10.0
}
fn default_retail_restock_fraction() -> f64 {
    0.3
}
fn default_ohs_casualty_dead_share() -> f64 {
    0.3
}
fn default_ohs_defect_threshold() -> f64 {
    0.05
}
fn default_ohs_ratio_threshold() -> f64 {
    0.8
}
fn default_ohs_defect_fine_wage_multiple() -> f64 {
    50_000.0
}
fn default_ohs_ratio_fine_wage_multiple() -> f64 {
    20_000.0
}
fn default_ohs_min_fine_wage_multiple() -> f64 {
    5_000.0
}
fn default_ohs_reputation_penalty() -> f64 {
    5.0
}
fn default_transport_degradation_rate() -> f64 {
    0.01
}
fn default_transport_repair_cost_wage_multiple() -> f64 {
    1000.0
}
fn default_civil_service_wage_ratio() -> f64 {
    0.8
}
fn default_commuter_inflow_coefficient() -> f64 {
    0.05
}
fn default_mod_procurement_reserve_ratio() -> f64 {
    0.3
}
fn default_political_capital_regen_per_turn() -> f64 {
    2.0
}
fn default_political_capital_cap() -> f64 {
    24.0
}
fn default_base_price_ema_old_weight() -> f64 {
    0.7
}
fn default_base_price_ema_new_weight() -> f64 {
    0.3
}
fn default_parliament_mp_salary_wage_ratio() -> f64 {
    0.1
}
fn default_parliament_mp_salary_multiplier() -> f64 {
    3.0
}
fn default_parliament_staff_salary_ratio() -> f64 {
    0.8
}
fn default_faction_tension_gain_per_event() -> f64 {
    0.15
}
fn default_subsistence_food_kg_per_turn() -> f64 {
    7.0
}
fn default_subsistence_clothing_units_per_turn() -> f64 {
    0.1
}
fn default_subsistence_min_labor_participation() -> f64 {
    0.5
}
fn default_base_price_fallback() -> f64 {
    100.0
}

impl Default for TurnConfig {
    fn default() -> Self {
        TurnConfig {
            retail_ask_markup: default_retail_ask_markup(),
            company_ask_quantity_cap: default_company_ask_quantity_cap(),
            retail_surplus_threshold: default_retail_surplus_threshold(),
            retail_restock_fraction: default_retail_restock_fraction(),
            ohs_casualty_dead_share: default_ohs_casualty_dead_share(),
            ohs_defect_threshold: default_ohs_defect_threshold(),
            ohs_ratio_threshold: default_ohs_ratio_threshold(),
            ohs_defect_fine_wage_multiple: default_ohs_defect_fine_wage_multiple(),
            ohs_ratio_fine_wage_multiple: default_ohs_ratio_fine_wage_multiple(),
            ohs_min_fine_wage_multiple: default_ohs_min_fine_wage_multiple(),
            ohs_reputation_penalty: default_ohs_reputation_penalty(),
            transport_degradation_rate: default_transport_degradation_rate(),
            transport_repair_cost_wage_multiple: default_transport_repair_cost_wage_multiple(),
            civil_service_wage_ratio: default_civil_service_wage_ratio(),
            commuter_inflow_coefficient: default_commuter_inflow_coefficient(),
            mod_procurement_reserve_ratio: default_mod_procurement_reserve_ratio(),
            political_capital_regen_per_turn: default_political_capital_regen_per_turn(),
            political_capital_cap: default_political_capital_cap(),
            base_price_ema_old_weight: default_base_price_ema_old_weight(),
            base_price_ema_new_weight: default_base_price_ema_new_weight(),
            parliament_mp_salary_wage_ratio: default_parliament_mp_salary_wage_ratio(),
            parliament_mp_salary_multiplier: default_parliament_mp_salary_multiplier(),
            parliament_staff_salary_ratio: default_parliament_staff_salary_ratio(),
            faction_tension_gain_per_event: default_faction_tension_gain_per_event(),
            subsistence_food_kg_per_turn: default_subsistence_food_kg_per_turn(),
            subsistence_clothing_units_per_turn: default_subsistence_clothing_units_per_turn(),
            subsistence_min_labor_participation: default_subsistence_min_labor_participation(),
            base_price_fallback: default_base_price_fallback(),
        }
    }
}

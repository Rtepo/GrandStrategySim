//! Phase 24D: Macroeconomic telemetry — GDP, inflation, and money supply.
//!
//! This module implements the P0 indicator fixes identified in the
//! `resurrection-ui-telemetry-audit.md` audit. Before Phase 24D, GDP,
//! inflation, and M0/M3 were frozen at world-generation and never
//! recomputed during the turn loop.
//!
//! # GDP (Expenditure Approach)
//! `official_gdp = C + G + I + NX` where:
//! * `C` = final household consumption (B2C retail clearing revenue)
//! * `G` = government spending (ministry procurement + subsidies + public wages)
//! * `I` = gross investment (fixed-asset purchases + construction project spend)
//! * `NX` = net exports (exports − imports)
//!
//! A parallel `shadow_gdp` tracks off-the-books wages and bribes.
//!
//! # Dual Inflation (CPI & PPI)
//! * CPI: weighted basket of consumer goods from `consumption_registry`, priced
//!   at VWAP from `MarketHistory`.
//! * PPI: weighted basket of producer goods (Steel, HardCoal, Energy,
//!   FreightCapacity, MaintenanceServices), priced at VWAP.
//! * `inflation = (cpi_index_t − cpi_index_{t-1}) / cpi_index_{t-1} × 100`
//!
//! # Money Supply (M0 / M3)
//! Aggregated from all `Company.brokerage_account.cash` (cash in circulation),
//! `BankBalanceSheet.reserves_at_central_bank` (bank reserves),
//! `BankBalanceSheet.deposits` (demand + time deposits), and
//! `Treasury.liquid_reserves`. Uses the existing `CentralBank::calculate_m0`
//! and `calculate_m3` formulas.
//!
//! # Double-Entry Safety
//! All functions in this module are **read-only** — they walk existing ledgers
//! to compute aggregate values but never mutate cash, deposits, or reserves.

use crate::data::consumption_registry::consumption_registry;
use crate::economy::market::market_history::MarketHistory;
use crate::entities::Company;
use crate::registries::enums::Commodity;
use crate::registries::enums::Sector;
use crate::society::geography::{ClassDemographics, Region};
use crate::state::macro_data::{GdpBreakdown, InflationIndices, MoneySupplySnapshot};
use crate::state::Country;
use std::collections::BTreeMap;
use std::collections::HashMap;

/// Phase 35: Per-region GDP accumulator for full per-region GDP accounting.
///
/// Tracks C+G+I+NX for each region so that national GDP is strictly derived
/// as sum(region.gdp). This eliminates the mismatch where national GDP evolved
/// via C+G+I+NX aggregation but regional GDP stayed frozen at generation.
#[derive(Debug, Clone, Default)]
pub struct RegionalGdpAccumulator {
    /// C: B2C retail revenue by region.
    pub consumption: f64,
    /// G: ministry spending by region.
    pub government_spending: f64,
    /// I: construction materials consumed by region.
    pub investment: f64,
    /// NX: exports minus imports by region.
    pub net_exports: f64,
    /// Phase 44: Imputed consumption from subsistence economy (Serf in-kind).
    pub imputed_consumption: f64,
}

impl RegionalGdpAccumulator {
    /// Compute official GDP (C + G + I + NX) for this region.
    pub fn official_gdp(&self) -> f64 {
        self.consumption
            + self.imputed_consumption
            + self.government_spending
            + self.investment
            + self.net_exports
    }
}

/// Per-turn accumulation buffer for GDP expenditure components.
///
/// The turn loop accumulates cash flows into this struct as they happen
/// (B2C clearing, ministry procurement, fixed-asset purchases, etc.).
/// At end-of-turn, `compute_gdp` reads it and writes `GdpBreakdown`
/// onto `MacroData`.
#[derive(Debug, Clone, Default)]
pub struct GdpAccumulator {
    /// Final household consumption: sum of B2C `store_revenue`.
    pub consumption: f64,
    /// Government spending: ministry procurement trade value + subsidies.
    pub government_spending: f64,
    /// Gross investment: fixed-asset purchase trade value + construction spend.
    pub investment: f64,
    /// Net exports (set from `trade_result` at end-of-turn).
    pub net_exports: f64,
    /// Shadow GDP: off-the-books shadow wages + bribes.
    pub shadow_gdp: f64,
    /// Phase 44: Imputed consumption from subsistence economy (Serf in-kind).
    pub imputed_consumption: f64,
    /// Phase 35: Per-region GDP breakdown for strict national = sum(regional) reconciliation.
    pub regional: HashMap<String, RegionalGdpAccumulator>,
}

impl GdpAccumulator {
    /// Phase 35: Record consumption in a specific region.
    pub fn add_consumption(&mut self, region_id: &str, amount: f64) {
        self.consumption += amount;
        self.regional
            .entry(region_id.to_string())
            .or_default()
            .consumption += amount;
    }

    /// Phase 44: Record imputed consumption (subsistence economy) in a specific region.
    pub fn add_imputed_consumption(&mut self, region_id: &str, amount: f64) {
        self.imputed_consumption += amount;
        self.regional
            .entry(region_id.to_string())
            .or_default()
            .imputed_consumption += amount;
    }

    /// Phase 35: Record government spending in a specific region.
    pub fn add_government_spending(&mut self, region_id: &str, amount: f64) {
        self.government_spending += amount;
        self.regional
            .entry(region_id.to_string())
            .or_default()
            .government_spending += amount;
    }

    /// Phase 35: Record investment in a specific region.
    pub fn add_investment(&mut self, region_id: &str, amount: f64) {
        self.investment += amount;
        self.regional
            .entry(region_id.to_string())
            .or_default()
            .investment += amount;
    }

    /// Phase 35: Record net exports for a specific region.
    pub fn add_net_exports(&mut self, region_id: &str, amount: f64) {
        self.net_exports += amount;
        self.regional
            .entry(region_id.to_string())
            .or_default()
            .net_exports += amount;
    }
}

/// Compute the official GDP and shadow GDP from the accumulator.
///
/// # Arguments
/// * `acc` - The per-turn GDP accumulator (consumption, G, I, NX, shadow).
/// * `previous_gdp` - Previous turn's official GDP (for growth rate).
///
/// # Returns
/// A `GdpBreakdown` with all components filled in.
///
/// # Rules
/// * Read-only: does not mutate any cash ledgers.
/// * `official_gdp = consumption + government_spending + investment + net_exports`.
/// * `shadow_gdp` is stored separately (not added to official GDP).
pub fn compute_gdp(acc: &GdpAccumulator, previous_gdp: f64) -> GdpBreakdown {
    let official_gdp = acc.consumption
        + acc.imputed_consumption
        + acc.government_spending
        + acc.investment
        + acc.net_exports;
    GdpBreakdown {
        consumption: acc.consumption,
        government_spending: acc.government_spending,
        investment: acc.investment,
        net_exports: acc.net_exports,
        official_gdp,
        previous_gdp,
        shadow_gdp: acc.shadow_gdp,
        imputed_consumption: acc.imputed_consumption,
    }
}

/// Compute shadow GDP from `ShadowEconomyState` + bribery records.
///
/// # Arguments
/// * `total_hidden_fte` - Total off-the-books FTE from `ShadowEconomyState`.
/// * `shadow_wage_per_fte` - Average shadow wage per FTE.
/// * `total_bribes` - Total bribes paid this turn (from bribery system).
///
/// # Returns
/// Shadow GDP = `hidden_fte × shadow_wage + bribes`.
pub fn compute_shadow_gdp(
    total_hidden_fte: f64,
    shadow_wage_per_fte: f64,
    total_bribes: f64,
) -> f64 {
    let fte = total_hidden_fte.max(0.0);
    let wage = shadow_wage_per_fte.max(0.0);
    (fte * wage) + total_bribes.max(0.0)
}

// ============================================================================
// INFLATION (CPI & PPI)
// ============================================================================

/// Build the CPI basket weights from `consumption_registry`.
///
/// Aggregates per-capita consumption quantities across all demographic
/// classes into a single weight map. Weights are proportional to total
/// per-capita consumption volume (summed across all classes and tiers).
///
/// # Returns
/// `BTreeMap<Commodity, f64>` — weight per commodity (sums to > 0).
fn build_cpi_basket_weights() -> BTreeMap<Commodity, f64> {
    let registry = consumption_registry();
    let mut weights: BTreeMap<Commodity, f64> = BTreeMap::new();
    for basket in registry.values() {
        for tier_map in basket.tiers.values() {
            for (&commodity, &qty) in tier_map {
                *weights.entry(commodity).or_insert(0.0) += qty;
            }
        }
    }
    weights
}

/// Build the PPI basket weights (fixed producer-goods basket).
///
/// # Returns
/// `BTreeMap<Commodity, f64>` — weight per commodity.
fn build_ppi_basket_weights() -> BTreeMap<Commodity, f64> {
    let mut weights = BTreeMap::new();
    // Core industrial inputs that drive producer costs.
    weights.insert(Commodity::Steel, 3.0);
    weights.insert(Commodity::HardCoal, 2.0);
    weights.insert(Commodity::Energy, 4.0);
    weights.insert(Commodity::FreightCapacity, 1.5);
    weights.insert(Commodity::MaintenanceServices, 1.5);
    weights.insert(Commodity::MechanicalComponents, 2.0);
    weights.insert(Commodity::Fuels, 2.0);
    weights.insert(Commodity::Cement, 1.0);
    weights
}

/// Compute a weighted price index from a basket and VWAP data.
///
/// # Arguments
/// * `weights` - Basket weights per commodity.
/// * `vwap` - VWAP per commodity from `MarketHistory`.
/// * `fallback_prices` - Global base prices (used when VWAP is missing).
///
/// # Returns
/// The weighted index value. Returns `100.0` if no priced commodities are found.
///
/// # Rules
/// * Only commodities with a non-zero VWAP or fallback price contribute.
/// * Index = `Σ(weight_i × price_i) / Σ(weight_i)`.
/// * This is a Laspeyres-style index with fixed basket quantities.
fn compute_weighted_index(
    weights: &BTreeMap<Commodity, f64>,
    vwap: &rustc_hash::FxHashMap<Commodity, f64>,
    fallback_prices: &rustc_hash::FxHashMap<Commodity, f64>,
) -> f64 {
    let mut total_value = 0.0;
    let mut total_weight = 0.0;
    for (&commodity, &weight) in weights {
        if weight <= 0.0 {
            continue;
        }
        let price = vwap
            .get(&commodity)
            .copied()
            .filter(|p| *p > 0.0)
            .or_else(|| fallback_prices.get(&commodity).copied())
            .filter(|p| *p > 0.0);
        if let Some(price) = price {
            total_value += weight * price;
            total_weight += weight;
        }
    }
    if total_weight > 0.0 {
        total_value / total_weight
    } else {
        100.0
    }
}

/// Compute dual inflation indices (CPI & PPI) from VWAP data.
///
/// # Arguments
/// * `history` - Market history with VWAP and fallback prices.
/// * `previous` - Previous turn's `InflationIndices` (for delta computation).
///
/// # Returns
/// Updated `InflationIndices` with current indices and inflation rates.
///
/// # Rules
/// * CPI basket is derived from `consumption_registry` (consumer goods).
/// * PPI basket is a fixed set of producer goods.
/// * `cpi_inflation = (cpi_index - previous_cpi_index) / previous_cpi_index × 100`.
/// * If previous index is zero, inflation is 0.0 (first turn).
pub fn compute_inflation(history: &MarketHistory, previous: &InflationIndices) -> InflationIndices {
    let cpi_weights = build_cpi_basket_weights();
    let ppi_weights = build_ppi_basket_weights();

    // Phase 25: CPI tracks B2C retail prices (what consumers actually pay),
    // falling back to global base prices if no retail trades occurred.
    // PPI remains linked to B2B VWAP (wholesale/producer prices).
    let cpi_index = compute_weighted_index(
        &cpi_weights,
        &history.retail_vwap_per_commodity,
        &history.global_base_prices,
    );
    let ppi_index = compute_weighted_index(
        &ppi_weights,
        &history.vwap_per_commodity,
        &history.global_base_prices,
    );

    let cpi_inflation = if previous.cpi_index > 0.0 {
        ((cpi_index - previous.cpi_index) / previous.cpi_index) * 100.0
    } else {
        0.0
    };
    let ppi_inflation = if previous.ppi_index > 0.0 {
        ((ppi_index - previous.ppi_index) / previous.ppi_index) * 100.0
    } else {
        0.0
    };

    InflationIndices {
        cpi_index,
        previous_cpi_index: previous.cpi_index,
        ppi_index,
        previous_ppi_index: previous.ppi_index,
        cpi_inflation,
        ppi_inflation,
    }
}

// ============================================================================
// MONEY SUPPLY (M0 / M3)
// ============================================================================

/// Compute the money supply snapshot by walking all ledgers.
///
/// # Arguments
/// * `companies` - All companies (for brokerage cash + bank balance sheets).
/// * `country` - Country (for treasury reserves + central bank + class savings).
///
/// # Returns
/// `MoneySupplySnapshot` with M0, M3, multiplier, and component breakdowns.
///
/// # Rules
/// * **Read-only**: walks ledgers but never mutates them.
/// * `cash_in_circulation` = sum of all `Company.brokerage_account.cash`
///   + all `ClassDemographics.savings` (citizen cash).
/// * `bank_reserves` = sum of all `BankBalanceSheet.reserves_at_central_bank`
///   + `cb_deposit_facility_balance`.
/// * `demand_deposits` = sum of all `BankBalanceSheet.deposits` (approximation:
///   banks don't split demand vs. time in the current schema; we use 80/20
///   split as a first approximation).
/// * `time_deposits` = 20% of total deposits (approximation).
/// * M0 = `cash_in_circulation + bank_reserves`.
/// * M3 = `M0 + demand_deposits + time_deposits`.
/// * `multiplier = M3 / M0` (0.0 if M0 ≤ 0).
pub fn compute_money_supply(companies: &[Company], country: &Country) -> MoneySupplySnapshot {
    // Cash in circulation: company brokerage cash + citizen class savings.
    let mut cash_in_circulation: f64 = 0.0;
    let mut total_bank_deposits: f64 = 0.0;
    let mut bank_reserves: f64 = 0.0;

    for company in companies {
        // Company brokerage cash (liquid funds in the economy).
        if let Some(ref ba) = company.brokerage_account {
            cash_in_circulation += ba.cash.max(0.0);
        } else {
            // Companies without brokerage accounts hold cash in available_cash.
            cash_in_circulation += company.available_cash.max(0.0);
        }

        // Bank balance sheet aggregation (only for banking-sector companies).
        if company.sector == Sector::Banking {
            if let Some(ref bs) = company.balance_sheet {
                bank_reserves += bs.reserves_at_central_bank.max(0.0);
                bank_reserves += bs.cb_deposit_facility_balance.max(0.0);
                total_bank_deposits += bs.deposits.max(0.0);
            }
        }
    }

    // Citizen savings from all regions' class demographics.
    for region in &country.regions {
        for demo in region.class_demographics.rural_classes.values() {
            cash_in_circulation += demo.savings.max(0.0);
        }
        for demo in region.class_demographics.urban_classes.values() {
            cash_in_circulation += demo.savings.max(0.0);
        }
    }

    // Treasury liquid reserves are government-held cash (part of M0).
    // Note: we do NOT count this as "cash in circulation" since it's not
    // circulating; but it IS part of the monetary base. We add it to
    // bank_reserves as a government deposit at the central bank.
    bank_reserves += country.budget.liquid_reserves.max(0.0);

    // Approximate demand vs. time deposit split (80/20).
    let demand_deposits = total_bank_deposits * 0.8;
    let time_deposits = total_bank_deposits * 0.2;

    // Use the CentralBank's formulas for consistency.
    let m0 = country
        .central_bank
        .calculate_m0(cash_in_circulation, bank_reserves);
    let m3 = country.central_bank.calculate_m3(
        m0,
        demand_deposits,
        time_deposits,
        0.0, // other_liquid_assets (not tracked yet)
    );
    let multiplier = country.central_bank.calculate_money_multiplier(m0, m3);

    MoneySupplySnapshot {
        m0,
        m3,
        multiplier,
        cash_in_circulation,
        bank_reserves,
        demand_deposits,
        time_deposits,
        previous_m3: 0.0, // set by caller from previous snapshot
    }
}

// ============================================================================
// OHS CASUALTIES → LABOR FEEDBACK (P0-4a)
// ============================================================================

/// Apply OHS/disaster casualties to a region's class demographics.
///
/// # Arguments
/// * `region` - Mutable region whose class demographics will be updated.
/// * `casualties_dead` - Number of workers killed (removed from population + FTE).
/// * `casualties_disabled` - Number of workers disabled (kept in population, removed from FTE).
/// * `is_rural` - If true, distribute across rural classes; otherwise urban.
///
/// # Rules
/// * Dead workers: `population -= dead`, `available_fte -= dead * labor_participation`,
///   `deceased += dead`.
/// * Disabled workers: `available_fte -= disabled * labor_participation`,
///   `active_disabled += disabled`, `unable_to_work += disabled * labor_participation`.
/// * `population` is NOT reduced for disabled workers (they're still alive).
/// * Casualties are distributed proportionally across classes by population share.
/// * `available_fte` is clamped at 0.0 (never goes negative).
pub fn apply_casualties_to_labor(
    region: &mut Region,
    casualties_dead: i64,
    casualties_disabled: i64,
    is_rural: bool,
) {
    if casualties_dead <= 0 && casualties_disabled <= 0 {
        return;
    }

    let classes: Vec<&mut ClassDemographics> = if is_rural {
        region
            .class_demographics
            .rural_classes
            .values_mut()
            .collect()
    } else {
        region
            .class_demographics
            .urban_classes
            .values_mut()
            .collect()
    };

    // If no classes, nothing to do.
    let total_pop: i64 = classes.iter().map(|c| c.population.max(0)).sum();
    if total_pop <= 0 {
        return;
    }

    let n_classes = classes.len();
    let mut classes = classes;

    if casualties_dead > 0 {
        let mut remaining_dead = casualties_dead;
        for (i, demo) in classes.iter_mut().enumerate() {
            if remaining_dead <= 0 {
                break;
            }
            let is_last = i == n_classes.saturating_sub(1);
            let share = demo.population as f64 / total_pop as f64;
            let dead = if is_last {
                remaining_dead // last class absorbs rounding remainder
            } else {
                (casualties_dead as f64 * share).round() as i64
            };
            let dead = dead.min(remaining_dead).min(demo.population);
            if dead > 0 {
                demo.population -= dead;
                demo.deceased += dead;
                let fte_lost = dead as f64 * demo.labor_participation;
                demo.available_fte = (demo.available_fte - fte_lost).max(0.0);
                remaining_dead -= dead;
            }
        }
    }

    if casualties_disabled > 0 {
        let mut remaining_disabled = casualties_disabled;
        for (i, demo) in classes.iter_mut().enumerate() {
            if remaining_disabled <= 0 {
                break;
            }
            let is_last = i == n_classes.saturating_sub(1);
            let share = demo.population as f64 / total_pop as f64;
            let disabled = if is_last {
                remaining_disabled
            } else {
                (casualties_disabled as f64 * share).round() as i64
            };
            let disabled = disabled.min(remaining_disabled);
            if disabled > 0 {
                demo.active_disabled += disabled;
                let fte_lost = disabled as f64 * demo.labor_participation;
                demo.available_fte = (demo.available_fte - fte_lost).max(0.0);
                demo.unable_to_work += fte_lost;
                remaining_disabled -= disabled;
            }
        }
    }
}

// ============================================================================
// COMMUTER DOUBLE-COUNT FIX (P0-4b)
// ============================================================================

/// Mark home-region FTE as "commuting out" before the host region clears its
/// labor market, preventing the same worker from being counted twice.
///
/// # Arguments
/// * `home_region` - The region whose workers are commuting out.
/// * `fte_commuting_out` - FTE that will commute to adjacent regions.
///
/// # Rules
/// * Deducts `fte_commuting_out` from `available_fte` on the home region's
///   classes proportionally by available FTE share.
/// * The deducted FTE is NOT lost — it will be re-credited as commuter wages
///   by the labor market resolver (`commuter_wages` in `LaborAllocationMatrix`).
/// * `available_fte` is clamped at 0.0.
/// * This must be called BEFORE the host region's `resolve_regional_labor_market`.
pub fn mark_commuting_out(home_region: &mut Region, fte_commuting_out: f64) {
    if fte_commuting_out <= 0.0 {
        return;
    }

    // Distribute the commuting deduction proportionally across all classes
    // (rural + urban) by their available_fte share.
    let mut all_demos: Vec<&mut ClassDemographics> = Vec::new();
    for demo in home_region.class_demographics.rural_classes.values_mut() {
        all_demos.push(demo);
    }
    for demo in home_region.class_demographics.urban_classes.values_mut() {
        all_demos.push(demo);
    }

    let total_available: f64 = all_demos.iter().map(|d| d.available_fte.max(0.0)).sum();
    if total_available <= 0.0 {
        return;
    }

    let mut remaining = fte_commuting_out;
    let n = all_demos.len();
    for (i, demo) in all_demos.iter_mut().enumerate() {
        if remaining <= 0.0 {
            break;
        }
        let is_last = i == n.saturating_sub(1);
        let share = demo.available_fte.max(0.0) / total_available;
        let deduction = if is_last {
            remaining
        } else {
            fte_commuting_out * share
        };
        let deduction = deduction.min(remaining).min(demo.available_fte);
        demo.available_fte = (demo.available_fte - deduction).max(0.0);
        remaining -= deduction;
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::market::market_history::MarketHistory;
    use crate::entities::Company;
    use crate::registries::enums::Commodity;
    use crate::registries::enums::Sector;
    use crate::state::macro_data::{GdpBreakdown, InflationIndices};
    use crate::state::{Country, Treasury};
    use rustc_hash::FxHashMap as HashMap;

    // ── GDP tests ──

    #[test]
    fn test_compute_gdp_sums_expenditure_components() {
        let acc = GdpAccumulator {
            consumption: 1_000_000.0,
            government_spending: 500_000.0,
            investment: 300_000.0,
            net_exports: -200_000.0,
            shadow_gdp: 50_000.0,
            imputed_consumption: 0.0,
            regional: std::collections::HashMap::default(),
        };
        let gdp = compute_gdp(&acc, 1_500_000.0);
        assert!((gdp.official_gdp - 1_600_000.0).abs() < 1e-6);
        assert!((gdp.consumption - 1_000_000.0).abs() < 1e-6);
        assert!((gdp.net_exports - (-200_000.0)).abs() < 1e-6);
        assert!((gdp.shadow_gdp - 50_000.0).abs() < 1e-6);
    }

    #[test]
    fn test_gdp_growth_rate_yoy() {
        let gdp = GdpBreakdown {
            official_gdp: 1_100_000.0,
            previous_gdp: 1_000_000.0,
            ..Default::default()
        };
        assert!((gdp.growth_rate() - 0.10).abs() < 1e-9);
    }

    #[test]
    fn test_gdp_growth_rate_zero_previous() {
        let gdp = GdpBreakdown {
            official_gdp: 1_000_000.0,
            previous_gdp: 0.0,
            ..Default::default()
        };
        assert_eq!(gdp.growth_rate(), 0.0);
    }

    #[test]
    fn test_compute_shadow_gdp() {
        let shadow = compute_shadow_gdp(500.0, 10.0, 5_000.0);
        // 500 FTE × 10 wage + 5000 bribes = 10000
        assert!((shadow - 10_000.0).abs() < 1e-6);
    }

    #[test]
    fn test_compute_shadow_gdp_negative_clamped() {
        let shadow = compute_shadow_gdp(-100.0, -5.0, -500.0);
        assert_eq!(shadow, 0.0);
    }

    // ── Inflation tests ──

    #[test]
    fn test_cpi_index_changes_with_vwap() {
        // Turn 1: base prices, no VWAP yet.
        let history_t1 = MarketHistory {
            vwap_per_commodity: HashMap::default(),
            last_trade_price: HashMap::default(),
            global_base_prices: {
                let mut m = HashMap::default();
                m.insert(Commodity::Cereal, 100.0);
                m.insert(Commodity::Vegetable, 80.0);
                m
            },
            retail_vwap_per_commodity: HashMap::default(),
            prev_net_surplus: HashMap::default(),
            ..Default::default()
        };
        let prev = InflationIndices::default();
        let indices_t1 = compute_inflation(&history_t1, &prev);
        let cpi_t1 = indices_t1.cpi_index;
        assert!(cpi_t1 > 0.0, "CPI should be positive at turn 1");

        // Turn 2: retail VWAP prices doubled for cereal (CPI tracks B2C retail prices).
        let history_t2 = MarketHistory {
            vwap_per_commodity: HashMap::default(),
            last_trade_price: HashMap::default(),
            global_base_prices: history_t1.global_base_prices.clone(),
            retail_vwap_per_commodity: {
                let mut m = HashMap::default();
                m.insert(Commodity::Cereal, 200.0); // doubled
                m.insert(Commodity::Vegetable, 80.0);
                m
            },
            prev_net_surplus: HashMap::default(),
            ..Default::default()
        };
        let indices_t2 = compute_inflation(&history_t2, &indices_t1);
        assert!(
            indices_t2.cpi_index > cpi_t1,
            "CPI should rise when cereal price doubles"
        );
        assert!(
            indices_t2.cpi_inflation > 0.0,
            "CPI inflation should be positive when prices rise"
        );
    }

    #[test]
    fn test_ppi_index_reflects_producer_goods() {
        let history = MarketHistory {
            vwap_per_commodity: {
                let mut m = HashMap::default();
                m.insert(Commodity::Steel, 500.0);
                m.insert(Commodity::HardCoal, 150.0);
                m.insert(Commodity::Energy, 300.0);
                m
            },
            last_trade_price: HashMap::default(),
            global_base_prices: HashMap::default(),
            retail_vwap_per_commodity: HashMap::default(),
            prev_net_surplus: HashMap::default(),
            ..Default::default()
        };
        let prev = InflationIndices::default();
        let indices = compute_inflation(&history, &prev);
        assert!(indices.ppi_index > 0.0, "PPI should be positive");
        // PPI should be dominated by Steel/Energy prices.
        assert!(
            indices.ppi_index > 200.0,
            "PPI should reflect high producer-goods prices"
        );
    }

    #[test]
    fn test_inflation_zero_on_first_turn() {
        let history = MarketHistory {
            vwap_per_commodity: HashMap::default(),
            last_trade_price: HashMap::default(),
            global_base_prices: {
                let mut m = HashMap::default();
                m.insert(Commodity::Cereal, 100.0);
                m
            },
            retail_vwap_per_commodity: HashMap::default(),
            prev_net_surplus: HashMap::default(),
            ..Default::default()
        };
        let prev = InflationIndices::default();
        let indices = compute_inflation(&history, &prev);
        assert_eq!(indices.cpi_inflation, 0.0, "inflation is 0 on first turn");
        assert_eq!(indices.ppi_inflation, 0.0);
    }

    #[test]
    fn test_inflation_negative_when_prices_fall() {
        let prev = InflationIndices {
            cpi_index: 150.0,
            previous_cpi_index: 100.0,
            ppi_index: 150.0,
            previous_ppi_index: 100.0,
            cpi_inflation: 50.0,
            ppi_inflation: 50.0,
        };
        // Prices drop back to base.
        let history = MarketHistory {
            vwap_per_commodity: {
                let mut m = HashMap::default();
                m.insert(Commodity::Cereal, 100.0);
                m.insert(Commodity::Vegetable, 80.0);
                m
            },
            last_trade_price: HashMap::default(),
            global_base_prices: HashMap::default(),
            retail_vwap_per_commodity: {
                let mut m = HashMap::default();
                m.insert(Commodity::Cereal, 100.0);
                m.insert(Commodity::Vegetable, 80.0);
                m
            },
            prev_net_surplus: HashMap::default(),
            ..Default::default()
        };
        let indices = compute_inflation(&history, &prev);
        assert!(
            indices.cpi_inflation < 0.0,
            "CPI inflation should be negative when prices fall"
        );
    }

    // ── Money Supply tests ──

    #[test]
    fn test_compute_money_supply_aggregates_cash_and_deposits() {
        let mut company = Company::default();
        company.sector = Sector::Agriculture;
        company.brokerage_account = Some(crate::securities::BrokerageAccount {
            cash: 500_000.0,
            ..Default::default()
        });

        let mut bank = Company::default();
        bank.sector = Sector::Banking;
        bank.balance_sheet = Some(crate::state::banking::BankBalanceSheet {
            reserves_at_central_bank: 200_000.0,
            deposits: 1_000_000.0,
            ..Default::default()
        });

        let mut country = Country::mock_for_tests();
        country.budget = Treasury {
            liquid_reserves: 100_000.0,
            ..Default::default()
        };

        let snapshot = compute_money_supply(&[company, bank], &country);
        // cash = 500k (company) + 0 (no class savings in mock)
        // bank_reserves = 200k + 100k (treasury) = 300k
        // M0 = 500k + 300k = 800k
        assert!((snapshot.cash_in_circulation - 500_000.0).abs() < 1e-6);
        assert!((snapshot.bank_reserves - 300_000.0).abs() < 1e-6);
        assert!((snapshot.m0 - 800_000.0).abs() < 1e-6);
        // deposits = 1M, demand = 800k, time = 200k
        assert!((snapshot.demand_deposits - 800_000.0).abs() < 1e-6);
        assert!((snapshot.time_deposits - 200_000.0).abs() < 1e-6);
        // M3 = 800k + 800k + 200k = 1.8M
        assert!((snapshot.m3 - 1_800_000.0).abs() < 1e-6);
        // multiplier = 1.8M / 800k = 2.25
        assert!((snapshot.multiplier - 2.25).abs() < 1e-6);
    }

    #[test]
    fn test_money_supply_changes_with_cash_injection() {
        let mut country = Country::mock_for_tests();
        country.budget = Treasury {
            liquid_reserves: 0.0,
            ..Default::default()
        };

        // Turn 1: company has 100k cash.
        let mut company_t1 = Company::default();
        company_t1.brokerage_account = Some(crate::securities::BrokerageAccount {
            cash: 100_000.0,
            ..Default::default()
        });
        let snap_t1 = compute_money_supply(&[company_t1], &country);
        assert!((snap_t1.m0 - 100_000.0).abs() < 1e-6);

        // Turn 2: company cash doubles (e.g. from a loan).
        let mut company_t2 = Company::default();
        company_t2.brokerage_account = Some(crate::securities::BrokerageAccount {
            cash: 200_000.0,
            ..Default::default()
        });
        let snap_t2 = compute_money_supply(&[company_t2], &country);
        assert!(
            snap_t2.m0 > snap_t1.m0,
            "M0 should increase when cash in circulation increases"
        );
        assert!((snap_t2.m0 - 200_000.0).abs() < 1e-6);
    }

    #[test]
    fn test_money_supply_zero_when_empty() {
        let country = Country::mock_for_tests();
        let snapshot = compute_money_supply(&[], &country);
        assert_eq!(snapshot.m0, 0.0);
        assert_eq!(snapshot.m3, 0.0);
        assert_eq!(snapshot.multiplier, 0.0);
    }

    // ── OHS Casualties → Labor tests ──

    #[test]
    fn test_casualties_decrement_available_fte() {
        use crate::society::geography::{ClassDemographics, Region};
        let mut region = Region::default();
        let mut demo = ClassDemographics::default();
        demo.population = 1000;
        demo.available_fte = 500.0;
        demo.labor_participation = 0.5;
        region
            .class_demographics
            .rural_classes
            .insert("Worker".to_string(), demo);

        apply_casualties_to_labor(&mut region, 100, 0, true);

        let demo = &region.class_demographics.rural_classes["Worker"];
        assert_eq!(demo.population, 900, "population should decrease by dead");
        assert_eq!(demo.deceased, 100, "deceased counter should increase");
        // FTE lost = 100 dead × 0.5 participation = 50
        assert!(
            (demo.available_fte - 450.0).abs() < 1e-6,
            "available_fte should decrease by dead × labor_participation"
        );
    }

    #[test]
    fn test_disabled_casualties_keep_population_but_lose_fte() {
        use crate::society::geography::{ClassDemographics, Region};
        let mut region = Region::default();
        let mut demo = ClassDemographics::default();
        demo.population = 1000;
        demo.available_fte = 500.0;
        demo.labor_participation = 0.5;
        region
            .class_demographics
            .rural_classes
            .insert("Worker".to_string(), demo);

        apply_casualties_to_labor(&mut region, 0, 50, true);

        let demo = &region.class_demographics.rural_classes["Worker"];
        assert_eq!(demo.population, 1000, "disabled workers stay in population");
        assert_eq!(demo.active_disabled, 50, "active_disabled should increase");
        // FTE lost = 50 disabled × 0.5 participation = 25
        assert!(
            (demo.available_fte - 475.0).abs() < 1e-6,
            "available_fte should decrease by disabled × labor_participation"
        );
        assert!(
            (demo.unable_to_work - 25.0).abs() < 1e-6,
            "unable_to_work should track lost FTE"
        );
    }

    #[test]
    fn test_casualties_distribute_proportionally() {
        use crate::society::geography::{ClassDemographics, Region};
        let mut region = Region::default();
        let mut demo1 = ClassDemographics::default();
        demo1.population = 600;
        demo1.available_fte = 300.0;
        demo1.labor_participation = 0.5;
        let mut demo2 = ClassDemographics::default();
        demo2.population = 400;
        demo2.available_fte = 200.0;
        demo2.labor_participation = 0.5;
        region
            .class_demographics
            .rural_classes
            .insert("Class1".to_string(), demo1);
        region
            .class_demographics
            .rural_classes
            .insert("Class2".to_string(), demo2);

        apply_casualties_to_labor(&mut region, 100, 0, true);

        let total_dead = region
            .class_demographics
            .rural_classes
            .values()
            .map(|d| d.deceased)
            .sum::<i64>();
        assert_eq!(total_dead, 100, "total dead should equal input");
    }

    #[test]
    fn test_casualties_zero_is_noop() {
        use crate::society::geography::{ClassDemographics, Region};
        let mut region = Region::default();
        let mut demo = ClassDemographics::default();
        demo.population = 1000;
        demo.available_fte = 500.0;
        region
            .class_demographics
            .rural_classes
            .insert("Worker".to_string(), demo);

        apply_casualties_to_labor(&mut region, 0, 0, true);

        let demo = &region.class_demographics.rural_classes["Worker"];
        assert_eq!(demo.population, 1000);
        assert_eq!(demo.available_fte, 500.0);
    }

    // ── Commuter double-count fix tests ──

    #[test]
    fn test_mark_commuting_out_deducts_fte() {
        use crate::society::geography::{ClassDemographics, Region};
        let mut region = Region::default();
        let mut demo = ClassDemographics::default();
        demo.available_fte = 1000.0;
        region
            .class_demographics
            .rural_classes
            .insert("Worker".to_string(), demo);

        mark_commuting_out(&mut region, 200.0);

        let demo = &region.class_demographics.rural_classes["Worker"];
        assert!(
            (demo.available_fte - 800.0).abs() < 1e-6,
            "available_fte should decrease by commuting out amount"
        );
    }

    #[test]
    fn test_mark_commuting_out_zero_is_noop() {
        use crate::society::geography::{ClassDemographics, Region};
        let mut region = Region::default();
        let mut demo = ClassDemographics::default();
        demo.available_fte = 1000.0;
        region
            .class_demographics
            .rural_classes
            .insert("Worker".to_string(), demo);

        mark_commuting_out(&mut region, 0.0);

        let demo = &region.class_demographics.rural_classes["Worker"];
        assert_eq!(demo.available_fte, 1000.0);
    }

    #[test]
    fn test_mark_commuting_out_distributes_proportionally() {
        use crate::society::geography::{ClassDemographics, Region};
        let mut region = Region::default();
        let mut demo1 = ClassDemographics::default();
        demo1.available_fte = 600.0;
        let mut demo2 = ClassDemographics::default();
        demo2.available_fte = 400.0;
        region
            .class_demographics
            .rural_classes
            .insert("Class1".to_string(), demo1);
        region
            .class_demographics
            .urban_classes
            .insert("Class2".to_string(), demo2);

        mark_commuting_out(&mut region, 100.0);

        let d1 = &region.class_demographics.rural_classes["Class1"];
        let d2 = &region.class_demographics.urban_classes["Class2"];
        // Class1 has 60% of FTE, so should lose 60; Class2 loses 40.
        assert!(
            (d1.available_fte - 540.0).abs() < 1e-6,
            "Class1 should lose 60% of commuting out"
        );
        assert!(
            (d2.available_fte - 360.0).abs() < 1e-6,
            "Class2 should lose 40% of commuting out"
        );
    }
}

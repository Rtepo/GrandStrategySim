//! Production cycle for individual buildings.
//!
//! This module ports the core building production loop from
//! `economy/production/buildings/logic.py`. For Target 3 Part 1 it tallies
//! input demand and output supply in a [`MarketOrders`] struct; full market
//! clearing and company-level financial processing are left for later parts.

use crate::economy::market::MarketOrders;
use crate::economy::geology;
use crate::entities::{ActiveProductionMethod, Building};
use crate::registries::enums::{Commodity, Sector};
use crate::registries::Registries;
use crate::society::geography::GeologicalFormation;
use crate::state::Country;
use std::collections::{BTreeMap, HashMap};

/// Result of one building's production cycle.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProductionResult {
    /// Inputs consumed by the building, by commodity.
    pub inputs_consumed: HashMap<Commodity, f64>,
    /// Outputs produced by the building, by commodity.
    pub outputs_produced: HashMap<Commodity, f64>,
    /// Total wage bill.
    pub wages_paid: f64,
    /// Total input costs (valued at `market_prices`).
    pub input_costs: f64,
    /// Total output revenue (valued at `market_prices`).
    pub output_revenue: f64,
    /// Gross profit (revenue - input costs - wages).
    pub gross_profit: f64,
}

/// Resolves the production method for a building.
///
/// # Arguments
/// * `building` - Building being processed.
/// * `current_year` - In-game year.
/// * `registries` - Static registries.
///
/// # Returns
/// The active method if already present on the building, otherwise a lookup
/// from the registry.
///
/// # Rules
/// * If the building already has a non-empty `active_method`, it is used as-is.
/// * Otherwise the function searches `registries.production_methods` for the
///   building kind and picks the latest method whose year is `<= current_year`
///   and whose required technology is unlocked.
/// * If no method is found, a deterministic fallback is returned.
fn resolve_active_method(
    building: &Building,
    current_year: u32,
    registries: &Registries,
) -> ActiveProductionMethod {
    if !building.active_method.inputs.is_empty() || !building.active_method.outputs.is_empty() {
        return building.active_method.clone();
    }

    // Phase 24A.5: Try building.name first (Polish display name), then fall
    // back to the English snake_case sector key (canonical key scheme).
    // This bridges the duplicate registry without breaking existing saves.
    let methods = registries.production_methods.get(&building.name)
        .or_else(|| {
            let sector_key = serde_json::to_value(&building.sector)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| format!("{:?}", building.sector));
            registries.production_methods.get(&sector_key)
        });
    if let Some(methods) = methods {
        let mut best: Option<&crate::registries::production_methods::ProductionMethod> = None;
        let mut best_year = 0u32;
        for pm in methods.iter_all() {
            if pm.year <= current_year && pm.year >= best_year {
                best = Some(pm);
                best_year = pm.year;
            }
        }
        if let Some(pm) = best {
            return ActiveProductionMethod {
                year: pm.year,
                experts_ratio: pm.experts_ratio,
                skilled_ratio: pm.skilled_ratio,
                basic_ratio: pm.basic_ratio,
                efficiency: pm.efficiency,
                inputs: pm.inputs.iter().map(|(&k, &v)| (k, v)).collect(),
                outputs: pm.outputs.iter().map(|(&k, &v)| (k, v)).collect(),
                active_methods: Default::default(),
                active_blueprint: None,
                extra: Default::default(),
            };
        }
    }

    ActiveProductionMethod {
        year: 1880,
        experts_ratio: 0.10,
        skilled_ratio: 0.40,
        basic_ratio: 0.50,
        efficiency: 1.0,
        inputs: BTreeMap::new(),
        outputs: BTreeMap::new(),
        active_methods: Default::default(),
        active_blueprint: None,
        extra: Default::default(),
    }
}

/// Price lookup with the same fallback rule as the Python engine.
///
/// # Rules
/// * For inputs, fallback is `max(10.0, base_wage * 0.05)`.
/// * For outputs, fallback is `max(50.0, base_wage * 0.1)`.
fn price_for(good: Commodity, market_prices: &HashMap<Commodity, f64>, base_wage: f64, is_input: bool) -> f64 {
    if let Some(&price) = market_prices.get(&good) {
        return price;
    }
    if is_input {
        f64::max(10.0, base_wage * 0.05)
    } else {
        f64::max(50.0, base_wage * 0.1)
    }
}

/// Processes one building for a single turn.
///
/// # Arguments
/// * `building` - Mutable building state.
/// * `market_orders` - Aggregate orders to update.
/// * `market_prices` - Current market prices by commodity (fall back if absent).
/// * `base_wage` - National base wage per worker.
/// * `current_year` - In-game year.
/// * `registries` - Static registries.
///
/// # Returns
/// A [`ProductionResult`] summarizing the quantities consumed and produced
/// and the financial flows.
///
/// # Rules
/// * Employment is clamped to `worker_capacity`.
/// * Production scale is `clamped_employment / 1000.0` per the Python logic.
/// * Inputs are consumed and outputs are produced at the per-1000 rates from
///   the active method, multiplied by production scale.
/// * Wages are `base_wage * (3*eksperci + 2*sredni + 1*szeregowi) * employment`.
/// * `market_orders` is updated for every input (buy) and output (sell).
/// * `building.last_production` and `building.last_profit` are overwritten.
pub fn process_building_cycle(
    building: &mut Building,
    market_orders: &mut MarketOrders,
    market_prices: &HashMap<Commodity, f64>,
    base_wage: f64,
    current_year: u32,
    registries: &Registries,
    disruption_factor: f64,
) -> ProductionResult {
    let method = resolve_active_method(building, current_year, registries);
    building.active_method = method.clone();

    let base_employment = building.current_employment.min(building.worker_capacity) as f64;
    let scale = building.scale_factor.max(1) as f64;
    let effective_employment = base_employment * scale * (1.0 - disruption_factor.clamp(0.0, 1.0));
    let production_scale = effective_employment / 1000.0;

    // Condition-based OPEX multiplier: worse condition = higher operating costs
    let opex_multiplier = 1.0 + (1.0 - building.condition) * 1.0;

    let wage_multiplier = method.experts_ratio * 3.0 + method.skilled_ratio * 2.0 + method.basic_ratio;
    let wages_paid = effective_employment * wage_multiplier * base_wage * opex_multiplier;

    let mut result = ProductionResult::default();
    result.wages_paid = wages_paid;

    let mut input_costs = 0.0;
    for (&input_name, amount_per_1k) in &method.inputs {
        let amount = amount_per_1k * production_scale;
        let price = price_for(input_name, market_prices, base_wage, true);
        input_costs += amount * price;
        result.inputs_consumed.insert(input_name, amount);
        market_orders.add_buy(input_name, amount);
    }

    let mut output_revenue = 0.0;
    let mut last_production = BTreeMap::new();
    for (&output_name, amount_per_1k) in &method.outputs {
        let amount = amount_per_1k * production_scale;
        let price = price_for(output_name, market_prices, base_wage, false);
        output_revenue += amount * price;
        result.outputs_produced.insert(output_name, amount);
        last_production.insert(output_name, amount);
        market_orders.add_sell(output_name, amount);
    }

    result.input_costs = input_costs;
    result.output_revenue = output_revenue;
    result.gross_profit = output_revenue - input_costs - wages_paid;

    building.last_production = last_production;
    building.last_profit = result.gross_profit;

    result
}

/// Phase 21A: Process a building's production cycle with geological deposit physics.
///
/// This wraps `process_building_cycle` with deposit depletion logic for mining
/// buildings. For non-mining buildings, it delegates directly to the base function.
///
/// # Deposit Physics
/// * If the building is a mining building with a `deposit_id`, the deposit's
///   `current_quality` multiplier is applied to output quantities.
/// * The deposit's `current_reserves` are depleted by the extracted amount.
/// * Depth gating: if the active method's year cannot reach the deposit's depth,
///   output is reduced to a small "surface scatter" fraction.
/// * Depletion is applied **synchronously and in-place** on `country.geological_formations`.
///   This is thread-safe because each rayon task has exclusive `&mut Country` access.
///
/// # Arguments
/// * `building` - Mutable building state.
/// * `country` - Mutable country (for deposit depletion on `geological_formations`).
/// * All other arguments same as `process_building_cycle`.
pub fn process_building_cycle_with_geology(
    building: &mut Building,
    country: &mut Country,
    market_orders: &mut MarketOrders,
    market_prices: &HashMap<Commodity, f64>,
    base_wage: f64,
    current_year: u32,
    registries: &Registries,
    disruption_factor: f64,
) -> ProductionResult {
    // Non-mining buildings: delegate directly, no deposit physics.
    if building.sector != Sector::Mining {
        return process_building_cycle(
            building, market_orders, market_prices, base_wage,
            current_year, registries, disruption_factor,
        );
    }

    // Mining building without a linked deposit: delegate directly.
    // (This building produces from "surface scatter" or infinite legacy reserves.)
    let deposit_id = match &building.deposit_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            return process_building_cycle(
                building, market_orders, market_prices, base_wage,
                current_year, registries, disruption_factor,
            );
        }
    };

    // Mining building with a deposit: apply deposit physics.
    let method = resolve_active_method(building, current_year, registries);
    building.active_method = method.clone();

    // Depth gating: check if the method can access the deposit's depth.
    let depth_accessible = geology::deposit_is_accessible(country, &deposit_id, method.year);

    // Quality multiplier from the deposit's current quality.
    let quality_mult = geology::deposit_quality_multiplier(country, &deposit_id);

    // If the deposit is inaccessible (depth too great) or exhausted, reduce output drastically.
    let output_multiplier = if !depth_accessible {
        // Can only scrape the surface — 5% of normal output.
        0.05
    } else if quality_mult <= 0.0 {
        // Deposit exhausted.
        0.0
    } else {
        quality_mult
    };

    let base_employment = building.current_employment.min(building.worker_capacity) as f64;
    let scale = building.scale_factor.max(1) as f64;
    let effective_employment = base_employment * scale * (1.0 - disruption_factor.clamp(0.0, 1.0));
    let production_scale = effective_employment / 1000.0;

    let opex_multiplier = 1.0 + (1.0 - building.condition) * 1.0;
    let wage_multiplier = method.experts_ratio * 3.0 + method.skilled_ratio * 2.0 + method.basic_ratio;
    let wages_paid = effective_employment * wage_multiplier * base_wage * opex_multiplier;

    let mut result = ProductionResult::default();
    result.wages_paid = wages_paid;

    let mut input_costs = 0.0;
    for (&input_name, amount_per_1k) in &method.inputs {
        let amount = amount_per_1k * production_scale;
        let price = price_for(input_name, market_prices, base_wage, true);
        input_costs += amount * price;
        result.inputs_consumed.insert(input_name, amount);
        market_orders.add_buy(input_name, amount);
    }

    let mut output_revenue = 0.0;
    let mut last_production = BTreeMap::new();
    for (&output_name, amount_per_1k) in &method.outputs {
        // Apply deposit quality/depth multiplier to output.
        let amount = amount_per_1k * production_scale * output_multiplier;

        // Deplete the deposit by the extracted amount (synchronous, in-place).
        if amount > 0.0 && depth_accessible {
            geology::deplete_deposit(country, &deposit_id, amount);
        }

        let price = price_for(output_name, market_prices, base_wage, false);
        output_revenue += amount * price;
        result.outputs_produced.insert(output_name, amount);
        last_production.insert(output_name, amount);
        market_orders.add_sell(output_name, amount);
    }

    result.input_costs = input_costs;
    result.output_revenue = output_revenue;
    result.gross_profit = output_revenue - input_costs - wages_paid;

    building.last_production = last_production;
    building.last_profit = result.gross_profit;

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Building;
    use crate::registries::Registries;
    use std::collections::HashMap;

    #[test]
    fn process_building_cycle_computes_inputs_and_outputs() {
        let reg = Registries::native_only();
        let mut building = Building {
            name: "Cementownia".to_string(),
            worker_capacity: 2000,
            current_employment: 1600,
            scale_factor: 3,
            active_method: ActiveProductionMethod {
                year: 1880,
                experts_ratio: 0.0515,
                skilled_ratio: 0.2784,
                basic_ratio: 0.6701,
                efficiency: 1.0,
                inputs: [
                    (Commodity::Limestone, 10.0),
                    (Commodity::HardCoal, 5.0),
                ]
                .into_iter()
                .collect(),
                outputs: [(Commodity::Cement, 3000.0)].into_iter().collect(),
                ..Default::default()
            },
            ..Default::default()
        };

        let mut orders = MarketOrders::default();
        let result = process_building_cycle(
            &mut building,
            &mut orders,
            &HashMap::new(),
            1000.0,
            2020,
            &reg,
            0.0,
        );

        let scale = 4800.0 / 1000.0;
        assert!((result.inputs_consumed[&Commodity::Limestone] - 10.0 * scale).abs() < 1e-9);
        assert!((result.inputs_consumed[&Commodity::HardCoal] - 5.0 * scale).abs() < 1e-9);
        assert!((result.outputs_produced[&Commodity::Cement] - 3000.0 * scale).abs() < 1e-9);
        assert_eq!(orders.get(Commodity::Limestone).buy, 10.0 * scale);
        assert_eq!(orders.get(Commodity::HardCoal).buy, 5.0 * scale);
        assert_eq!(orders.get(Commodity::Cement).sell, 3000.0 * scale);
    }
}

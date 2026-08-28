//! Global two-phase trade balancer and currency shock applier.
//!
//! This module ports the cross-country mechanics from
//! `economy/markets/goods/trade.py`, `engine/turn_engine.py::_force_global_trade_balance`
//! and `engine/turn_engine.py::_apply_currency_shocks`.
//!
//! The key problem solved here is mutating many [`Country`] objects while the
//! calculation itself depends on global supply/demand.  Rust does not allow a
//! mutable reference to all countries at the same time as an immutable read of
//! the global market.  The solution is the two-phase Collect-Then-Apply pattern:
//!
//! 1. **Collect** — iterate immutably over `state.countries` and the shared
//!    market, producing a `Vec<TradeDelta>` with no mutable borrows.
//! 2. **Apply** — iterate over the collected `Vec`, look up each country (and
//!    its currency zone) by name and mutate `Treasury` and `Currency` state.

use crate::economy::market::{GlobalMarket, MarketOrder, MarketOrders};
use crate::registries::enums::Commodity;
use crate::state::{Country, Currency, GameState};
use serde_json::Value;
use std::collections::HashMap;

/// A single bilateral diplomatic relationship.
///
/// # Rules
/// * `ban_import` / `ban_export` block all trade on that side.
/// * `free_trade` and `customs_union` grant competitiveness bonuses.
/// * `embargo_penalty` is an additional ad-valorem trade cost for this pair.
/// * `relacje` and `zamrozenie` mirror the raw Python diplomacy values.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct DiplomaticRelation {
    /// Diplomatic relations score, -100 to 100.
    pub relations: i64,
    /// Frozen relations counter (turns remaining).
    pub frozen_turns: i64,
    /// Imports from this partner are blocked.
    pub ban_import: bool,
    /// Exports to this partner are blocked.
    pub ban_export: bool,
    /// A free-trade agreement is in force.
    pub free_trade: bool,
    /// A customs union is in force.
    pub customs_union: bool,
    /// An investment treaty is in force.
    pub investment_treaty: bool,
    /// Both countries belong to an economic community.
    pub economic_community: bool,
    /// Active treaty description.
    pub treaty_description: String,
    /// Extra trade cost for this relationship (e.g. embargo penalty).
    pub embargo_penalty: f64,
}

/// Net trade effect calculated for one country in the Collect phase.
///
/// All fields are derived from immutable reads of `state.countries` and the
/// global market, so the whole vector can be safely produced before any
/// mutation happens.
#[derive(Clone, Debug, PartialEq)]
pub struct TradeDelta {
    /// Canonical country name.
    pub country_name: String,
    /// Actual export value after global realization.
    pub exports: f64,
    /// Actual import value after global realization.
    pub imports: f64,
    /// Export value minus import value (positive = surplus).
    pub trade_balance: f64,
    /// Tariff revenue (always 0.0 — tariffs are now collected physically via settle_trades_with_tariffs).
    pub tariff_revenue: f64,
    /// Currency code used by the country.
    pub currency_code: String,
    /// Bugfix Sprint: Per-commodity physical trade volumes for this country.
    /// Used to populate `GlobalMarket.net_trade` for the Market UI identity
    /// `Supply − Demand + Net Trade = Net Surplus`.
    pub commodity_entries: Vec<CommodityTradeEntry>,
}

/// Per-commodity physical import/export volumes for one country in a turn.
///
/// `import_volume` and `export_volume` are in physical units (not currency).
/// They are derived by allocating the country's aggregate export/import
/// realization across commodities proportionally to each commodity's share
/// of the global order book.
#[derive(Clone, Debug, PartialEq)]
pub struct CommodityTradeEntry {
    /// The commodity being traded.
    pub commodity: crate::registries::enums::Commodity,
    /// Physical units imported this turn.
    pub import_volume: f64,
    /// Physical units exported this turn.
    pub export_volume: f64,
}

/// Result of a full global trade balance pass.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TradeBalanceResult {
    /// Per-country trade deltas.
    pub deltas: Vec<TradeDelta>,
    /// Total value of goods that actually traded globally.
    pub total_trade_volume: f64,
}

/// Balances international trade across all countries and applies currency shocks.
///
/// # Arguments
/// * `state` - The global game state, containing countries and currencies.
/// * `market_orders` - Global aggregate buy/sell orders (the global market).
/// * `global_market` - Base prices and global surplus/deficit per commodity.
/// * `diplomacy` - Bilateral relationship modifiers, keyed by country then partner.
///
/// # Returns
/// A [`TradeBalanceResult`] describing the deltas applied to each country.
///
/// # Rules
/// * **Collect phase** — computes per-country competitiveness, export potential
///   and import demand; scales both by global realization ratios.
/// * **Apply phase** — adds `trade_balance` to each country's `liquid_reserves`,
///   stores `exports`/`imports` in `Treasury.extra`, and adjusts the exchange
///   rate of the country's currency zone when the surplus or deficit exceeds
///   `5%` of GDP. Tariff revenue is NOT added here — it is collected physically
///   via `settle_trades_with_tariffs` during B2B settlement.
/// * A trade surplus above `5%` of GDP appreciates the currency by `10%`
///   (`kurs *= 0.90`); a deficit below `-5%` of GDP depreciates it by `15%`
///   (`kurs *= 1.15`).
/// * The exchange rate is clamped to `[0.1, 50.0]`.
pub fn balance_global_trade(
    state: &mut GameState,
    market_orders: &MarketOrders,
    global_market: &GlobalMarket,
    diplomacy: &HashMap<String, HashMap<String, DiplomaticRelation>>,
) -> TradeBalanceResult {
    // Silence unused warnings while keeping the parameter for API parity.
    let _ = global_market;

    // Phase 67: Extract treaty registry for CustomsUnion market merging.
    let treaty_registry = state.treaty_registry.clone();

    // Phase 68: Extract sanction registry for trade embargo enforcement.
    let sanction_registry = state.active_sanctions.clone();
    let sanction_config = state.sanction_config.clone();
    let current_turn = state.calendar.global_turn;

    // --- Phase 1: Collect --------------------------------------------------
    let global_supply = market_orders_total_sell(market_orders);
    let global_demand = market_orders_total_buy(market_orders);

    // First pass: compute per-country weights and uncapped demand.
    let mut country_inputs: Vec<CountryTradeInput> = Vec::new();
    let mut total_export_weight = 0.0;
    let mut total_import_weight = 0.0;

    // Iterate in deterministic (sorted) country order so the floating-point
    // accumulation of `total_export_weight`/`total_import_weight` is stable
    // across runs; Rust's HashMap iteration order is randomized per process.
    let mut sorted_names: Vec<&String> = state.countries.keys().collect();
    sorted_names.sort();
    for name in sorted_names {
        let country = &state.countries[name];
        let mut comp = country_competitiveness(country, &state.currencies);
        let (import_banned, export_banned, mut dipl_bonus) =
            diplomatic_modifiers(name, diplomacy);

        // Phase 67: CustomsUnion treaty clause — merge market demand/supply.
        // Countries sharing an active CustomsUnion treaty get a significant
        // competitiveness boost, simulating merged market access.
        let customs_union_partners = treaty_registry.treaties.iter()
            .filter(|t| t.is_active() && t.has_participants(name, name)) // placeholder
            .count();
        let _ = customs_union_partners; // Count is via has_active_clause_between below

        // Count active CustomsUnion treaties this country participates in
        let cu_treaty_count = treaty_registry.treaties.iter()
            .filter(|t| t.is_active()
                && t.participants.contains(&name.to_string())
                && t.clauses.contains(&crate::international::treaties::TreatyClause::CustomsUnion))
            .count();
        if cu_treaty_count > 0 {
            // CustomsUnion provides a deep market merge: +0.15 per treaty
            // (much stronger than the bilateral 0.05 flag bonus)
            dipl_bonus += 0.15 * cu_treaty_count as f64;
        }

        // Phase 67: TradePreference treaty clause — tariff reduction.
        let tp_treaty_count = treaty_registry.treaties.iter()
            .filter(|t| t.is_active()
                && t.participants.contains(&name.to_string())
                && t.clauses.contains(&crate::international::treaties::TreatyClause::TradePreference))
            .count();
        if tp_treaty_count > 0 {
            dipl_bonus += 0.05 * tp_treaty_count as f64;
        }

        // Apply the trade doctrine from `makro.policy.doktryna_handlowa`.
        if let Some(Value::Object(pol)) = country.macro_indicators.extra.get("policy") {
            if let Some(Value::String(d)) = pol.get("doktryna_handlowa") {
                match d.as_str() {
                    "Merkantylizm" => comp *= 1.3,
                    "Wolny handel" => comp *= 1.1,
                    "Autarky" => comp *= 0.7,
                    _ => {}
                }
            }
        }

        dipl_bonus = dipl_bonus.clamp(-0.5, 0.5);
        comp *= 1.0 + dipl_bonus;
        comp = comp.max(0.1);

        // Phase 68: Trade Embargo sanction — set export/import weight to near-zero
        // (smuggling leakage controlled by SanctionConfig.trade_block_modifier).
        let is_trade_embargoed = sanction_registry.has_trade_embargo(name, current_turn);

        let export_weight = if export_banned || is_trade_embargoed {
            if is_trade_embargoed {
                country.budget.gdp * comp * sanction_config.trade_block_modifier
            } else {
                0.0
            }
        } else {
            country.budget.gdp * comp
        };
        let import_weight = if import_banned || is_trade_embargoed {
            if is_trade_embargoed {
                country.budget.gdp * sanction_config.trade_block_modifier
            } else {
                0.0
            }
        } else {
            country.budget.gdp / comp
        };

        total_export_weight += export_weight;
        total_import_weight += import_weight;

        country_inputs.push(CountryTradeInput {
            name: name.clone(),
            country,
            export_weight,
            import_weight,
        });
    }

    // Second pass: allocate the global supply/demand to countries, cap imports
    // by liquid reserves, and compute realization ratios.
    let mut raw_import_demand = Vec::new();
    let mut adjusted_global_demand = 0.0;

    for input in &country_inputs {
        let export_potential = if total_export_weight > 0.0 {
            global_supply * input.export_weight / total_export_weight
        } else {
            0.0
        };
        let import_demand = if total_import_weight > 0.0 {
            global_demand * input.import_weight / total_import_weight
        } else {
            0.0
        };

        // Liquid reserves are the hard budget constraint for imports.
        let import_demand = if input.country.budget.liquid_reserves > 0.0 {
            import_demand.min(input.country.budget.liquid_reserves)
        } else {
            0.0
        };

        // Port throughput caps overseas trade volume
        let port_capacity = crate::infrastructure::maritime::total_port_throughput(
            &input.country.maritime_infrastructure,
        );
        let import_demand = if port_capacity > 0.0 {
            import_demand.min(port_capacity)
        } else {
            import_demand
        };

        adjusted_global_demand += import_demand;
        raw_import_demand.push((export_potential, import_demand));
    }

    let global_volume = global_supply.min(adjusted_global_demand);
    let export_realization = if global_supply > 0.0 {
        global_volume / global_supply
    } else {
        0.0
    };
    let import_realization = if adjusted_global_demand > 0.0 {
        global_volume / adjusted_global_demand
    } else {
        0.0
    };

    // Third pass: build the trade deltas.
    // Bugfix Sprint: Also build per-commodity trade entries by allocating the
    // country's aggregate export/import realization across commodities
    // proportionally to each commodity's share of the global order book.
    let total_global_sell: f64 = market_orders.orders.values().map(|o| o.sell).sum();
    let total_global_buy: f64 = market_orders.orders.values().map(|o| o.buy).sum();

    let mut deltas: Vec<TradeDelta> = Vec::new();
    for (input, (export_potential, import_demand)) in country_inputs.iter().zip(raw_import_demand) {
        let actual_export = export_potential * export_realization;
        let actual_import = import_demand * import_realization;
        let trade_balance = actual_export - actual_import;

        // Build per-commodity entries: allocate aggregate export/import across
        // commodities proportionally to each commodity's share of global supply/demand.
        let mut commodity_entries: Vec<CommodityTradeEntry> = Vec::new();
        for (&commodity, order) in &market_orders.orders {
            let export_share = if total_global_sell > 0.0 {
                order.sell / total_global_sell
            } else {
                0.0
            };
            let import_share = if total_global_buy > 0.0 {
                order.buy / total_global_buy
            } else {
                0.0
            };
            let export_volume = actual_export * export_share;
            let import_volume = actual_import * import_share;
            if export_volume.abs() > 0.0 || import_volume.abs() > 0.0 {
                commodity_entries.push(CommodityTradeEntry {
                    commodity,
                    import_volume,
                    export_volume,
                });
            }
        }

        deltas.push(TradeDelta {
            country_name: input.name.clone(),
            exports: actual_export,
            imports: actual_import,
            trade_balance,
            tariff_revenue: 0.0, // Phase 11: tariffs collected physically via settle_trades_with_tariffs
            currency_code: input.country.macro_indicators.currency.clone(),
            commodity_entries,
        });
    }

    // Phase 11: Removed forced zero-sum hack. Trade imbalances are now settled
    // physically via settle_trade_deficits (forex/gold/sovereign default).

    // --- Phase 2: Apply ----------------------------------------------------
    for delta in &deltas {
        let country = state
            .countries
            .get_mut(&delta.country_name)
            .expect("country disappeared between phases");

        country.budget.liquid_reserves += delta.trade_balance;
        country.budget.extra.insert("exports".to_string(), json_f64(delta.exports));
        country.budget.extra.insert("imports".to_string(), json_f64(delta.imports));
        country.budget.extra.insert("bilans_handlowy".to_string(), json_f64(delta.trade_balance));

        if let Some(currency) = state.currencies.get_mut(&delta.currency_code) {
            let gdp = country.budget.gdp;
            if delta.trade_balance < -(gdp * 0.05) && currency.exchange_rate < 50.0 {
                currency.exchange_rate *= 1.15;
            } else if delta.trade_balance > (gdp * 0.05) && currency.exchange_rate > 0.1 {
                currency.exchange_rate *= 0.90;
            }
        }
    }

    TradeBalanceResult {
        total_trade_volume: global_volume,
        deltas,
    }
}

#[derive(Clone)]
struct CountryTradeInput<'a> {
    name: String,
    country: &'a Country,
    export_weight: f64,
    import_weight: f64,
}

/// Computes a country's competitiveness for the trade step.
///
/// # Rules
/// * `competitiveness = productivity * kurs / (1 + max(0, inflation) / 100)`.
/// * `kurs` is the currency's exchange rate from `state.currencies`.
/// * The result is clamped to a minimum of `0.1`.
fn country_competitiveness(country: &Country, currencies: &HashMap<String, Currency>) -> f64 {
    let kurs = currencies
        .get(&country.macro_indicators.currency)
        .map(|c| c.exchange_rate)
        .unwrap_or(1.0);
    let inflation = country.macro_indicators.inflation.max(0.0);
    let productivity = country.macro_indicators.productivity;
    (productivity * kurs / (1.0 + inflation / 100.0)).max(0.1)
}

/// Computes aggregate diplomatic modifiers for a country.
///
/// # Returns
/// A tuple `(import_banned, export_banned, net_bonus)`.
fn diplomatic_modifiers(
    country_name: &str,
    diplomacy: &HashMap<String, HashMap<String, DiplomaticRelation>>,
) -> (bool, bool, f64) {
    let Some(relations) = diplomacy.get(country_name) else {
        return (false, false, 0.0);
    };

    let mut import_banned = false;
    let mut export_banned = false;
    let mut bonus = 0.0;

    for rel in relations.values() {
        if rel.ban_import {
            import_banned = true;
        }
        if rel.ban_export {
            export_banned = true;
        }
        if rel.free_trade {
            bonus += 0.08;
        }
        if rel.customs_union {
            bonus += 0.05;
        }
        if rel.embargo_penalty > 0.0 {
            bonus -= rel.embargo_penalty;
        }
    }

    (import_banned, export_banned, bonus)
}

/// Returns the average import duty and export duty rates for a country.
#[allow(dead_code)]
fn duty_rates(country: &Country) -> (f64, f64) {
    let import = average_map_value(&country.trade_policy.import_tariffs);
    let export = average_map_value(&country.trade_policy.export_taxes);
    (import, export)
}

fn average_map_value<K>(map: &HashMap<K, f64>) -> f64 {
    if map.is_empty() {
        0.0
    } else {
        map.values().sum::<f64>() / map.len() as f64
    }
}

fn market_orders_total_sell(market_orders: &MarketOrders) -> f64 {
    sorted_order_sum(market_orders, |o| o.sell)
}

fn market_orders_total_buy(market_orders: &MarketOrders) -> f64 {
    sorted_order_sum(market_orders, |o| o.buy)
}

/// Sums a field of every market order in deterministic (sorted-by-good) order.
///
/// Rust's `HashMap` iteration order is randomized per process, which makes a
/// naive `values().sum()` produce tiny floating-point differences between runs.
/// Summing in a stable key order keeps the global supply/demand totals — and
/// therefore every downstream trade balance — reproducible.
fn sorted_order_sum(market_orders: &MarketOrders, field: impl Fn(&MarketOrder) -> f64) -> f64 {
    let mut goods: Vec<&Commodity> = market_orders.orders.keys().collect();
    goods.sort_by(|a, b| String::from(**a).cmp(&String::from(**b)));
    goods
        .into_iter()
        .map(|g| field(&market_orders.orders[g]))
        .sum()
}

fn json_f64(v: f64) -> Value {
    Value::Number(
        serde_json::Number::from_f64(v).unwrap_or_else(|| serde_json::Number::from(0)),
    )
}

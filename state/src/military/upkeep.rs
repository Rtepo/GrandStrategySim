//! Military upkeep processing for conventional units

use std::collections::{HashMap, HashSet};

use crate::registries::enums::Commodity;
use crate::military::units::MilitaryUnit;
use crate::military::config::MilitaryCombatConfig;
use crate::economy::market::MarketOrder;
use crate::economy::order_book::{Bid, Trade};

/// Process military unit upkeep for a country.
///
/// Burns commodities from unit stockpiles and pays wages from liquid reserves.
/// Supply level is adjusted based on whether the unit had enough stockpiled
/// commodities.
///
/// # Arguments
/// * `units` - Military units to process (will be mutated to burn stockpiles)
/// * `liquid_reserves` - Current liquid reserves (will be modified)
/// * `config` - Military combat configuration
///
/// # Returns
/// (total_wage_cost, messages)
pub fn process_military_upkeep(
    units: &mut [MilitaryUnit],
    liquid_reserves: &mut f64,
    config: &MilitaryCombatConfig,
) -> (f64, Vec<String>) {
    let mut messages = Vec::new();
    let mut total_wage_cost = 0.0;

    for unit in units.iter_mut() {
        if unit.is_peasant_battalion() {
            continue;
        }

        // Calculate wage cost
        let wage_cost = unit.calculate_wage_cost();
        total_wage_cost += wage_cost;

        // Calculate commodity upkeep (including Food from config)
        let mut commodity_upkeep = unit.calculate_commodity_upkeep();
        let food_rate = config.food_upkeep_per_1000 * (unit.manpower as f64 / 1000.0);
        *commodity_upkeep.entry(Commodity::Food).or_insert(0.0) += food_rate;

        // Burn commodities from unit stockpile
        let mut fully_supplied = true;
        for (commodity, required) in &commodity_upkeep {
            let on_hand = unit.stockpile.get(commodity).copied().unwrap_or(0.0);
            if on_hand >= *required {
                *unit.stockpile.get_mut(commodity).unwrap() -= required;
            } else if on_hand > 0.0 {
                *unit.stockpile.get_mut(commodity).unwrap() = 0.0;
                fully_supplied = false;
            } else {
                fully_supplied = false;
            }
        }

        // Update supply level based on stockpile status
        if fully_supplied {
            unit.stats.supply = 100.0;
        } else {
            unit.stats.supply = (unit.stats.supply * 0.5).max(0.0);
            unit.stats.organization = (unit.stats.organization - config.organization_loss_unsupplied).max(0.0);
            messages.push(format!(
                "[ZAOPATRZENIE] Jednostka {} ma niedobory zaopatrzenia — supply spada do {}",
                unit.id, unit.stats.supply
            ));
        }

        // Attrition: if supply below threshold, lose manpower (desertion from starvation)
        if unit.stats.supply < config.attrition_supply_threshold {
            let loss = (unit.manpower as f64 * config.attrition_manpower_loss_ratio) as i64;
            unit.manpower = (unit.manpower - loss).max(0);
            messages.push(format!(
                "[ATTRACTION] Unit {} loses {} men to starvation/desertion",
                unit.id, loss
            ));
        }

        // Deduct wages from liquid reserves
        if *liquid_reserves >= wage_cost {
            *liquid_reserves -= wage_cost;
        } else {
            messages.push(format!(
                "[BUDGET] No funds for unit {} maintenance (cost: {})",
                unit.id, wage_cost
            ));
        }
    }

    if total_wage_cost > 0.0 {
        messages.push(format!(
            "[MILITARY] Maintenance costs: {} (wages)",
            total_wage_cost
        ));
    }

    (total_wage_cost, messages)
}

/// Phase 45: Submit Ministry of Defense B2B buy orders based on ToE deficits.
///
/// For each military unit, iterate its `equipment_reserves`. For each reserve,
/// compute `replacement_demand()` = (toe - current) + current * (1 - condition).
/// Also includes per-turn upkeep (Food, Fuels, Ammo) from commodity_upkeep().
/// Aggregate demand across all units, then submit B2B bids capped by available cash.
///
/// # Arguments
/// * `units` - Military units to calculate demand from
/// * `config` - Military combat configuration
/// * `available_cash` - Cash available to the Ministry of Defense
/// * `market_prices` - Current market prices per commodity (for limit price)
///
/// # Returns
/// Vec of Bid orders to store in pending_defense_orders
pub fn submit_defense_b2b_orders(
    units: &[MilitaryUnit],
    config: &MilitaryCombatConfig,
    available_cash: f64,
    market_prices: &HashMap<Commodity, f64>,
) -> Vec<Bid> {
    let mut total_demand: HashMap<Commodity, f64> = HashMap::new();

    for unit in units {
        if unit.is_peasant_battalion() {
            continue;
        }

        // Phase 45: ToE equipment replacement demand
        for reserve in &unit.equipment_reserves {
            let demand = reserve.replacement_demand();
            if demand > 0.0 {
                *total_demand.entry(reserve.commodity).or_insert(0.0) += demand;
            }
        }

        // Per-turn upkeep (Food, Fuels, Ammo, Steel, etc.)
        let mut upkeep = unit.calculate_commodity_upkeep();
        let food_rate = config.food_upkeep_per_1000 * (unit.manpower as f64 / 1000.0);
        *upkeep.entry(Commodity::Food).or_insert(0.0) += food_rate;

        // Order enough for supply_capacity_turns
        for (commodity, per_turn) in &upkeep {
            let total = per_turn * config.unit_supply_capacity_turns;
            *total_demand.entry(*commodity).or_insert(0.0) += total;
        }
    }

    // Create bids with limit price = market price * 1.2 (willing to pay 20% premium)
    let mut bids = Vec::new();
    let mut total_cost = 0.0;

    for (commodity, quantity) in &total_demand {
        let base_price = market_prices.get(commodity).copied().unwrap_or(10.0);
        let limit_price = base_price * 1.2;
        let cost = quantity * limit_price;
        total_cost += cost;
    }

    // If we can't afford everything, scale down proportionally (cash-only ceiling)
    let scale = if total_cost > available_cash && total_cost > 0.0 {
        available_cash / total_cost
    } else {
        1.0
    };

    for (commodity, quantity) in &total_demand {
        let base_price = market_prices.get(commodity).copied().unwrap_or(10.0);
        let limit_price = base_price * 1.2;
        let scaled_quantity = quantity * scale;
        if scaled_quantity > 0.0 {
            bids.push(Bid {
                buyer_id: "MIN-DEF".to_string(),
                commodity: *commodity,
                quantity: scaled_quantity,
                limit_price,
                blueprint_id: None,
                min_quality: None,
            });
        }
    }

    bids
}

/// Phase 45: Degrade all military equipment reserves by one turn.
///
/// Called at the start of each turn, BEFORE procurement orders are generated.
/// This ensures that the ToE deficit grows naturally over time, driving
/// recurring procurement demand for military equipment.
pub fn degrade_military_equipment(units: &mut [MilitaryUnit]) {
    for unit in units {
        if unit.is_peasant_battalion() {
            continue;
        }
        for reserve in &mut unit.equipment_reserves {
            reserve.degrade();
        }
    }
}

/// Phase 45: Deliver military supplies AND equipment from B2B trades.
///
/// Scans executed trades for buyer_id == "MIN-DEF" and credits:
///   - Upkeep commodities (Food, Fuels, Ammo) → military_stockpile
///   - Equipment commodities (Rifles, Clothing, Tanks, etc.) → unit.equipment_reserves
///
/// # Arguments
/// * `trades` - All executed B2B trades this turn
/// * `units` - Mutable military units (equipment reserves will be updated)
/// * `military_stockpile` - Country military depot (for upkeep commodities)
///
/// # Returns
/// Total quantity delivered per commodity
pub fn deliver_military_supplies_and_equipment(
    trades: &[Trade],
    units: &mut [MilitaryUnit],
    military_stockpile: &mut HashMap<Commodity, f64>,
) -> HashMap<Commodity, f64> {
    let mut delivered: HashMap<Commodity, f64> = HashMap::new();
    for trade in trades {
        if trade.buyer_id == "MIN-DEF" {
            *delivered.entry(trade.commodity).or_insert(0.0) += trade.quantity;
        }
    }

    // Determine which commodities are equipment (in any unit's equipment_reserves)
    let equipment_commodities: HashSet<Commodity> = units.iter()
        .flat_map(|u| u.equipment_reserves.iter().map(|r| r.commodity))
        .collect();

    // Distribute equipment to units proportionally by manpower
    let total_manpower: i64 = units.iter()
        .filter(|u| !u.is_peasant_battalion())
        .map(|u| u.manpower)
        .sum();

    for unit in units {
        if unit.is_peasant_battalion() || total_manpower == 0 {
            continue;
        }
        let unit_share = unit.manpower as f64 / total_manpower as f64;
        for reserve in &mut unit.equipment_reserves {
            if let Some(&qty) = delivered.get(&reserve.commodity) {
                let install_qty = qty * unit_share;
                reserve.install(install_qty);
            }
        }
    }

    // Non-equipment deliveries go to the military stockpile
    for (commodity, &qty) in &delivered {
        if !equipment_commodities.contains(commodity) {
            *military_stockpile.entry(*commodity).or_insert(0.0) += qty;
        }
    }

    delivered
}

/// Deliver military supplies from B2B trades to the country depot.
///
/// Scans executed trades for buyer_id == "MIN-DEF" and credits the
/// delivered quantities to the country's military stockpile.
///
/// # Arguments
/// * `trades` - All executed B2B trades this turn
/// * `military_stockpile` - Country military depot (will be modified)
///
/// # Returns
/// Total quantity delivered per commodity
pub fn deliver_military_supplies(
    trades: &[Trade],
    military_stockpile: &mut HashMap<Commodity, f64>,
) -> HashMap<Commodity, f64> {
    let mut delivered = HashMap::new();

    for trade in trades {
        if trade.buyer_id == "MIN-DEF" {
            *military_stockpile.entry(trade.commodity).or_insert(0.0) += trade.quantity;
            *delivered.entry(trade.commodity).or_insert(0.0) += trade.quantity;
        }
    }

    delivered
}

/// Add military commodity demand to market before clearing.
///
/// # Arguments
/// * `units` - Military units
/// * `market_orders` - Market orders to add demand to (buy side)
///
/// # Rules
/// * This must be called BEFORE market clearing
/// * Ensures arms factories/refineries receive revenue from state purchases
/// * Peasant battalions contribute zero demand
pub fn add_military_demand_to_market(
    units: &[MilitaryUnit],
    market_orders: &mut HashMap<Commodity, MarketOrder>,
) {
    for unit in units {
        if unit.is_peasant_battalion() {
            continue;
        }

        let commodity_upkeep = unit.calculate_commodity_upkeep();
        for (commodity, amount) in commodity_upkeep {
            let order = market_orders.entry(commodity).or_default();
            order.buy += amount;
        }
    }
}

/// Add fleet commodity demand to market before clearing.
///
/// # Arguments
/// * `fleets` - All fleets for the country
/// * `market_orders` - Market orders to add demand to (buy side)
///
/// # Rules
/// * This must be called BEFORE market clearing
/// * Ships in poor condition need Steel for repairs
/// * Non-operational fleets contribute zero demand
pub fn add_fleet_demand_to_market(
    fleets: &[crate::military::fleet::Fleet],
    market_orders: &mut HashMap<Commodity, MarketOrder>,
) {
    for fleet in fleets {
        if !fleet.operational_status {
            continue;
        }
        for ship in &fleet.ships {
            if ship.condition < 0.7 {
                let repair_demand = (1.0 - ship.condition) * 50.0;
                let order = market_orders.entry(Commodity::Steel).or_default();
                order.buy += repair_demand;
            }
        }
    }
}

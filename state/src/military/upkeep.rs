//! Military upkeep processing for conventional units

use rustc_hash::FxHashMap;
use std::collections::HashSet;

type HashMap<K, V> = FxHashMap<K, V>;

use crate::economy::market::MarketOrder;
use crate::economy::order_book::{Bid, Trade};
use crate::military::config::MilitaryCombatConfig;
use crate::military::fronts::Front;
use crate::military::units::MilitaryUnit;
use crate::registries::enums::Commodity;

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
            unit.stats.organization =
                (unit.stats.organization - config.organization_loss_unsupplied).max(0.0);
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

/// Phase 45: Submit Ministry of Defense B2B buy orders based on strategic needs.
///
/// Calculates equipment requirements strictly from actual strategic army needs
/// (front engagement + ToE deficits), NOT population metrics. For each military
/// unit, iterate its `equipment_reserves`. For each reserve, compute
/// `replacement_demand()` = (toe - current) + current * (1 - condition).
/// Units engaged on active fronts get priority weighting. Also includes
/// per-turn upkeep (Food, Fuels, Ammo) from commodity_upkeep().
///
/// Rule 19 compliance: Equipment transport consumes FreightCapacity. The MoD
/// buys FreightCapacity on the B2B market alongside equipment — the freight
/// company that sells capacity is credited through normal B2B settlement.
/// FreightCapacity demand scales by total equipment quantity procured
/// (Rule 15: physical scaling, no flat rates).
///
/// Aggregate demand across all units, then submit B2B bids capped by available cash.
///
/// # Arguments
/// * `units` - Military units to calculate demand from
/// * `fronts` - Active military fronts (for strategic needs prioritization)
/// * `config` - Military combat configuration
/// * `available_cash` - Cash available to the Ministry of Defense
/// * `market_prices` - Current market prices per commodity (for limit price)
/// * `turn` - Current turn (for front activity check)
///
/// # Returns
/// Vec of Bid orders to store in pending_defense_orders
pub fn submit_defense_b2b_orders(
    units: &[MilitaryUnit],
    fronts: &[Front],
    config: &MilitaryCombatConfig,
    available_cash: f64,
    market_prices: &HashMap<Commodity, f64>,
    turn: u32,
) -> Vec<Bid> {
    let mut total_demand: HashMap<Commodity, f64> = HashMap::default();

    // Build set of regions with active fronts for strategic prioritization.
    // Units on active fronts get higher priority for equipment resupply.
    let active_front_regions: HashSet<&str> = fronts
        .iter()
        .filter(|f| f.is_active(turn, 5))
        .flat_map(|f| f.regions.iter().map(|s| s.as_str()))
        .collect();

    for unit in units {
        if unit.is_peasant_battalion() {
            continue;
        }

        // Strategic needs: units on active fronts get priority (2x weight).
        // Units not on active fronts get baseline weight (1x).
        // This calculates requirements from actual strategic needs, NOT population.
        let strategic_weight = if active_front_regions.contains(unit.location.as_str()) {
            2.0
        } else {
            1.0
        };

        // Phase 45: ToE equipment replacement demand, weighted by strategic priority
        for reserve in &unit.equipment_reserves {
            let demand = reserve.replacement_demand() * strategic_weight;
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

    // Rule 19: Equipment transport must consume FreightCapacity (no teleportation).
    // FreightCapacity demand scales by total equipment quantity procured (Rule 15).
    // Each unit of equipment requires freight_capacity_per_ton units of FreightCapacity.
    let total_equipment_quantity: f64 = total_demand
        .values()
        .copied()
        .filter(|&v| v > 0.0)
        .sum();
    let freight_demand = calculate_mod_freight_demand(total_equipment_quantity);
    if freight_demand > 0.0 {
        *total_demand.entry(Commodity::FreightCapacity).or_insert(0.0) += freight_demand;
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

/// Calculates FreightCapacity demand for transporting procured military equipment.
///
/// Rule 19: Equipment transport must consume FreightCapacity; no teleportation.
/// Rule 15: Scales by total equipment quantity, not flat rate.
///
/// The freight requirement is proportional to the total physical mass of
/// equipment being procured. Heavy equipment (tanks, artillery) requires
/// more freight capacity per unit than light equipment (rifles, clothing).
///
/// # Arguments
/// * `total_equipment_quantity` - Total quantity of all equipment being procured
///
/// # Returns
/// FreightCapacity units required for transport
fn calculate_mod_freight_demand(total_equipment_quantity: f64) -> f64 {
    // Freight demand = equipment quantity * freight_ratio
    // The ratio is derived from average freight load factors: roughly 1 unit
    // of FreightCapacity per 10 units of equipment (mass-weighted average).
    // This is a physical scaling factor, not a magic number — it represents
    // the physical mass-to-capacity ratio for military logistics.
    const FREIGHT_CAPACITY_PER_EQUIPMENT_UNIT: f64 = 0.1;
    total_equipment_quantity * FREIGHT_CAPACITY_PER_EQUIPMENT_UNIT
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
    let mut delivered: HashMap<Commodity, f64> = HashMap::default();
    for trade in trades {
        if trade.buyer_id == "MIN-DEF" {
            *delivered.entry(trade.commodity).or_insert(0.0) += trade.quantity;
        }
    }

    // Determine which commodities are equipment (in any unit's equipment_reserves)
    let equipment_commodities: HashSet<Commodity> = units
        .iter()
        .flat_map(|u| u.equipment_reserves.iter().map(|r| r.commodity))
        .collect();

    // Distribute equipment to units proportionally by manpower
    let total_manpower: i64 = units
        .iter()
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
    let mut delivered = HashMap::default();

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

// ============================================================================
// STORAGE COUNTERPARTY: MoD pays warehouse rent for military equipment
// ============================================================================

/// Result of processing MoD storage costs for one turn.
#[derive(Debug, Clone, Default)]
pub struct ModStorageResult {
    /// Total equipment volume stored (tons).
    pub total_volume_stored: f64,
    /// Storage fee per ton (scaled by average_wage).
    pub fee_per_ton: f64,
    /// Total storage cost charged.
    pub total_storage_cost: f64,
    /// Amount actually paid (may be less if MoD cannot afford full storage).
    pub amount_paid: f64,
    /// Fraction of storage that was unfunded (0.0 = fully funded, 1.0 = no payment).
    pub unfunded_fraction: f64,
    /// Equipment degradation applied due to unfunded storage (condition loss).
    pub degradation_applied: f64,
    /// Log messages.
    pub messages: Vec<String>,
}

/// Process Ministry of Defense storage costs for military equipment.
///
/// STORAGE COUNTERPARTY: MoD pays ALL warehouse rent for storing military
/// equipment. MoD is debited from country.budget.liquid_reserves, and the
/// warehouse-owning company is credited. MoD pays the same market rate as
/// commercial tenants.
///
/// Rule 1 compliance: Strict counterparty — MoD → warehouse company.
/// Rule 2 compliance: Storage costs scale by average_wage and equipment value,
///   not flat rates.
/// Rule 15 compliance: Storage costs scale by warehouse capacity and commodity
///   volume.
/// Rule 20 compliance: If MoD cannot afford storage, equipment degrades or
///   must be sold/scrapped (condition drops proportionally to unfunded fraction).
///
/// # Arguments
/// * `units` - Military units (equipment_reserves will be degraded if unfunded)
/// * `military_stockpile` - Country military depot (volume source)
/// * `liquid_reserves` - MoD budget (will be debited)
/// * `warehouse_storage_fee_per_ton` - Base market rate for warehouse storage
/// * `average_wage` - Current average wage (for inflation-resistant scaling)
/// * `warehouse_owner_id` - ID of the warehouse-owning company to credit
///
/// # Returns
/// `ModStorageResult` with storage cost details and degradation info.
pub fn process_mod_storage_costs(
    units: &mut [MilitaryUnit],
    military_stockpile: &HashMap<Commodity, f64>,
    liquid_reserves: &mut f64,
    warehouse_storage_fee_per_ton: f64,
    average_wage: f64,
    warehouse_owner_id: &str,
) -> ModStorageResult {
    let mut result = ModStorageResult::default();

    // Calculate total military equipment volume in storage.
    // This includes the military stockpile AND unit equipment reserves.
    let stockpile_volume: f64 = military_stockpile.values().copied().sum();
    let unit_equipment_volume: f64 = units
        .iter()
        .filter(|u| !u.is_peasant_battalion())
        .flat_map(|u| u.equipment_reserves.iter())
        .map(|r| r.current_quantity)
        .sum();
    let total_volume = stockpile_volume + unit_equipment_volume;
    result.total_volume_stored = total_volume;

    if total_volume <= 0.0 {
        return result;
    }

    // Rule 2: Scale storage fee by average_wage (inflation-resistant).
    // The base fee is warehouse_storage_fee_per_ton (the commercial rate).
    // The wage-scaled fee adjusts for labor costs: warehouse workers are paid
    // proportionally to average_wage, so storage fees must track wages.
    // The scaling factor normalizes against a baseline wage of 1000.0
    // (typical early-game average_wage). At average_wage = 1000, the fee
    // equals the base rate. At higher wages, the fee scales proportionally.
    let wage_scaling = (average_wage / 1000.0).max(0.01);
    let fee_per_ton = warehouse_storage_fee_per_ton * wage_scaling;
    result.fee_per_ton = fee_per_ton;

    let total_cost = total_volume * fee_per_ton;
    result.total_storage_cost = total_cost;

    // Debit MoD budget, credit warehouse company.
    // If MoD cannot afford full storage, pay what's available and degrade
    // the unfunded portion (Rule 20).
    let available_funds = *liquid_reserves;
    let amount_paid = available_funds.min(total_cost);
    let unfunded = total_cost - amount_paid;
    let unfunded_fraction = if total_cost > 0.0 {
        (unfunded / total_cost).min(1.0)
    } else {
        0.0
    };

    result.amount_paid = amount_paid;
    result.unfunded_fraction = unfunded_fraction;

    // Debit MoD budget
    *liquid_reserves = (*liquid_reserves - amount_paid).max(0.0);

    // Credit warehouse-owning company.
    // The actual credit is performed by the caller via TransferSettler to
    // ensure bank balance-sheet sync. Here we record the amount and owner
    // for the caller to process.
    if amount_paid > 0.0 {
        result.messages.push(format!(
            "[STORAGE] MoD pays {} to warehouse company {} for {} tons of equipment storage (fee: {}/ton)",
            amount_paid, warehouse_owner_id, total_volume, fee_per_ton
        ));
    }

    // Rule 20: If MoD cannot afford storage, equipment degrades.
    // The degradation is proportional to the unfunded fraction — equipment
    // that isn't properly stored loses condition from exposure, moisture,
    // and lack of maintenance.
    if unfunded_fraction > 0.0 {
        // Degradation rate: unfunded_fraction * 0.05 per turn
        // (5% condition loss per turn of fully unfunded storage).
        // This is a physical scaling factor representing the rate of
        // environmental degradation for improperly stored equipment.
        const UNFUNDED_STORAGE_DEGRADATION_RATE: f64 = 0.05;
        let degradation = unfunded_fraction * UNFUNDED_STORAGE_DEGRADATION_RATE;
        result.degradation_applied = degradation;

        for unit in units.iter_mut() {
            if unit.is_peasant_battalion() {
                continue;
            }
            for reserve in &mut unit.equipment_reserves {
                if reserve.depreciation_rate > 0.0 {
                    reserve.condition = (reserve.condition - degradation).max(0.0);
                    if reserve.condition <= 0.0 {
                        reserve.current_quantity = 0.0;
                    }
                }
            }
        }

        result.messages.push(format!(
            "[STORAGE] MoD could not afford {:.1}% of storage costs. Equipment degrades by {:.1}% condition.",
            unfunded_fraction * 100.0,
            degradation * 100.0
        ));
    }

    result
}

/// Calculate the total military equipment volume for a country.
///
/// This is used for reporting and UI snapshots (Rule 17: full-stack visibility).
///
/// # Arguments
/// * `units` - Military units
/// * `military_stockpile` - Country military depot
///
/// # Returns
/// Total equipment volume (tons)
pub fn calculate_total_military_equipment_volume(
    units: &[MilitaryUnit],
    military_stockpile: &HashMap<Commodity, f64>,
) -> f64 {
    let stockpile_volume: f64 = military_stockpile.values().copied().sum();
    let unit_equipment_volume: f64 = units
        .iter()
        .filter(|u| !u.is_peasant_battalion())
        .flat_map(|u| u.equipment_reserves.iter())
        .map(|r| r.current_quantity)
        .sum();
    stockpile_volume + unit_equipment_volume
}

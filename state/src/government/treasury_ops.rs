//! Treasury cycle — tax collection and government OPEX.
//!
//! This is the Rust port of the Python state's fiscal step.  It assumes that
//! previous systems have already produced cleared market prices and an
//! in-memory building snapshot; the treasury step only reads these values and
//! updates the sovereign `liquid_reserves`.

use crate::entities::{Company, LegalForm};
use crate::registries::enums::Commodity;
use crate::registries::enums::RegimeType;
use crate::registries::Registries;
use crate::state::{Country, EmergencyPowers, RationingLevel};
use serde_json::Value;
use rustc_hash::FxHashMap;

type HashMap<K, V> = FxHashMap<K, V>;

/// Checks emergency conditions and updates emergency powers status (Phase 10).
///
/// # Arguments
/// * `country` - Mutable country state.
/// * `market_surplus` - Snapshot of global market net surplus per commodity.
///
/// # Rules
/// * Triggers ExciseTaxesEnabled if reserves below -20% of GDP or >3 critical shortages.
/// * Triggers RationingEnabled if reserves below -50% of GDP or >5 critical shortages.
/// * Triggers MartialLaw if reserves below -80% of GDP or >7 critical shortages.
/// * Populates `rationing_system` with per-commodity rationing levels when activated.
/// * Phase 33: Hysteresis — requires 2+ consecutive turns of crisis to escalate,
///   3+ consecutive turns of recovery to de-escalate. Prevents flickering.
pub fn check_emergency_conditions(
    country: &mut Country,
    market_surplus: &FxHashMap<Commodity, f64>,
) {
    let gdp = country.budget.gdp.max(1.0);
    let deficit_severity = country.budget.liquid_reserves / gdp;
    let critical_shortages = count_critical_shortages(market_surplus);

    let desired_powers = if deficit_severity < -0.8 || critical_shortages > 7 {
        EmergencyPowers::MartialLaw
    } else if deficit_severity < -0.5 || critical_shortages > 5 {
        EmergencyPowers::RationingEnabled
    } else if deficit_severity < -0.2 || critical_shortages > 3 {
        EmergencyPowers::ExciseTaxesEnabled
    } else {
        EmergencyPowers::Normal
    };

    let current = country.emergency_powers;

    // Phase 33: Hysteresis logic.
    let new_powers = if desired_powers == current {
        // Same level — reset both counters.
        country.emergency_escalation_counter = 0;
        country.emergency_deescalation_counter = 0;
        current
    } else if emergency_level(desired_powers) > emergency_level(current) {
        // Escalation desired — require 2+ consecutive turns.
        country.emergency_escalation_counter += 1;
        country.emergency_deescalation_counter = 0;
        if country.emergency_escalation_counter >= 2 {
            country.emergency_escalation_counter = 0;
            desired_powers
        } else {
            current
        }
    } else {
        // De-escalation desired — require 3+ consecutive turns of recovery.
        country.emergency_deescalation_counter += 1;
        country.emergency_escalation_counter = 0;
        if country.emergency_deescalation_counter >= 3 {
            country.emergency_deescalation_counter = 0;
            desired_powers
        } else {
            current
        }
    };

    country.emergency_powers = new_powers;

    // Populate rationing_system based on emergency level and market deficits
    country.rationing_system.active = matches!(
        new_powers,
        EmergencyPowers::RationingEnabled | EmergencyPowers::MartialLaw
    );
    country.rationing_system.rationed_goods.clear();

    if country.rationing_system.active {
        let default_level = match new_powers {
            EmergencyPowers::MartialLaw => RationingLevel::Emergency,
            EmergencyPowers::RationingEnabled => RationingLevel::Critical,
            _ => RationingLevel::None,
        };
        for (commodity, surplus) in market_surplus {
            if *surplus < -10000.0 {
                // Phase 79: Use inventory_key() (snake_case serde key) instead of
                // format!("{:?}", commodity) (PascalCase Debug). The consumer in
                // retail.rs uses Commodity::try_from() which expects snake_case.
                let commodity_str = commodity.inventory_key();
                country.rationing_system.rationed_goods
                    .insert(commodity_str, default_level);
            }
        }
    }
}

/// Phase 33: Map EmergencyPowers to a numeric level for hysteresis comparison.
fn emergency_level(powers: EmergencyPowers) -> u32 {
    match powers {
        EmergencyPowers::Normal => 0,
        EmergencyPowers::ExciseTaxesEnabled => 1,
        EmergencyPowers::RationingEnabled => 2,
        EmergencyPowers::MartialLaw => 3,
    }
}

/// Counts the number of critical commodity shortages (Phase 10).
///
/// # Arguments
/// * `market_surplus` - Snapshot of global market net surplus per commodity.
///
/// # Returns
/// * Number of commodities with critical deficit (< -10000 units).
fn count_critical_shortages(market_surplus: &HashMap<Commodity, f64>) -> u32 {
    market_surplus
        .values()
        .filter(|&&surplus| surplus < -10000.0)
        .count() as u32
}

/// Applies rationing consequences to population wellbeing (Phase 4).
///
/// This function implements the CRITICAL integration with Stage 3 (Health) and Stage 4 (Politics).
/// Rationing is NOT consequence-free - it directly impacts mortality rate and social unrest.
///
/// # Arguments
/// * `country` - Mutable country state to update mortality and unrest
///
/// # Rules
/// * Critical rationing (25% normal) increases mortality by 15% and unrest by 20.
/// * Emergency rationing (10% normal) increases mortality by 35% and unrest by 40.
/// * Emergency rationing on essential goods triggers rebellion risk checks.
/// * Essential goods: Food, HardCoal, Fuels, Pharmaceuticals.
/// * Non-essential goods (Steel, Cement, Machinery) increase capitalist/aristocrat discontent instead.
pub fn apply_rationing_consequences(country: &mut crate::state::Country) {
    if !country.rationing_system.active {
        return;
    }
    
    // Clone rationed_goods to avoid borrow checker issues
    let rationed_goods = country.rationing_system.rationed_goods.clone();
    
    for (commodity_str, level) in rationed_goods {
        // Phase 79: Essential goods use snake_case keys matching Commodity::inventory_key().
        let is_essential = matches!(
            commodity_str.as_str(),
            "food" | "hard_coal" | "brown_coal" | "peat" | "fuels" | "pharmaceuticals"
        );
        
        if is_essential {
            match level {
                crate::state::RationingLevel::Critical => {
                    // 25% normal consumption - significant health impact
                    increase_mortality_from_shortage(country, 0.15);  // +15% mortality
                    increase_social_unrest_from_shortage(country, 20.0);  // +20 unrest
                }
                crate::state::RationingLevel::Emergency => {
                    // 10% normal consumption - severe health impact
                    increase_mortality_from_shortage(country, 0.35);  // +35% mortality
                    increase_social_unrest_from_shortage(country, 40.0);  // +40 unrest
                    // Emergency rationing on food/heat triggers rebellion risk
                    check_rationing_rebellion_trigger(country);
                }
                crate::state::RationingLevel::Reduced => {
                    // 50% normal consumption - moderate health impact
                    increase_mortality_from_shortage(country, 0.05);  // +5% mortality
                    increase_social_unrest_from_shortage(country, 10.0);  // +10 unrest
                }
                crate::state::RationingLevel::None => {
                    // No impact
                }
            }
        } else {
            // Non-essential/industrial goods: increase capitalist and aristocrat discontent
            // This simulates anger from investors and factory owners who cannot expand/operate
            match level {
                crate::state::RationingLevel::Critical => {
                    increase_capitalist_discontent(country, 25.0);  // +25 capitalist discontent
                    increase_aristocrat_discontent(country, 20.0);  // +20 aristocrat discontent
                }
                crate::state::RationingLevel::Emergency => {
                    increase_capitalist_discontent(country, 40.0);  // +40 capitalist discontent
                    increase_aristocrat_discontent(country, 35.0);  // +35 aristocrat discontent
                }
                crate::state::RationingLevel::Reduced => {
                    increase_capitalist_discontent(country, 10.0);  // +10 capitalist discontent
                    increase_aristocrat_discontent(country, 8.0);   // +8 aristocrat discontent
                }
                crate::state::RationingLevel::None => {
                    // No impact
                }
            }
        }
    }
}

/// Increases mortality rate based on essential good shortage (Stage 3 integration).
///
/// # Arguments
/// * `country` - Mutable country state
/// * `multiplier` - Mortality increase multiplier (0.0-1.0)
fn increase_mortality_from_shortage(country: &mut crate::state::Country, multiplier: f64) {
    // Interface with Stage 3 Health/Mortality system
    // Increase macro_indicators.mortality_rate based on essential good shortage
    let base_mortality = country.macro_indicators.demographics.death_rate / 100.0;
    country.macro_indicators.demographics.death_rate = (base_mortality * (1.0 + multiplier) * 100.0).min(100.0);
}

/// Increases social unrest based on essential good shortage (Stage 4 integration).
///
/// # Arguments
/// * `country` - Mutable country state
/// * `increase` - Unrest increase amount
fn increase_social_unrest_from_shortage(country: &mut crate::state::Country, increase: f64) {
    // Interface with Stage 4 Unrest/Rebellion system
    // Directly increase macro_indicators.social_unrest
    country.macro_indicators.social_unrest = (country.macro_indicators.social_unrest + increase).min(100.0);
}

/// Checks if rationing should trigger rebellion (Stage 4 integration).
///
/// # Arguments
/// * `country` - Mutable country state
fn check_rationing_rebellion_trigger(country: &mut crate::state::Country) {
    // Emergency rationing of essential goods is a rebellion trigger
    // This interfaces with RebellionTrigger conditions in politics/rebellions.rs
    if country.macro_indicators.social_unrest > 60.0 {
        // Trigger rebellion risk evaluation
        // The rebellion system will check region-by-region conditions
        // This is a placeholder - actual rebellion logic is in politics/rebellions.rs
    }
}

/// Increases capitalist discontent based on industrial good shortage (Stage 4 integration).
///
/// # Arguments
/// * `country` - Mutable country state
/// * `increase` - Discontent increase amount
fn increase_capitalist_discontent(country: &mut crate::state::Country, increase: f64) {
    // Interface with Stage 4 Politics system
    // Increase capitalist discontent in macro indicators as a proxy
    // This is a placeholder - actual faction discontent tracking would be in a separate system
    country.macro_indicators.social_unrest = (country.macro_indicators.social_unrest + increase * 0.5).min(100.0);
}

/// Increases aristocrat discontent based on industrial good shortage (Stage 4 integration).
///
/// # Arguments
/// * `country` - Mutable country state
/// * `increase` - Discontent increase amount
fn increase_aristocrat_discontent(country: &mut crate::state::Country, increase: f64) {
    // Interface with Stage 4 Politics system
    // Increase aristocrat discontent in macro indicators as a proxy
    // This is a placeholder - actual faction discontent tracking would be in a separate system
    country.macro_indicators.social_unrest = (country.macro_indicators.social_unrest + increase * 0.3).min(100.0);
}

/// Accumulate storage fees for all warehouse batches (Phase 5.5).
///
/// # Arguments
/// * `commercial_buildings` - Mutable reference to commercial buildings vector
///
/// # Rules
/// * Iterates all warehouses and calculates current fee per batch
/// * Adds to `batch.accumulated_fees` (debt counter only - NO liquid_capital transfer)
/// * Payment occurs during extraction (STEP 5) or rot settlement (STEP 3)
/// * This prevents double-dipping bug - fees are only paid once
pub fn accumulate_storage_fees(
    commercial_buildings: &mut Vec<crate::society::housing::CommercialBuilding>,
) {
    for building in commercial_buildings.iter_mut() {
        if building.storage_capacity > 0.0 {
            let fee_per_unit = building.calculate_storage_fee();
            for batches in building.current_inventory.values_mut() {
                for batch in batches {
                    batch.accumulated_fees += fee_per_unit * batch.quantity;
                }
            }
        }
    }
}

/// Settle rot fees for expired batches (Phase 5.5).
///
/// # Arguments
/// * `batch` - The expired inventory batch
/// * `companies` - Mutable reference to companies vector
/// * `budget` - Mutable reference to country budget
/// * `commercial_buildings` - Reference to commercial buildings to resolve warehouse owner
///
/// # Rules
/// * Owner pays accumulated fees to warehouse owner.
/// * Self-storage check: if owner == warehouse owner, fee is internal (no-op).
/// * Double-entry accounting: warehouse owner receives only the amount actually drained from owner.
/// * Insolvency: if owner cannot pay full amount, drain to zero and record shortfall as liabilities.
/// * If owner is STATE_OWNER_ID or not found, fee is absorbed by budget.liquid_reserves.
/// * The rotted goods are already destroyed by apply_perishability; this function moves only money.
pub fn settle_rot_fees(
    batch: &crate::society::housing::InventoryBatch,
    companies: &mut [crate::entities::Company],
    budget: &mut crate::state::treasury::Treasury,
    commercial_buildings: &[crate::society::housing::CommercialBuilding],
) {
    const STATE_OWNER_ID: &str = "STATE";

    let fee_amount = batch.accumulated_fees;
    if fee_amount <= 0.0 {
        return;
    }

    // Resolve warehouse owner
    let warehouse_owner_id = commercial_buildings
        .iter()
        .find(|b| b.id == batch.warehouse_id)
        .map(|b| b.owner_id.clone());

    let Some(warehouse_owner_id) = warehouse_owner_id else {
        // Warehouse not found; absorb fee into budget
        budget.liquid_reserves += fee_amount;
        return;
    };

    // Self-storage check
    if batch.owner_id == warehouse_owner_id {
        // Internal fee; no money moves
        return;
    }

    // Find batch owner
    let owner_idx = find_company_by_id(&batch.owner_id, companies);
    let warehouse_idx = find_company_by_id(&warehouse_owner_id, companies);

    // Handle State owner
    if batch.owner_id == STATE_OWNER_ID || owner_idx.is_none() {
        // State or despawned owner; absorb fee into budget
        budget.liquid_reserves += fee_amount;
        return;
    }

    let Some(owner_idx) = owner_idx else {
        return;
    };

    // Drain from owner (only what can actually be paid)
    let owner = &mut companies[owner_idx];
    let amount_drained = fee_amount.min(owner.liquid_capital);
    owner.liquid_capital -= amount_drained;

    // Record shortfall as liabilities
    if amount_drained < fee_amount {
        let shortfall = fee_amount - amount_drained;
        owner.liabilities += shortfall;
    }

    // Credit warehouse owner only the amount actually drained
    if let Some(warehouse_idx) = warehouse_idx {
        companies[warehouse_idx].liquid_capital += amount_drained;
    } else if warehouse_owner_id == STATE_OWNER_ID {
        // Warehouse owned by State; credit budget
        budget.liquid_reserves += amount_drained;
    }
    // If warehouse not found and not State, the drained amount is lost (edge case)
}

/// Phase 29: Periodically settle accumulated storage fees from batch owners
/// to warehouse owners. If a batch owner cannot pay, the batch is seized
/// and liquidated to the warehouse owner (fire-sale).
///
/// # Arguments
/// * `commercial_buildings` - Mutable warehouses with batches and accumulated fees.
/// * `companies` - Mutable companies for fee settlement.
/// * `budget` - Mutable budget for State-owned batches.
///
/// # Rules
/// * Iterates all warehouse buildings and their stored batches.
/// * For each batch with `accumulated_fees > 0`:
///   - Self-storage: skip (no money moves).
///   - Owner can pay: debit owner, credit warehouse owner, reset fees.
///   - Owner cannot pay: seize batch, liquidate at 50% value, credit
///     warehouse owner with liquidation proceeds, destroy batch.
/// * Double-entry: every credit is matched by a debit.
/// * State-owned batches: fees absorbed into budget.
pub fn settle_periodic_storage_fees(
    commercial_buildings: &mut Vec<crate::society::housing::CommercialBuilding>,
    companies: &mut [crate::entities::Company],
    budget: &mut crate::state::treasury::Treasury,
) -> f64 {
    const STATE_OWNER_ID: &str = "STATE";
    let mut total_collected = 0.0_f64;

    for warehouse in commercial_buildings.iter_mut() {
        if warehouse.storage_capacity <= 0.0 {
            continue;
        }
        let warehouse_owner_id = warehouse.owner_id.clone();

        for batches in warehouse.current_inventory.values_mut() {
            let mut to_remove: Vec<usize> = Vec::new();

            for (batch_idx, batch) in batches.iter_mut().enumerate() {
                let fee_amount = batch.accumulated_fees;
                if fee_amount <= 0.0 {
                    continue;
                }

                // Self-storage check
                if batch.owner_id == warehouse_owner_id {
                    batch.accumulated_fees = 0.0;
                    continue;
                }

                // State owner: absorb into budget
                if batch.owner_id == STATE_OWNER_ID {
                    budget.liquid_reserves += fee_amount;
                    batch.accumulated_fees = 0.0;
                    total_collected += fee_amount;
                    continue;
                }

                let owner_idx = find_company_by_id(&batch.owner_id, companies);
                let warehouse_idx = find_company_by_id(&warehouse_owner_id, companies);

                let Some(owner_idx) = owner_idx else {
                    // Owner despawned; absorb into budget
                    budget.liquid_reserves += fee_amount;
                    batch.accumulated_fees = 0.0;
                    total_collected += fee_amount;
                    continue;
                };

                // Try to collect from owner
                let owner = &mut companies[owner_idx];
                let amount_drained = fee_amount.min(owner.liquid_capital);
                owner.liquid_capital -= amount_drained;

                if amount_drained >= fee_amount {
                    // Full payment — credit warehouse owner
                    batch.accumulated_fees = 0.0;
                    if let Some(widx) = warehouse_idx {
                        companies[widx].liquid_capital += amount_drained;
                    } else if warehouse_owner_id == STATE_OWNER_ID {
                        budget.liquid_reserves += amount_drained;
                    }
                    total_collected += amount_drained;
                } else {
                    // Partial payment — seize batch and liquidate
                    let shortfall = fee_amount - amount_drained;
                    owner.liabilities += shortfall;

                    // Liquidation: batch is worth 50% of its market value
                    // (fire-sale). Credit warehouse owner with liquidation
                    // proceeds plus the partial payment.
                    let liquidation_value = batch.quantity * 50.0 * 0.5; // Simplified: 50 currency/ton * 50%
                    let total_to_warehouse = amount_drained + liquidation_value;

                    if let Some(widx) = warehouse_idx {
                        companies[widx].liquid_capital += total_to_warehouse;
                    } else if warehouse_owner_id == STATE_OWNER_ID {
                        budget.liquid_reserves += total_to_warehouse;
                    }
                    total_collected += total_to_warehouse;

                    // Mark batch for removal (seized)
                    to_remove.push(batch_idx);
                }
            }

            // Remove seized batches (reverse order to preserve indices)
            for &idx in to_remove.iter().rev() {
                batches.remove(idx);
            }
        }
    }

    total_collected
}

/// Find company by ID in companies vector (Phase 5.5).
///
/// # Arguments
/// * `id` - Company ID to search for
/// * `companies` - Reference to companies vector
///
/// # Returns
/// * `Some(index)` if company found
/// * `None` if company not found
fn find_company_by_id(id: &str, companies: &[crate::entities::Company]) -> Option<usize> {
    companies.iter().position(|c| c.id == id)
}

/// Process storage transactions from warehouse extraction (Phase 5.5).
///
/// # Arguments
/// * `transactions` - Financial transactions from warehouse extraction
/// * `companies` - Mutable reference to companies vector
/// * `cleared_price` - Market price at which goods were sold
/// * `treasury` - Mutable country budget receiving transport fees
///
/// # Rules
/// * Sale proceeds `(cleared_price * quantity) - transport_cost` are credited to the batch owner.
/// * The deducted `transport_cost` is credited to `treasury.logistics_revenue`, so the fee is
///   transferred to the State rather than destroyed (matches the fire-sale path in `agriculture.rs`).
/// * Storage fees are then settled as a separate transfer from batch owner to warehouse owner.
/// * Double-entry accounting: the warehouse owner is credited **only** the amount actually
///   drained from the batch owner. Cash is never conjured to make the warehouse whole.
/// * Insolvency: the owner's `liquid_capital` is drained to zero and the unpaid remainder is
///   recorded as `liabilities` (bad debt borne by the warehouse owner).
/// * Self-storage check: if owner == warehouse owner, the fee is internal and no money moves.
/// * If the batch owner is not found, the transaction is skipped entirely and no transport
///   fee is levied, because no sale was settled.
/// * This is the ONLY successful sales payment point.
pub fn process_storage_transactions(
    transactions: Vec<crate::economy::clearing::FinancialTransaction>,
    companies: &mut Vec<crate::entities::Company>,
    cleared_price: f64,
    treasury: &mut crate::state::treasury::Treasury,
) {
    for transaction in transactions {
        let total_revenue = cleared_price * transaction.quantity;
        let net_revenue = total_revenue - transaction.transport_cost;

        // Find company indices
        let owner_idx = find_company_by_id(&transaction.batch_owner, companies);
        let logistics_idx = find_company_by_id(&transaction.warehouse_owner, companies);

        // Without a batch owner there is no counterparty to debit; skip.
        // No transport fee is levied because no sale was settled.
        let Some(owner_idx) = owner_idx else {
            continue;
        };

        // The haulier's cut is withheld at source and remitted to the State.
        treasury.logistics_revenue += transaction.transport_cost;

        // Step 1: credit sale proceeds (external market inflow) to the batch owner.
        // A negative net revenue (transport exceeded gross) is absorbed as debt.
        {
            let owner = &mut companies[owner_idx];
            owner.liquid_capital += net_revenue;
            if owner.liquid_capital < 0.0 {
                owner.liabilities += -owner.liquid_capital;
                owner.liquid_capital = 0.0;
            }
        }

        let fee_amount = transaction.accumulated_fees;
        if fee_amount <= 0.0 {
            continue;
        }

        // Step 2: self-storage check - the fee is internal, no money moves.
        if logistics_idx == Some(owner_idx) {
            continue;
        }

        // Step 3: drain from the owner only what can actually be paid.
        let amount_drained = {
            let owner = &mut companies[owner_idx];
            let drained = fee_amount.min(owner.liquid_capital);
            owner.liquid_capital -= drained;

            // Record the unpaid remainder as bad debt.
            if drained < fee_amount {
                owner.liabilities += fee_amount - drained;
            }
            drained
        };

        // Step 4: credit the warehouse owner only the amount actually drained.
        if let Some(logistics_idx) = logistics_idx {
            companies[logistics_idx].liquid_capital += amount_drained;
        }
        // If the warehouse owner is not a tracked company, the drained amount is
        // retired rather than conjured elsewhere (edge case).
    }
}

/// Calculates the Black Ops budget for the current regime.
///
/// # Rules
/// * Democracies siphon 2% of the official defense and security budget.
/// * Autocracies draw from the shadow economy, corruption, and asset confiscation.
/// * If the regime cannot be identified, the authoritarian path is used.
pub fn calculate_black_ops_budget(country: &Country, registries: &Registries) -> f64 {
    if is_democratic(country, registries) {
        let budget = &country.budget.allocations;
        let defense = budget.armed_forces;
        let security = budget
            .extra
            .get("Public Security")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let official = country.budget.nominal_budget;

        (defense + security) * official * 0.02
    } else {
        let gdp = country.budget.gdp;
        let crime = country
            .macro_indicators
            .extra
            .get("crime_rate")
            .and_then(Value::as_object);

        let shadow = crime
            .as_ref()
            .and_then(|m| m.get("szara_strefa_wartosc"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            * 0.05;

        let corruption = crime
            .as_ref()
            .and_then(|m| m.get("korupcja"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let corruption_funding = gdp * (corruption / 100.0) * 0.03;

        let unrest = country.macro_indicators.social_unrest;
        let confiscation = if unrest > 50.0 {
            gdp * (unrest / 100.0) * 0.02
        } else {
            0.0
        };

        shadow + corruption_funding + confiscation
    }
}

/// Returns `true` if the current regime is democratic.
///
/// # Rules
/// * Looks up `politics.system` in `ctx.registries.government_forms`.
/// * Defaults to `false` (autocratic) if either the registry entry or the
///   political data is missing.
fn is_democratic(country: &Country, registries: &Registries) -> bool {
    let regime = country
        .macro_indicators
        .extra
        .get("policy")
        .and_then(Value::as_object)
        .and_then(|m| m.get("system"))
        .and_then(Value::as_str);

    match regime {
        Some(name) => registries
            .government_forms
            .get(name)
            .map(|f| f.regime_type == RegimeType::Democracy)
            .unwrap_or(false),
        None => false,
    }
}

/// Processes Black Ops funding with strict double-entry accounting (Phase 10).
///
/// # Arguments
/// * `country` - Mutable country state.
/// * `registries` - Immutable game registries.
///
/// # Rules
/// * Calculates the Black Ops budget using regime-specific logic.
/// * Caps the allocation at 10% of available liquid_reserves.
/// * Debits `Treasury.liquid_reserves` and credits `intelligence_budget.current_budget`.
/// * Updates `Treasury.black_ops_budget` for fiscal reporting.
pub fn process_black_ops_funding(
    country: &mut Country,
    registries: &Registries,
) {
    let black_ops = calculate_black_ops_budget(country, registries);
    let capped = black_ops.min(country.budget.liquid_reserves.max(0.0) * 0.10);

    country.budget.liquid_reserves -= capped;
    country.budget.black_ops_budget = capped;
    country.intelligence_budget.current_budget = capped;
}

/// Processes maintenance costs for state-owned strategic reserve warehouses (Phase 10).
///
/// # Arguments
/// * `country` - Mutable country state (for budget debit).
/// * `companies` - Mutable companies slice (to find and credit the reserve agency).
///
/// # Rules
/// * Maintenance cost = total_reserve_units * base_price * 0.001 (0.1% per turn).
/// * Debits `Treasury.liquid_reserves`, credits the Strategic Reserve Agency's `liquid_capital`.
/// * If no agency exists or reserves are negative, the cost is capped at available reserves.
pub fn process_state_reserve_maintenance(
    country: &mut Country,
    companies: &mut [Company],
) {
    const MAINTENANCE_RATE: f64 = 0.001;
    const BASE_PRICE: f64 = 100.0;

    for company in companies.iter_mut() {
        if let LegalForm::StrategicReserveAgency(data) = &company.legal_form {
            let total_units: f64 = data.commodity_reserves.values().sum();
            let cost = total_units * BASE_PRICE * MAINTENANCE_RATE;
            let actual_cost = cost.min(country.budget.liquid_reserves.max(0.0));
            country.budget.liquid_reserves -= actual_cost;
            company.liquid_capital += actual_cost;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::clearing::FinancialTransaction;
    use crate::entities::Company;
    use crate::state::treasury::Treasury;

    /// Build a bare company with an explicit `liquid_capital` balance.
    ///
    /// `Company::new` diverts liquid capital into a brokerage account, but the
    /// storage settlement path reads the `liquid_capital` field directly, so the
    /// tests construct the struct literally.
    fn company_with_cash(id: &str, liquid_capital: f64) -> Company {
        Company {
            id: id.to_string(),
            liquid_capital,
            ..Default::default()
        }
    }

    fn transaction(
        batch_owner: &str,
        warehouse_owner: &str,
        quantity: f64,
        accumulated_fees: f64,
        transport_cost: f64,
    ) -> FinancialTransaction {
        FinancialTransaction {
            batch_owner: batch_owner.to_string(),
            warehouse_owner: warehouse_owner.to_string(),
            quantity,
            accumulated_fees,
            transport_cost,
            commodity: Commodity::Cereal,
        }
    }

    /// Total spendable cash held across all companies.
    fn total_cash(companies: &[Company]) -> f64 {
        companies.iter().map(|c| c.liquid_capital).sum()
    }

    /// Total cash in the closed system: corporate balances plus State logistics receipts.
    fn system_cash(companies: &[Company], treasury: &Treasury) -> f64 {
        total_cash(companies) + treasury.logistics_revenue
    }

    #[test]
    fn solvent_owner_pays_full_fee_to_warehouse() {
        let mut companies = vec![
            company_with_cash("FARM", 1_000.0),
            company_with_cash("WAREHOUSE", 500.0),
        ];

        // 10 tons at 100.0 = 1000 gross, minus 50 transport = 950 net revenue.
        let mut treasury = Treasury::default();
        let txs = vec![transaction("FARM", "WAREHOUSE", 10.0, 200.0, 50.0)];
        process_storage_transactions(txs, &mut companies, 100.0, &mut treasury);

        // Farmer: 1000 + 950 - 200 = 1750
        assert_eq!(companies[0].liquid_capital, 1_750.0);
        assert_eq!(companies[0].liabilities, 0.0);
        // Warehouse receives the full fee.
        assert_eq!(companies[1].liquid_capital, 700.0);
        // The State collects the transport fee.
        assert_eq!(treasury.logistics_revenue, 50.0);
    }

    #[test]
    fn insolvent_owner_credits_warehouse_only_what_was_drained() {
        // Farmer has no cash and the sale barely covers transport, so the
        // storage fee cannot be paid in full.
        let mut companies = vec![
            company_with_cash("FARM", 0.0),
            company_with_cash("WAREHOUSE", 0.0),
        ];

        // 1 ton at 10.0 = 10 gross, minus 0 transport = 10 net revenue.
        // Fee owed is 100.0, but only 10.0 is actually available.
        let mut treasury = Treasury::default();
        let txs = vec![transaction("FARM", "WAREHOUSE", 1.0, 100.0, 0.0)];
        process_storage_transactions(txs, &mut companies, 10.0, &mut treasury);

        // Farmer drained to zero, 90.0 unpaid remainder booked as bad debt.
        assert_eq!(companies[0].liquid_capital, 0.0);
        assert_eq!(companies[0].liabilities, 90.0);

        // REGRESSION: the warehouse must receive only the 10.0 actually drained,
        // NOT the full 100.0 fee. Crediting 100.0 here conjured 90.0 of fiat.
        assert_eq!(companies[1].liquid_capital, 10.0);
        assert_eq!(companies[1].liabilities, 0.0);
    }

    #[test]
    fn insolvent_settlement_conserves_cash() {
        let mut companies = vec![
            company_with_cash("FARM", 25.0),
            company_with_cash("WAREHOUSE", 0.0),
        ];

        let mut treasury = Treasury::default();
        let before = system_cash(&companies, &treasury);

        // 2 tons at 5.0 = 10 gross, no transport. Fee owed 500.0.
        let txs = vec![transaction("FARM", "WAREHOUSE", 2.0, 500.0, 0.0)];
        process_storage_transactions(txs, &mut companies, 5.0, &mut treasury);

        // The fee settlement is a pure transfer, so the only change to system
        // cash is the 10.0 of external sale revenue. Under the old buggy code
        // the warehouse was credited the full 500.0, inflating this by 465.0.
        let expected = before + 10.0;
        let after = system_cash(&companies, &treasury);

        assert!(
            (after - expected).abs() < 1e-9,
            "cash not conserved: before={before}, after={after}, expected={expected}"
        );

        // The farmer had 25.0 + 10.0 = 35.0 available against a 500.0 fee.
        assert_eq!(companies[0].liquid_capital, 0.0);
        assert_eq!(companies[0].liabilities, 465.0);
        assert_eq!(companies[1].liquid_capital, 35.0);
    }

    #[test]
    fn self_storage_moves_no_fee() {
        let mut companies = vec![company_with_cash("AGRO", 1_000.0)];

        // Owner stores in its own warehouse: the fee is internal.
        let mut treasury = Treasury::default();
        let txs = vec![transaction("AGRO", "AGRO", 10.0, 300.0, 100.0)];
        process_storage_transactions(txs, &mut companies, 50.0, &mut treasury);

        // 1000 + (500 gross - 100 transport) = 1400, fee never leaves the firm.
        assert_eq!(companies[0].liquid_capital, 1_400.0);
        assert_eq!(companies[0].liabilities, 0.0);
        // Transport is still a real haulage cost even for self-storage.
        assert_eq!(treasury.logistics_revenue, 100.0);
    }

    #[test]
    fn transport_exceeding_revenue_becomes_debt_not_negative_cash() {
        let mut companies = vec![
            company_with_cash("FARM", 0.0),
            company_with_cash("WAREHOUSE", 0.0),
        ];

        // 1 ton at 10.0 = 10 gross, minus 60 transport = -50 net revenue.
        let mut treasury = Treasury::default();
        let txs = vec![transaction("FARM", "WAREHOUSE", 1.0, 0.0, 60.0)];
        process_storage_transactions(txs, &mut companies, 10.0, &mut treasury);

        assert_eq!(companies[0].liquid_capital, 0.0);
        assert_eq!(companies[0].liabilities, 50.0);
        assert_eq!(treasury.logistics_revenue, 60.0);
    }

    #[test]
    fn missing_batch_owner_skips_transaction() {
        let mut companies = vec![company_with_cash("WAREHOUSE", 400.0)];

        let mut treasury = Treasury::default();
        let txs = vec![transaction("GHOST_FARM", "WAREHOUSE", 10.0, 200.0, 25.0)];
        process_storage_transactions(txs, &mut companies, 100.0, &mut treasury);

        // No counterparty to debit, so the warehouse must not be paid.
        assert_eq!(companies[0].liquid_capital, 400.0);
        // No sale settled, so no haulage fee is levied.
        assert_eq!(treasury.logistics_revenue, 0.0);
    }

    #[test]
    fn transport_cost_is_transferred_not_destroyed() {
        let mut companies = vec![
            company_with_cash("FARM", 0.0),
            company_with_cash("WAREHOUSE", 0.0),
        ];
        let mut treasury = Treasury::default();

        let before = system_cash(&companies, &treasury);

        // 10 tons at 100.0 = 1000 gross, 250 transport, no storage fee.
        let txs = vec![transaction("FARM", "WAREHOUSE", 10.0, 0.0, 250.0)];
        process_storage_transactions(txs, &mut companies, 100.0, &mut treasury);

        // Farmer keeps 750, the State collects the 250 haulage fee.
        assert_eq!(companies[0].liquid_capital, 750.0);
        assert_eq!(treasury.logistics_revenue, 250.0);

        // REGRESSION: system cash must rise by the FULL 1000.0 gross. The old
        // code silently destroyed the 250.0 transport fee.
        let after = system_cash(&companies, &treasury);
        assert!(
            (after - (before + 1_000.0)).abs() < 1e-9,
            "transport fee leaked: before={before}, after={after}"
        );
    }

    #[test]
    fn combined_transport_and_insolvent_fee_conserves_cash() {
        let mut companies = vec![
            company_with_cash("FARM", 40.0),
            company_with_cash("WAREHOUSE", 15.0),
        ];
        let mut treasury = Treasury::default();

        let before = system_cash(&companies, &treasury);

        // 4 tons at 30.0 = 120 gross, 45 transport, 900 storage fee owed.
        let txs = vec![transaction("FARM", "WAREHOUSE", 4.0, 900.0, 45.0)];
        process_storage_transactions(txs, &mut companies, 30.0, &mut treasury);

        // Farmer: 40 + (120 - 45) = 115 available against a 900 fee.
        assert_eq!(companies[0].liquid_capital, 0.0);
        assert_eq!(companies[0].liabilities, 785.0);
        assert_eq!(companies[1].liquid_capital, 130.0);
        assert_eq!(treasury.logistics_revenue, 45.0);

        // Every unit of the 120.0 gross is accounted for somewhere.
        let after = system_cash(&companies, &treasury);
        assert!(
            (after - (before + 120.0)).abs() < 1e-9,
            "cash not conserved: before={before}, after={after}"
        );
    }

    // Phase 33: Hysteresis tests for emergency powers.

    fn country_with_deficit(gdp: f64, reserves: f64) -> crate::state::Country {
        let mut country = crate::state::Country::default();
        country.budget.gdp = gdp;
        country.budget.liquid_reserves = reserves;
        country.emergency_powers = crate::state::EmergencyPowers::Normal;
        country.emergency_escalation_counter = 0;
        country.emergency_deescalation_counter = 0;
        country
    }

    #[test]
    fn test_emergency_hysteresis_no_flicker_on_single_turn() {
        // A single turn of deficit should NOT escalate from Normal to ExciseTaxes.
        let mut country = country_with_deficit(1000.0, -250.0); // -25% → ExciseTaxes desired
        let surplus = HashMap::default();
        check_emergency_conditions(&mut country, &surplus);
        // First turn: hysteresis prevents escalation.
        assert_eq!(country.emergency_powers, crate::state::EmergencyPowers::Normal);
        // Second turn: escalation allowed.
        check_emergency_conditions(&mut country, &surplus);
        assert_eq!(country.emergency_powers, crate::state::EmergencyPowers::ExciseTaxesEnabled);
    }

    #[test]
    fn test_emergency_hysteresis_deescalation_requires_3_turns() {
        // Escalate to MartialLaw, then recover — should take 3 turns to de-escalate.
        let mut country = country_with_deficit(1000.0, -900.0); // -90% → MartialLaw desired
        let crisis_surplus = HashMap::default();
        // Escalate over 2 turns.
        check_emergency_conditions(&mut country, &crisis_surplus);
        check_emergency_conditions(&mut country, &crisis_surplus);
        assert_eq!(country.emergency_powers, crate::state::EmergencyPowers::MartialLaw);
        // Now recover.
        country.budget.liquid_reserves = 0.0; // 0% → Normal desired
        let ok_surplus = HashMap::default();
        check_emergency_conditions(&mut country, &ok_surplus);
        assert_eq!(country.emergency_powers, crate::state::EmergencyPowers::MartialLaw); // Still ML
        check_emergency_conditions(&mut country, &ok_surplus);
        assert_eq!(country.emergency_powers, crate::state::EmergencyPowers::MartialLaw); // Still ML
        check_emergency_conditions(&mut country, &ok_surplus);
        assert_eq!(country.emergency_powers, crate::state::EmergencyPowers::Normal); // Now de-escalated
    }

    #[test]
    fn test_emergency_hysteresis_recovery_resets_escalation_counter() {
        let mut country = country_with_deficit(1000.0, -250.0);
        let crisis_surplus = HashMap::default();
        // One turn of crisis — escalation counter = 1.
        check_emergency_conditions(&mut country, &crisis_surplus);
        assert_eq!(country.emergency_escalation_counter, 1);
        // Recovery — counters reset.
        country.budget.liquid_reserves = 500.0;
        let ok_surplus = HashMap::default();
        check_emergency_conditions(&mut country, &ok_surplus);
        assert_eq!(country.emergency_escalation_counter, 0);
        // Another single crisis turn should NOT escalate (counter back to 1).
        country.budget.liquid_reserves = -250.0;
        check_emergency_conditions(&mut country, &crisis_surplus);
        assert_eq!(country.emergency_powers, crate::state::EmergencyPowers::Normal);
    }
}

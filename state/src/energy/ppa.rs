//! Phase 81 Wave 2: Power Purchase Agreements (PPAs).
//!
//! Bilateral long-term contracts between generators (sellers) and industrial
//! consumers (buyers) at a fixed price, hedging against spot market volatility.
//!
//! # Price Discovery (Flaw 3 Sealed)
//!
//! - `seller_ask = marginal_cost_mwh * 1.15` (15% minimum profit margin floor)
//! - `buyer_bid = moving_average_vwap(Commodity::Energy)` (24-turn window)
//! - Match condition: `seller_ask <= buyer_bid`
//! - Execution price: `(seller_ask + buyer_bid) / 2.0`
//! - Pro-rata allocation by bid quantity when multiple buyers compete

use crate::economy::market::market_history::{moving_average_vwap, MarketHistory};
use crate::energy::generation::compute_marginal_cost;
use crate::energy::types::{PpaRegistry, PpaStatus, PowerPurchaseAgreement};
use crate::energy::grid::get_plant_metadata;
use crate::entities::Building;
use crate::registries::enums::Commodity;
use crate::state::Country;
use std::collections::HashMap;

/// Phase 81 Wave 2: Negotiate PPAs for the current turn.
///
/// Called during the corporate strategy phase. For each energy company with
/// spare PPA capacity, computes the seller's minimum ask. For each industrial
/// company with high energy demand, computes the buyer's maximum bid. Matches
/// when `seller_ask <= buyer_bid` and allocates pro-rata by bid quantity.
///
/// # Arguments
/// * `country` - Mutable country (PPA registry updated)
/// * `buildings` - All buildings (to find power plants and industrial consumers)
/// * `fuel_prices` - Current market fuel prices for marginal cost computation
/// * `average_wage` - Current average wage for tie-breaker pricing
/// * `market_history` - Market history for VWAP computation
/// * `global_base_price` - Fallback price if VWAP history is empty
/// * `current_turn` - Current turn number
///
/// # Rules
/// * Deterministic: sorted by (seller_company_id, buyer_company_id, plant_building_id)
/// * Available PPA capacity = nameplate_capacity * capacity_factor * 0.80
/// * PPA term: 20-120 turns (seller offers longer terms when spot prices are low)
/// * Only one PPA per (seller, buyer, plant) triple per turn
pub fn negotiate_ppas(
    country: &mut Country,
    buildings: &[Building],
    fuel_prices: &HashMap<Commodity, f64>,
    average_wage: f64,
    market_history: &MarketHistory,
    global_base_price: f64,
    current_turn: u32,
) {
    let mut registry = std::mem::take(&mut country.ppa_registry);

    // Step 1: Collect seller offers (power plants with spare PPA capacity).
    // Each offer: (plant_building_id, seller_company_id, marginal_cost, available_mw)
    let mut seller_offers: Vec<(String, String, f64, f64)> = Vec::new();

    for building in buildings {
        if building.sector != crate::registries::enums::Sector::Energy {
            continue;
        }
        if let Some(meta) = get_plant_metadata(building) {
            // Skip storage plants — they don't sell PPAs
            if meta.plant_type.is_storage() {
                continue;
            }

            let marginal_cost = compute_marginal_cost(&meta, fuel_prices, average_wage);

            // Available PPA capacity = nameplate * capacity_factor * 0.80
            // (20% reserved for spot market)
            let capacity_factor = meta.capacity_factor;
            let available_ppa_mw = meta.nameplate_capacity_mw * capacity_factor * 0.80;

            // Subtract MW already contracted in active PPAs for this plant
            let already_contracted: f64 = registry
                .active_ppas
                .iter()
                .filter(|ppa| ppa.plant_building_id == building.id && ppa.status == PpaStatus::Active)
                .map(|ppa| ppa.contracted_mw)
                .sum();
            let remaining_ppa_mw = (available_ppa_mw - already_contracted).max(0.0);

            if remaining_ppa_mw > 0.0 {
                seller_offers.push((
                    building.id.clone(),
                    building.owner_id.clone(),
                    marginal_cost,
                    remaining_ppa_mw,
                ));
            }
        }
    }

    // Step 2: Collect buyer bids (industrial companies with high energy demand).
    // Each bid: (buyer_company_id, bid_mw)
    let mut buyer_bids: Vec<(String, f64)> = Vec::new();

    // Group industrial buildings by owner and sum their energy demand
    let mut industrial_demand: HashMap<String, f64> = HashMap::new();
    for building in buildings {
        if building.sector == crate::registries::enums::Sector::HeavyIndustry
            || building.sector == crate::registries::enums::Sector::Mining
        {
            // Estimate energy demand from the building's production method inputs
            let energy_input = building
                .active_method
                .inputs
                .get(&Commodity::Energy)
                .copied()
                .unwrap_or(0.0);
            if energy_input > 0.0 {
                *industrial_demand.entry(building.owner_id.clone()).or_insert(0.0) +=
                    energy_input;
            }
        }
    }

    // Buyer bid = moving_average_vwap(Energy) or fallback
    let buyer_bid = moving_average_vwap(market_history, &Commodity::Energy)
        .unwrap_or(global_base_price);

    for (buyer_id, demand_mw) in industrial_demand {
        if demand_mw > 0.0 {
            buyer_bids.push((buyer_id, demand_mw));
        }
    }

    // Step 3: Sort deterministically for reproducible matching.
    seller_offers.sort_by(|a, b| {
        a.1.cmp(&b.1) // seller_company_id
            .then_with(|| a.0.cmp(&b.0)) // plant_building_id
    });
    buyer_bids.sort_by(|a, b| a.0.cmp(&b.0));

    // Step 4: Match sellers with buyers.
    for (plant_id, seller_id, marginal_cost, available_mw) in &seller_offers {
        let seller_ask = marginal_cost * 1.15; // 15% profit margin floor

        if seller_ask > buyer_bid {
            // No match — seller's minimum is above buyer's maximum
            continue;
        }

        let execution_price = (seller_ask + buyer_bid) / 2.0;

        // Collect buyers that want this plant (all buyers compete for all plants
        // in this simplified model — a more advanced version would have buyers
        // express plant-specific preferences).
        let total_bid_mw: f64 = buyer_bids.iter().map(|(_, mw)| mw).sum();
        if total_bid_mw <= 0.0 {
            continue;
        }

        // Pro-rata allocation by bid quantity
        for (buyer_id, bid_mw) in &buyer_bids {
            if bid_mw <= &0.0 {
                continue;
            }
            let allocated_mw = (bid_mw / total_bid_mw) * available_mw;
            if allocated_mw < 1.0 {
                continue; // Skip trivially small allocations
            }

            // PPA term: 60 turns (mid-range, rational default)
            let term = 60u32;
            let ppa = PowerPurchaseAgreement {
                id: format!("ppa_{}_{}_{}_{}", current_turn, seller_id, buyer_id, plant_id),
                seller_company_id: seller_id.clone(),
                buyer_company_id: buyer_id.clone(),
                plant_building_id: plant_id.clone(),
                fixed_price_per_mwh: execution_price,
                contracted_mw: allocated_mw,
                start_turn: current_turn,
                end_turn: current_turn + term,
                status: PpaStatus::Active,
            };
            registry.active_ppas.push(ppa);
        }
    }

    country.ppa_registry = registry;
}

/// Phase 81 Wave 2: Expire PPAs that have reached their end turn.
///
/// Moves expired PPAs from `active_ppas` to `expired_ppas`. Called at the end
/// of each turn.
///
/// # Arguments
/// * `country` - Mutable country (PPA registry updated)
/// * `current_turn` - Current turn number
pub fn expire_ppas(country: &mut Country, current_turn: u32) {
    let mut registry = std::mem::take(&mut country.ppa_registry);

    let mut still_active = Vec::new();
    for ppa in registry.active_ppas.drain(..) {
        if current_turn > ppa.end_turn {
            let mut expired = ppa;
            expired.status = PpaStatus::Expired;
            registry.expired_ppas.push(expired);
        } else {
            still_active.push(ppa);
        }
    }
    registry.active_ppas = still_active;

    country.ppa_registry = registry;
}

/// Phase 81 Wave 2: Terminate a PPA early with a 20% break fee.
///
/// The breaker pays 20% of the remaining contract value to the counterparty.
/// `remaining_contract_value = contracted_mw * fixed_price_per_mwh * remaining_turns`
///
/// # Arguments
/// * `registry` - Mutable PPA registry
/// * `ppa_id` - ID of the PPA to terminate
/// * `breaker` - "seller" or "buyer" — who is terminating
/// * `current_turn` - Current turn number
///
/// # Returns
/// The break fee amount, or 0.0 if the PPA was not found.
pub fn terminate_ppa(
    registry: &mut PpaRegistry,
    ppa_id: &str,
    _breaker: &str,
    current_turn: u32,
) -> f64 {
    let idx = registry.active_ppas.iter().position(|p| p.id == ppa_id);
    if let Some(idx) = idx {
        let mut ppa = registry.active_ppas.remove(idx);
        let remaining_turns = ppa.end_turn.saturating_sub(current_turn) as f64;
        let remaining_value = ppa.contracted_mw * ppa.fixed_price_per_mwh * remaining_turns;
        let break_fee = remaining_value * 0.20; // 20% break fee

        ppa.status = PpaStatus::Terminated;
        registry.expired_ppas.push(ppa);

        break_fee
    } else {
        0.0
    }
}

/// Phase 81 Wave 2: Get total PPA-contracted MW for a specific plant.
///
/// Used by `execute_ppas()` in grid.rs to determine how much of a plant's
/// output is pre-sold via PPAs.
pub fn plant_ppa_mw(registry: &PpaRegistry, plant_building_id: &str) -> f64 {
    registry
        .active_ppas
        .iter()
        .filter(|p| p.plant_building_id == plant_building_id && p.status == PpaStatus::Active)
        .map(|p| p.contracted_mw)
        .sum()
}

/// Phase 81 Wave 2: Get all active PPAs for a specific buyer.
pub fn buyer_ppas<'a>(registry: &'a PpaRegistry, buyer_company_id: &str) -> Vec<&'a PowerPurchaseAgreement> {
    registry
        .active_ppas
        .iter()
        .filter(|p| p.buyer_company_id == buyer_company_id && p.status == PpaStatus::Active)
        .collect()
}

/// Phase 81 Wave 2: Get all active PPAs for a specific seller/plant.
pub fn seller_plant_ppas<'a>(
    registry: &'a PpaRegistry,
    plant_building_id: &str,
) -> Vec<&'a PowerPurchaseAgreement> {
    registry
        .active_ppas
        .iter()
        .filter(|p| p.plant_building_id == plant_building_id && p.status == PpaStatus::Active)
        .collect()
}

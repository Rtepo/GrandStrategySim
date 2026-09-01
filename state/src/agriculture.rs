//! Agricultural simulation logic for Phase 6.3 Agriculture 2.0
//!
//! Implements the dynamic state machine for crop lifecycle, FTE demand calculation,
//! and yield/rot mechanics integrated with the 24-tick calendar.

use crate::entities::{Building, Company, CropBatch, CropState};
use crate::registries::crops::{CropDefinition, LandType};
use crate::registries::enums::Commodity;
use crate::registries::Registries;
use crate::society::geography::Region;
use crate::society::housing::CommercialBuilding;
use crate::state::climate::ClimateConfig;
use crate::state::treasury::Treasury;
use crate::state::Calendar;
const STATE_OWNER_ID: &str = "STATE";
use std::collections::BTreeMap;

/// Land reclamation data for despawned agricultural companies.
///
/// # Rules
/// * Tracks hectares reclaimed from company ownership to State ownership.
/// * Keyed by region ID for deterministic application.
/// * Used to prevent land loss during corporate liquidation.
#[derive(Debug, Clone, Default)]
pub struct LandReclamationData {
    /// Region ID where land is reclaimed.
    pub region_id: String,
    /// Total hectares reclaimed (rounded to i64).
    pub total_hectares_reclaimed: i64,
    /// Per-soil-class redistribution: soil class -> hectares transferred to State.
    pub soil_class_transfers: BTreeMap<String, i64>,
}

/// Transition agricultural crop states based on calendar
///
/// # Arguments
/// * `company` - Mutable reference to the company
/// * `calendar` - Current calendar state
/// * `registries` - Crop registry
/// * `buildings` - Mutable slice of production buildings (for seed inventory withdrawal)
///
/// # Rules
/// * Crops in receivership continue natural cycle but cannot sow new seeds
/// * Arable crops reset active_hectares after harvest
/// * Plantation crops preserve active_hectares across cycles
/// * Phase 46: Seeds are physical commodities withdrawn from `building.inventory`
pub fn transition_agricultural_states(
    company: &mut Company,
    calendar: &Calendar,
    registries: &Registries,
    buildings: &mut [Building],
) {
    let Some(agri_profile) = &mut company.agricultural_profile else {
        return;
    };

    if company.sector != crate::registries::enums::Sector::Agriculture {
        return;
    }

    let current_turn = calendar.global_turn % 24;
    if current_turn == 0 {
        return; // Invalid turn
    }

    let company_id = company.id.clone();
    let is_in_receivership = company.is_in_receivership;

    for batch in &mut agri_profile.batches {
        let Some(crop_def) = registries.crops.get(&batch.crop_id) else {
            continue;
        };

        match crop_def.land_type {
            LandType::Arable => {
                transition_arable_crop(
                    batch,
                    crop_def,
                    current_turn,
                    is_in_receivership,
                    &company_id,
                    buildings,
                );
            }
            LandType::Plantation => {
                transition_plantation_crop(batch, crop_def, current_turn, is_in_receivership);
            }
        }
    }
}

/// Transition arable crop (annual, requires sowing)
///
/// Phase 46: Seeds are physical commodities withdrawn from `building.inventory`.
/// No cash flow, no Treasury credit. If insufficient seeds in storage, sown
/// hectares are reduced proportionally. The farm must procure seeds via the
/// existing B2B market (production method BOM already includes `Commodity::Seeds`).
fn transition_arable_crop(
    batch: &mut CropBatch,
    crop_def: &CropDefinition,
    current_turn: u32,
    is_in_receivership: bool,
    company_id: &str,
    buildings: &mut [Building],
) {
    match batch.state {
        CropState::Idle => {
            // Idle -> Sowing: Within sowing window, not in receivership
            if current_turn >= crop_def.sowing_schedule.start_turn
                && current_turn <= crop_def.sowing_schedule.end_turn
                && !is_in_receivership
            {
                let seed_needed = batch.planned_hectares * crop_def.seed_quantity_per_hectare;

                // Withdraw physical seeds from production buildings' inventory
                let mut seeds_remaining = seed_needed;
                for building in buildings.iter_mut() {
                    if building.owner_id != company_id {
                        continue;
                    }
                    if seeds_remaining <= 0.0 {
                        break;
                    }
                    if let Some(stored) = building.inventory.get_mut(&crop_def.seed_commodity) {
                        let taken = seeds_remaining.min(*stored);
                        *stored -= taken;
                        seeds_remaining -= taken;
                    }
                }
                let total_seeds_withdrawn = seed_needed - seeds_remaining;

                // Calculate actual sown hectares based on available seeds
                let actual_sown_hectares = if crop_def.seed_quantity_per_hectare > 0.0 {
                    total_seeds_withdrawn / crop_def.seed_quantity_per_hectare
                } else {
                    batch.planned_hectares
                };

                if actual_sown_hectares > 0.0 {
                    batch.active_hectares = actual_sown_hectares;
                    batch.state = CropState::Sowing;
                    batch.planted_turn = current_turn;
                }
                // If no seeds available, skip sowing this turn.
                // The farm must wait for B2B seed delivery to its building.inventory.
            }
        }
        CropState::Sowing => {
            // Sowing -> Growing: After sowing window
            if current_turn > crop_def.sowing_schedule.end_turn {
                batch.state = CropState::Growing;
            }
        }
        CropState::Growing => {
            // Growing -> Harvesting: Within harvest window
            if current_turn >= crop_def.harvest_schedule.start_turn
                && current_turn <= crop_def.harvest_schedule.end_turn
            {
                batch.state = CropState::Harvesting;
            }
        }
        CropState::Harvesting => {
            // Harvesting -> Idle: After harvest window
            if current_turn > crop_def.harvest_schedule.end_turn {
                // Solvent companies reset accumulators (yield already deposited to warehouses)
                if !is_in_receivership {
                    batch.accumulated_yield = 0.0;
                    batch.rot_accumulator = 0.0;
                }
                // Bankrupt companies preserve accumulators for liquidation
                batch.active_hectares = 0.0; // Arable must re-sow
                batch.state = CropState::Idle;
            }
        }
    }
}

/// Transition plantation crop (perennial, skips sowing)
fn transition_plantation_crop(
    batch: &mut CropBatch,
    crop_def: &CropDefinition,
    current_turn: u32,
    is_in_receivership: bool,
) {
    match batch.state {
        CropState::Idle => {
            // Idle -> Growing: Within sowing window (plantations skip sowing)
            if current_turn >= crop_def.sowing_schedule.start_turn
                && current_turn <= crop_def.sowing_schedule.end_turn
            {
                batch.state = CropState::Growing;
                batch.planted_turn = current_turn;
            }
        }
        CropState::Sowing => {
            // Plantations should never be in Sowing state
            batch.state = CropState::Growing;
        }
        CropState::Growing => {
            // Growing -> Harvesting: Within harvest window
            if current_turn >= crop_def.harvest_schedule.start_turn
                && current_turn <= crop_def.harvest_schedule.end_turn
            {
                batch.state = CropState::Harvesting;
            }
        }
        CropState::Harvesting => {
            // Harvesting -> Idle: After harvest window
            if current_turn > crop_def.harvest_schedule.end_turn {
                // Solvent companies reset accumulators (yield already deposited to warehouses)
                if !is_in_receivership {
                    batch.accumulated_yield = 0.0;
                    batch.rot_accumulator = 0.0;
                }
                // Bankrupt companies preserve accumulators for liquidation
                // Plantations preserve active_hectares (perennial)
                batch.state = CropState::Idle;
            }
        }
    }
}

/// AI & Stability Audit (Pillar 3): Predict the crop state for the next turn.
///
/// Applies the same transition logic as `transition_arable_crop` and
/// `transition_plantation_crop` but without mutating the batch. Used for
/// anticipatory labor ramp-up.
fn predict_next_crop_state(
    batch: &CropBatch,
    crop_def: &CropDefinition,
    next_turn: u32,
) -> CropState {
    match crop_def.land_type {
        LandType::Arable => predict_next_arable_state(batch, crop_def, next_turn),
        LandType::Plantation => predict_next_plantation_state(batch, crop_def, next_turn),
    }
}

fn predict_next_arable_state(
    batch: &CropBatch,
    crop_def: &CropDefinition,
    next_turn: u32,
) -> CropState {
    match batch.state {
        CropState::Idle => {
            if next_turn >= crop_def.sowing_schedule.start_turn
                && next_turn <= crop_def.sowing_schedule.end_turn
            {
                CropState::Sowing
            } else {
                CropState::Idle
            }
        }
        CropState::Sowing => {
            if next_turn > crop_def.sowing_schedule.end_turn {
                CropState::Growing
            } else {
                CropState::Sowing
            }
        }
        CropState::Growing => {
            if next_turn >= crop_def.harvest_schedule.start_turn
                && next_turn <= crop_def.harvest_schedule.end_turn
            {
                CropState::Harvesting
            } else {
                CropState::Growing
            }
        }
        CropState::Harvesting => {
            if next_turn > crop_def.harvest_schedule.end_turn {
                CropState::Idle
            } else {
                CropState::Harvesting
            }
        }
    }
}

fn predict_next_plantation_state(
    batch: &CropBatch,
    crop_def: &CropDefinition,
    next_turn: u32,
) -> CropState {
    match batch.state {
        CropState::Idle => {
            if next_turn >= crop_def.sowing_schedule.start_turn
                && next_turn <= crop_def.sowing_schedule.end_turn
            {
                CropState::Growing
            } else {
                CropState::Idle
            }
        }
        CropState::Sowing => CropState::Growing,
        CropState::Growing => {
            if next_turn >= crop_def.harvest_schedule.start_turn
                && next_turn <= crop_def.harvest_schedule.end_turn
            {
                CropState::Harvesting
            } else {
                CropState::Growing
            }
        }
        CropState::Harvesting => {
            if next_turn > crop_def.harvest_schedule.end_turn {
                CropState::Idle
            } else {
                CropState::Harvesting
            }
        }
    }
}

/// Calculate agricultural FTE demand (physical and target)
///
/// # Arguments
/// * `company` - Mutable reference to the company
/// * `registries` - Crop registry
///
/// # Rules
/// * Physical demand is raw requirement before liquidity clamping
/// * Target demand is liquidity-clamped (except for receivership with active crops)
/// * Receivership companies bid for all active states (Sowing, Growing, Harvesting)
pub fn calculate_agricultural_fte_demand(
    company: &mut Company,
    calendar: &Calendar,
    registries: &Registries,
) {
    let Some(agri_profile) = &company.agricultural_profile else {
        return;
    };

    // Receivership check: Active Liquidator
    let is_treasury_funded = if company.is_in_receivership {
        // Check if any batch is in active state
        let has_active = agri_profile.batches.iter().any(|b| {
            matches!(
                b.state,
                CropState::Sowing | CropState::Growing | CropState::Harvesting
            )
        });

        if !has_active {
            // No active crops, no funding needed
            company.physical_fte_demand = 0;
            company.target_fte_demand = 0;
            company.offered_wage_per_fte = 0.0;
            return;
        }

        true // Treasury funds maintenance of existing assets
    } else {
        false
    };

    // Calculate physical demand
    let mut physical_demand = 0.0;
    for batch in &agri_profile.batches {
        let Some(crop_def) = registries.crops.get(&batch.crop_id) else {
            continue;
        };

        let labor_fte = match batch.state {
            CropState::Sowing => {
                crop_def.labor_demand.sowing_fte_per_hectare * batch.active_hectares
            }
            CropState::Growing => {
                crop_def.labor_demand.growing_fte_per_hectare * batch.active_hectares
            }
            CropState::Harvesting => {
                crop_def.labor_demand.harvesting_fte_per_hectare * batch.active_hectares
            }
            CropState::Idle => 0.0,
        };

        physical_demand += labor_fte;

        // AI & Stability Audit (Pillar 3): Anticipatory labor ramp-up.
        // Predict next turn's crop state and pre-ramp FTE demand to 50% of
        // the next turn's requirement. This ensures workers are hired BEFORE
        // the seasonal phase begins, preventing 1-turn labor demand spikes
        // that the labor market cannot satisfy.
        //
        // Only ramp up if next turn's demand is HIGHER than current (e.g.,
        // Growing → Harvesting transition). No ramp-down needed (furlough
        // handles that).
        let next_turn = (calendar.global_turn + 1) % 24;
        if next_turn == 0 {
            continue; // Invalid turn
        }
        let next_state = predict_next_crop_state(batch, crop_def, next_turn);
        let next_labor_fte = match next_state {
            CropState::Sowing => {
                crop_def.labor_demand.sowing_fte_per_hectare
                    * batch.active_hectares.max(batch.planned_hectares)
            }
            CropState::Growing => {
                crop_def.labor_demand.growing_fte_per_hectare * batch.active_hectares
            }
            CropState::Harvesting => {
                crop_def.labor_demand.harvesting_fte_per_hectare * batch.active_hectares
            }
            CropState::Idle => 0.0,
        };
        // Ramp factor: 50% of next turn's demand this turn
        const RAMP_FACTOR: f64 = 0.5;
        let anticipatory_demand = next_labor_fte * RAMP_FACTOR;
        physical_demand = physical_demand.max(anticipatory_demand);
    }

    company.physical_fte_demand = physical_demand.round() as u32;

    // Calculate target demand (liquidity-clamped)
    if is_treasury_funded {
        company.target_fte_demand = physical_demand.round() as u32; // Treasury covers wage bill
    } else {
        // Phase 25: Do NOT clamp target_fte_demand by liquidity here.
        // The labor market clearing does its own affordability check
        // (brokerage_account.cash / offered_wage_per_fte). Setting
        // target_fte_demand to physical_demand allows the company to
        // bid for its full labor need; the clearing will clamp it.
        company.target_fte_demand = physical_demand.round() as u32;
    }

    // Phase 25: Do NOT set offered_wage_per_fte here. The corporate
    // wage-setting pass (set_wage_offers) handles this AFTER this function
    // runs, using the updated target_fte_demand and actual post-B2B cash.
}

/// Calculate harvest yield and rot accumulation (Phase 6.3.5)
///
/// # Arguments
/// * `company` - Mutable reference to the company
/// * `calendar` - Current calendar state
/// * `registries` - Crop registry
/// * `climate_config` - Climate configuration
/// * `region` - Region for climate profile
/// * `country_budget` - Mutable reference to country budget for liquidation
/// * `commercial_buildings` - Mutable slice of commercial buildings for storage
/// * `current_turn` - Current turn number for batch tracking
///
/// # Rules
/// * Rot accumulates in ALL active states (Sowing, Growing, Harvesting)
/// * Yield is calculated ONLY in Harvesting state
/// * Rot penalty is 10% per unstaffed turn
/// * Yield is distributed across harvest duration (calendar wrap-around safe)
/// * Multi-yield crops produce multiple commodities (e.g., corn grain + stalks)
/// * Yield is routed to company-owned warehouses using deposit_inventory
/// * Phase 46: Excess beyond warehouse capacity rots in the field (no fire sale)
pub fn calculate_harvest_yield_and_rot(
    company: &mut Company,
    calendar: &Calendar,
    registries: &Registries,
    climate_config: &ClimateConfig,
    region: &Region,
    _country_budget: &mut Treasury,
    commercial_buildings: &mut [CommercialBuilding],
    current_turn: u32,
) {
    let Some(agri_profile) = &mut company.agricultural_profile else {
        return;
    };

    if company.sector != crate::registries::enums::Sector::Agriculture {
        return;
    }

    let season = calendar.get_season();
    let modifiers = climate_config.get_modifiers(region.climate_profile, season);

    for batch in &mut agri_profile.batches {
        let Some(crop_def) = registries.crops.get(&batch.crop_id) else {
            continue;
        };

        // Skip idle batches
        if batch.state == CropState::Idle {
            continue;
        }

        // Calculate labor efficiency for ALL active states
        let labor_efficiency = if company.physical_fte_demand > 0 {
            (company.fulfilled_fte as f64 / company.physical_fte_demand as f64).min(1.0)
        } else {
            1.0
        };

        // Lazy Farmer penalty: 10% damage per unstaffed turn
        let neglect_penalty = (1.0 - labor_efficiency) * 0.1;
        batch.rot_accumulator = (batch.rot_accumulator + neglect_penalty).min(1.0);

        // Calculate yield ONLY in Harvesting state
        if batch.state == CropState::Harvesting {
            // Calendar wrap-around safe duration calculation
            let mut duration = crop_def.harvest_schedule.end_turn as i32
                - crop_def.harvest_schedule.start_turn as i32;
            if duration < 0 {
                duration += 24;
            }
            let harvest_duration_turns = (duration + 1) as f64;

            // World Generation & Climate Audit (v0.5.3):
            // Calculate the total yield-per-hectare across all commodities
            // to determine each commodity's share of the pre-accumulated yield.
            let total_yield_per_hectare: f64 = crop_def.yields.values().sum();

            // For each (commodity, tons_per_hectare) in crop_def.yields:
            for (commodity, tons_per_hectare) in &crop_def.yields {
                // Standard per-turn yield calculation (climate-modulated).
                let base_commodity_yield =
                    batch.active_hectares * tons_per_hectare * modifiers.agriculture_multiplier;
                let turn_commodity_yield = base_commodity_yield / harvest_duration_turns;
                let final_yield = turn_commodity_yield * (1.0 - batch.rot_accumulator);

                // Pre-accumulated yield guarantee: if the crop was pre-seeded
                // at world generation with accumulated_yield > 0, ensure the
                // harvest produces at least the pre-accumulated biomass
                // distributed across the harvest window. This represents the
                // physical biomass that grew during the previous growing season
                // and is now being harvested.
                //
                // The accumulated yield is distributed proportionally across
                // commodities (by their yield share) and across harvest turns.
                // It is decremented each turn so it is consumed over the
                // harvest window and does not create infinite food.
                let guaranteed_yield =
                    if total_yield_per_hectare > 0.0 && batch.accumulated_yield > 0.0 {
                        let commodity_share = tons_per_hectare / total_yield_per_hectare;
                        let per_turn_accumulated =
                            batch.accumulated_yield * commodity_share / harvest_duration_turns;
                        per_turn_accumulated * (1.0 - batch.rot_accumulator)
                    } else {
                        0.0
                    };

                // The actual yield is the maximum of the standard calculation
                // and the pre-accumulated guarantee. This ensures pre-seeded
                // crops produce food even if the climate multiplier is low,
                // while not suppressing higher yields from favorable conditions.
                let actual_yield = final_yield.max(guaranteed_yield);
                let commodity_key = commodity.inventory_key();

                // Find company's owned warehouse buildings and deposit using encapsulated methods
                let mut remaining_yield = actual_yield;
                for building_id in &company.building_ids {
                    if let Some(building) = commercial_buildings
                        .iter_mut()
                        .find(|b| &b.id == building_id)
                    {
                        if matches!(
                            building.building_type,
                            crate::society::housing::CommercialBuildingType::Warehouse
                        ) {
                            let excess = building.deposit_inventory(
                                commodity_key.clone(),
                                remaining_yield,
                                company.id.clone(),
                                current_turn,
                            );
                            remaining_yield = excess;
                            if remaining_yield == 0.0 {
                                break; // All yield stored
                            }
                        }
                    }
                }

                // Phase 46: Excess beyond warehouse capacity rots in the field.
                // No fire sale, no cash generation. remaining_yield is simply lost.
                // This forces companies to build sufficient storage or lose harvest.
            }

            // Decrement accumulated_yield proportionally for this turn's harvest.
            // Each turn consumes 1/harvest_duration_turns of the accumulated yield.
            if harvest_duration_turns > 0.0 && batch.accumulated_yield > 0.0 {
                let consumed = batch.accumulated_yield / harvest_duration_turns;
                batch.accumulated_yield = (batch.accumulated_yield - consumed).max(0.0);
            }
        }
    }
}

/// Placeholder auto-sell loop for Phase 6.3.5
///
/// Solvent companies: Sell 20% of inventory per turn (gradual market absorption)
/// Receivership: Sell 100% immediately (liquidation)
/// Uses building's withdraw_inventory method for encapsulation
/// Includes logistics transport fee (anti-teleportation) with double-entry accounting
/// Transfers physical assets to State before despawn (prevents orphaning)
/// Will be replaced by Phase 6.4 B2B market
///
/// # Arguments
/// * `company` - Mutable reference to the company
/// * `treasury` - Mutable reference to treasury for logistics revenue
/// * `commercial_buildings` - Mutable slice of commercial buildings
/// * `is_in_receivership` - Whether company is in receivership
/// Submit harvest asks to B2B market (Phase 6.5, Phase ORD).
///
/// # Arguments
/// * `company` - Agricultural company with harvest
/// * `commercial_buildings` - Warehouses containing inventory
/// * `market_prices` - Current market prices for commodities
///
/// # Returns
/// * `Vec<(Commodity, f64, f64)>` — (commodity, quantity, ask_price) sell orders
///
/// # Rules
/// * Agricultural companies submit sell orders for their harvest
/// * Ask price = market_price * (1 + margin) where margin is lower for perishables
/// * Only submits for inventory in company-owned warehouses
/// * Used in ORD phase before B2B clearing
pub fn submit_harvest_asks(
    company: &Company,
    commercial_buildings: &[CommercialBuilding],
    market_prices: &BTreeMap<Commodity, f64>,
) -> Vec<(Commodity, f64, f64)> {
    let mut sell_orders = Vec::new();

    if company.sector != crate::registries::enums::Sector::Agriculture {
        return sell_orders;
    }

    // Iterate over company-owned warehouses
    for building_id in &company.building_ids {
        if let Some(building) = commercial_buildings.iter().find(|b| &b.id == building_id) {
            if !matches!(
                building.building_type,
                crate::society::housing::CommercialBuildingType::Warehouse
            ) {
                continue;
            }

            // Iterate over all inventory batches for this building
            for (commodity_key, batches) in &building.current_inventory {
                // Calculate total quantity for this commodity owned by this company
                let total_owned: f64 = batches
                    .iter()
                    .filter(|b| b.owner_id == company.id)
                    .map(|b| b.quantity)
                    .sum();

                if total_owned <= 0.0 {
                    continue;
                }

                // Parse commodity key to Commodity enum
                let commodity = match commodity_key.as_str() {
                    "Cereal" => Commodity::Cereal,
                    "Vegetable" => Commodity::Vegetable,
                    "Fruit" => Commodity::Fruit,
                    "Meat" => Commodity::Meat,
                    "Seeds" => Commodity::Seeds,
                    _ => continue,
                };

                // Get market price or skip if no reference price available
                let market_price = match market_prices.get(&commodity) {
                    Some(p) if *p > 0.0 => *p,
                    _ => continue, // No market price — skip this commodity
                };

                // Calculate ask price with margin (lower margin for perishables)
                let is_perishable = matches!(
                    commodity,
                    Commodity::Vegetable | Commodity::Fruit | Commodity::Meat
                );
                let margin = if is_perishable { 0.05 } else { 0.10 };
                let ask_price = market_price * (1.0 + margin);

                sell_orders.push((commodity, total_owned, ask_price));
            }
        }
    }

    sell_orders
}

/// Process agricultural company despawn and land reclamation for bankrupt farms.
///
/// Phase 46: No placeholder cash generation. Solvent companies' inventory is
/// sold via the B2B market by `submit_harvest_asks`. Receivership companies'
/// physical inventory is transferred to State ownership (reassign `owner_id`
/// on inventory batches to `"STATE"`), and buildings are transferred to State.
/// The State can then sell these goods via B2B in subsequent turns.
///
/// # Returns
/// * `(Option<String>, LandReclamationData)` — despawn signal and land reclamation data
pub fn process_agricultural_despawn(
    company: &mut Company,
    _treasury: &mut Treasury,
    commercial_buildings: &mut [CommercialBuilding],
    is_in_receivership: bool,
) -> (Option<String>, LandReclamationData) {
    let mut reclamation_data = LandReclamationData::default();

    if is_in_receivership {
        // Transfer physical inventory in warehouses to State ownership
        for building_id in &company.building_ids {
            if let Some(building) = commercial_buildings
                .iter_mut()
                .find(|b| &b.id == building_id)
            {
                // Reassign all inventory batches owned by this company to STATE
                for batches in building.current_inventory.values_mut() {
                    for batch in batches.iter_mut() {
                        if batch.owner_id == company.id {
                            batch.owner_id = STATE_OWNER_ID.to_string();
                        }
                    }
                }
            }
        }

        // Asset Transfer: Reassign all company buildings to State before despawn
        for building_id in &company.building_ids {
            if let Some(building) = commercial_buildings
                .iter_mut()
                .find(|b| &b.id == building_id)
            {
                building.owner_id = STATE_OWNER_ID.to_string();
            }
        }

        // Land Reclamation: Calculate hectares to reclaim from AgriculturalProfile
        if let Some(agri_profile) = &company.agricultural_profile {
            let total_hectares_f64 =
                agri_profile.arable_land_hectares + agri_profile.plantation_hectares;
            let total_hectares = total_hectares_f64.round() as i64;
            reclamation_data.total_hectares_reclaimed = total_hectares;
        }

        // Liquidator Despawn: Remove bankrupt company shell after asset transfer
        (Some(company.id.clone()), reclamation_data)
    } else {
        // Solvent company: inventory selling is handled by B2B market via submit_harvest_asks.
        // No placeholder cash generation here.
        (None, reclamation_data)
    }
}

/// Reclaim agricultural land from a despawned company to the State land bank.
///
/// # Arguments
/// * `region` - Mutable reference to the region where land is reclaimed
/// * `reclamation_data` - Land reclamation data from the despawned company
///
/// # Rules
/// * Proportionally redistributes reclaimed hectares across soil classes.
/// * Uses largest-remainder rounding to ensure exact total conservation.
/// * Clamps corporation_hectares to never go negative.
/// * Carries unabsorbed remainder to the next soil class.
/// * Failsafe: any final remainder is added to the first soil class.
pub fn reclaim_agricultural_land(region: &mut Region, mut reclamation_data: LandReclamationData) {
    if reclamation_data.total_hectares_reclaimed <= 0 {
        return;
    }

    // Calculate total corporation hectares across all soil classes
    let total_corp_hectares: i64 = region
        .land_distribution
        .values()
        .map(|d| d.corporation_hectares)
        .sum();

    if total_corp_hectares == 0 {
        // No corporation land exists; add all to first soil class as failsafe
        if let Some((first_class, dist)) = region.land_distribution.iter_mut().next() {
            dist.state_hectares += reclamation_data.total_hectares_reclaimed;
            reclamation_data.soil_class_transfers.insert(
                first_class.clone(),
                reclamation_data.total_hectares_reclaimed,
            );
        }
        return;
    }

    // Proportional largest-remainder redistribution
    let mut remainder = reclamation_data.total_hectares_reclaimed;
    let mut fractional_parts: Vec<(String, f64)> = Vec::new();

    for (soil_class, dist) in &region.land_distribution {
        let share = dist.corporation_hectares as f64 / total_corp_hectares as f64;
        let fractional = share * reclamation_data.total_hectares_reclaimed as f64;
        fractional_parts.push((soil_class.clone(), fractional));
    }

    // Sort by fractional part descending for largest-remainder
    fractional_parts.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    // Clone keys for remainder distribution before consuming fractional_parts
    let sorted_classes: Vec<_> = fractional_parts.iter().map(|(k, _)| k.clone()).collect();

    for (soil_class, fractional) in fractional_parts {
        if remainder <= 0 {
            break;
        }

        let allocated = fractional.floor() as i64;
        if allocated > 0 {
            if let Some(dist) = region.land_distribution.get_mut(&soil_class) {
                let actual_transfer = allocated.min(dist.corporation_hectares).min(remainder);
                dist.corporation_hectares -= actual_transfer;
                dist.state_hectares += actual_transfer;
                remainder -= actual_transfer;
                reclamation_data
                    .soil_class_transfers
                    .insert(soil_class.clone(), actual_transfer);
            }
        }
    }

    // Distribute remainder one hectare at a time to largest fractional parts
    while remainder > 0 {
        let mut transferred_any = false;
        for soil_class in &sorted_classes {
            if remainder <= 0 {
                break;
            }
            if let Some(dist) = region.land_distribution.get_mut(soil_class) {
                if dist.corporation_hectares > 0 {
                    dist.corporation_hectares -= 1;
                    dist.state_hectares += 1;
                    remainder -= 1;
                    *reclamation_data
                        .soil_class_transfers
                        .entry(soil_class.clone())
                        .or_insert(0) += 1;
                    transferred_any = true;
                }
            }
        }
        if !transferred_any {
            // No corporation land left; add remainder to first class as failsafe
            if let Some((first_class, dist)) = region.land_distribution.iter_mut().next() {
                dist.state_hectares += remainder;
                *reclamation_data
                    .soil_class_transfers
                    .entry(first_class.clone())
                    .or_insert(0) += remainder;
            }
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{AgriculturalProfile, Building, Company, CropBatch, CropState};
    use crate::registries::crops::{
        CropCategory, CropDefinition, LaborDemandProfile, LandType, TurnRange,
    };
    use crate::registries::enums::{Commodity, Sector};
    use crate::registries::Registries;
    use crate::society::geography::ClimateProfile;
    use crate::state::Calendar;
    use std::collections::HashMap;
    use std::sync::OnceLock;

    /// Build a test CropDefinition for wheat (arable, requires sowing).
    fn test_wheat_def() -> CropDefinition {
        let mut yields = HashMap::new();
        yields.insert(Commodity::Cereal, 4.5);
        yields.insert(Commodity::Fodder, 2.0);
        CropDefinition {
            id: "wheat".to_string(),
            name: "Pszenica".to_string(),
            category: CropCategory::Cereal,
            land_type: LandType::Arable,
            compatible_climates: vec![ClimateProfile::Temperate],
            sowing_schedule: TurnRange {
                start_turn: 5,
                end_turn: 7,
            },
            harvest_schedule: TurnRange {
                start_turn: 17,
                end_turn: 19,
            },
            labor_demand: LaborDemandProfile {
                sowing_fte_per_hectare: 0.12,
                growing_fte_per_hectare: 0.04,
                harvesting_fte_per_hectare: 0.18,
            },
            yields,
            seed_cost_per_hectare: 150.0,
            seed_commodity: Commodity::Seeds,
            seed_quantity_per_hectare: 0.05,
            sowing_wage_multiplier: 1.5,
            harvesting_wage_multiplier: 2.8,
        }
    }

    /// Build a test registries containing only the test wheat crop.
    fn test_registries() -> &'static Registries {
        static REG: OnceLock<Registries> = OnceLock::new();
        REG.get_or_init(|| {
            let mut crops = crate::registries::crops::CropRegistry::default();
            crops.crops.insert("wheat".to_string(), test_wheat_def());
            Registries {
                tech_tree: HashMap::new(),
                production_methods: HashMap::new(),
                building_templates: HashMap::new(),
                government_forms: HashMap::new(),
                crops,
            }
        })
    }

    /// Build a test agricultural company with one idle wheat batch.
    fn test_farm_company(seed_inventory: f64) -> (Company, Building) {
        let mut company = Company::new(
            "FARM-001".to_string(),
            "Test Farm".to_string(),
            Sector::Agriculture,
            crate::entities::legal_form::LegalForm::JointStockCompany(
                crate::entities::legal_form::JointStockData::default(),
            ),
            100_000.0,
            0.0,
            10,
        );
        let mut profile = AgriculturalProfile::default();
        let batch = CropBatch {
            crop_id: "wheat".to_string(),
            state: CropState::Idle,
            planned_hectares: 100.0,
            active_hectares: 0.0,
            planted_turn: 0,
            accumulated_yield: 0.0,
            rot_accumulator: 0.0,
        };
        profile.batches.push(batch);
        company.agricultural_profile = Some(profile);

        let mut building = Building::default();
        building.id = "BLD-001".to_string();
        building.owner_id = "FARM-001".to_string();
        building.sector = Sector::Agriculture;
        if seed_inventory > 0.0 {
            building.inventory.insert(Commodity::Seeds, seed_inventory);
        }
        company.building_ids.push("BLD-001".to_string());

        (company, building)
    }

    #[test]
    fn test_sowing_withdraws_physical_seeds_no_cash_flow() {
        let (mut company, building) = test_farm_company(5.0); // 5 tons of seeds
        let mut buildings = vec![building];
        let registries = test_registries();
        let calendar = Calendar {
            global_turn: 6,
            ..Default::default()
        };

        let _initial_treasury = 0.0; // We don't pass treasury anymore
        let initial_seeds = buildings[0]
            .inventory
            .get(&Commodity::Seeds)
            .copied()
            .unwrap_or(0.0);

        transition_agricultural_states(&mut company, &calendar, registries, &mut buildings);

        // Verify sowing occurred
        let batch = &company.agricultural_profile.as_ref().unwrap().batches[0];
        assert_eq!(batch.state, CropState::Sowing);
        assert!(batch.active_hectares > 0.0);

        // Verify seeds were withdrawn from building inventory
        let remaining_seeds = buildings[0]
            .inventory
            .get(&Commodity::Seeds)
            .copied()
            .unwrap_or(0.0);
        assert!(
            remaining_seeds < initial_seeds,
            "Seeds should have been withdrawn"
        );

        // Verify no money was created (liquid_capital should still be 0)
        assert_eq!(company.liquid_capital, 0.0);
    }

    #[test]
    fn test_insufficient_seeds_reduces_sown_hectares() {
        // 1 ton of seeds, wheat needs 0.05 tons/hectare → max 20 hectares
        let (mut company, building) = test_farm_company(1.0);
        let mut buildings = vec![building];
        let registries = test_registries();
        let calendar = Calendar {
            global_turn: 6,
            ..Default::default()
        };

        transition_agricultural_states(&mut company, &calendar, registries, &mut buildings);

        let batch = &company.agricultural_profile.as_ref().unwrap().batches[0];
        assert_eq!(batch.state, CropState::Sowing);
        // 1.0 ton / 0.05 tons/hectare = 20 hectares (not 100 planned)
        assert!((batch.active_hectares - 20.0).abs() < 1e-6);
    }

    #[test]
    fn test_no_seeds_skips_sowing() {
        let (mut company, building) = test_farm_company(0.0);
        let mut buildings = vec![building];
        let registries = test_registries();
        let calendar = Calendar {
            global_turn: 6,
            ..Default::default()
        };

        transition_agricultural_states(&mut company, &calendar, registries, &mut buildings);

        let batch = &company.agricultural_profile.as_ref().unwrap().batches[0];
        // No seeds → no sowing, batch stays Idle
        assert_eq!(batch.state, CropState::Idle);
        assert_eq!(batch.active_hectares, 0.0);
    }
}

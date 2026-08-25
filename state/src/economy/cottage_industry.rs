//! Phase 85: Cottage industry — self-production of B2C goods by households.
//!
//! Citizens allocate FTE to self-produce finished goods (Clothing, Furniture, Food)
//! from raw materials (TextileWaste, Timber, Cereal) purchased on the B2B market.
//!
//! # Key Mechanics
//! - **FTE Reservation (Fix 2)**: Cottage FTE is reserved in the Pre-labor phase
//!   BEFORE the industrial labor market clears. Only remaining FTE is exposed
//!   to corporate hiring.
//! - **Temporal Causality (Fix 5)**: Production in Turn N consumes from
//!   `cottage_raw_inventory` (purchased in Turn N-1). B2B buy orders submitted
//!   in Turn N replenish for Turn N+1.
//! - **Demand Clamping (Fix 3)**: Cottage output is clamped by the demographic's
//!   UtilityDemand for that good. Citizens only produce what they consume.
//! - **Opportunity Cost (Rule 8)**: Citizens compare cottage_value_per_fte vs
//!   industrial_wage. Higher wages → less cottage FTE.
//! - **Mass Conservation (Rule 1)**: Recipes include waste byproducts routed
//!   to the Phase 84 WasteGridState.

#![allow(missing_docs)]

use crate::registries::enums::Commodity;
use crate::society::geography::{ClassDemographics, MicroRegion};
use std::collections::BTreeMap;

/// A cottage recipe — physically grounded, mass-conserving (Rule 1/3).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CottageRecipe {
    /// Finished good produced (Clothing, Furniture, Food)
    pub output: Commodity,
    /// Raw material input (TextileWaste, Timber, Cereal)
    pub input: Commodity,
    /// Physical units of input per unit output
    pub input_per_unit: f64,
    /// FTE required per unit output
    pub fte_per_unit: f64,
    /// Base efficiency (0.0-1.0, modified by domain bonus)
    pub efficiency_base: f64,
    /// Waste byproduct (mass conservation: input = output + waste)
    pub waste_output: Commodity,
    /// Waste units per unit output
    pub waste_per_unit: f64,
}

impl CottageRecipe {
    /// Get all cottage recipes (static, physically grounded — Rule 3).
    pub fn all_recipes() -> Vec<CottageRecipe> {
        vec![
            // Clothing: 2.0 TextileWaste + 0.5 FTE → 1.0 Clothing + 1.0 TextileWaste (offcuts)
            CottageRecipe {
                output: Commodity::Clothing,
                input: Commodity::TextileWaste,
                input_per_unit: 2.0,
                fte_per_unit: 0.5,
                efficiency_base: 0.6,
                waste_output: Commodity::TextileWaste,
                waste_per_unit: 1.0,
            },
            // Furniture: 3.0 Timber + 1.0 FTE → 1.0 Furniture + 2.0 MixedWaste (sawdust/scraps)
            CottageRecipe {
                output: Commodity::Furniture,
                input: Commodity::Timber,
                input_per_unit: 3.0,
                fte_per_unit: 1.0,
                efficiency_base: 0.5,
                waste_output: Commodity::MixedWaste,
                waste_per_unit: 2.0,
            },
            // Food: 1.0 Cereal + 0.3 FTE → 1.0 Food + 0.1 BioWaste (preparation waste)
            CottageRecipe {
                output: Commodity::Food,
                input: Commodity::Cereal,
                input_per_unit: 1.0,
                fte_per_unit: 0.3,
                efficiency_base: 0.8,
                waste_output: Commodity::BioWaste,
                waste_per_unit: 0.1,
            },
        ]
    }

    /// Find recipe for a given output commodity.
    pub fn for_output(output: Commodity) -> Option<CottageRecipe> {
        Self::all_recipes().into_iter().find(|r| r.output == output)
    }
}

/// Configuration for cottage industry — no magic numbers (Rule 2).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CottageConfig {
    /// Maximum fraction of available_fte that can go to cottage industry
    /// (self-regulating via opportunity cost, but hard-capped for stability).
    pub max_cottage_fte_fraction: f64,
    /// How strongly the wage gap affects cottage share (0.0 = no response, 1.0 = full)
    pub opportunity_cost_sensitivity: f64,
}

impl Default for CottageConfig {
    fn default() -> Self {
        Self {
            max_cottage_fte_fraction: 0.5,
            opportunity_cost_sensitivity: 0.8,
        }
    }
}

/// Result of cottage FTE reservation for a single class demographic.
#[derive(Debug, Clone, Default)]
pub struct CottageReservationResult {
    /// FTE reserved for cottage production
    pub cottage_fte: f64,
    /// FTE reserved for guild workshops
    pub guild_fte: f64,
    /// FTE remaining for the industrial labor market
    pub labor_pool_fte: f64,
    /// Planned output by commodity (clamped by demand and inventory)
    pub planned_output: BTreeMap<Commodity, f64>,
    /// Raw materials to purchase via B2B for next turn
    pub raw_material_demand: BTreeMap<Commodity, f64>,
}

/// Reserve cottage and guild FTE for a class demographic.
///
/// Called in the Pre-labor phase, BEFORE the industrial labor market clears.
/// Citizens evaluate opportunity cost and decide how much FTE to reserve
/// for self-production vs factory work.
///
/// # Arguments
/// * `demo` - The class demographic (mutated: cottage_fte_allocated, guild_fte_allocated set)
/// * `average_wage` - Current/historical industrial wage per FTE
/// * `market_prices` - Current market prices for cottage inputs/outputs
/// * `utility_demand` - The class's physical internal demand for finished goods
/// * `domain` - The factional domain this class belongs to (for cottage bonus)
/// * `config` - Cottage industry configuration
pub fn reserve_cottage_fte(
    demo: &mut ClassDemographics,
    average_wage: f64,
    market_prices: &BTreeMap<Commodity, f64>,
    utility_demand: &BTreeMap<Commodity, f64>,
    domain: Option<&MicroRegion>,
    config: &CottageConfig,
) -> CottageReservationResult {
    let mut result = CottageReservationResult::default();

    // Available FTE for reservation (before labor market)
    let available = demo.available_fte - demo.allocated_fte;
    if available <= 0.0 {
        return result;
    }

    // Domain cottage bonus
    let cottage_bonus = domain
        .map(|d| d.local_laws.cottage_industry_bonus)
        .unwrap_or(0.0);

    // Evaluate each recipe and compute total cottage FTE
    let mut total_cottage_fte = 0.0;

    for recipe in CottageRecipe::all_recipes() {
        // Get market prices (fallback to 0 if unknown)
        let output_price = market_prices.get(&recipe.output).copied().unwrap_or(0.0);
        let input_price = market_prices.get(&recipe.input).copied().unwrap_or(0.0);

        // Cottage value per FTE (net of raw material cost)
        let efficiency = recipe.efficiency_base + cottage_bonus;
        let output_per_fte = efficiency / recipe.fte_per_unit;
        let input_cost_per_fte = input_price * recipe.input_per_unit * output_per_fte;
        let cottage_value_per_fte = output_price * output_per_fte - input_cost_per_fte;

        // Opportunity cost: if industrial wage is higher, don't do cottage
        if average_wage > cottage_value_per_fte {
            continue;
        }

        // Demand clamping (Fix 3): only produce what the class consumes
        let demand = utility_demand.get(&recipe.output).copied().unwrap_or(0.0);
        if demand <= 0.0 {
            continue;
        }

        // Inventory clamping (Fix 5): limited by raw inventory from last turn
        let available_raw = demo.cottage_raw_inventory.get(&recipe.input).copied().unwrap_or(0.0);
        let max_output_from_inventory = available_raw / recipe.input_per_unit;
        let max_output = demand.min(max_output_from_inventory);
        if max_output <= 0.0 {
            continue;
        }

        // FTE needed for demand-capped, inventory-capped output
        let fte_needed = max_output * recipe.fte_per_unit / efficiency;

        // Cap by available FTE and max fraction
        let max_fte = available * config.max_cottage_fte_fraction;
        let cottage_fte = fte_needed.min(max_fte).min(available - total_cottage_fte);

        if cottage_fte > 0.0 {
            let actual_output = cottage_fte * efficiency / recipe.fte_per_unit;
            result.planned_output.insert(recipe.output, actual_output);
            total_cottage_fte += cottage_fte;

            // Plan raw material purchase for next turn (replenish inventory)
            let raw_needed = actual_output * recipe.input_per_unit;
            *result.raw_material_demand.entry(recipe.input).or_insert(0.0) += raw_needed;
        }
    }

    // Clamp total cottage FTE
    total_cottage_fte = total_cottage_fte.min(available * config.max_cottage_fte_fraction);

    // Guild FTE: compare guild dividend vs wage (guild_fte starts at 0 in Phase 85,
    // will be populated when guild system is fully integrated)
    let guild_fte = 0.0; // TODO: Guild FTE reservation in Step 9

    // Compute labor pool for industrial market
    let labor_pool_fte = (available - total_cottage_fte - guild_fte).max(0.0);

    // Update demographic fields
    demo.cottage_fte_allocated = total_cottage_fte;
    demo.guild_fte_allocated = guild_fte;

    result.cottage_fte = total_cottage_fte;
    result.guild_fte = guild_fte;
    result.labor_pool_fte = labor_pool_fte;

    result
}

/// Execute cottage production for a class demographic.
///
/// Called AFTER labor market clearing, consuming raw materials from
/// `cottage_raw_inventory` (purchased in Turn N-1).
///
/// # Arguments
/// * `demo` - The class demographic (mutated: cottage_raw_inventory consumed, cottage_output set)
/// * `domain` - The factional domain (for cottage bonus)
///
/// # Returns
/// Vector of (waste_commodity, waste_amount) for waste routing.
pub fn execute_cottage_production(
    demo: &mut ClassDemographics,
    domain: Option<&MicroRegion>,
) -> Vec<(Commodity, f64)> {
    let mut waste_generated = Vec::new();
    demo.cottage_output.clear();

    let cottage_bonus = domain
        .map(|d| d.local_laws.cottage_industry_bonus)
        .unwrap_or(0.0);

    let cottage_fte = demo.cottage_fte_allocated;
    if cottage_fte <= 0.0 {
        return waste_generated;
    }

    for recipe in CottageRecipe::all_recipes() {
        let efficiency = recipe.efficiency_base + cottage_bonus;

        // Available raw material from inventory (purchased in N-1)
        let available_raw = demo.cottage_raw_inventory.get(&recipe.input).copied().unwrap_or(0.0);
        if available_raw <= 0.0 {
            continue;
        }

        // Max output from available raw material
        let max_output_from_raw = available_raw / recipe.input_per_unit;

        // Max output from allocated FTE
        let max_output_from_fte = cottage_fte * efficiency / recipe.fte_per_unit;

        // Actual output: limited by both raw material and FTE (Fix 5 + Rule 1)
        let output = max_output_from_raw.min(max_output_from_fte);
        if output <= 0.0 {
            continue;
        }

        // Consume raw material (mass conservation)
        let raw_consumed = output * recipe.input_per_unit;
        let current_raw = demo.cottage_raw_inventory.entry(recipe.input).or_insert(0.0);
        *current_raw = (*current_raw - raw_consumed).max(0.0);

        // Generate waste byproduct (mass conservation: input = output + waste)
        let waste = output * recipe.waste_per_unit;
        waste_generated.push((recipe.waste_output, waste));

        // Store output
        demo.cottage_output.insert(recipe.output, output);
    }

    waste_generated
}

/// Get the effective labor pool FTE for a demographic after cottage/guild reservation.
///
/// This is the FTE that the labor market should see (Fix 2).
pub fn get_labor_pool_fte(demo: &ClassDemographics) -> f64 {
    (demo.available_fte - demo.allocated_fte - demo.cottage_fte_allocated - demo.guild_fte_allocated).max(0.0)
}

/// Reset cottage FTE allocation at the start of a turn (before reservation).
pub fn reset_cottage_allocation(demo: &mut ClassDemographics) {
    demo.cottage_fte_allocated = 0.0;
    demo.guild_fte_allocated = 0.0;
    demo.cottage_output.clear();
}

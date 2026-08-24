//! Phase 81 Wave 2: Consumption method Bill of Materials (BOM) computation.
//!
//! Resolves the active consumption methods (lighting, heating, ventilation,
//! power generation) for a building and computes the per-turn commodity
//! consumption, scaled by the building's physical capacity (Flaw 1 correction).
//!
//! # Scaling (Flaw 1)
//! - Housing: `scale = occupied_slots` (per occupant)
//! - Commercial: `scale = (office_capacity + retail_capacity) / 100.0` (per 100 sqm)
//! - Industrial: `scale = effective_employment / 1000.0` (per 1000 workers)

use crate::entities::Building;
use crate::registries::enums::Commodity;
use crate::registries::production_methods::{BuildingMethods, MethodSlot, ProductionMethod};
use crate::society::housing::{CommercialBuilding, HousingBuilding};
use std::collections::BTreeMap;

/// Phase 81 Wave 2: Per-turn commodity consumption from active consumption methods.
///
/// Separated into grid utilities (Energy, Heat) and physical commodities
/// (Oil, HardCoal, Fuels, CoalGas) because they route through different
/// market systems.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConsumptionBom {
    /// Grid utility demand (Commodity → quantity per turn).
    /// Contains Energy and Heat demand from lighting, heating, ventilation.
    /// Routed through the grid distribution system.
    pub grid_utility_demand: BTreeMap<Commodity, f64>,
    /// Physical commodity demand (Commodity → quantity per turn).
    /// Contains Oil, HardCoal, Fuels, CoalGas from lighting and heating.
    /// Routed through B2C (housing) or B2B (industrial) markets.
    pub physical_commodity_demand: BTreeMap<Commodity, f64>,
    /// Microgeneration output (Energy → quantity per turn).
    /// Reduces grid demand. Excess feeds back to grid via net metering.
    pub microgeneration_output: BTreeMap<Commodity, f64>,
}

impl ConsumptionBom {
    /// Add another ConsumptionBom's values into this one.
    pub fn add(&mut self, other: &ConsumptionBom) {
        for (&c, &q) in &other.grid_utility_demand {
            *self.grid_utility_demand.entry(c).or_insert(0.0) += q;
        }
        for (&c, &q) in &other.physical_commodity_demand {
            *self.physical_commodity_demand.entry(c).or_insert(0.0) += q;
        }
        for (&c, &q) in &other.microgeneration_output {
            *self.microgeneration_output.entry(c).or_insert(0.0) += q;
        }
    }
}

/// Phase 82: Check if a heating method name belongs to the District Heating track.
///
/// The heating slot contains methods from two parallel tracks:
/// - **Standalone Track**: Primitive Fireplace, Peat Stove, Coal Stove, etc.
///   These consume physical fuel directly (HardCoal, Fuels, Timber, etc.)
/// - **District Heating Track**: Unmetered Radiators, Thermostatic Valves,
///   Smart Substations. These consume `Commodity::Heat` from the thermal grid.
///
/// This helper is the single point of truth for track determination.
/// The consumption BOM resolver uses it to decide whether a building's
/// heating demand goes to the physical commodity market (Standalone) or
/// the thermal grid (District Heating).
pub fn is_district_heating_method(method_name: &str) -> bool {
    matches!(
        method_name,
        "Unmetered Radiators" | "Thermostatic Valves" | "Smart Substations"
    )
}

/// Phase 81 Wave 2: Compute the scale factor for a housing building.
/// `scale = occupied_slots` (per occupant).
pub fn housing_scale_factor(building: &HousingBuilding) -> f64 {
    let primary = building.primary_slots.occupied_slots as f64;
    let sublet = building
        .sublet_slots
        .as_ref()
        .map(|s| s.occupied_slots as f64)
        .unwrap_or(0.0);
    (primary + sublet).max(0.0)
}

/// Phase 81 Wave 2: Compute the scale factor for a commercial building.
/// `scale = (office_capacity + retail_capacity) / 100.0` (per 100 sqm).
pub fn commercial_scale_factor(building: &CommercialBuilding) -> f64 {
    ((building.office_capacity + building.retail_capacity) / 100.0).max(0.0)
}

/// Phase 81 Wave 2: Compute the scale factor for an industrial building.
/// `scale = effective_employment / 1000.0` (per 1000 workers).
pub fn industrial_scale_factor(building: &Building) -> f64 {
    let employment = building.current_employment as f64;
    (employment / 1000.0).max(0.0)
}

/// Phase 81 Wave 2: Resolve a consumption method from the registry.
///
/// Looks up the method by name in the specified slot. If the method is not
/// found or the name is empty, falls back to "None" (no consumption).
///
/// # Arguments
/// * `methods` - The BuildingMethods registry for this building type
/// * `slot` - The MethodSlot to look up (Lighting, Heating, etc.)
/// * `method_name` - The active method name string
///
/// # Returns
/// A reference to the resolved ProductionMethod, or the "None" method if
/// not found.
pub fn resolve_consumption_method<'a>(
    methods: &'a BuildingMethods,
    slot: MethodSlot,
    method_name: &str,
) -> Option<&'a ProductionMethod> {
    if method_name.is_empty() {
        return methods.get(slot, "None");
    }
    methods.get(slot, method_name)
}

/// Phase 81 Wave 2: Compute the consumption BOM for a housing building.
///
/// Resolves the active lighting, heating, and power generation methods from
/// the housing consumption registry, then scales the per-unit rates by the
/// building's occupied slots (Flaw 1 correction).
///
/// # Arguments
/// * `building` - The housing building
/// * `methods` - The housing consumption method registry
///
/// # Returns
/// A ConsumptionBom with grid utility demand, physical commodity demand,
/// and microgeneration output.
pub fn compute_housing_consumption_bom(
    building: &HousingBuilding,
    methods: &BuildingMethods,
) -> ConsumptionBom {
    let scale = housing_scale_factor(building);
    let mut bom = ConsumptionBom::default();

    // Lighting
    if let Some(method) = resolve_consumption_method(methods, MethodSlot::Lighting, &building.active_lighting) {
        add_method_inputs(&mut bom, method, scale);
    }

    // Heating
    if let Some(method) = resolve_consumption_method(methods, MethodSlot::Heating, &building.active_heating) {
        add_method_inputs(&mut bom, method, scale);
    }

    // Power generation (microgeneration)
    if let Some(method) = resolve_consumption_method(methods, MethodSlot::PowerGeneration, &building.active_power_generation) {
        add_method_outputs(&mut bom, method, scale);
    }

    bom
}

/// Phase 81 Wave 2: Compute the consumption BOM for a commercial building.
pub fn compute_commercial_consumption_bom(
    building: &CommercialBuilding,
    methods: &BuildingMethods,
) -> ConsumptionBom {
    let scale = commercial_scale_factor(building);
    let mut bom = ConsumptionBom::default();

    if let Some(method) = resolve_consumption_method(methods, MethodSlot::Lighting, &building.active_lighting) {
        add_method_inputs(&mut bom, method, scale);
    }

    if let Some(method) = resolve_consumption_method(methods, MethodSlot::Heating, &building.active_heating) {
        add_method_inputs(&mut bom, method, scale);
    }

    if let Some(method) = resolve_consumption_method(methods, MethodSlot::PowerGeneration, &building.active_power_generation) {
        add_method_outputs(&mut bom, method, scale);
    }

    bom
}

/// Phase 81 Wave 2: Compute the consumption BOM for an industrial building.
pub fn compute_industrial_consumption_bom(
    building: &Building,
    methods: &BuildingMethods,
) -> ConsumptionBom {
    let scale = industrial_scale_factor(building);
    let mut bom = ConsumptionBom::default();

    // Industrial buildings use ProductionMethodChoice for consumption methods
    let active = &building.active_method.active_methods;

    if let Some(method) = resolve_consumption_method(methods, MethodSlot::Lighting, &active.lighting) {
        add_method_inputs(&mut bom, method, scale);
    }

    if let Some(method) = resolve_consumption_method(methods, MethodSlot::Heating, &active.heating) {
        add_method_inputs(&mut bom, method, scale);
    }

    if let Some(method) = resolve_consumption_method(methods, MethodSlot::Ventilation, &active.ventilation) {
        add_method_inputs(&mut bom, method, scale);
    }

    // Industrial buildings do NOT use PowerGeneration (they use PPAs instead)

    bom
}

/// Add a method's per-turn inputs to the consumption BOM, scaled by the
/// building's scale factor. Routes Energy and Heat to grid utility demand,
/// all other commodities to physical commodity demand.
fn add_method_inputs(bom: &mut ConsumptionBom, method: &ProductionMethod, scale: f64) {
    for (&commodity, &per_unit) in &method.inputs {
        let quantity = per_unit * scale;
        if quantity <= 0.0 {
            continue;
        }
        match commodity {
            Commodity::Energy | Commodity::Heat => {
                *bom.grid_utility_demand.entry(commodity).or_insert(0.0) += quantity;
            }
            _ => {
                *bom.physical_commodity_demand.entry(commodity).or_insert(0.0) += quantity;
            }
        }
    }
}

/// Add a method's per-turn outputs (microgeneration) to the consumption BOM,
/// scaled by the building's scale factor.
fn add_method_outputs(bom: &mut ConsumptionBom, method: &ProductionMethod, scale: f64) {
    for (&commodity, &per_unit) in &method.outputs {
        let quantity = per_unit * scale;
        if quantity <= 0.0 {
            continue;
        }
        *bom.microgeneration_output.entry(commodity).or_insert(0.0) += quantity;
    }
}

/// Phase 81 Wave 2: Check if a building can adopt District Heating.
///
/// A building can only adopt District Heating if the region has an active
/// thermal grid (Constraint 2). The regional heat supply is tracked via
/// `region.capacity_pool[CapacityType::DistrictHeating]`.
///
/// # Arguments
/// * `region` - The region where the building is located
///
/// # Returns
/// `true` if the region has district heating capacity > 0.
pub fn can_adopt_district_heating(region: &crate::society::geography::Region) -> bool {
    use crate::infrastructure::CapacityType;
    let heat_supply = region
        .capacity_pool
        .get(&CapacityType::DistrictHeating)
        .copied()
        .unwrap_or(0.0);
    heat_supply > 0.0
}

/// Phase 81 Wave 2: Compute the CAPEX BOM for a method upgrade, scaled by
/// the building's physical capacity (Flaw 1 correction).
///
/// # Arguments
/// * `target_method` - The target ProductionMethod (must have `capex` field)
/// * `scale` - The building's scale factor
///
/// # Returns
/// A BTreeMap of Commodity → total CAPEX quantity needed.
pub fn compute_capex_bom(
    target_method: &ProductionMethod,
    scale: f64,
) -> BTreeMap<Commodity, f64> {
    target_method
        .capex
        .iter()
        .map(|(&commodity, &per_unit)| (commodity, per_unit * scale))
        .filter(|&(_, q)| q > 0.0)
        .collect()
}

// ============================================================================
// PHASE 83: WATER SUPPLY & SANITATION HELPERS (Water Quality Spectrum)
// ============================================================================

/// Phase 83: Returns true if the water supply method name is a centralized
/// (municipal grid) method that draws from `WaterNetworkState`.
/// Standalone methods (Local Well, Rainwater Catchment, etc.) draw from
/// `WaterReserveState` and return false.
pub fn is_centralized_water_method(method_name: &str) -> bool {
    matches!(
        method_name,
        "Municipal Mains (Basic)"
            | "Metered Connection"
            | "Pressurized Mains"
            | "Smart Meter Connection"
    )
}

/// Phase 83: Returns true if the sanitation method name is a centralized
/// (municipal sewer) method that discharges to `SewerNetworkState`.
/// Standalone methods (Open Defecation, Cesspool, etc.) discharge to the
/// environment and return false.
pub fn is_centralized_sanitation_method(method_name: &str) -> bool {
    matches!(
        method_name,
        "Municipal Sewer (Basic)"
            | "Improved Sewer Connection"
            | "Modern Sewer + Treatment"
            | "Advanced Sewer + Tertiary"
    )
}

/// Phase 83: Returns the biohazard factor for a standalone sanitation method.
/// Represents the biological pollution mass per unit of water discharged.
/// Centralized methods have near-zero residual biohazard (imperfections in
/// collection). Standalone methods have high biohazard (open defecation = 5.0).
/// "None" = 5.0 (no sanitation at all = maximum biohazard).
pub fn sanitation_biohazard_factor(method_name: &str) -> f64 {
    match method_name {
        "None" => 5.0,
        "Open Defecation" => 5.0,
        "Cesspool" => 3.0,
        "Outhouse" => 2.5,
        "Septic Tank" => 1.0,
        "Improved Septic" => 0.5,
        "Municipal Sewer (Basic)" => 0.2,
        "Improved Sewer Connection" => 0.1,
        "Modern Sewer + Treatment" => 0.02,
        "Advanced Sewer + Tertiary" => 0.005,
        _ => 5.0, // Unknown = treat as no sanitation
    }
}

/// Phase 83: Returns the water source quality for a standalone water supply
/// method. Groundwater-sourced methods deliver quality 0.9; surface-sourced
/// methods deliver quality 0.6. Centralized methods deliver grid quality
/// (determined at runtime from `WaterNetworkState.current_quality`).
pub fn standalone_water_source_quality(method_name: &str) -> Option<f64> {
    match method_name {
        "Local Well" | "Hand Pump Well" | "Shallow Tube Well" => Some(0.9), // Groundwater
        "Rainwater Catchment" => Some(0.6), // Surface water
        "None" => None,                    // No water at all
        _ => None,                         // Centralized or unknown — quality from grid
    }
}

/// Phase 83: Returns true if a standalone water method draws from groundwater
/// (vs. surface water). Used by the hydro grid to determine which reserve to
/// draw from.
pub fn standalone_water_uses_groundwater(method_name: &str) -> bool {
    matches!(
        method_name,
        "Local Well" | "Hand Pump Well" | "Shallow Tube Well"
    )
}

/// Phase 83: Returns true if a standalone sanitation method leaks into
/// groundwater (cesspools, septic tanks). Others discharge to surface/environment.
pub fn standalone_sanitation_leaks_to_groundwater(method_name: &str) -> bool {
    matches!(method_name, "Cesspool" | "Septic Tank" | "Improved Septic")
}

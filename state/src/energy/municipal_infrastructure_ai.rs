//! Phase 83: Unified Municipal Infrastructure AI.
//!
//! Combines four infrastructure domains into a single decision-making framework:
//! - **Thermal** (Phase 82): District heating pipes + heating plants
//! - **Electrical** (retroactive): Generation + HV interconnectors
//! - **Water** (Phase 83): Water mains + treatment plants (quality-aware)
//! - **Sanitation** (Phase 83): Sewer network + wastewater treatment
//!
//! ## Decision Process
//!
//! 1. **Diagnose** all four domains for deficits
//! 2. **Calculate ROI** for each domain (mortality reduction / CAPEX)
//! 3. **Crisis Override** (REFINEMENT 2): If any crisis condition is met,
//!    mortality-reducing infrastructure bypasses ROI sorting
//! 4. **Sort** by ROI descending (non-crisis domains)
//! 5. **Allocate** budget to highest-priority domains first
//!
//! ## Crisis Conditions
//!
//! - `biohazard_level > 50.0` (epidemic risk)
//! - `smog_level > 50.0` (severe air pollution)
//! - `winter_mortality_multiplier > 2.0` (winter death crisis)
//! - `surface_water_quality < 0.3` (PARADIGM SHIFT: environmental collapse)
//! - `dehydration_mortality > 2.0` (GUARDRAIL 2: dry wells)

use crate::energy::municipal_heating_ai::HeatingInvestmentPlan;
use crate::utilities::hydro_grid::{SewerNetworkState, WaterNetworkState, WaterReserveState};
use crate::utilities::hydro_types::{WastewaterPlantType, WaterPlantType};
use serde::{Deserialize, Serialize};

/// The infrastructure domains managed by the unified Municipal AI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InfrastructureDomain {
    #[default]
    /// Thermal (district heating) infrastructure.
    Thermal,
    /// Electrical (generation + grid) infrastructure.
    Electrical,
    /// Water supply (treatment + distribution) infrastructure.
    Water,
    /// Sanitation (sewer + wastewater treatment) infrastructure.
    Sanitation,
    /// Phase 84: Waste (collection + treatment + disposal) infrastructure.
    Waste,
}

/// Water infrastructure investment plan (PARADIGM SHIFT: quality-aware).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WaterInvestmentPlan {
    /// Expand water main pipe network by this many km (0.0 = no expansion).
    #[serde(default)]
    pub expand_pipes_km: f64,
    /// Build a new water treatment plant of this type (None = no new plant).
    #[serde(default)]
    pub build_treatment_plant: Option<WaterPlantType>,
    /// Estimated CAPEX of the plan (in currency units).
    #[serde(default)]
    pub estimated_capex: f64,
    /// Expected mortality reduction value (in currency units).
    #[serde(default)]
    pub expected_mortality_reduction_value: f64,
    /// PARADIGM SHIFT: Current water quality in the grid (0.0-1.0).
    #[serde(default)]
    pub current_water_quality: f64,
    /// Whether the plan passes the cost-benefit gate.
    #[serde(default)]
    pub passes_cost_benefit_gate: bool,
    /// Whether this domain is in crisis (bypasses ROI sorting).
    #[serde(default)]
    pub is_crisis: bool,
    /// Human-readable reason for the decision.
    #[serde(default)]
    pub rationale: String,
}

/// Sanitation infrastructure investment plan (PARADIGM SHIFT: environmental healing).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SanitationInvestmentPlan {
    /// Expand sewer pipe network by this many km (0.0 = no expansion).
    #[serde(default)]
    pub expand_sewers_km: f64,
    /// Build a new wastewater treatment plant of this type (None = no new plant).
    #[serde(default)]
    pub build_wastewater_plant: Option<WastewaterPlantType>,
    /// Estimated CAPEX of the plan (in currency units).
    #[serde(default)]
    pub estimated_capex: f64,
    /// Expected mortality reduction value (in currency units).
    #[serde(default)]
    pub expected_mortality_reduction_value: f64,
    /// PARADIGM SHIFT: Current surface water quality (0.0-1.0).
    #[serde(default)]
    pub surface_water_quality: f64,
    /// Whether the plan passes the cost-benefit gate.
    #[serde(default)]
    pub passes_cost_benefit_gate: bool,
    /// Whether this domain is in crisis (bypasses ROI sorting).
    #[serde(default)]
    pub is_crisis: bool,
    /// Human-readable reason for the decision.
    #[serde(default)]
    pub rationale: String,
}

/// Phase 84: Waste management infrastructure investment plan.
///
/// Diagnoses uncollected waste, landfill utilization, illegal-dumping biohazard,
/// and burning emissions. Evaluates collection-route expansion, waste plant
/// construction, and WtE investment. ROI from mortality reduction + recycling revenue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WasteInvestmentPlan {
    /// Expand collection route network by this many km (0.0 = no expansion).
    #[serde(default)]
    pub expand_collection_routes_km: f64,
    /// Build a new waste plant of this type (None = no new plant).
    #[serde(default)]
    pub build_waste_plant: Option<crate::utilities::waste_grid::WastePlantType>,
    /// Estimated CAPEX of the plan (in currency units).
    #[serde(default)]
    pub estimated_capex: f64,
    /// Expected mortality reduction from reduced biohazard (in currency units).
    #[serde(default)]
    pub expected_mortality_reduction_value: f64,
    /// Current landfill utilization (0.0-1.0).
    #[serde(default)]
    pub landfill_utilization: f64,
    /// Total uncollected waste mass (tons).
    #[serde(default)]
    pub uncollected_waste_mass: f64,
    /// Whether the plan passes the cost-benefit gate.
    #[serde(default)]
    pub passes_cost_benefit_gate: bool,
    /// Whether this domain is in crisis (bypasses ROI sorting).
    /// Crisis when: landfill full (LOGISTICAL BOUND 2) or uncollected biohazard critical.
    #[serde(default)]
    pub is_crisis: bool,
    /// Human-readable reason for the decision.
    #[serde(default)]
    pub rationale: String,
}

/// Electrical infrastructure investment plan (retroactive fix).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ElectricalInvestmentPlan {
    /// Build new generation capacity (MW).
    #[serde(default)]
    pub new_generation_mw: f64,
    /// Expand HV interconnector capacity (MW).
    #[serde(default)]
    pub expand_hv_mw: f64,
    /// Estimated CAPEX of the plan.
    #[serde(default)]
    pub estimated_capex: f64,
    /// Expected economic value from preventing blackouts.
    #[serde(default)]
    pub expected_blackout_prevention_value: f64,
    /// Whether the plan passes the cost-benefit gate.
    #[serde(default)]
    pub passes_cost_benefit_gate: bool,
    /// Whether this domain is in crisis.
    #[serde(default)]
    pub is_crisis: bool,
    /// Human-readable reason for the decision.
    #[serde(default)]
    pub rationale: String,
}

/// Unified municipal infrastructure plan covering all domains.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MunicipalInfrastructurePlan {
    /// Thermal (heating) investment plan.
    #[serde(default)]
    pub thermal_plan: HeatingInvestmentPlan,
    /// Electrical investment plan.
    #[serde(default)]
    pub electrical_plan: ElectricalInvestmentPlan,
    /// Water investment plan.
    #[serde(default)]
    pub water_plan: WaterInvestmentPlan,
    /// Sanitation investment plan.
    #[serde(default)]
    pub sanitation_plan: SanitationInvestmentPlan,
    /// Phase 84: Waste management investment plan.
    #[serde(default)]
    pub waste_plan: WasteInvestmentPlan,
    /// Which domain is prioritized for funding this turn.
    #[serde(default)]
    pub prioritized_domain: InfrastructureDomain,
    /// Total CAPEX across all funded plans.
    #[serde(default)]
    pub total_capex: f64,
    /// Available budget for infrastructure investment.
    #[serde(default)]
    pub available_budget: f64,
    /// Human-readable summary of the overall decision.
    #[serde(default)]
    pub rationale: String,
}

/// Crisis condition thresholds (REFINEMENT 2 + GUARDRAIL 2).
pub const CRISIS_BIOHAZARD_THRESHOLD: f64 = 50.0;
pub const CRISIS_SMOG_THRESHOLD: f64 = 50.0;
pub const CRISIS_WINTER_MORTALITY_THRESHOLD: f64 = 2.0;
pub const CRISIS_SURFACE_WATER_QUALITY_THRESHOLD: f64 = 0.3;
pub const CRISIS_DEHYDRATION_MORTALITY_THRESHOLD: f64 = 2.0;

/// Check if any crisis condition is met.
///
/// Crisis conditions (per plan E.6):
/// - `biohazard_level > 50.0` (epidemic risk)
/// - `smog_level > 50.0` (severe air pollution)
/// - `winter_mortality_multiplier > 2.0` (winter death crisis)
/// - `surface_water_quality < 0.3` (environmental collapse)
/// - `dehydration_mortality > 2.0` (dry wells)
pub fn is_crisis_condition(
    biohazard_level: f64,
    smog_level: f64,
    winter_mortality_multiplier: f64,
    surface_water_quality: f64,
    dehydration_mortality: f64,
) -> bool {
    biohazard_level > CRISIS_BIOHAZARD_THRESHOLD
        || smog_level > CRISIS_SMOG_THRESHOLD
        || winter_mortality_multiplier > CRISIS_WINTER_MORTALITY_THRESHOLD
        || surface_water_quality < CRISIS_SURFACE_WATER_QUALITY_THRESHOLD
        || dehydration_mortality > CRISIS_DEHYDRATION_MORTALITY_THRESHOLD
}

/// Run the water investment decision tree for a single region.
///
/// PARADIGM SHIFT: Water investment is quality-aware, not just volume-aware.
/// The AI checks both throughput deficits and quality deficits.
///
/// # Arguments
/// * `water_network` - Current water network state
/// * `water_reserves` - Current water reserves
/// * `buildings_wanting_mains` - Number of buildings wanting water main connection
/// * `treatment_throughput` - Current treatment throughput (liters/turn)
/// * `water_demand` - Total water demand (liters/turn)
/// * `available_plant_types` - Cost data for all unlocked plant types
/// * `average_wage` - Current average wage (for CAPEX scaling)
/// * `mortality_cost_per_death` - Economic value of preventing one death
/// * `estimated_deaths_from_water_quality` - Deaths from low-quality water
/// * `dehydration_mortality` - Current dehydration mortality multiplier
/// * `financing_available` - Whether the municipality can finance the CAPEX
pub fn run_water_investment_ai(
    water_network: &WaterNetworkState,
    _water_reserves: &WaterReserveState,
    buildings_wanting_mains: usize,
    treatment_throughput: f64,
    water_demand: f64,
    available_plant_types: &[WaterPlantCostData],
    average_wage: f64,
    mortality_cost_per_death: f64,
    estimated_deaths_from_water_quality: f64,
    dehydration_mortality: f64,
    financing_available: bool,
) -> WaterInvestmentPlan {
    let mut plan = WaterInvestmentPlan::default();
    let mut rationales: Vec<String> = Vec::new();

    plan.current_water_quality = water_network.current_quality;

    // Crisis check: water quality collapse or dehydration
    let quality_crisis = water_network.current_quality < 0.9 && water_network.current_quality > 0.0;
    let dehydration_crisis = dehydration_mortality > CRISIS_DEHYDRATION_MORTALITY_THRESHOLD;
    plan.is_crisis = quality_crisis || dehydration_crisis;

    // Step 1: Pipe capacity check
    let max_connectable = water_network.max_connectable_buildings(0.8);
    if max_connectable < buildings_wanting_mains {
        plan.expand_pipes_km = 5.0;
        plan.estimated_capex += 5.0 * average_wage * 800.0; // 5km * 800 wages/km
        rationales.push(format!(
            "Pipe capacity check: max_connectable={} < demand={}, expanding pipes by 5 km",
            max_connectable, buildings_wanting_mains
        ));
    }

    // Step 2: Quality deficit check (PARADIGM SHIFT)
    if water_network.current_quality < 0.9 && water_network.current_quality > 0.0 {
        rationales.push(format!(
            "Quality deficit: current_quality={:.2} < 0.9, planning treatment plant upgrade",
            water_network.current_quality
        ));
        plan.build_treatment_plant = select_best_water_plant(available_plant_types);
        if let Some(plant_type) = plan.build_treatment_plant {
            let plant_capex = average_wage * 35000.0; // Approximate plant CAPEX
            plan.estimated_capex += plant_capex;
            rationales.push(format!(
                "Plant selection: {:?} (CAPEX={:.0})",
                plant_type, plant_capex
            ));
        }
    }

    // Step 3: Volume deficit check
    if treatment_throughput < water_demand {
        rationales.push(format!(
            "Volume deficit: throughput={:.0} < demand={:.0}, planning treatment plant",
            treatment_throughput, water_demand
        ));
        if plan.build_treatment_plant.is_none() {
            plan.build_treatment_plant = select_best_water_plant(available_plant_types);
            if let Some(plant_type) = plan.build_treatment_plant {
                let plant_capex = average_wage * 35000.0;
                plan.estimated_capex += plant_capex;
                rationales.push(format!(
                    "Plant selection: {:?} (CAPEX={:.0})",
                    plant_type, plant_capex
                ));
            }
        }
    }

    // Step 4: Cost-benefit gate
    let expected_deaths_prevented = estimated_deaths_from_water_quality * 0.4;
    plan.expected_mortality_reduction_value =
        expected_deaths_prevented * mortality_cost_per_death;

    plan.passes_cost_benefit_gate = financing_available
        && plan.estimated_capex > 0.0
        && (plan.expected_mortality_reduction_value > plan.estimated_capex || plan.is_crisis);

    if plan.is_crisis && plan.estimated_capex > 0.0 && financing_available {
        rationales.push("Crisis override: water quality/dehydration crisis, bypassing ROI gate".to_string());
    } else if !financing_available {
        rationales.push("Cost-benefit gate: financing not available".to_string());
    } else if plan.estimated_capex == 0.0 {
        rationales.push("No investment needed".to_string());
    } else if !plan.passes_cost_benefit_gate {
        rationales.push(format!(
            "Cost-benefit gate: mortality_value={:.0} < capex={:.0}, rejected",
            plan.expected_mortality_reduction_value, plan.estimated_capex
        ));
    } else {
        rationales.push("Cost-benefit gate: approved".to_string());
    }

    plan.rationale = rationales.join("; ");
    plan
}

/// Run the sanitation investment decision tree for a single region.
///
/// PARADIGM SHIFT: Sanitation investment considers environmental healing
/// (surface water quality) in addition to mortality reduction.
pub fn run_sanitation_investment_ai(
    sewer_network: &SewerNetworkState,
    water_reserves: &WaterReserveState,
    buildings_wanting_sewers: usize,
    sewer_throughput: f64,
    wastewater_treatment_capacity: f64,
    biohazard_level: f64,
    available_plant_types: &[WastewaterPlantCostData],
    average_wage: f64,
    mortality_cost_per_death: f64,
    estimated_deaths_from_biohazard: f64,
    industrial_corrosion_cost: f64,
    financing_available: bool,
) -> SanitationInvestmentPlan {
    let mut plan = SanitationInvestmentPlan::default();
    let mut rationales: Vec<String> = Vec::new();

    plan.surface_water_quality = water_reserves.surface_water_quality;

    // Crisis check: biohazard epidemic or surface water collapse
    let biohazard_crisis = biohazard_level > CRISIS_BIOHAZARD_THRESHOLD;
    let surface_water_crisis = water_reserves.surface_water_quality < CRISIS_SURFACE_WATER_QUALITY_THRESHOLD;
    plan.is_crisis = biohazard_crisis || surface_water_crisis;

    // Step 1: Sewer capacity check
    let max_connectable = sewer_network.max_connectable_buildings(0.8);
    if max_connectable < buildings_wanting_sewers {
        plan.expand_sewers_km = 5.0;
        plan.estimated_capex += 5.0 * average_wage * 1000.0;
        rationales.push(format!(
            "Sewer capacity check: max_connectable={} < demand={}, expanding sewers by 5 km",
            max_connectable, buildings_wanting_sewers
        ));
    }

    // Step 2: Treatment capacity check
    if sewer_throughput > wastewater_treatment_capacity {
        rationales.push(format!(
            "Treatment deficit: sewer_throughput={:.0} > treatment_capacity={:.0}, planning wastewater plant",
            sewer_throughput, wastewater_treatment_capacity
        ));
        plan.build_wastewater_plant = select_best_wastewater_plant(available_plant_types);
        if let Some(plant_type) = plan.build_wastewater_plant {
            let plant_capex = average_wage * 40000.0;
            plan.estimated_capex += plant_capex;
            rationales.push(format!(
                "Plant selection: {:?} (CAPEX={:.0})",
                plant_type, plant_capex
            ));
        }
    }

    // Step 3: Surface water quality crisis (PARADIGM SHIFT)
    if surface_water_crisis {
        rationales.push(format!(
            "Surface water crisis: quality={:.2} < 0.3, planning wastewater plant for environmental healing",
            water_reserves.surface_water_quality
        ));
        if plan.build_wastewater_plant.is_none() {
            plan.build_wastewater_plant = select_best_wastewater_plant(available_plant_types);
            if let Some(plant_type) = plan.build_wastewater_plant {
                let plant_capex = average_wage * 40000.0;
                plan.estimated_capex += plant_capex;
                rationales.push(format!(
                    "Plant selection: {:?} (CAPEX={:.0})",
                    plant_type, plant_capex
                ));
            }
        }
    }

    // Step 4: Cost-benefit gate
    let expected_deaths_prevented = estimated_deaths_from_biohazard * 0.5;
    plan.expected_mortality_reduction_value =
        expected_deaths_prevented * mortality_cost_per_death + industrial_corrosion_cost;

    plan.passes_cost_benefit_gate = financing_available
        && plan.estimated_capex > 0.0
        && (plan.expected_mortality_reduction_value > plan.estimated_capex || plan.is_crisis);

    if plan.is_crisis && plan.estimated_capex > 0.0 && financing_available {
        rationales.push("Crisis override: biohazard/surface water crisis, bypassing ROI gate".to_string());
    } else if !financing_available {
        rationales.push("Cost-benefit gate: financing not available".to_string());
    } else if plan.estimated_capex == 0.0 {
        rationales.push("No investment needed".to_string());
    } else if !plan.passes_cost_benefit_gate {
        rationales.push(format!(
            "Cost-benefit gate: value={:.0} < capex={:.0}, rejected",
            plan.expected_mortality_reduction_value, plan.estimated_capex
        ));
    } else {
        rationales.push("Cost-benefit gate: approved".to_string());
    }

    plan.rationale = rationales.join("; ");
    plan
}

/// Cost data for a water treatment plant type, used for OPEX comparison.
#[derive(Debug, Clone)]
pub struct WaterPlantCostData {
    pub plant_type: WaterPlantType,
    /// Chemicals + Energy OPEX per liter of throughput.
    pub opex_per_liter: f64,
    /// CAPEX per liter of nameplate capacity.
    pub capex_per_liter: f64,
    /// Output water quality (0.0-1.0).
    pub output_water_quality: f64,
    /// Whether the required technology is unlocked.
    pub tech_unlocked: bool,
    /// Whether the region has the required geological trait (coastal/arid for desalination).
    pub geologically_eligible: bool,
}

/// Cost data for a wastewater treatment plant type.
#[derive(Debug, Clone)]
pub struct WastewaterPlantCostData {
    pub plant_type: WastewaterPlantType,
    /// OPEX per liter of throughput.
    pub opex_per_liter: f64,
    /// CAPEX per liter of nameplate capacity.
    pub capex_per_liter: f64,
    /// Discharge water quality (0.0-1.0).
    pub discharge_quality: f64,
    /// Whether the required technology is unlocked.
    pub tech_unlocked: bool,
}

/// Select the best water treatment plant type: highest output_water_quality
/// per OPEX among unlocked + geographically eligible types.
fn select_best_water_plant(available: &[WaterPlantCostData]) -> Option<WaterPlantType> {
    available
        .iter()
        .filter(|p| p.tech_unlocked && p.geologically_eligible)
        .max_by(|a, b| {
            // Quality per OPEX — higher is better
            let a_score = a.output_water_quality / a.opex_per_liter.max(0.001);
            let b_score = b.output_water_quality / b.opex_per_liter.max(0.001);
            a_score.partial_cmp(&b_score).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|p| p.plant_type)
}

/// Select the best wastewater treatment plant type: highest discharge_quality
/// per OPEX among unlocked types.
fn select_best_wastewater_plant(available: &[WastewaterPlantCostData]) -> Option<WastewaterPlantType> {
    available
        .iter()
        .filter(|p| p.tech_unlocked)
        .max_by(|a, b| {
            let a_score = a.discharge_quality / a.opex_per_liter.max(0.001);
            let b_score = b.discharge_quality / b.opex_per_liter.max(0.001);
            a_score.partial_cmp(&b_score).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|p| p.plant_type)
}

/// Run the unified Municipal Infrastructure AI for a single region.
///
/// Combines all domain plans, applies crisis override, and allocates
/// budget to the highest-priority domains.
///
/// # Arguments
/// * `thermal_plan` - Pre-computed thermal investment plan
/// * `electrical_plan` - Pre-computed electrical investment plan
/// * `water_plan` - Pre-computed water investment plan
/// * `sanitation_plan` - Pre-computed sanitation investment plan
/// * `waste_plan` - Phase 84: Pre-computed waste investment plan
/// * `available_budget` - Total budget available for infrastructure
pub fn run_unified_municipal_ai(
    thermal_plan: HeatingInvestmentPlan,
    electrical_plan: ElectricalInvestmentPlan,
    water_plan: WaterInvestmentPlan,
    sanitation_plan: SanitationInvestmentPlan,
    waste_plan: WasteInvestmentPlan,
    available_budget: f64,
) -> MunicipalInfrastructurePlan {
    let mut plan = MunicipalInfrastructurePlan {
        thermal_plan,
        electrical_plan,
        water_plan,
        sanitation_plan,
        waste_plan,
        prioritized_domain: InfrastructureDomain::Thermal,
        total_capex: 0.0,
        available_budget,
        rationale: String::new(),
    };

    // Collect all domains with their crisis status and ROI
    let domains: Vec<(InfrastructureDomain, f64, bool, f64)> = vec![
        (
            InfrastructureDomain::Thermal,
            plan.thermal_plan.estimated_capex,
            false, // Thermal crisis is checked separately (winter mortality)
            plan.thermal_plan.expected_mortality_reduction_value,
        ),
        (
            InfrastructureDomain::Electrical,
            plan.electrical_plan.estimated_capex,
            plan.electrical_plan.is_crisis,
            plan.electrical_plan.expected_blackout_prevention_value,
        ),
        (
            InfrastructureDomain::Water,
            plan.water_plan.estimated_capex,
            plan.water_plan.is_crisis,
            plan.water_plan.expected_mortality_reduction_value,
        ),
        (
            InfrastructureDomain::Sanitation,
            plan.sanitation_plan.estimated_capex,
            plan.sanitation_plan.is_crisis,
            plan.sanitation_plan.expected_mortality_reduction_value,
        ),
        (
            InfrastructureDomain::Waste,
            plan.waste_plan.estimated_capex,
            plan.waste_plan.is_crisis,
            plan.waste_plan.expected_mortality_reduction_value,
        ),
    ];

    // REFINEMENT 2: Crisis override — crisis domains go first
    let mut budget_remaining = available_budget;
    let mut funded: Vec<String> = Vec::new();
    let mut rationales: Vec<String> = Vec::new();

    // First: fund crisis domains
    for (domain, capex, _is_crisis, _) in domains.iter().filter(|(_, _, c, _)| *c) {
        if *capex > 0.0 && budget_remaining >= *capex {
            budget_remaining -= *capex;
            plan.total_capex += *capex;
            funded.push(format!("{:?} (crisis)", domain));
            if plan.prioritized_domain == InfrastructureDomain::Thermal {
                plan.prioritized_domain = *domain;
            }
        } else if *capex > 0.0 {
            rationales.push(format!("{:?} (crisis): insufficient budget ({:.0} < {:.0})", domain, budget_remaining, *capex));
        }
    }

    // Second: sort non-crisis domains by ROI (mortality_value / capex)
    let mut non_crisis: Vec<_> = domains
        .iter()
        .filter(|(_, _, c, _)| !*c)
        .collect();
    non_crisis.sort_by(|a, b| {
        let a_roi = if a.1 > 0.0 { a.3 / a.1 } else { 0.0 };
        let b_roi = if b.1 > 0.0 { b.3 / b.1 } else { 0.0 };
        b_roi.partial_cmp(&a_roi).unwrap_or(std::cmp::Ordering::Equal)
    });

    for (domain, capex, _, _) in non_crisis {
        if *capex > 0.0 && budget_remaining >= *capex {
            budget_remaining -= *capex;
            plan.total_capex += *capex;
            funded.push(format!("{:?}", domain));
            if plan.prioritized_domain == InfrastructureDomain::Thermal && plan.total_capex == *capex {
                plan.prioritized_domain = *domain;
            }
        }
    }

    if funded.is_empty() {
        rationales.push("No domains funded (insufficient budget or no investment needed)".to_string());
    } else {
        rationales.push(format!("Funded: {}", funded.join(", ")));
    }
    rationales.push(format!("Total CAPEX: {:.0} / Budget: {:.0}", plan.total_capex, available_budget));

    plan.rationale = rationales.join("; ");
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_water_plant_costs() -> Vec<WaterPlantCostData> {
        vec![
            WaterPlantCostData {
                plant_type: WaterPlantType::SlowSandFilter,
                opex_per_liter: 0.0001,
                capex_per_liter: 0.5,
                output_water_quality: 0.95,
                tech_unlocked: true,
                geologically_eligible: true,
            },
            WaterPlantCostData {
                plant_type: WaterPlantType::ModernTreatmentPlant,
                opex_per_liter: 0.0005,
                capex_per_liter: 2.0,
                output_water_quality: 0.99,
                tech_unlocked: true,
                geologically_eligible: true,
            },
            WaterPlantCostData {
                plant_type: WaterPlantType::DesalinationPlant,
                opex_per_liter: 0.004,
                capex_per_liter: 5.0,
                output_water_quality: 0.99,
                tech_unlocked: true,
                geologically_eligible: false, // No coastal access
            },
        ]
    }

    fn test_wastewater_plant_costs() -> Vec<WastewaterPlantCostData> {
        vec![
            WastewaterPlantCostData {
                plant_type: WastewaterPlantType::PrimarySettling,
                opex_per_liter: 0.0002,
                capex_per_liter: 0.8,
                discharge_quality: 0.30,
                tech_unlocked: true,
            },
            WastewaterPlantCostData {
                plant_type: WastewaterPlantType::AdvancedWastewaterPlant,
                opex_per_liter: 0.001,
                capex_per_liter: 3.0,
                discharge_quality: 0.85,
                tech_unlocked: true,
            },
        ]
    }

    #[test]
    fn test_crisis_condition_detection() {
        assert!(!is_crisis_condition(10.0, 10.0, 1.0, 0.8, 1.0));
        assert!(is_crisis_condition(60.0, 10.0, 1.0, 0.8, 1.0)); // biohazard
        assert!(is_crisis_condition(10.0, 60.0, 1.0, 0.8, 1.0)); // smog
        assert!(is_crisis_condition(10.0, 10.0, 3.0, 0.8, 1.0)); // winter mortality
        assert!(is_crisis_condition(10.0, 10.0, 1.0, 0.2, 1.0)); // surface water
        assert!(is_crisis_condition(10.0, 10.0, 1.0, 0.8, 3.0)); // dehydration
    }

    #[test]
    fn test_water_investment_no_deficit() {
        let network = WaterNetworkState {
            pipe_network_km: 100.0,
            pipe_condition: 1.0,
            current_quality: 0.98,
            throughput_liters: 5000.0,
            ..Default::default()
        };
        let reserves = WaterReserveState::default();
        let plan = run_water_investment_ai(
            &network, &reserves, 100, 5000.0, 4000.0,
            &test_water_plant_costs(), 10.0, 10000.0, 0.0, 1.0, true,
        );
        assert_eq!(plan.expand_pipes_km, 0.0);
        assert_eq!(plan.build_treatment_plant, None);
        assert!(!plan.is_crisis);
    }

    #[test]
    fn test_water_investment_quality_deficit() {
        let network = WaterNetworkState {
            pipe_network_km: 100.0,
            pipe_condition: 1.0,
            current_quality: 0.5, // Low quality
            throughput_liters: 5000.0,
            ..Default::default()
        };
        let reserves = WaterReserveState::default();
        let plan = run_water_investment_ai(
            &network, &reserves, 100, 5000.0, 4000.0,
            &test_water_plant_costs(), 10.0, 10000.0, 10.0, 1.0, true,
        );
        assert!(plan.build_treatment_plant.is_some());
        assert!(plan.is_crisis);
    }

    #[test]
    fn test_sanitation_investment_surface_water_crisis() {
        let sewer = SewerNetworkState {
            pipe_network_km: 100.0,
            pipe_condition: 1.0,
            throughput_liters: 1000.0,
            ..Default::default()
        };
        let reserves = WaterReserveState {
            surface_water_quality: 0.2, // Crisis
            ..Default::default()
        };
        let plan = run_sanitation_investment_ai(
            &sewer, &reserves, 100, 1000.0, 500.0, 10.0,
            &test_wastewater_plant_costs(), 10.0, 10000.0, 5.0, 1000.0, true,
        );
        assert!(plan.build_wastewater_plant.is_some());
        assert!(plan.is_crisis);
    }

    #[test]
    fn test_unified_ai_crisis_override() {
        let thermal = HeatingInvestmentPlan::default();
        let electrical = ElectricalInvestmentPlan::default();
        let water = WaterInvestmentPlan {
            estimated_capex: 50000.0,
            expected_mortality_reduction_value: 10000.0, // Low ROI
            is_crisis: true,
            passes_cost_benefit_gate: true,
            ..Default::default()
        };
        let sanitation = SanitationInvestmentPlan::default();

        let plan = run_unified_municipal_ai(thermal, electrical, water, sanitation, WasteInvestmentPlan::default(), 100000.0);
        assert_eq!(plan.prioritized_domain, InfrastructureDomain::Water);
        assert!(plan.total_capex > 0.0);
    }

    #[test]
    fn test_select_best_water_plant_excludes_ineligible() {
        let plants = test_water_plant_costs();
        let selected = select_best_water_plant(&plants);
        assert!(selected.is_some());
        // Desalination is geologically ineligible, should not be selected
        assert_ne!(selected.unwrap(), WaterPlantType::DesalinationPlant);
    }

    #[test]
    fn test_select_best_wastewater_plant() {
        let plants = test_wastewater_plant_costs();
        let selected = select_best_wastewater_plant(&plants);
        assert_eq!(selected, Some(WastewaterPlantType::AdvancedWastewaterPlant));
    }
}

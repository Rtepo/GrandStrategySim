//! Phase 82: Municipal AI for heating infrastructure investment.
//!
//! The Municipal AI is an explicit decision tree that determines whether
//! a region's municipal government should invest in heating infrastructure.
//! It is NOT a black box — every step is transparent and auditable.
//!
//! ## Decision Tree (CORRECTION 2: Municipal AI Black Box)
//!
//! 1. **Pipe capacity check**: If `max_connectable_buildings < district_heating_demand`,
//!    plan to expand pipes by 5 km.
//! 2. **Heat supply check**: If `effective_heat_supply < district_heating_demand`,
//!    plan to build one heating plant.
//! 3. **Plant type selection**: Select the best unlocked and geographically
//!    eligible heating-plant type, using lowest OPEX/GJ.
//! 4. **Cost-benefit gate**: Apply the plan only when expected mortality-reduction
//!    value exceeds CAPEX and financing is viable.
//!
//! ## Regulated Pricing (CORRECTION 3 + 5)
//!
//! Heat is a natural monopoly. The price is set by a regulated cost-plus formula
//! that includes fuel OPEX, maintenance OPEX, labor OPEX, amortized CAPEX, and
//! a regulated margin (1.10). The denominator uses a 24-turn rolling average
//! of heat sales to prevent summer price spikes.

use crate::energy::heating_types::HeatingPlantType;
use crate::energy::thermal_grid::ThermalGridState;
use serde::{Deserialize, Serialize};

/// Investment plan produced by the Municipal AI decision tree.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HeatingInvestmentPlan {
    /// Expand pipe network by this many km (0.0 = no expansion).
    #[serde(default)]
    pub pipe_expansion_km: f64,
    /// Build a new heating plant of this type (None = no new plant).
    #[serde(default)]
    pub new_plant_type: Option<HeatingPlantType>,
    /// Estimated CAPEX of the plan (in currency units).
    #[serde(default)]
    pub estimated_capex: f64,
    /// Expected mortality reduction value (in currency units).
    #[serde(default)]
    pub expected_mortality_reduction_value: f64,
    /// Whether the plan passes the cost-benefit gate.
    #[serde(default)]
    pub passes_cost_benefit_gate: bool,
    /// Human-readable reason for the decision.
    #[serde(default)]
    pub rationale: String,
}

/// Cost data for a heating plant type, used for OPEX comparison.
#[derive(Debug, Clone)]
pub struct PlantTypeCostData {
    pub plant_type: HeatingPlantType,
    /// Fuel OPEX per GJ of heat output.
    pub fuel_opex_per_gj: f64,
    /// Maintenance OPEX per GJ.
    pub maintenance_opex_per_gj: f64,
    /// CAPEX per GJ of nameplate capacity.
    pub capex_per_gj: f64,
    /// Whether the required technology is unlocked.
    pub tech_unlocked: bool,
    /// Whether the region has the required geological trait.
    pub geologically_eligible: bool,
}

/// Rolling history of heat sales for smoothed pricing (CORRECTION 5).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeatSalesHistory {
    /// Rolling window of heat sold per turn (GJ).
    #[serde(default)]
    pub sales_history: Vec<f64>,
    /// Maximum window size (24 turns = 6 years).
    #[serde(default = "default_window_size")]
    pub window_size: usize,
}

fn default_window_size() -> usize {
    24
}

impl Default for HeatSalesHistory {
    fn default() -> Self {
        Self {
            sales_history: Vec::new(),
            window_size: default_window_size(),
        }
    }
}

impl HeatSalesHistory {
    /// Record heat sold this turn.
    pub fn record(&mut self, heat_sold_gj: f64) {
        self.sales_history.push(heat_sold_gj);
        if self.sales_history.len() > self.window_size {
            self.sales_history.remove(0);
        }
    }

    /// Compute the rolling average of heat sales.
    pub fn rolling_average(&self) -> f64 {
        if self.sales_history.is_empty() {
            0.0
        } else {
            let sum: f64 = self.sales_history.iter().sum();
            sum / self.sales_history.len() as f64
        }
    }
}

/// Run the Municipal AI decision tree for a single region.
///
/// This is the explicit, auditable heuristic required by CORRECTION 2.
/// Every step produces a clear rationale string.
///
/// # Arguments
/// * `thermal_grid` - Current thermal grid state
/// * `district_heating_demand` - Number of buildings wanting DH connection
/// * `effective_heat_supply` - Current heat supply after transmission losses (GJ)
/// * `heat_demand_gj` - Total heat demand in GJ
/// * `available_plant_types` - Cost data for all plant types
/// * `average_wage` - Current average wage (for CAPEX scaling)
/// * `mortality_cost_per_death` - Economic value of preventing one death
/// * `estimated_deaths_from_smog` - Estimated annual deaths from current smog
/// * `financing_available` - Whether the municipality can finance the CAPEX
pub fn run_municipal_heating_ai(
    thermal_grid: &ThermalGridState,
    district_heating_demand: usize,
    effective_heat_supply: f64,
    heat_demand_gj: f64,
    available_plant_types: &[PlantTypeCostData],
    average_wage: f64,
    mortality_cost_per_death: f64,
    estimated_deaths_from_smog: f64,
    financing_available: bool,
) -> HeatingInvestmentPlan {
    let mut plan = HeatingInvestmentPlan::default();
    let mut rationales: Vec<String> = Vec::new();

    // Step 1: Pipe capacity check
    let max_connectable = thermal_grid.max_connectable_buildings(0.8); // Assume urban
    if max_connectable < district_heating_demand {
        plan.pipe_expansion_km = 5.0;
        plan.estimated_capex += 5.0 * average_wage * 1000.0; // 5km * 1000 wages/km
        rationales.push(format!(
            "Pipe capacity check: max_connectable={} < demand={}, expanding pipes by 5 km",
            max_connectable, district_heating_demand
        ));
    }

    // Step 2: Heat supply check
    if effective_heat_supply < heat_demand_gj {
        rationales.push(format!(
            "Heat supply check: effective_supply={:.1} < demand={:.1}, planning new plant",
            effective_heat_supply, heat_demand_gj
        ));

        // Step 3: Plant type selection — lowest total OPEX/GJ
        let best_plant = available_plant_types
            .iter()
            .filter(|p| p.tech_unlocked && p.geologically_eligible)
            .min_by(|a, b| {
                let a_opex = a.fuel_opex_per_gj + a.maintenance_opex_per_gj;
                let b_opex = b.fuel_opex_per_gj + b.maintenance_opex_per_gj;
                a_opex
                    .partial_cmp(&b_opex)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

        if let Some(plant) = best_plant {
            plan.new_plant_type = Some(plant.plant_type);
            let plant_capex = plant.capex_per_gj * 50.0 * average_wage * 100.0; // 50 GJ plant
            plan.estimated_capex += plant_capex;
            rationales.push(format!(
                "Plant selection: {:?} (OPEX={:.2}/GJ, CAPEX={:.0})",
                plant.plant_type,
                plant.fuel_opex_per_gj + plant.maintenance_opex_per_gj,
                plant_capex
            ));
        } else {
            rationales.push("Plant selection: no eligible plant type available".to_string());
        }
    }

    // Step 4: Cost-benefit gate
    // Expected mortality reduction: assume 30% of smog deaths preventable
    let expected_deaths_prevented = estimated_deaths_from_smog * 0.3;
    plan.expected_mortality_reduction_value = expected_deaths_prevented * mortality_cost_per_death;

    plan.passes_cost_benefit_gate = financing_available
        && plan.estimated_capex > 0.0
        && plan.expected_mortality_reduction_value > plan.estimated_capex;

    if !financing_available {
        rationales.push("Cost-benefit gate: financing not available, plan rejected".to_string());
    } else if plan.estimated_capex == 0.0 {
        rationales.push("Cost-benefit gate: no investment needed".to_string());
    } else if !plan.passes_cost_benefit_gate {
        rationales.push(format!(
            "Cost-benefit gate: mortality_value={:.0} < capex={:.0}, plan rejected",
            plan.expected_mortality_reduction_value, plan.estimated_capex
        ));
    } else {
        rationales.push(format!(
            "Cost-benefit gate: mortality_value={:.0} > capex={:.0}, plan approved",
            plan.expected_mortality_reduction_value, plan.estimated_capex
        ));
    }

    plan.rationale = rationales.join("; ");
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_plant_costs() -> Vec<PlantTypeCostData> {
        vec![
            PlantTypeCostData {
                plant_type: HeatingPlantType::WoodBoiler,
                fuel_opex_per_gj: 3.0,
                maintenance_opex_per_gj: 0.5,
                capex_per_gj: 10.0,
                tech_unlocked: true,
                geologically_eligible: true,
            },
            PlantTypeCostData {
                plant_type: HeatingPlantType::CoalHeatPlant,
                fuel_opex_per_gj: 2.0,
                maintenance_opex_per_gj: 0.5,
                capex_per_gj: 15.0,
                tech_unlocked: true,
                geologically_eligible: true,
            },
            PlantTypeCostData {
                plant_type: HeatingPlantType::GeothermalHeatPlant,
                fuel_opex_per_gj: 0.1,
                maintenance_opex_per_gj: 0.3,
                capex_per_gj: 50.0,
                tech_unlocked: true,
                geologically_eligible: false, // No geothermal trait
            },
        ]
    }

    #[test]
    fn test_no_investment_needed() {
        let grid = ThermalGridState {
            pipe_network_km: 100.0,
            pipe_condition: 1.0,
            loss_per_km: 0.02,
        };
        let plan = run_municipal_heating_ai(
            &grid,
            100,   // demand
            500.0, // supply > demand
            400.0, // heat demand
            &test_plant_costs(),
            10.0,
            10000.0,
            0.0,
            true,
        );
        assert_eq!(plan.pipe_expansion_km, 0.0);
        assert_eq!(plan.new_plant_type, None);
        assert!(!plan.passes_cost_benefit_gate);
    }

    #[test]
    fn test_pipe_expansion_when_capacity_insufficient() {
        let grid = ThermalGridState {
            pipe_network_km: 5.0, // Very small network
            pipe_condition: 1.0,
            loss_per_km: 0.02,
        };
        let plan = run_municipal_heating_ai(
            &grid,
            500,   // demand >> capacity
            100.0, // supply
            100.0, // heat demand
            &test_plant_costs(),
            10.0,
            10000.0,
            0.0,
            true,
        );
        assert_eq!(plan.pipe_expansion_km, 5.0);
    }

    #[test]
    fn test_plant_selection_lowest_opex() {
        let grid = ThermalGridState {
            pipe_network_km: 100.0,
            pipe_condition: 1.0,
            loss_per_km: 0.02,
        };
        let plan = run_municipal_heating_ai(
            &grid,
            100,
            100.0, // supply < demand
            500.0, // heat demand > supply
            &test_plant_costs(),
            10.0,
            100000.0,
            10.0,
            true,
        );
        // Coal has lowest OPEX (2.0 + 0.5 = 2.5) among eligible plants
        assert_eq!(plan.new_plant_type, Some(HeatingPlantType::CoalHeatPlant));
    }

    #[test]
    fn test_geothermal_excluded_without_geological_trait() {
        let grid = ThermalGridState {
            pipe_network_km: 100.0,
            pipe_condition: 1.0,
            loss_per_km: 0.02,
        };
        let plan = run_municipal_heating_ai(
            &grid,
            100,
            100.0,
            500.0,
            &test_plant_costs(),
            10.0,
            100000.0,
            10.0,
            true,
        );
        // Geothermal has lowest OPEX but is geologically ineligible
        assert_ne!(
            plan.new_plant_type,
            Some(HeatingPlantType::GeothermalHeatPlant)
        );
    }

    #[test]
    fn test_cost_benefit_gate_rejects_low_mortality() {
        let grid = ThermalGridState {
            pipe_network_km: 5.0,
            pipe_condition: 1.0,
            loss_per_km: 0.02,
        };
        let plan = run_municipal_heating_ai(
            &grid,
            500,
            100.0,
            500.0,
            &test_plant_costs(),
            10.0,
            100.0, // Low mortality cost
            1.0,   // Few deaths
            true,
        );
        assert!(!plan.passes_cost_benefit_gate);
    }

    #[test]
    fn test_financing_not_available_rejects() {
        let grid = ThermalGridState {
            pipe_network_km: 5.0,
            pipe_condition: 1.0,
            loss_per_km: 0.02,
        };
        let plan = run_municipal_heating_ai(
            &grid,
            500,
            100.0,
            500.0,
            &test_plant_costs(),
            10.0,
            1000000.0,
            100.0,
            false, // No financing
        );
        assert!(!plan.passes_cost_benefit_gate);
    }

    #[test]
    fn test_heat_sales_history_rolling_average() {
        let mut h = HeatSalesHistory::default();
        h.record(100.0);
        h.record(200.0);
        h.record(300.0);
        assert!((h.rolling_average() - 200.0).abs() < 1e-9);
    }

    #[test]
    fn test_heat_sales_history_window_limit() {
        let mut h = HeatSalesHistory {
            window_size: 3,
            ..Default::default()
        };
        h.record(100.0);
        h.record(200.0);
        h.record(300.0);
        h.record(400.0); // Should push out 100.0
        assert_eq!(h.sales_history.len(), 3);
        assert!((h.rolling_average() - 300.0).abs() < 1e-9);
    }

    #[test]
    fn test_heat_sales_history_empty_average() {
        let h = HeatSalesHistory::default();
        assert_eq!(h.rolling_average(), 0.0);
    }
}

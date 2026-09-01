//! Phase 82: Thermal grid infrastructure — the anti-grid.
//!
//! Unlike electricity, heat suffers from catastrophic transmission losses over
//! distance. There is no inter-regional "High Voltage" equivalent for heat.
//! Each region has its own insulated pipe network with aggressive transmission
//! losses, making district heating a strictly intra-regional (urban-only)
//! utility.
//!
//! ## Key Physics
//!
//! - **Radial delivery distance** (CORRECTION 1): Heat travels from plant to
//!   home (the radius), not through every pipe sequentially. A branching
//!   network's average delivery distance is approximated by:
//!   `average_delivery_distance_km = (pipe_network_km / active_plants).sqrt() * 1.5`
//! - **Transmission loss**: `1.0 - (1.0 - loss_per_km).powf(avg_distance)`
//! - **Connectable buildings**: Scales with pipe length and urban density.

use crate::society::geography::Region;
use serde::{Deserialize, Serialize};

/// Thermal grid state for a region — the intra-regional pipe network.
///
/// Heat cannot be transmitted inter-regionally. Each region has its own
/// insulated pipe network with aggressive transmission losses. The pipe
/// network determines how many buildings can adopt District Heating and
/// how much heat is lost in transmission.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThermalGridState {
    /// Total pipe network length (km). Constructed by municipal CAPEX.
    /// Determines maximum connectable building count.
    #[serde(default)]
    pub pipe_network_km: f64,

    /// Pipe condition (0.0 = collapsed, 1.0 = pristine). Degrades per turn.
    /// Multiplies effective heat supply — damaged pipes leak heat.
    #[serde(default = "default_pipe_condition")]
    pub pipe_condition: f64,

    /// Transmission loss per km (fraction/km). PHYSICAL CONSTANT:
    /// 0.02 = 2% heat dissipation per km through insulated steel pipe.
    /// Derived from steady-state heat conduction through insulation
    /// (k=0.04 W/m·K for mineral wool, pipe diameter ~200mm, ΔT=80K).
    #[serde(default = "default_loss_per_km")]
    pub loss_per_km: f64,
}

impl Default for ThermalGridState {
    fn default() -> Self {
        Self {
            pipe_network_km: 0.0,
            pipe_condition: default_pipe_condition(),
            loss_per_km: default_loss_per_km(),
        }
    }
}

fn default_pipe_condition() -> f64 {
    1.0
}

fn default_loss_per_km() -> f64 {
    0.02
}

impl ThermalGridState {
    /// Compute the average delivery distance for heat in this region.
    ///
    /// CORRECTION 1 (Topological Physics): Heat travels from plant to home
    /// (the radial distance), NOT through every pipe sequentially. A branching
    /// tree network of total length L with N sources has average path length
    /// ~√(L/N) × branching factor (1.5 accounts for non-straight routing).
    ///
    /// A 500km city network with 5 plants → avg distance ~10.6 km, not 500 km.
    pub fn average_delivery_distance_km(&self, active_heating_plants: usize) -> f64 {
        let plants = active_heating_plants.max(1) as f64;
        (self.pipe_network_km / plants).sqrt() * 1.5
    }

    /// Compute transmission loss fraction (0.0 = no loss, 1.0 = total loss).
    ///
    /// Uses the radial delivery distance, not total pipe length.
    /// At avg 5 km: ~9.5% loss. At avg 20 km: ~33% loss. At avg 50 km: ~64%.
    pub fn transmission_loss(&self, active_heating_plants: usize) -> f64 {
        if self.pipe_network_km <= 0.0 {
            return 1.0; // No pipes = total loss (no delivery possible)
        }
        let avg_distance = self.average_delivery_distance_km(active_heating_plants);
        1.0 - (1.0 - self.loss_per_km).powf(avg_distance)
    }

    /// Compute effective heat supply after transmission losses and pipe condition.
    ///
    /// `effective_heat = raw_heat * (1.0 - transmission_loss) * pipe_condition`
    pub fn effective_heat_supply(
        &self,
        raw_heat_supply_gj: f64,
        active_heating_plants: usize,
    ) -> f64 {
        let loss = self.transmission_loss(active_heating_plants);
        raw_heat_supply_gj * (1.0 - loss) * self.pipe_condition
    }

    /// Compute maximum connectable buildings based on pipe network and urban density.
    ///
    /// `max_connectable = pipe_network_km * (5.0 + development_level * 15.0)`
    /// Rural (dev=0.1): ~6.5 buildings/km. Urban (dev=0.8): ~17 buildings/km.
    pub fn max_connectable_buildings(&self, development_level: f64) -> usize {
        if self.pipe_network_km <= 0.0 {
            return 0;
        }
        let buildings_per_km = 5.0 + development_level * 15.0;
        (self.pipe_network_km * buildings_per_km) as usize
    }

    /// Degrade pipe condition by one turn.
    ///
    /// `degradation_rate = 0.002 * (1.0 + winter_severity_factor)`
    /// Physical constant: 0.2% per turn baseline, faster in harsh winters.
    pub fn degrade(&mut self, winter_severity_factor: f64) {
        let degradation = 0.002 * (1.0 + winter_severity_factor);
        self.pipe_condition = (self.pipe_condition - degradation).max(0.0);
    }
}

/// Compute the average delivery distance for a region's thermal grid.
/// Convenience function that takes a Region reference.
pub fn average_delivery_distance(region: &Region, active_heating_plants: usize) -> f64 {
    region
        .thermal_grid
        .average_delivery_distance_km(active_heating_plants)
}

/// Compute transmission loss fraction for a region's thermal grid.
pub fn transmission_loss(region: &Region, active_heating_plants: usize) -> f64 {
    region.thermal_grid.transmission_loss(active_heating_plants)
}

/// Compute effective heat supply for a region after transmission losses.
pub fn effective_heat_supply(
    region: &Region,
    raw_heat_supply_gj: f64,
    active_heating_plants: usize,
) -> f64 {
    region
        .thermal_grid
        .effective_heat_supply(raw_heat_supply_gj, active_heating_plants)
}

/// Compute maximum connectable buildings for a region.
pub fn max_connectable_buildings(region: &Region) -> usize {
    region
        .thermal_grid
        .max_connectable_buildings(region.development_level)
}

/// Compute the regulated cost-plus heat price for a municipal heating utility.
///
/// CORRECTION 5 (Amortization + Smoothing): Heat is a natural monopoly — it
/// cannot be traded on the open market. The price is set by a cost-plus formula
/// that ensures the utility recovers BOTH operating costs AND amortized capital
/// investment, while preventing monopoly price-gouging and seasonal death spirals.
///
/// # Formula
/// `heat_price = (fuel_opex + maintenance_opex + labor_opex + amortized_capex)
///               / smoothed_heat_sales * cost_plus_margin`
///
/// # Amortized CAPEX
/// The municipal utility's total asset value (pipes + plant) is amortized
/// over a 40-year lifespan. With 4 turns/year, that's 160 turns.
/// `amortized_capex_per_turn = total_asset_value / 160.0`
///
/// # Smoothed Heat Sales
/// Heat demand plummets in summer. Using per-turn sales would cause the price
/// to skyrocket in July. A 24-turn rolling average ensures stable pricing.
pub fn compute_regulated_heat_price(
    fuel_opex: f64,
    maintenance_opex: f64,
    labor_opex: f64,
    total_asset_value: f64,
    amortization_turns: f64,
    smoothed_heat_sales_gj: f64,
    cost_plus_margin: f64,
    average_wage: f64,
) -> f64 {
    let amortized_capex = if amortization_turns > 0.0 {
        total_asset_value / amortization_turns
    } else {
        0.0
    };
    if smoothed_heat_sales_gj > 0.0 {
        let total_cost = fuel_opex + maintenance_opex + labor_opex + amortized_capex;
        (total_cost / smoothed_heat_sales_gj) * cost_plus_margin
    } else {
        // Fallback: no sales history yet. Use wage-anchored price.
        average_wage * 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_thermal_grid_state() {
        let grid = ThermalGridState::default();
        assert_eq!(grid.pipe_network_km, 0.0);
        assert_eq!(grid.pipe_condition, 1.0);
        assert_eq!(grid.loss_per_km, 0.02);
    }

    #[test]
    fn test_no_pipes_total_loss() {
        let grid = ThermalGridState::default();
        assert_eq!(grid.transmission_loss(1), 1.0);
    }

    #[test]
    fn test_average_delivery_distance() {
        let grid = ThermalGridState {
            pipe_network_km: 500.0,
            pipe_condition: 1.0,
            loss_per_km: 0.02,
        };
        // 500 km / 5 plants = 100, sqrt(100) = 10, * 1.5 = 15.0
        let dist = grid.average_delivery_distance_km(5);
        assert!((dist - 15.0).abs() < 0.01);
    }

    #[test]
    fn test_transmission_loss_at_various_distances() {
        let grid = ThermalGridState {
            pipe_network_km: 100.0,
            pipe_condition: 1.0,
            loss_per_km: 0.02,
        };
        // With 1 plant: avg_distance = sqrt(100) * 1.5 = 15.0 km
        let loss = grid.transmission_loss(1);
        // (1 - 0.02)^15 = 0.98^15 ≈ 0.7386, so loss ≈ 0.2614
        assert!((loss - 0.2614).abs() < 0.01);

        // With 4 plants: avg_distance = sqrt(25) * 1.5 = 7.5 km
        let loss2 = grid.transmission_loss(4);
        // 0.98^7.5 ≈ 0.8607, so loss ≈ 0.1393
        assert!((loss2 - 0.1393).abs() < 0.01);
    }

    #[test]
    fn test_max_connectable_buildings_rural() {
        let grid = ThermalGridState {
            pipe_network_km: 10.0,
            pipe_condition: 1.0,
            loss_per_km: 0.02,
        };
        // Rural: dev=0.1 → buildings_per_km = 5 + 0.1*15 = 6.5
        let max = grid.max_connectable_buildings(0.1);
        assert_eq!(max, 65);
    }

    #[test]
    fn test_max_connectable_buildings_urban() {
        let grid = ThermalGridState {
            pipe_network_km: 10.0,
            pipe_condition: 1.0,
            loss_per_km: 0.02,
        };
        // Urban: dev=0.8 → buildings_per_km = 5 + 0.8*15 = 17.0
        let max = grid.max_connectable_buildings(0.8);
        assert_eq!(max, 170);
    }

    #[test]
    fn test_effective_heat_supply() {
        let grid = ThermalGridState {
            pipe_network_km: 50.0,
            pipe_condition: 0.9,
            loss_per_km: 0.02,
        };
        // 1 plant: avg_distance = sqrt(50)*1.5 ≈ 10.607
        // loss = 1 - 0.98^10.607 ≈ 1 - 0.8062 ≈ 0.1938
        // effective = 100.0 * (1 - 0.1938) * 0.9 ≈ 72.56
        let effective = grid.effective_heat_supply(100.0, 1);
        assert!((effective - 72.56).abs() < 1.0);
    }

    #[test]
    fn test_pipe_degradation() {
        let mut grid = ThermalGridState {
            pipe_network_km: 10.0,
            pipe_condition: 1.0,
            loss_per_km: 0.02,
        };
        grid.degrade(0.0); // No winter severity
                           // degradation = 0.002 * 1.0 = 0.002
        assert!((grid.pipe_condition - 0.998).abs() < 1e-9);

        grid.degrade(2.0); // Harsh winter
                           // degradation = 0.002 * 3.0 = 0.006
        assert!((grid.pipe_condition - 0.992).abs() < 1e-9);
    }

    #[test]
    fn test_regulated_heat_price_normal() {
        let price = compute_regulated_heat_price(
            1000.0,  // fuel_opex
            200.0,   // maintenance_opex
            300.0,   // labor_opex
            50000.0, // total_asset_value
            160.0,   // amortization_turns (40 years × 4 turns/yr)
            50.0,    // smoothed_heat_sales_gj
            1.10,    // cost_plus_margin
            10.0,    // average_wage
        );
        // amortized_capex = 50000 / 160 = 312.5
        // total_cost = 1000 + 200 + 300 + 312.5 = 1812.5
        // price = 1812.5 / 50 * 1.10 = 39.875
        assert!((price - 39.875).abs() < 0.01);
    }

    #[test]
    fn test_regulated_heat_price_no_sales_fallback() {
        let price =
            compute_regulated_heat_price(1000.0, 200.0, 300.0, 50000.0, 160.0, 0.0, 1.10, 10.0);
        // Fallback: average_wage * 0.5 = 5.0
        assert!((price - 5.0).abs() < 1e-9);
    }
}

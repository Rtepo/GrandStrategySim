//! Phase 81: The Energy Epic — Physical Grid Infrastructure.
//!
//! This module implements a three-tier (LV/MV/HV) region-level electricity grid
//! with specialized power plant types, weather-coupled generation, tiered load
//! shedding for deficits, and overproduction/curtailment mechanics for surpluses.
//!
//! ## Architecture
//!
//! - **HV (High Voltage)**: Inter-regional transmission lines with explicit
//!   topology, transmission losses, and DC flow balancing. Lines are stored in
//!   `PowerGridState.hv_lines`.
//! - **MV (Medium Voltage)**: Abstracted as a regional capacity limit
//!   (`PowerGridState.region_mv_capacity`). No graph algorithm — just a
//!   mathematical cap on how much power can be distributed within a region.
//! - **LV (Low Voltage)**: Abstracted as a regional capacity limit
//!   (`PowerGridState.region_lv_capacity`). The final distribution bottleneck
//!   before consumers.
//!
//! ## Turn Integration
//!
//! The grid distribution runs in the sequential gap between Wave 1 (energy
//! production) and Wave 3 (general production) in the turn loop. It produces
//! `building_efficiency_penalties` (load shedding = positive, industrial buff =
//! negative) that are passed to Wave 3 via the existing `task_penalties`
//! mechanism.

#![allow(missing_docs)]

pub mod chp;
pub mod generation;
pub mod grid;
pub mod heating_types;
pub mod load_shedding;
pub mod municipal_heating_ai;
pub mod municipal_infrastructure_ai;
pub mod ppa;
pub mod thermal_grid;
pub mod types;

pub use heating_types::{ChpRetrofitMetadata, HeatingPlantMetadata, HeatingPlantType};
pub use municipal_heating_ai::{
    run_municipal_heating_ai, HeatSalesHistory, HeatingInvestmentPlan, PlantTypeCostData,
};
pub use thermal_grid::{
    average_delivery_distance, compute_regulated_heat_price, effective_heat_supply,
    max_connectable_buildings, transmission_loss, ThermalGridState,
};
pub use types::*;

//! Capacity-Based Infrastructure Model
//!
//! This module implements the capacity-based infrastructure system for public services,
//! healthcare, education, and care facilities. Buildings generate "Capacity" (beds/seats per turn)
//! instead of tradable commodities.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Capacity type for infrastructure buildings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum CapacityType {
    /// Acute care beds
    HospitalBeds,
    /// Outpatient visits per turn
    ClinicVisits,
    /// Rehabilitation capacity
    RehabSlots,
    /// Preventative care stays
    SanatoriumStays,
    /// 24/7 care home capacity (Social Care Home)
    DPSCapacity,
    /// Daycare capacity (Dom Dziennego Pobytu)
    DDPCapacity,
    /// Childcare seats (0-3 years)
    NurserySeats,
    /// Primary school seats
    PrimarySeats,
    /// Middle school seats
    MiddleSeats,
    /// High school seats
    HighSchoolSeats,
    /// University enrollment slots
    UniversitySlots,
    /// Monastic housing
    MonasteryCells,
    /// Worship capacity
    TempleCapacity,
    /// Cultural events per turn
    CulturalEventCapacity,
    /// Surface water supply (liters per turn) - drawn from rivers/lakes, vulnerable to sewage pollution
    SurfaceWaterSupply,
    /// Groundwater supply (liters per turn) - drawn via underground pumps, immune to surface sewage but higher cost
    GroundwaterSupply,
    /// Sewage treatment capacity (liters per turn)
    SewageTreatment,
    /// District heating capacity (GJ per turn)
    DistrictHeating,
    /// Electricity supply (kWh per turn)
    ElectricitySupply,
    /// Landfill capacity (tons per turn) - modular waste management
    LandfillCapacity,
}

/// Per-turn capacity generation by an infrastructure building
/// This replaces commodity output for infrastructure companies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapacityOutput {
    /// Type of capacity generated

    pub capacity_type: CapacityType,

    /// Base capacity per turn

    pub base_capacity: f64,

    /// Capacity per worker (efficiency multiplier)

    pub capacity_per_worker: f64,

    /// Current utilization (0.0-1.0)
    #[serde(default)]
    pub utilization: f64,
}

pub mod healthcare;
pub mod care;
pub mod education;
pub mod cultural;
pub mod effects;
pub mod pricing;
pub mod maritime;
pub mod building_condition;
pub mod heritage;

pub use maritime::{
    Shipyard, Port, Dock, ShipType, ShipConstructionProject,
    MaritimeConfig, MaritimeInfrastructure,
    submit_shipyard_construction_orders, advance_shipyard_projects,
    refund_unfilled_shipyard_bids, total_port_throughput,
    process_ports_turn, process_shipyard_maintenance,
};
pub use building_condition::{
    calculate_renovation_bom, calculate_maintenance_bom, calculate_opex_multiplier,
    calculate_degradation_rate, RenovationError, RenovationResult,
    BuildingConditionConfig,
};
pub use cultural::{
    CulturalBuilding, CulturalReliefConfig, VoluntaryDonationRates,
    EndowmentDonationRates, CulturalBuildingType, CulturalFunding, CulturalTemplate,
    collect_cultural_donations, distribute_cash_relief, submit_relief_b2b_orders,
    refund_unfilled_cultural_bids, deliver_relief_goods,
};
pub use heritage::{
    HeritageBuilding, HeritageError, check_heritage_eligibility, apply_heritage_effects,
    can_demolish, can_upgrade_technology, apply_heritage_subsidy, Market,
    process_heritage_effects,
};

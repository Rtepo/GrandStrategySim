//! Society and cultural systems used during world generation and later
//! social-simulation phases.

#![allow(missing_docs)]

pub mod cadastre;
pub mod charities;
pub mod culture_registry;
pub mod cultures;
pub mod disasters;
pub mod factional_domains;
pub mod geography;
pub mod geography_config;
pub mod housing;
pub mod planet;
pub mod real_estate_market;
pub mod religious_authority;
pub mod tourism;
pub mod urbanization;

pub use housing::{
    CommercialBuilding, CommercialBuildingType, CommercialInventory, HousingBuilding,
    HousingInventory, HousingSlots, HousingType, UtilityConnections,
};
pub use tourism::{
    compute_tourism_demand, create_natural_wonder, settle_tourism_revenue, DestinationSettlement,
    NaturalWonder, TourismDemandResult, TourismDestination, WonderType,
};

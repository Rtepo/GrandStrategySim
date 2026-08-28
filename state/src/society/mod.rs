//! Society and cultural systems used during world generation and later
//! social-simulation phases.

#![allow(missing_docs)]

pub mod cultures;
pub mod culture_registry;
pub mod religious_authority;
pub mod geography;
pub mod geography_config;
pub mod tourism;
pub mod housing;
pub mod charities;
pub mod cadastre;
pub mod real_estate_market;
pub mod disasters;
pub mod factional_domains;
pub mod urbanization;

pub use tourism::{NaturalWonder, WonderType, TourismDestination, TourismIndustry, TourismTurnResult, create_natural_wonder, create_tourism_destination, process_tourism_turn};
pub use housing::{HousingType, HousingBuilding, HousingSlots, CommercialBuilding, CommercialBuildingType, UtilityConnections, HousingInventory, CommercialInventory};

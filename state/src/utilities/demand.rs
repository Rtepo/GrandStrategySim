//! Utility demand calculations for buildings and housing
//!
//! This module implements utility demand calculations per building type,
//! with different footprints for various housing qualities (Huts, Slums, Tenements, etc.).

use serde::{Deserialize, Serialize};

use crate::society::housing::{HousingBuilding, HousingType, CommercialBuilding, CommercialBuildingType};
use crate::state::Season;

/// Utility demand for a building
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct UtilityDemand {
    /// Surface water demand (liters per turn) - from rivers/lakes
    #[serde(default)]
    pub surface_water_demand: f64,
    
    /// Groundwater demand (liters per turn) - from wells/pumps
    #[serde(default)]
    pub groundwater_demand: f64,
    
    /// Sewage generation (liters per turn)
    #[serde(default)]
    pub sewage_generation: f64,
    
    /// Heating demand (GJ per turn)
    #[serde(default)]
    pub heating_demand: f64,
    
    /// Electricity demand (kWh per turn)
    #[serde(default)]
    pub electricity_demand: f64,
    
    /// Waste generation (tons per turn)
    #[serde(default)]
    pub waste_generation: f64,
    
    /// Recyclable waste fraction (0-1)
    #[serde(default)]
    pub recyclable_fraction: f64,
}

impl UtilityDemand {
    /// Calculate demand for a housing building
    ///
    /// # Arguments
    /// * `building` - The housing building
    /// * `season` - Current season for seasonal variations
    ///
    /// # Returns
    /// * Utility demand for the building
    pub fn for_housing(building: &HousingBuilding, season: Season) -> Self {
        let occupied_slots = building.primary_slots.occupied_slots as f64;
        let sublet_occupied = building.sublet_slots.as_ref()
            .map(|s| s.occupied_slots as f64)
            .unwrap_or(0.0);
        let total_occupied = occupied_slots + sublet_occupied;
        
        let base_demand = match building.housing_type {
            HousingType::Hut => UtilityDemand {
                surface_water_demand: 100.0 * total_occupied,
                groundwater_demand: 0.0,
                sewage_generation: 80.0 * total_occupied,
                heating_demand: if season == Season::Winter { 5.0 * total_occupied } else { 0.0 },
                electricity_demand: 10.0 * total_occupied,
                waste_generation: 0.5 * total_occupied,
                recyclable_fraction: 0.1, // Low recycling in rural huts
            },
            HousingType::Slum => UtilityDemand {
                surface_water_demand: 50.0 * total_occupied, // Limited access
                groundwater_demand: 0.0,
                sewage_generation: 40.0 * total_occupied, // Often untreated
                heating_demand: if season == Season::Winter { 2.0 * total_occupied } else { 0.0 }, // Poor insulation
                electricity_demand: 5.0 * total_occupied,
                waste_generation: 0.3 * total_occupied,
                recyclable_fraction: 0.05, // Very low recycling in slums
            },
            HousingType::Familok => UtilityDemand {
                surface_water_demand: 150.0 * total_occupied,
                groundwater_demand: 0.0,
                sewage_generation: 120.0 * total_occupied,
                heating_demand: if season == Season::Winter { 8.0 * total_occupied } else { 0.0 },
                electricity_demand: 20.0 * total_occupied,
                waste_generation: 0.8 * total_occupied,
                recyclable_fraction: 0.2,
            },
            HousingType::Beamciok => UtilityDemand {
                surface_water_demand: 200.0 * total_occupied,
                groundwater_demand: 0.0,
                sewage_generation: 160.0 * total_occupied,
                heating_demand: if season == Season::Winter { 10.0 * total_occupied } else { 0.0 },
                electricity_demand: 30.0 * total_occupied,
                waste_generation: 1.0 * total_occupied,
                recyclable_fraction: 0.3,
            },
            HousingType::Tenement => UtilityDemand {
                surface_water_demand: 180.0 * total_occupied,
                groundwater_demand: 50.0 * total_occupied, // Some groundwater backup
                sewage_generation: 150.0 * total_occupied,
                heating_demand: if season == Season::Winter { 12.0 * total_occupied } else { 0.0 },
                electricity_demand: 25.0 * total_occupied,
                waste_generation: 0.9 * total_occupied,
                recyclable_fraction: 0.25,
            },
            HousingType::CityPalace => UtilityDemand {
                surface_water_demand: 500.0 * total_occupied,
                groundwater_demand: 200.0 * total_occupied,
                sewage_generation: 400.0 * total_occupied,
                heating_demand: if season == Season::Winter { 30.0 * total_occupied } else { 5.0 * total_occupied },
                electricity_demand: 100.0 * total_occupied,
                waste_generation: 3.0 * total_occupied,
                recyclable_fraction: 0.4,
            },
            HousingType::Palace => UtilityDemand {
                surface_water_demand: 400.0 * total_occupied,
                groundwater_demand: 150.0 * total_occupied,
                sewage_generation: 350.0 * total_occupied,
                heating_demand: if season == Season::Winter { 25.0 * total_occupied } else { 4.0 * total_occupied },
                electricity_demand: 80.0 * total_occupied,
                waste_generation: 2.5 * total_occupied,
                recyclable_fraction: 0.35,
            },
            HousingType::Rectory => UtilityDemand {
                surface_water_demand: 250.0 * total_occupied,
                groundwater_demand: 100.0 * total_occupied,
                sewage_generation: 200.0 * total_occupied,
                heating_demand: if season == Season::Winter { 15.0 * total_occupied } else { 3.0 * total_occupied },
                electricity_demand: 40.0 * total_occupied,
                waste_generation: 1.2 * total_occupied,
                recyclable_fraction: 0.3,
            },
            HousingType::Monastery => UtilityDemand {
                surface_water_demand: 300.0 * total_occupied,
                groundwater_demand: 100.0 * total_occupied,
                sewage_generation: 250.0 * total_occupied,
                heating_demand: if season == Season::Winter { 18.0 * total_occupied } else { 4.0 * total_occupied },
                electricity_demand: 50.0 * total_occupied,
                waste_generation: 1.5 * total_occupied,
                recyclable_fraction: 0.35,
            },
            HousingType::SocialHousing => UtilityDemand {
                surface_water_demand: 120.0 * total_occupied,
                groundwater_demand: 30.0 * total_occupied,
                sewage_generation: 100.0 * total_occupied,
                heating_demand: if season == Season::Winter { 7.0 * total_occupied } else { 0.0 },
                electricity_demand: 15.0 * total_occupied,
                waste_generation: 0.6 * total_occupied,
                recyclable_fraction: 0.2,
            },
            HousingType::FolwarkHousing => UtilityDemand {
                surface_water_demand: 80.0 * total_occupied,
                groundwater_demand: 20.0 * total_occupied,
                sewage_generation: 60.0 * total_occupied,
                heating_demand: if season == Season::Winter { 4.0 * total_occupied } else { 0.0 },
                electricity_demand: 8.0 * total_occupied,
                waste_generation: 0.4 * total_occupied,
                recyclable_fraction: 0.15,
            },
        };
        
        // Apply utility connections if available
        let connections = &building.utility_connections;
        
        
        UtilityDemand {
            surface_water_demand: base_demand.surface_water_demand.min(connections.surface_water_capacity),
            groundwater_demand: base_demand.groundwater_demand.min(connections.groundwater_capacity),
            sewage_generation: base_demand.sewage_generation,
            heating_demand: base_demand.heating_demand.min(connections.district_heating_capacity),
            electricity_demand: base_demand.electricity_demand.min(connections.electricity_capacity),
            waste_generation: base_demand.waste_generation,
            recyclable_fraction: base_demand.recyclable_fraction,
        }
    }
    
    /// Calculate demand for a commercial building
    ///
    /// # Arguments
    /// * `building` - The commercial building
    /// * `season` - Current season for seasonal variations
    ///
    /// # Returns
    /// * Utility demand for the building
    pub fn for_commercial(building: &CommercialBuilding, season: Season) -> Self {
        let office_sqm = building.office_capacity;
        let retail_sqm = building.retail_capacity;
        let total_sqm = office_sqm + retail_sqm;
        
        let base_demand = match building.building_type {
            CommercialBuildingType::Office => UtilityDemand {
                surface_water_demand: 50.0 * office_sqm / 100.0,
                groundwater_demand: 20.0 * office_sqm / 100.0,
                sewage_generation: 40.0 * office_sqm / 100.0,
                heating_demand: if season == Season::Winter { 15.0 * office_sqm / 100.0 } else { 2.0 * office_sqm / 100.0 },
                electricity_demand: 40.0 * office_sqm / 100.0,
                waste_generation: 0.8 * office_sqm / 100.0,
                recyclable_fraction: 0.5,
            },
            CommercialBuildingType::Retail => UtilityDemand {
                surface_water_demand: 30.0 * retail_sqm / 100.0,
                groundwater_demand: 10.0 * retail_sqm / 100.0,
                sewage_generation: 25.0 * retail_sqm / 100.0,
                heating_demand: if season == Season::Winter { 12.0 * retail_sqm / 100.0 } else { 1.5 * retail_sqm / 100.0 },
                electricity_demand: 60.0 * retail_sqm / 100.0, // High for lighting/displays
                waste_generation: 1.5 * retail_sqm / 100.0, // High waste from packaging
                recyclable_fraction: 0.6,
            },
            CommercialBuildingType::MixedUse => UtilityDemand {
                surface_water_demand: 40.0 * total_sqm / 100.0,
                groundwater_demand: 15.0 * total_sqm / 100.0,
                sewage_generation: 32.0 * total_sqm / 100.0,
                heating_demand: if season == Season::Winter { 13.0 * total_sqm / 100.0 } else { 1.8 * total_sqm / 100.0 },
                electricity_demand: 50.0 * total_sqm / 100.0,
                waste_generation: 1.2 * total_sqm / 100.0,
                recyclable_fraction: 0.55,
            },
            CommercialBuildingType::Warehouse => UtilityDemand {
                surface_water_demand: 20.0 * total_sqm / 100.0,
                groundwater_demand: 5.0 * total_sqm / 100.0,
                sewage_generation: 15.0 * total_sqm / 100.0,
                heating_demand: if season == Season::Winter { 8.0 * total_sqm / 100.0 } else { 1.0 * total_sqm / 100.0 },
                electricity_demand: 15.0 * total_sqm / 100.0,
                waste_generation: 0.3 * total_sqm / 100.0,
                recyclable_fraction: 0.3,
            },
            // Phase 6.5: Marketplace (open-air, similar to Retail but lower utilities)
            CommercialBuildingType::Marketplace => UtilityDemand {
                surface_water_demand: 25.0 * retail_sqm / 100.0,
                groundwater_demand: 8.0 * retail_sqm / 100.0,
                sewage_generation: 20.0 * retail_sqm / 100.0,
                heating_demand: if season == Season::Winter { 10.0 * retail_sqm / 100.0 } else { 1.0 * retail_sqm / 100.0 },
                electricity_demand: 30.0 * retail_sqm / 100.0,
                waste_generation: 1.2 * retail_sqm / 100.0,
                recyclable_fraction: 0.5,
            },
            // Phase 6.5: Wholesaler (similar to Warehouse with higher logistics demand)
            CommercialBuildingType::Wholesaler => UtilityDemand {
                surface_water_demand: 25.0 * total_sqm / 100.0,
                groundwater_demand: 8.0 * total_sqm / 100.0,
                sewage_generation: 18.0 * total_sqm / 100.0,
                heating_demand: if season == Season::Winter { 10.0 * total_sqm / 100.0 } else { 1.2 * total_sqm / 100.0 },
                electricity_demand: 25.0 * total_sqm / 100.0,
                waste_generation: 0.5 * total_sqm / 100.0,
                recyclable_fraction: 0.4,
            },
            // Phase 6.5: RetailStore (similar to Retail)
            CommercialBuildingType::RetailStore => UtilityDemand {
                surface_water_demand: 30.0 * retail_sqm / 100.0,
                groundwater_demand: 10.0 * retail_sqm / 100.0,
                sewage_generation: 25.0 * retail_sqm / 100.0,
                heating_demand: if season == Season::Winter { 12.0 * retail_sqm / 100.0 } else { 1.5 * retail_sqm / 100.0 },
                electricity_demand: 60.0 * retail_sqm / 100.0,
                waste_generation: 1.5 * retail_sqm / 100.0,
                recyclable_fraction: 0.6,
            },
            // Phase 6.5: supermarket (higher electricity for refrigeration)
            CommercialBuildingType::Supermarket => UtilityDemand {
                surface_water_demand: 35.0 * retail_sqm / 100.0,
                groundwater_demand: 12.0 * retail_sqm / 100.0,
                sewage_generation: 28.0 * retail_sqm / 100.0,
                heating_demand: if season == Season::Winter { 14.0 * retail_sqm / 100.0 } else { 2.0 * retail_sqm / 100.0 },
                electricity_demand: 80.0 * retail_sqm / 100.0, // High for refrigeration
                waste_generation: 2.0 * retail_sqm / 100.0,
                recyclable_fraction: 0.65,
            },
            // Phase 6.5: DepartmentStore (similar to supermarket but larger footprint)
            CommercialBuildingType::DepartmentStore => UtilityDemand {
                surface_water_demand: 35.0 * retail_sqm / 100.0,
                groundwater_demand: 12.0 * retail_sqm / 100.0,
                sewage_generation: 28.0 * retail_sqm / 100.0,
                heating_demand: if season == Season::Winter { 14.0 * retail_sqm / 100.0 } else { 2.0 * retail_sqm / 100.0 },
                electricity_demand: 75.0 * retail_sqm / 100.0,
                waste_generation: 1.8 * retail_sqm / 100.0,
                recyclable_fraction: 0.65,
            },
            // Phase 6.5: ShoppingCenter (enclosed mall, high HVAC and lighting)
            CommercialBuildingType::ShoppingCenter => UtilityDemand {
                surface_water_demand: 40.0 * total_sqm / 100.0,
                groundwater_demand: 15.0 * total_sqm / 100.0,
                sewage_generation: 32.0 * total_sqm / 100.0,
                heating_demand: if season == Season::Winter { 15.0 * total_sqm / 100.0 } else { 2.5 * total_sqm / 100.0 },
                electricity_demand: 70.0 * total_sqm / 100.0,
                waste_generation: 1.6 * total_sqm / 100.0,
                recyclable_fraction: 0.6,
            },
            // Phase 9: Hotel (high water/sewage, moderate electricity)
            CommercialBuildingType::Hotel => UtilityDemand {
                surface_water_demand: 80.0 * total_sqm / 100.0,
                groundwater_demand: 30.0 * total_sqm / 100.0,
                sewage_generation: 70.0 * total_sqm / 100.0,
                heating_demand: if season == Season::Winter { 20.0 * total_sqm / 100.0 } else { 5.0 * total_sqm / 100.0 },
                electricity_demand: 55.0 * total_sqm / 100.0,
                waste_generation: 2.0 * total_sqm / 100.0,
                recyclable_fraction: 0.5,
            },
            // Phase 9: Resort (very high water, high electricity for amenities)
            CommercialBuildingType::Resort => UtilityDemand {
                surface_water_demand: 100.0 * total_sqm / 100.0,
                groundwater_demand: 40.0 * total_sqm / 100.0,
                sewage_generation: 90.0 * total_sqm / 100.0,
                heating_demand: if season == Season::Winter { 18.0 * total_sqm / 100.0 } else { 3.0 * total_sqm / 100.0 },
                electricity_demand: 65.0 * total_sqm / 100.0,
                waste_generation: 2.5 * total_sqm / 100.0,
                recyclable_fraction: 0.45,
            },
            // Phase 9: Restaurant (high water/sewage, high electricity for cooking)
            CommercialBuildingType::Restaurant => UtilityDemand {
                surface_water_demand: 60.0 * retail_sqm / 100.0,
                groundwater_demand: 20.0 * retail_sqm / 100.0,
                sewage_generation: 55.0 * retail_sqm / 100.0,
                heating_demand: if season == Season::Winter { 10.0 * retail_sqm / 100.0 } else { 2.0 * retail_sqm / 100.0 },
                electricity_demand: 80.0 * retail_sqm / 100.0,
                waste_generation: 3.0 * retail_sqm / 100.0,
                recyclable_fraction: 0.55,
            },
            // Phase 9: Casino (very high electricity for lighting/gaming, moderate water)
            CommercialBuildingType::Casino => UtilityDemand {
                surface_water_demand: 50.0 * retail_sqm / 100.0,
                groundwater_demand: 15.0 * retail_sqm / 100.0,
                sewage_generation: 40.0 * retail_sqm / 100.0,
                heating_demand: if season == Season::Winter { 12.0 * retail_sqm / 100.0 } else { 3.0 * retail_sqm / 100.0 },
                electricity_demand: 90.0 * retail_sqm / 100.0,
                waste_generation: 1.8 * retail_sqm / 100.0,
                recyclable_fraction: 0.5,
            },
            // Phase 30: GasStation (moderate utilities, some electricity for pumps/lighting)
            CommercialBuildingType::GasStation => UtilityDemand {
                surface_water_demand: 20.0 * retail_sqm / 100.0,
                groundwater_demand: 5.0 * retail_sqm / 100.0,
                sewage_generation: 15.0 * retail_sqm / 100.0,
                heating_demand: if season == Season::Winter { 8.0 * retail_sqm / 100.0 } else { 1.0 * retail_sqm / 100.0 },
                electricity_demand: 40.0 * retail_sqm / 100.0,
                waste_generation: 0.5 * retail_sqm / 100.0,
                recyclable_fraction: 0.3,
            },
        };
        
        // Apply utility connections if available
        let connections = &building.utility_connections;
        
        
        UtilityDemand {
            surface_water_demand: base_demand.surface_water_demand.min(connections.surface_water_capacity),
            groundwater_demand: base_demand.groundwater_demand.min(connections.groundwater_capacity),
            sewage_generation: base_demand.sewage_generation,
            heating_demand: base_demand.heating_demand.min(connections.district_heating_capacity),
            electricity_demand: base_demand.electricity_demand.min(connections.electricity_capacity),
            waste_generation: base_demand.waste_generation,
            recyclable_fraction: base_demand.recyclable_fraction,
        }
    }
    
    /// Calculate winter mortality multiplier based on heating deficit
    ///
    /// # Arguments
    /// * `heating_deficit` - Heating deficit in GJ
    /// * `housing_quality` - Housing quality 0-1 (0=slum, 1=palace)
    ///
    /// # Returns
    /// * Mortality multiplier (1.0 = baseline, >1.0 = increased mortality)
    pub fn calculate_winter_mortality(heating_deficit: f64, housing_quality: f64) -> f64 {
        if heating_deficit > 0.0 {
            // Mortality increases with heating deficit and poor housing quality
            1.0 + (heating_deficit * 0.5 * (1.0 - housing_quality))
        } else {
            1.0
        }
    }
}


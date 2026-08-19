//! Capacity pricing for private funding models
//!
//! This module calculates market-determined capacity prices based on
//! supply/demand dynamics for private funding models.

use crate::infrastructure::CapacityType;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Market-determined capacity prices for private funding
/// Calculated based on supply/demand dynamics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityPricing {
    /// Price per capacity unit by type
    #[serde(rename = "ceny_pojemności")]
    pub prices: BTreeMap<CapacityType, f64>,
}

/// Calculate capacity prices based on market dynamics
pub fn calculate_capacity_prices(region: &crate::society::geography::Region) -> BTreeMap<CapacityType, f64> {
    let mut prices = BTreeMap::new();

    for (capacity_type, available_capacity) in &region.capacity_pool {
        let demand = match capacity_type {
            CapacityType::HospitalBeds => calculate_healthcare_demand(region).acute,
            CapacityType::ClinicVisits => calculate_healthcare_demand(region).routine,
            CapacityType::PrimarySeats => calculate_education_demand(region).primary,
            CapacityType::HighSchoolSeats => calculate_education_demand(region).high_school,
            CapacityType::UniversitySlots => calculate_education_demand(region).university,
            _ => 0.0,
        };

        // Price based on supply/demand ratio
        let supply_demand_ratio = demand / available_capacity.max(1.0);
        let base_price = match capacity_type {
            CapacityType::HospitalBeds => 500.0,
            CapacityType::ClinicVisits => 50.0,
            CapacityType::PrimarySeats => 30.0,
            CapacityType::UniversitySlots => 2000.0,
            _ => 100.0,
        };

        // Price increases with scarcity (supply < demand)
        let price = base_price * (1.0 + (supply_demand_ratio - 1.0).max(0.0) * 2.0);
        prices.insert(*capacity_type, price);
    }

    prices
}

// Helper functions (stubs for now)

fn calculate_healthcare_demand(_region: &crate::society::geography::Region) -> HealthcareDemand {
    HealthcareDemand {
        acute: 0.0,
        routine: 0.0,
    }
}

fn calculate_education_demand(_region: &crate::society::geography::Region) -> EducationDemand {
    EducationDemand {
        primary: 0.0,
        high_school: 0.0,
        university: 0.0,
    }
}

#[derive(Debug, Clone)]
struct HealthcareDemand {
    acute: f64,
    routine: f64,
}

#[derive(Debug, Clone)]
struct EducationDemand {
    primary: f64,
    high_school: f64,
    university: f64,
}

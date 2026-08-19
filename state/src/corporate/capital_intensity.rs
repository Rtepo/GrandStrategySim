//! Capital intensity registry for sector-specific CAPEX barriers.
//!
//! This module implements dynamic capital requirements based on sector and
//! macro indicators, ensuring barriers scale with inflation and economic development.

use crate::registries::enums::Sector;
use serde::{Deserialize, Serialize};

/// Capital intensity tier for sector classification.
///
/// Different sectors have vastly different capital requirements.
/// Heavy industry requires massive CAPEX, while services can start with minimal capital.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CapitalIntensity {
    /// 10x average_wage (street vendors, services)
    Micro,
    /// 100x average_wage (retail, small workshops)
    Low,
    /// 1,000x average_wage (manufacturing, construction)
    Medium,
    /// 10,000x average_wage (heavy industry, utilities)
    High,
    /// 100,000x average_wage (infrastructure, aerospace)
    Massive,
}

/// Determines the capital intensity tier for a given sector.
///
/// # Arguments
/// * `sector` - The sector to classify
///
/// # Returns
/// The appropriate `CapitalIntensity` tier for the sector
pub fn sector_capital_intensity(sector: &Sector) -> CapitalIntensity {
    match sector {
        Sector::LocalServices | Sector::ExportServices => CapitalIntensity::Low,
        Sector::LightIndustry => CapitalIntensity::Medium,
        Sector::HeavyIndustry | Sector::ArmamentsIndustry | Sector::Mining => CapitalIntensity::High,
        Sector::Construction => CapitalIntensity::Medium,
        Sector::Agriculture => CapitalIntensity::Low,
        Sector::Energy => CapitalIntensity::Massive,
        Sector::TransportLogistics => CapitalIntensity::High,
        Sector::PublicServices => CapitalIntensity::Medium,
        Sector::MedicalServices => CapitalIntensity::High, // Medical services require significant capital
        Sector::EducationalServices => CapitalIntensity::Medium,
        Sector::PublicAdministration => CapitalIntensity::Medium,
        Sector::Banking => CapitalIntensity::High, // Banking requires significant capital
        Sector::MediaAndEntertainment => CapitalIntensity::Medium, // Media requires moderate capital
        Sector::WasteManagement => CapitalIntensity::High, // Waste management requires significant capital
        Sector::Hospitality => CapitalIntensity::Medium, // Hospitality requires moderate capital
        Sector::NGO => CapitalIntensity::Micro, // NGOs are service entities, minimal capital
        Sector::Religion => CapitalIntensity::Micro, // Religious institutions are service entities
        Sector::MaintenanceWorkshops => CapitalIntensity::Medium, // Phase 19B: repair shops need moderate capital (tools, benches)
        Sector::Government => CapitalIntensity::High, // Phase 32: Parliament building requires significant capital
    }
}

/// Calculates the minimum capital required for a sector based on macro indicators.
///
/// # Arguments
/// * `sector` - The sector to calculate requirements for
/// * `average_wage` - The country's average wage (inflation index)
///
/// # Returns
/// The minimum capital required to enter the sector
///
/// # Rules
/// * Uses dynamic macro indicators instead of hardcoded floats
/// * Ensures barriers scale with inflation, wage growth, and country wealth
pub fn minimum_capital_for_sector(sector: &Sector, average_wage: f64) -> f64 {
    match sector_capital_intensity(sector) {
        CapitalIntensity::Micro => average_wage * 10.0,
        CapitalIntensity::Low => average_wage * 100.0,
        CapitalIntensity::Medium => average_wage * 1_000.0,
        CapitalIntensity::High => average_wage * 10_000.0,
        CapitalIntensity::Massive => average_wage * 100_000.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sector_capital_intensity_services() {
        let intensity = sector_capital_intensity(&Sector::LocalServices);
        assert_eq!(intensity, CapitalIntensity::Low);
    }

    #[test]
    fn test_sector_capital_intensity_heavy_industry() {
        let intensity = sector_capital_intensity(&Sector::HeavyIndustry);
        assert_eq!(intensity, CapitalIntensity::High);
    }

    #[test]
    fn test_sector_capital_intensity_mining() {
        let intensity = sector_capital_intensity(&Sector::Mining);
        assert_eq!(intensity, CapitalIntensity::High);
    }

    #[test]
    fn test_minimum_capital_for_sector_low() {
        let min_cap = minimum_capital_for_sector(&Sector::LocalServices, 10.0);
        assert_eq!(min_cap, 1000.0);  // Low intensity = 100x average_wage
    }

    #[test]
    fn test_minimum_capital_for_sector_massive() {
        let min_cap = minimum_capital_for_sector(&Sector::Energy, 10.0);
        assert_eq!(min_cap, 1_000_000.0);
    }

    #[test]
    fn test_minimum_capital_scales_with_inflation() {
        let min_cap_10 = minimum_capital_for_sector(&Sector::HeavyIndustry, 10.0);
        let min_cap_20 = minimum_capital_for_sector(&Sector::HeavyIndustry, 20.0);
        assert_eq!(min_cap_20, min_cap_10 * 2.0);
    }
}

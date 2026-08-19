//! Building condition system for degradation, OPEX scaling, and renovation.
//!
//! This module implements physical asset lifecycle mechanics including
//! condition degradation, maintenance costs, and market-based renovation.

use crate::corporate::capital_intensity::{CapitalIntensity, sector_capital_intensity};
use crate::registries::enums::{Commodity, Sector};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Configuration for building condition lifecycle (no magic numbers).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BuildingConditionConfig {
    /// Base degradation rate per year (fraction, e.g. 0.01 = 1%)
    pub base_degradation_rate: f64,
    /// Maximum age factor denominator (years to reach full age penalty)
    pub max_age_years: f64,
    /// Condition penalty multiplier for accelerated decay
    pub condition_decay_multiplier: f64,
    /// OPEX slope: OPEX = 1.0 + (1.0 - condition) * slope
    pub opex_slope: f64,
    /// Fraction of condition restored per unit of maintenance BOM fulfilled
    pub maintenance_restoration_rate: f64,
}

impl Default for BuildingConditionConfig {
    fn default() -> Self {
        Self {
            base_degradation_rate: 0.01,
            max_age_years: 100.0,
            condition_decay_multiplier: 0.5,
            opex_slope: 1.0,
            maintenance_restoration_rate: 0.1,
        }
    }
}

/// Error types for renovation operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenovationError {
    /// Insufficient funds for renovation
    InsufficientFunds,
    /// No construction capacity available
    NoConstructionCapacity,
    /// Market clearing failed
    MarketClearingFailed,
}

/// Result of a renovation operation.
#[derive(Debug, Clone, PartialEq)]
pub struct RenovationResult {
    /// Amount of condition restored (0.0 to 1.0)
    pub condition_restored: f64,
    /// Actual investment used
    pub investment_used: f64,
    /// Orders that were cleared by the market
    pub orders_cleared: BTreeMap<Commodity, f64>,
}

/// Bill of Materials for full renovation based on Sector and CapitalIntensity.
///
/// # Arguments
/// * `sector` - The building's sector
///
/// # Returns
/// Physical units required for 100% condition restoration
///
/// # Rules
/// * Physical units are STATIC and based on CapitalIntensity, NOT fiat currency
/// * This prevents unit confusion where inflation would artificially inflate physical requirements
pub fn calculate_renovation_bom(sector: &Sector) -> BTreeMap<Commodity, f64> {
    let mut bom = BTreeMap::new();
    
    let intensity = sector_capital_intensity(sector);
    
    // Static physical requirements based on CapitalIntensity
    match intensity {
        CapitalIntensity::Micro => {
            // Micro: tiny physical amounts (street vendors, services)
            bom.insert(Commodity::RenovationServices, 10.0);
            bom.insert(Commodity::Timber, 5.0);
            bom.insert(Commodity::Bricks, 3.0);
        }
        CapitalIntensity::Low => {
            // Low: small physical amounts (retail, small workshops)
            bom.insert(Commodity::RenovationServices, 50.0);
            bom.insert(Commodity::Timber, 25.0);
            bom.insert(Commodity::Bricks, 15.0);
            bom.insert(Commodity::Cement, 10.0);
        }
        CapitalIntensity::Medium => {
            // Medium: moderate physical amounts (manufacturing, construction)
            bom.insert(Commodity::RenovationServices, 200.0);
            bom.insert(Commodity::Steel, 100.0);
            bom.insert(Commodity::Cement, 80.0);
            bom.insert(Commodity::Bricks, 50.0);
        }
        CapitalIntensity::High => {
            // High: large physical amounts (heavy industry, utilities)
            bom.insert(Commodity::RenovationServices, 500.0);
            bom.insert(Commodity::Steel, 300.0);
            bom.insert(Commodity::Cement, 200.0);
            bom.insert(Commodity::Bricks, 100.0);
        }
        CapitalIntensity::Massive => {
            // Massive: enormous physical amounts (infrastructure, aerospace)
            bom.insert(Commodity::RenovationServices, 1000.0);
            bom.insert(Commodity::Steel, 500.0);
            bom.insert(Commodity::Cement, 300.0);
            bom.insert(Commodity::Bricks, 150.0);
        }
    }
    
    bom
}

/// Bill of Materials for heritage maintenance (high-service, low-material).
///
/// # Arguments
/// * `sector` - The building's sector
///
/// # Returns
/// Physical units required for one maintenance cycle
///
/// # Rules
/// * Physical units are STATIC and based on CapitalIntensity, NOT fiat currency
/// * Maintenance is 90% services, 10% materials (ongoing upkeep)
pub fn calculate_maintenance_bom(sector: &Sector) -> BTreeMap<Commodity, f64> {
    let mut bom = BTreeMap::new();
    
    let intensity = sector_capital_intensity(sector);
    
    // Maintenance is 90% services, 10% materials (ongoing upkeep)
    // Static physical requirements based on CapitalIntensity
    match intensity {
        CapitalIntensity::Micro => {
            // Micro: tiny maintenance needs
            bom.insert(Commodity::RenovationServices, 5.0);
            bom.insert(Commodity::Timber, 0.5);
        }
        CapitalIntensity::Low => {
            // Low: small maintenance needs
            bom.insert(Commodity::RenovationServices, 20.0);
            bom.insert(Commodity::Timber, 2.0);
            bom.insert(Commodity::Bricks, 1.0);
        }
        CapitalIntensity::Medium => {
            // Medium: moderate maintenance needs
            bom.insert(Commodity::RenovationServices, 80.0);
            bom.insert(Commodity::Steel, 5.0);
            bom.insert(Commodity::Cement, 3.0);
        }
        CapitalIntensity::High => {
            // High: large maintenance needs
            bom.insert(Commodity::RenovationServices, 200.0);
            bom.insert(Commodity::Steel, 15.0);
            bom.insert(Commodity::Cement, 10.0);
        }
        CapitalIntensity::Massive => {
            // Massive: enormous maintenance needs
            bom.insert(Commodity::RenovationServices, 400.0);
            bom.insert(Commodity::Steel, 30.0);
            bom.insert(Commodity::Cement, 20.0);
        }
    }
    
    bom
}

/// Calculates OPEX multiplier based on building condition.
///
/// # Arguments
/// * `condition` - Building condition (0.0 to 1.0)
/// * `config` - Building condition configuration
///
/// # Returns
/// OPEX multiplier (1.0+ where higher = more expensive)
///
/// # Rules
/// * OPEX increases as condition degrades
/// * At 1.0 condition: 1.0x OPEX
/// * At 0.5 condition: 1.5x OPEX (with default slope=1.0)
/// * At 0.0 condition: 2.0x OPEX (with default slope=1.0)
pub fn calculate_opex_multiplier(condition: f64, config: &BuildingConditionConfig) -> f64 {
    1.0 + (1.0 - condition) * config.opex_slope
}

/// Calculates degradation rate based on building age and characteristics.
///
/// # Arguments
/// * `years_since_build` - Years since building was constructed
/// * `condition` - Current building condition
/// * `config` - Building condition configuration
///
/// # Returns
/// Degradation rate per turn (0.0 to 1.0)
///
/// # Rules
/// * Newer buildings degrade slower
/// * Buildings in poor condition degrade faster (accelerated decay)
pub fn calculate_degradation_rate(years_since_build: u32, condition: f64, config: &BuildingConditionConfig) -> f64 {
    let base_rate = config.base_degradation_rate;
    let age_factor = (years_since_build as f64 / config.max_age_years).min(1.0);
    let condition_factor = (1.0 - condition) * config.condition_decay_multiplier;
    
    base_rate * (1.0 + age_factor + condition_factor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_renovation_bom_low() {
        let bom = calculate_renovation_bom(&Sector::LocalServices);
        assert_eq!(bom.get(&Commodity::RenovationServices), Some(&50.0));
        assert_eq!(bom.get(&Commodity::Timber), Some(&25.0));
    }

    #[test]
    fn test_calculate_renovation_bom_massive() {
        let bom = calculate_renovation_bom(&Sector::Energy);
        assert_eq!(bom.get(&Commodity::RenovationServices), Some(&1000.0));
        assert_eq!(bom.get(&Commodity::Steel), Some(&500.0));
    }

    #[test]
    fn test_calculate_maintenance_bom_medium() {
        let bom = calculate_maintenance_bom(&Sector::Construction);
        assert_eq!(bom.get(&Commodity::RenovationServices), Some(&80.0));
        assert_eq!(bom.get(&Commodity::Steel), Some(&5.0));
    }

    #[test]
    fn test_calculate_opex_multiplier_perfect_condition() {
        let config = BuildingConditionConfig::default();
        let multiplier = calculate_opex_multiplier(1.0, &config);
        assert_eq!(multiplier, 1.0);
    }

    #[test]
    fn test_calculate_opex_multiplier_half_condition() {
        let config = BuildingConditionConfig::default();
        let multiplier = calculate_opex_multiplier(0.5, &config);
        assert_eq!(multiplier, 1.5);  // 1.0 + (1.0 - 0.5) * 1.0 = 1.5
    }

    #[test]
    fn test_calculate_opex_multiplier_zero_condition() {
        let config = BuildingConditionConfig::default();
        let multiplier = calculate_opex_multiplier(0.0, &config);
        assert_eq!(multiplier, 2.0);  // 1.0 + (1.0 - 0.0) * 1.0 = 2.0
    }

    #[test]
    fn test_calculate_degradation_rate_new_building() {
        let config = BuildingConditionConfig::default();
        let rate = calculate_degradation_rate(0, 1.0, &config);
        assert!(rate < 0.02);  // Should be close to base rate
    }

    #[test]
    fn test_calculate_degradation_rate_old_building() {
        let config = BuildingConditionConfig::default();
        let rate = calculate_degradation_rate(50, 0.5, &config);
        assert!(rate > 0.01);  // Should be higher due to age and condition
    }
}

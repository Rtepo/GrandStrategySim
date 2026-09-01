//! Infrastructure funding and production configuration.
//!
//! Phase C.1: Dynamic, inflation-proof configuration.
//!
//! All cost values are now **multipliers** on `average_wage`, not static
//! nominal floats. This ensures the configuration is mathematically stable
//! at Turn 1 and Turn 1,000, regardless of hyperinflation or deflation
//! (Rule 2: Eradicate Magic Numbers).
//!
//! The actual cost per worker is computed as:
//! ```text
//!   cost_per_worker = average_wage * sector_multiplier
//! ```
//!
//! Physical material requirements (BOM) remain static and based on physical
//! traits (Rule 3: Separation of Physics and Finance).

use serde::{Deserialize, Serialize};

/// Configuration for infrastructure funding and production.
///
/// Phase C.1: All values are `average_wage` multipliers, not nominal floats.
/// This makes the config inflation-proof (Rule 2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InfrastructureConfig {
    /// `average_wage` multiplier for education buildings.
    /// Default 10.0 (education costs ~10× average annual wage per worker).
    #[serde(default = "default_education_cost")]
    pub education_cost_per_worker: f64,

    /// `average_wage` multiplier for healthcare buildings.
    /// Default 15.0 (healthcare costs ~15× average annual wage per worker).
    #[serde(default = "default_healthcare_cost")]
    pub healthcare_cost_per_worker: f64,

    /// `average_wage` multiplier for municipal/public buildings.
    /// Default 8.0 (municipal costs ~8× average annual wage per worker).
    #[serde(default = "default_municipal_cost")]
    pub municipal_cost_per_worker: f64,

    /// `average_wage` multiplier for other sectors.
    /// Default 5.0 (default costs ~5× average annual wage per worker).
    #[serde(default = "default_cost")]
    pub default_cost_per_worker: f64,
}

fn default_education_cost() -> f64 {
    10.0
}

fn default_healthcare_cost() -> f64 {
    15.0
}

fn default_municipal_cost() -> f64 {
    8.0
}

fn default_cost() -> f64 {
    5.0
}

impl Default for InfrastructureConfig {
    fn default() -> Self {
        Self {
            education_cost_per_worker: default_education_cost(),
            healthcare_cost_per_worker: default_healthcare_cost(),
            municipal_cost_per_worker: default_municipal_cost(),
            default_cost_per_worker: default_cost(),
        }
    }
}

impl InfrastructureConfig {
    /// Compute the dynamic cost per worker for a given sector, scaled by
    /// `average_wage` (Rule 2: no magic nominal constants).
    ///
    /// # Arguments
    /// * `sector` - The building's GDP sector
    /// * `average_wage` - Current national average wage (must be > 0)
    ///
    /// # Returns
    /// Cost per worker in current fiat currency units.
    pub fn cost_per_worker(
        &self,
        sector: crate::registries::enums::Sector,
        average_wage: f64,
    ) -> f64 {
        use crate::registries::enums::Sector;
        let multiplier = match sector {
            Sector::EducationalServices => self.education_cost_per_worker,
            Sector::MedicalServices => self.healthcare_cost_per_worker,
            Sector::PublicAdministration | Sector::WasteManagement | Sector::PublicServices => {
                self.municipal_cost_per_worker
            }
            _ => self.default_cost_per_worker,
        };
        average_wage.max(1.0) * multiplier
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries::enums::Sector;

    #[test]
    fn default_config_multipliers() {
        let config = InfrastructureConfig::default();
        assert_eq!(config.education_cost_per_worker, 10.0);
        assert_eq!(config.healthcare_cost_per_worker, 15.0);
        assert_eq!(config.municipal_cost_per_worker, 8.0);
        assert_eq!(config.default_cost_per_worker, 5.0);
    }

    #[test]
    fn dynamic_cost_scales_with_wage() {
        let config = InfrastructureConfig::default();
        let wage_low = 1000.0;
        let wage_high = 10_000.0;

        let cost_low = config.cost_per_worker(Sector::EducationalServices, wage_low);
        let cost_high = config.cost_per_worker(Sector::EducationalServices, wage_high);

        assert_eq!(cost_low, 10_000.0); // 1000 * 10
        assert_eq!(cost_high, 100_000.0); // 10000 * 10
        assert!(cost_high > cost_low, "cost must scale with wage");
    }

    #[test]
    fn dynamic_cost_by_sector() {
        let config = InfrastructureConfig::default();
        let wage = 1000.0;

        let edu = config.cost_per_worker(Sector::EducationalServices, wage);
        let med = config.cost_per_worker(Sector::MedicalServices, wage);
        let mun = config.cost_per_worker(Sector::WasteManagement, wage);
        let other = config.cost_per_worker(Sector::HeavyIndustry, wage);

        assert_eq!(edu, 10_000.0);
        assert_eq!(med, 15_000.0);
        assert_eq!(mun, 8_000.0);
        assert_eq!(other, 5_000.0);
    }

    #[test]
    fn dynamic_cost_clamps_zero_wage() {
        let config = InfrastructureConfig::default();
        let cost = config.cost_per_worker(Sector::EducationalServices, 0.0);
        assert!(cost > 0.0, "cost must be positive even with zero wage");
    }
}

//! Healthcare infrastructure templates and configurations

use crate::registries::enums::LaborTier;
use serde::{Deserialize, Serialize};

/// Healthcare building types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthcareBuildingType {
    /// Severe cases, high cost, increases lifespan
    Hospital,
    /// Rural/cheap, basic health maintenance
    Clinic,
    /// Restores dependency to working capacity
    RehabCenter,
    /// Preventative care, health maintenance
    Sanatorium,
}

/// Healthcare building template
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthcareTemplate {
    /// Type of building
    #[serde(rename = "typ_budynku")]
    pub building_type: HealthcareBuildingType,

    /// Base capacity per turn
    #[serde(rename = "pojemność_bazowa")]
    pub base_capacity: f64,

    /// Cost per capacity unit
    #[serde(rename = "koszt_na_miejsce")]
    pub cost_per_capacity: f64,

    /// Required qualification for workers
    #[serde(rename = "wymagana_kwalifikacja")]
    pub required_qualification: LaborTier,

    /// Impact on lifespan (+0.5 years per Hospital bed/year)
    #[serde(rename = "wpływ_na_długość_życia")]
    pub lifespan_impact: f64,
}

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
    pub building_type: HealthcareBuildingType,

    /// Base capacity per turn
    pub base_capacity: f64,

    /// Cost per capacity unit
    pub cost_per_capacity: f64,

    /// Required qualification for workers
    pub required_qualification: LaborTier,

    /// Impact on lifespan (+0.5 years per Hospital bed/year)
    pub lifespan_impact: f64,
}

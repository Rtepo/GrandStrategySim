//! Care facility infrastructure templates and configurations

use serde::{Deserialize, Serialize};

/// Care facility types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CareFacilityType {
    /// Social Care Home - 24/7 full care
    DPS,
    /// Dom Dziennego Pobytu - Daycare
    DDP,
}

/// Care facility template
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CareFacilityTemplate {
    /// Type of facility
    pub facility_type: CareFacilityType,

    /// Base capacity per turn
    pub base_capacity: f64,

    /// Cost per capacity unit
    pub cost_per_capacity: f64,

    /// Caregiver liberation factor (DPS: 1.0 full, DDP: 0.5 partial)
    pub caregiver_liberation: f64,
}

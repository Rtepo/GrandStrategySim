//! Care facility infrastructure templates and configurations

use serde::{Deserialize, Serialize};

/// Care facility types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CareFacilityType {
    /// Dom Pomocy Społecznej - 24/7 full care
    DPS,
    /// Dom Dziennego Pobytu - Daycare
    DDP,
}

/// Care facility template
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CareFacilityTemplate {
    /// Type of facility
    #[serde(rename = "typ_obiektu")]
    pub facility_type: CareFacilityType,

    /// Base capacity per turn
    #[serde(rename = "pojemność_bazowa")]
    pub base_capacity: f64,

    /// Cost per capacity unit
    #[serde(rename = "koszt_na_miejsce")]
    pub cost_per_capacity: f64,

    /// Caregiver liberation factor (DPS: 1.0 full, DDP: 0.5 partial)
    #[serde(rename = "uwalnienie_pracowników")]
    pub caregiver_liberation: f64,
}

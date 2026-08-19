//! Education infrastructure templates and configurations

use crate::registries::enums::LaborTier;
use serde::{Deserialize, Serialize};

/// Education building types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EducationBuildingType {
    /// 0-3 years, frees female workforce
    Nursery,
    /// Childcare for orphans
    Orphanage,
    /// Basic literacy/numeracy
    PrimarySchool,
    /// Gimnazja (optional based on law)
    MiddleSchool,
    /// Liceum/Technikum/Zawodówka
    HighSchool,
    /// Higher education
    University,
    /// Specialized medical training
    MedicalUniversity,
    /// Engineering/technical
    Polytechnic,
    /// Officer training
    MilitaryAcademy,
}

/// Education building template
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EducationTemplate {
    /// Type of institution
    #[serde(rename = "typ_instytucji")]
    pub institution_type: EducationBuildingType,

    /// Base capacity per turn
    #[serde(rename = "pojemność_bazowa")]
    pub base_capacity: f64,

    /// Cost per capacity unit
    #[serde(rename = "koszt_na_miejsce")]
    pub cost_per_capacity: f64,

    /// Required qualification for staff
    #[serde(rename = "wymagana_kwalifikacja_kadra")]
    pub staff_qualification: LaborTier,

    /// Probability of class advancement
    #[serde(rename = "mobilność_klasowa")]
    pub class_mobility_impact: f64,
}

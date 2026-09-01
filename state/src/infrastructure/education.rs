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
    /// high_school/Technical/Vocational
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
    pub institution_type: EducationBuildingType,

    /// Base capacity per turn
    pub base_capacity: f64,

    /// Cost per capacity unit
    pub cost_per_capacity: f64,

    /// Required qualification for staff
    pub staff_qualification: LaborTier,

    /// Probability of class advancement
    pub class_mobility_impact: f64,
}

//! Building template registry.
//!
//! Ports `BUILDING_REGISTRY` from
//! `economy/production/buildings/registry.py` (108 building kinds). The typed
//! [`BuildingTemplate`] plus [`load_building_registry`] deserialize the bulk
//! set from JSON; [`state_apparatus_templates`] natively encodes the four
//! security/justice buildings that Stage 0's production methods depend on.

use crate::registries::enums::Sector;
use crate::registries::tech_tree::TechId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Name of a building kind, e.g. `"military_base"`. Kept as a string newtype
/// rather than a 108-variant enum to keep the registry data-driven.
pub type BuildingKind = String;

/// Static definition of a building type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BuildingTemplate {
    /// GDP sector this building belongs to (`"gdp_sector"`).

    pub sector: Sector,

    /// One-time construction cost in currency units (`"koszt_budowy"`).

    pub build_cost: u64,

    /// Construction time in turns (`"czas_budowy"`).

    pub build_time_turns: u32,

    /// Maximum number of workers employed (`"worker_capacity"`).

    pub worker_capacity: u32,

    /// Earliest year the building may be constructed (`"min_year"`).

    pub min_year: u32,

    /// Technology required to unlock, if any (`"required_tech"`).
    #[serde(default)]
    pub required_tech: Option<TechId>,

    /// The tier this building upgrades from, if any (`"lower_tier"`).
    #[serde(default)]
    pub lower_tier: Option<BuildingKind>,

    /// Land footprint in hectares (`"powierzchnia_ha"`).

    pub area_ha: u32,
}

impl BuildingTemplate {
    /// Determines whether this building can be constructed given the current
    /// year and the set of discovered technologies.
    ///
    /// # Arguments
    /// * `current_year` - The current simulation year.
    /// * `discovered_tech` - IDs of technologies already researched.
    ///
    /// # Returns
    /// `true` if the era and technology prerequisites are met.
    ///
    /// # Rules
    /// * Fails if `current_year < min_year`.
    /// * Fails if `required_tech` is set and not present in `discovered_tech`.
    /// * Direct port of `is_tech_available` from the Python registry.
    pub fn is_available(&self, current_year: u32, discovered_tech: &[TechId]) -> bool {
        if current_year < self.min_year {
            return false;
        }
        match &self.required_tech {
            Some(tech) => discovered_tech.iter().any(|t| t == tech),
            None => true,
        }
    }
}

/// Deserializes the full building registry from its JSON representation.
///
/// # Arguments
/// * `json` - Raw JSON mapping `building name -> template`.
///
/// # Returns
/// `Ok(HashMap<BuildingKind, BuildingTemplate>)`, or a [`serde_json::Error`].
///
/// # Rules
/// * Polish field names preserved via `#[serde(rename)]`.
/// * `required_tech` / `lower_tier` default to `None` when JSON holds `null`.
pub fn load_building_registry(
    json: &str,
) -> Result<HashMap<BuildingKind, BuildingTemplate>, serde_json::Error> {
    serde_json::from_str(json)
}

/// Natively encodes the four state-apparatus building templates.
///
/// # Returns
/// A map of the security/justice buildings referenced by
/// [`crate::registries::production_methods::state_building_methods`].
///
/// # Rules
/// * All four belong to the `public_services` ([`Sector::PublicServices`])
///   sector and require no technology.
/// * Values mirror the `STATE_APPARATUS` block of the Python `BUILDING_REGISTRY`.
pub fn state_apparatus_templates() -> HashMap<BuildingKind, BuildingTemplate> {
    HashMap::from([
        (
            "military_base".to_string(),
            BuildingTemplate {
                sector: Sector::PublicServices,
                build_cost: 800_000,
                build_time_turns: 5,
                worker_capacity: 5000,
                min_year: 1850,
                required_tech: None,
                lower_tier: None,
                area_ha: 200,
            },
        ),
        (
            "police_station".to_string(),
            BuildingTemplate {
                sector: Sector::PublicServices,
                build_cost: 250_000,
                build_time_turns: 2,
                worker_capacity: 500,
                min_year: 1850,
                required_tech: None,
                lower_tier: None,
                area_ha: 15,
            },
        ),
        (
            "courthouse".to_string(),
            BuildingTemplate {
                sector: Sector::PublicServices,
                build_cost: 350_000,
                build_time_turns: 3,
                worker_capacity: 300,
                min_year: 1850,
                required_tech: None,
                lower_tier: None,
                area_ha: 20,
            },
        ),
        (
            "intelligence_hq".to_string(),
            BuildingTemplate {
                sector: Sector::PublicServices,
                build_cost: 600_000,
                build_time_turns: 4,
                worker_capacity: 800,
                min_year: 1900,
                required_tech: None,
                lower_tier: None,
                area_ha: 30,
            },
        ),
        (
            "prison".to_string(),
            BuildingTemplate {
                sector: Sector::PublicServices,
                build_cost: 400_000,
                build_time_turns: 3,
                worker_capacity: 400,
                min_year: 1850,
                required_tech: None,
                lower_tier: None,
                area_ha: 25,
            },
        ),
    ])
}

/// Natively encodes Phase 6.5 retail building templates.
///
/// # Returns
/// A map of retail/wholesale building templates for B2C market.
///
/// # Rules
/// * Marketplace: Historical open-air stalls, low cost, low capacity
/// * Wholesaler: Distribution center, high capacity, requires logistics
/// * RetailStore: Small independent store, low cost, low capacity
/// * supermarket: Modern self-service, medium cost, medium capacity
/// * DepartmentStore: Multi-category, high cost, high capacity
/// * ShoppingCenter: Enclosed mall, very high cost, very high capacity
pub fn retail_building_templates() -> HashMap<BuildingKind, BuildingTemplate> {
    HashMap::from([
        (
            "marketplace".to_string(),
            BuildingTemplate {
                sector: Sector::LocalServices,
                build_cost: 50_000,
                build_time_turns: 2,
                worker_capacity: 50,
                min_year: 1850,
                required_tech: None,
                lower_tier: None,
                area_ha: 5,
            },
        ),
        (
            "wholesale".to_string(),
            BuildingTemplate {
                sector: Sector::TransportLogistics,
                build_cost: 500_000,
                build_time_turns: 4,
                worker_capacity: 200,
                min_year: 1900,
                required_tech: None,
                lower_tier: None,
                area_ha: 50,
            },
        ),
        (
            "retail_shop".to_string(),
            BuildingTemplate {
                sector: Sector::LocalServices,
                build_cost: 100_000,
                build_time_turns: 2,
                worker_capacity: 20,
                min_year: 1850,
                required_tech: None,
                lower_tier: None,
                area_ha: 2,
            },
        ),
        (
            "supermarket".to_string(),
            BuildingTemplate {
                sector: Sector::LocalServices,
                build_cost: 300_000,
                build_time_turns: 3,
                worker_capacity: 50,
                min_year: 1950,
                required_tech: None,
                lower_tier: Some("retail_shop".to_string()),
                area_ha: 5,
            },
        ),
        (
            "department_store".to_string(),
            BuildingTemplate {
                sector: Sector::LocalServices,
                build_cost: 800_000,
                build_time_turns: 5,
                worker_capacity: 150,
                min_year: 1900,
                required_tech: None,
                lower_tier: Some("supermarket".to_string()),
                area_ha: 15,
            },
        ),
        (
            "shopping_mall".to_string(),
            BuildingTemplate {
                sector: Sector::LocalServices,
                build_cost: 2_000_000,
                build_time_turns: 8,
                worker_capacity: 500,
                min_year: 1970,
                required_tech: None,
                lower_tier: Some("department_store".to_string()),
                area_ha: 50,
            },
        ),
    ])
}

/// Natively encodes Phase 7 education building templates.
///
/// # Returns
/// A map of education building templates for schools and universities.
///
/// # Rules
/// * PrimarySchool: Basic education, low cost, low capacity
/// * HighSchool: Secondary education, medium cost, medium capacity
/// * University: Higher education, high cost, high capacity, requires tech
/// * MedicalUniversity: Medical specialization, very high cost, very high capacity
/// * Polytechnic: Technical specialization, high cost, high capacity
pub fn education_building_templates() -> HashMap<BuildingKind, BuildingTemplate> {
    HashMap::from([
        (
            "primary_school".to_string(),
            BuildingTemplate {
                sector: Sector::EducationalServices,
                build_cost: 200_000,
                build_time_turns: 3,
                worker_capacity: 100,
                min_year: 1850,
                required_tech: None,
                lower_tier: None,
                area_ha: 10,
            },
        ),
        (
            "high_school".to_string(),
            BuildingTemplate {
                sector: Sector::EducationalServices,
                build_cost: 400_000,
                build_time_turns: 4,
                worker_capacity: 150,
                min_year: 1850,
                required_tech: None,
                lower_tier: Some("primary_school".to_string()),
                area_ha: 15,
            },
        ),
        (
            "university".to_string(),
            BuildingTemplate {
                sector: Sector::EducationalServices,
                build_cost: 2_000_000,
                build_time_turns: 8,
                worker_capacity: 500,
                min_year: 1850,
                required_tech: None,
                lower_tier: None,
                area_ha: 50,
            },
        ),
        (
            "university Medyczny".to_string(),
            BuildingTemplate {
                sector: Sector::MedicalServices,
                build_cost: 3_000_000,
                build_time_turns: 10,
                worker_capacity: 600,
                min_year: 1900,
                required_tech: None,
                lower_tier: None,
                area_ha: 60,
            },
        ),
        (
            "technical_university".to_string(),
            BuildingTemplate {
                sector: Sector::EducationalServices,
                build_cost: 2_500_000,
                build_time_turns: 9,
                worker_capacity: 550,
                min_year: 1900,
                required_tech: None,
                lower_tier: None,
                area_ha: 55,
            },
        ),
    ])
}

/// Natively encodes Phase 7 healthcare building templates.
///
/// # Returns
/// A map of healthcare building templates for hospitals and clinics.
///
/// # Rules
/// * Clinic: Basic healthcare, low cost, low capacity
/// * Hospital: General hospital, high cost, high capacity
/// * ResearchHospital: Advanced research and treatment, very high cost, very high capacity
pub fn healthcare_building_templates() -> HashMap<BuildingKind, BuildingTemplate> {
    HashMap::from([
        (
            "clinic".to_string(),
            BuildingTemplate {
                sector: Sector::MedicalServices,
                build_cost: 300_000,
                build_time_turns: 3,
                worker_capacity: 80,
                min_year: 1850,
                required_tech: None,
                lower_tier: None,
                area_ha: 8,
            },
        ),
        (
            "hospital".to_string(),
            BuildingTemplate {
                sector: Sector::MedicalServices,
                build_cost: 1_500_000,
                build_time_turns: 6,
                worker_capacity: 300,
                min_year: 1850,
                required_tech: None,
                lower_tier: Some("clinic".to_string()),
                area_ha: 30,
            },
        ),
        (
            "research_hospital".to_string(),
            BuildingTemplate {
                sector: Sector::MedicalServices,
                build_cost: 4_000_000,
                build_time_turns: 12,
                worker_capacity: 500,
                min_year: 1950,
                required_tech: None,
                lower_tier: Some("hospital".to_string()),
                area_ha: 70,
            },
        ),
    ])
}

/// Natively encodes Phase 7 municipal building templates.
///
/// # Returns
/// A map of municipal building templates for local government services.
///
/// # Rules
/// * CityHall: Local government administration, medium cost, medium capacity
/// * WaterTreatment: Water supply infrastructure, high cost, high capacity
/// * WasteManagement: Waste disposal infrastructure, medium cost, medium capacity
pub fn municipal_building_templates() -> HashMap<BuildingKind, BuildingTemplate> {
    HashMap::from([
        (
            "Ratusz".to_string(),
            BuildingTemplate {
                sector: Sector::PublicAdministration,
                build_cost: 500_000,
                build_time_turns: 4,
                worker_capacity: 200,
                min_year: 1850,
                required_tech: None,
                lower_tier: None,
                area_ha: 20,
            },
        ),
        (
            "water_intake".to_string(),
            BuildingTemplate {
                sector: Sector::PublicServices,
                build_cost: 1_000_000,
                build_time_turns: 6,
                worker_capacity: 150,
                min_year: 1850,
                required_tech: None,
                lower_tier: None,
                area_ha: 40,
            },
        ),
        (
            "wastewater_treatment".to_string(),
            BuildingTemplate {
                sector: Sector::PublicServices,
                build_cost: 800_000,
                build_time_turns: 5,
                worker_capacity: 100,
                min_year: 1900,
                required_tech: None,
                lower_tier: None,
                area_ha: 35,
            },
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_templates_are_public_services() {
        let reg = state_apparatus_templates();
        for (_, t) in reg.iter() {
            assert_eq!(t.sector, Sector::PublicServices);
            assert!(t.required_tech.is_none());
        }
    }

    #[test]
    fn availability_respects_year_and_tech() {
        let reg = state_apparatus_templates();
        let hq = &reg["intelligence_hq"];
        assert!(!hq.is_available(1899, &[]));
        assert!(hq.is_available(1900, &[]));
    }

    #[test]
    fn json_loader_parses_tech_gated_building() {
        let json = r#"{
            "Uranium Mine": {
                "sector": "mining",
                "build_cost": 500000,
                "build_time_turns": 5,
                "worker_capacity": 1500,
                "min_year": 1945,
                "required_tech": "tech_046",
                "lower_tier": "Open Pit Mine",
                "area_ha": 50
            }
        }"#;
        let reg = load_building_registry(json).unwrap();
        let mine = &reg["Uranium Mine"];
        assert_eq!(mine.sector, Sector::Mining);
        assert_eq!(mine.required_tech.as_deref(), Some("tech_046"));
        assert!(!mine.is_available(1945, &[]));
        assert!(mine.is_available(1945, &["tech_046".to_string()]));
    }
}

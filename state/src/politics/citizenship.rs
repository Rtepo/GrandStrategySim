//! Citizenship law and economic discrimination system.
//!
//! Replaces the raw `prawo_obywatelskie` string with structured discrimination
//! rules that affect wages, job access, and property ownership for non-citizens.
//!
//! # Rules
//! * `OpenCitizenship`: No discrimination.
//! * `CulturalAssimilation`: Citizenship blocked if cultural_distance > threshold.
//! * `Segregation`: All non-dominant-culture populations are non-citizens.

use serde::{Deserialize, Serialize};

/// Type of citizenship law.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CitizenshipLaw {
    /// No discrimination — all populations are citizens.
    #[default]
    OpenCitizenship,
    /// Assimilation based on cultural distance — distant cultures are non-citizens.
    CulturalAssimilation,
    /// Full discrimination — all non-dominant-culture populations are non-citizens.
    Segregation,
}

impl CitizenshipLaw {
    /// Parse from the existing Polish string in `Politics.civil_rights_law`.
    pub fn from_polish(s: &str) -> Self {
        match s {
            "Segregacja" => CitizenshipLaw::Segregation,
            "5-Year Assimilation" | "Asymilacja 10 lat" => CitizenshipLaw::CulturalAssimilation,
            _ => CitizenshipLaw::OpenCitizenship,
        }
    }
}

/// Configuration for economic discrimination against non-citizens.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscriminationConfig {
    /// Wage multiplier for non-citizens (e.g., 0.7 = 30% wage cut).
    pub non_citizen_wage_multiplier: f64,
    /// If true, non-citizens cannot hold expert-tier jobs.
    pub block_expert_jobs: bool,
    /// If true, non-citizens cannot hold skilled-tier jobs.
    pub block_skilled_jobs: bool,
    /// If true, non-citizens cannot own companies.
    pub property_ownership_restricted: bool,
    /// Cultural distance threshold above which citizenship is blocked under CulturalAssimilation.
    pub cultural_distance_threshold: f64,
}

impl Default for DiscriminationConfig {
    fn default() -> Self {
        Self {
            non_citizen_wage_multiplier: 0.7,
            block_expert_jobs: true,
            block_skilled_jobs: false,
            property_ownership_restricted: false,
            cultural_distance_threshold: 0.6,
        }
    }
}

/// Determine whether a given minority culture is considered a citizen.
///
/// # Arguments
/// * `law` - The citizenship law type.
/// * `cultural_distance` - Distance between minority and dominant culture (0.0–1.0).
/// * `config` - Discrimination configuration.
///
/// # Returns
/// `true` if the minority culture has citizenship rights, `false` otherwise.
pub fn is_citizen(
    law: CitizenshipLaw,
    cultural_distance: f64,
    config: &DiscriminationConfig,
) -> bool {
    match law {
        CitizenshipLaw::OpenCitizenship => true,
        CitizenshipLaw::CulturalAssimilation => {
            cultural_distance <= config.cultural_distance_threshold
        }
        CitizenshipLaw::Segregation => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_citizenship() {
        let config = DiscriminationConfig::default();
        assert!(is_citizen(CitizenshipLaw::OpenCitizenship, 0.9, &config));
    }

    #[test]
    fn test_cultural_assimilation_within_threshold() {
        let config = DiscriminationConfig::default();
        assert!(is_citizen(
            CitizenshipLaw::CulturalAssimilation,
            0.5,
            &config
        ));
    }

    #[test]
    fn test_cultural_assimilation_beyond_threshold() {
        let config = DiscriminationConfig::default();
        assert!(!is_citizen(
            CitizenshipLaw::CulturalAssimilation,
            0.7,
            &config
        ));
    }

    #[test]
    fn test_segregation() {
        let config = DiscriminationConfig::default();
        assert!(!is_citizen(CitizenshipLaw::Segregation, 0.0, &config));
    }

    #[test]
    fn test_from_polish() {
        assert_eq!(CitizenshipLaw::from_polish("Segregacja"), CitizenshipLaw::Segregation);
        assert_eq!(
            CitizenshipLaw::from_polish("5-Year Assimilation"),
            CitizenshipLaw::CulturalAssimilation
        );
        assert_eq!(
            CitizenshipLaw::from_polish("Asymilacja 10 lat"),
            CitizenshipLaw::CulturalAssimilation
        );
        assert_eq!(
            CitizenshipLaw::from_polish(""),
            CitizenshipLaw::OpenCitizenship
        );
    }
}

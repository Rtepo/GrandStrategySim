//! State structure system — defines the relationship between central and regional governments.
//!
//! Phase 65: Introduces the `StateStructure` enum and `StateStructureConfig` to control
//! tax retention rates, regional law authority, and autonomous republic designations.
//! All configuration values are explicit — no magic numbers.

use serde::{Deserialize, Serialize};

/// The structural relationship between central government and regional governments (JSTs).
///
/// Determines how tax revenue is split between central and regional treasuries,
/// whether regions can pass their own laws, and whether autonomous republics exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum StateStructure {
    /// Unitary state: regional governments retain a small, fixed percentage of
    /// local taxes; the rest goes to the central budget. Regions cannot pass
    /// their own law variations.
    #[default]
    Unitary,
    /// Federation: regions retain a large percentage of taxes and gain the
    /// ability to pass regional variations of certain laws.
    Federation,
    /// Totalitarian / Absolute: 100% of tax revenue goes to the central
    /// treasury; JSTs only survive on central grants.
    #[serde(rename = "totalitarian")]
    Totalitarian,
    /// Autonomous Republic: a specific regional designation for areas with
    /// distinct cultures/nationalities. Has its own VIP Premier and a high
    /// risk of separatist provocations if unrest grows.
    AutonomousRepublic,
}

impl StateStructure {
    /// Returns true if regions can pass their own law variations.
    pub fn allows_regional_laws(self) -> bool {
        matches!(
            self,
            StateStructure::Federation | StateStructure::AutonomousRepublic
        )
    }

    /// Returns true if the central government controls all tax revenue.
    pub fn is_centralized(self) -> bool {
        matches!(self, StateStructure::Totalitarian)
    }

    /// Returns a human-readable label for this state structure.
    pub fn as_str(self) -> &'static str {
        match self {
            StateStructure::Unitary => "Unitary",
            StateStructure::Federation => "Federation",
            StateStructure::Totalitarian => "Totalitarian",
            StateStructure::AutonomousRepublic => "Autonomous Republic",
        }
    }
}

/// Configuration for state structure tax retention and regional authority.
///
/// All values are explicit configuration fields — no magic numbers.
/// Tax shares must sum to 1.0 (central + region + microregion).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateStructureConfig {
    /// Tax share retained by the central treasury for each state structure type.
    /// Indexed by `StateStructure` discriminant via the `central_shares` method.
    pub unitary_central_share: f64,
    pub unitary_region_share: f64,
    pub unitary_microregion_share: f64,

    pub federation_central_share: f64,
    pub federation_region_share: f64,
    pub federation_microregion_share: f64,

    pub totalitarian_central_share: f64,
    pub totalitarian_region_share: f64,
    pub totalitarian_microregion_share: f64,

    pub autonomous_republic_central_share: f64,
    pub autonomous_republic_region_share: f64,
    pub autonomous_republic_microregion_share: f64,

    /// Base separatist risk per turn for autonomous republics (0.0 = none, 1.0 = certain).
    pub autonomous_republic_separatism_base_risk: f64,

    /// Unrest threshold above which separatist provocations begin for autonomous republics.
    pub autonomous_republic_unrest_threshold: f64,
}

impl Default for StateStructureConfig {
    fn default() -> Self {
        Self {
            // Unitary: regions keep 15%, central gets 80%, microregions get 5%
            unitary_central_share: 0.80,
            unitary_region_share: 0.15,
            unitary_microregion_share: 0.05,

            // Federation: regions keep 55%, central gets 35%, microregions get 10%
            federation_central_share: 0.35,
            federation_region_share: 0.55,
            federation_microregion_share: 0.10,

            // Totalitarian: central gets 100%, regions get 0%
            totalitarian_central_share: 1.0,
            totalitarian_region_share: 0.0,
            totalitarian_microregion_share: 0.0,

            // Autonomous Republic: region keeps 65%, central gets 25%, microregions get 10%
            autonomous_republic_central_share: 0.25,
            autonomous_republic_region_share: 0.65,
            autonomous_republic_microregion_share: 0.10,

            // Separatism: 2% base risk per turn, triggers when unrest > 60
            autonomous_republic_separatism_base_risk: 0.02,
            autonomous_republic_unrest_threshold: 60.0,
        }
    }
}

impl StateStructureConfig {
    /// Returns the (central, region, microregion) tax shares for the given state structure.
    pub fn shares_for(&self, structure: StateStructure) -> (f64, f64, f64) {
        match structure {
            StateStructure::Unitary => (
                self.unitary_central_share,
                self.unitary_region_share,
                self.unitary_microregion_share,
            ),
            StateStructure::Federation => (
                self.federation_central_share,
                self.federation_region_share,
                self.federation_microregion_share,
            ),
            StateStructure::Totalitarian => (
                self.totalitarian_central_share,
                self.totalitarian_region_share,
                self.totalitarian_microregion_share,
            ),
            StateStructure::AutonomousRepublic => (
                self.autonomous_republic_central_share,
                self.autonomous_republic_region_share,
                self.autonomous_republic_microregion_share,
            ),
        }
    }

    /// Returns the central tax share for the given state structure.
    pub fn central_share(&self, structure: StateStructure) -> f64 {
        self.shares_for(structure).0
    }

    /// Returns the regional tax share for the given state structure.
    pub fn region_share(&self, structure: StateStructure) -> f64 {
        self.shares_for(structure).1
    }

    /// Returns the microregion tax share for the given state structure.
    pub fn microregion_share(&self, structure: StateStructure) -> f64 {
        self.shares_for(structure).2
    }

    /// Validates that all share triplets sum to 1.0 (within floating-point tolerance).
    pub fn validate(&self) -> bool {
        let tolerance = 1e-6;
        let sums = [
            self.unitary_central_share + self.unitary_region_share + self.unitary_microregion_share,
            self.federation_central_share
                + self.federation_region_share
                + self.federation_microregion_share,
            self.totalitarian_central_share
                + self.totalitarian_region_share
                + self.totalitarian_microregion_share,
            self.autonomous_republic_central_share
                + self.autonomous_republic_region_share
                + self.autonomous_republic_microregion_share,
        ];
        sums.iter().all(|s| (s - 1.0).abs() < tolerance)
    }
}

/// A regional law that can be enacted by Federation or AutonomousRepublic regions.
///
/// These are variations of national laws that regions can customize within
/// the bounds allowed by the state structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionalLaw {
    /// Unique identifier for this regional law.
    pub id: String,
    /// Region ID that enacted this law.
    pub region_id: String,
    /// Type of regional law.
    pub law_type: RegionalLawType,
    /// Turn when this law was enacted.
    pub enacted_turn: u32,
}

/// Types of laws that Federation/AutonomousRepublic regions can enact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RegionalLawType {
    /// Regional tax rate variation (within bounds set by central government).
    RegionalTaxVariation {
        /// Regional income tax surcharge (can be negative for tax breaks).
        income_tax_surcharge: f64,
        /// Regional corporate tax surcharge.
        corporate_tax_surcharge: f64,
    },
    /// Regional zoning preferences (already exists via governor zoning plans).
    RegionalZoningPlan,
    /// Regional education policy variation.
    RegionalEducationPolicy {
        /// Regional education spending multiplier.
        spending_multiplier: f64,
    },
    /// Regional language policy (for autonomous republics with distinct cultures).
    RegionalLanguagePolicy {
        /// Official regional language.
        language: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_structure_allows_regional_laws() {
        assert!(!StateStructure::Unitary.allows_regional_laws());
        assert!(StateStructure::Federation.allows_regional_laws());
        assert!(!StateStructure::Totalitarian.allows_regional_laws());
        assert!(StateStructure::AutonomousRepublic.allows_regional_laws());
    }

    #[test]
    fn test_state_structure_is_centralized() {
        assert!(!StateStructure::Unitary.is_centralized());
        assert!(!StateStructure::Federation.is_centralized());
        assert!(StateStructure::Totalitarian.is_centralized());
        assert!(!StateStructure::AutonomousRepublic.is_centralized());
    }

    #[test]
    fn test_state_structure_as_str() {
        assert_eq!(StateStructure::Unitary.as_str(), "Unitary");
        assert_eq!(StateStructure::Federation.as_str(), "Federation");
        assert_eq!(StateStructure::Totalitarian.as_str(), "Totalitarian");
        assert_eq!(
            StateStructure::AutonomousRepublic.as_str(),
            "Autonomous Republic"
        );
    }

    #[test]
    fn test_default_config_shares_sum_to_one() {
        let config = StateStructureConfig::default();
        assert!(config.validate(), "All share triplets must sum to 1.0");
    }

    #[test]
    fn test_unitary_shares() {
        let config = StateStructureConfig::default();
        let (central, region, micro) = config.shares_for(StateStructure::Unitary);
        assert!((central - 0.80).abs() < 1e-9);
        assert!((region - 0.15).abs() < 1e-9);
        assert!((micro - 0.05).abs() < 1e-9);
    }

    #[test]
    fn test_federation_shares() {
        let config = StateStructureConfig::default();
        let (central, region, micro) = config.shares_for(StateStructure::Federation);
        assert!((central - 0.35).abs() < 1e-9);
        assert!((region - 0.55).abs() < 1e-9);
        assert!((micro - 0.10).abs() < 1e-9);
    }

    #[test]
    fn test_totalitarian_shares() {
        let config = StateStructureConfig::default();
        let (central, region, micro) = config.shares_for(StateStructure::Totalitarian);
        assert!((central - 1.0).abs() < 1e-9);
        assert!((region - 0.0).abs() < 1e-9);
        assert!((micro - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_autonomous_republic_shares() {
        let config = StateStructureConfig::default();
        let (central, region, micro) = config.shares_for(StateStructure::AutonomousRepublic);
        assert!((central - 0.25).abs() < 1e-9);
        assert!((region - 0.65).abs() < 1e-9);
        assert!((micro - 0.10).abs() < 1e-9);
    }

    #[test]
    fn test_custom_config_validation() {
        let mut config = StateStructureConfig::default();
        config.unitary_central_share = 0.90;
        config.unitary_region_share = 0.15;
        // Now sums to 1.10 — should fail validation
        assert!(!config.validate());
    }

    #[test]
    fn test_regional_law_serialization() {
        let law = RegionalLaw {
            id: "LAW_R1_001".to_string(),
            region_id: "R1".to_string(),
            law_type: RegionalLawType::RegionalTaxVariation {
                income_tax_surcharge: 0.02,
                corporate_tax_surcharge: 0.01,
            },
            enacted_turn: 100,
        };
        let json = serde_json::to_string(&law).unwrap();
        let deserialized: RegionalLaw = serde_json::from_str(&json).unwrap();
        assert_eq!(law, deserialized);
    }
}

//! Regional governance and fiscal structures for local government (JST)

use crate::politics::local_council::LocalCouncil;
use crate::politics::system::Leader;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Regional governance structure (JST - Local Government Unit)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct RegionalGovernance {
    /// Unique identifier for the regional government
    #[serde(default)]
    pub id: String,

    /// Type of regional head (Mayor/Village Head/Governor)
    #[serde(default)]
    pub head_type: RegionalHeadType,

    /// Current regional head
    #[serde(default)]
    pub head: Leader,

    /// Local council structure
    #[serde(default)]
    pub council: LocalCouncil,

    /// Regional budget
    #[serde(default)]
    pub budget: RegionalBudget,

    /// Debt issued by the region
    #[serde(default)]
    pub debt: RegionalDebt,

    /// Administrative status (Normal, Commissary Administration)
    #[serde(default)]
    pub admin_status: AdministrativeStatus,

    /// Date of last local election
    #[serde(default)]
    pub last_election_year: u32,

    /// Years until next local election
    #[serde(default)]
    pub years_to_next_election: u32,

    /// Phase 59: Zoning plan registry (MPZP plans enacted by this governor).
    #[serde(default)]
    pub zoning_plans: crate::society::cadastre::ZoningPlanRegistry,
}

/// Type of regional head
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RegionalHeadType {
    #[default]
    /// Elected mayor (democratic systems)
    Mayor,
    /// Appointed village head (authoritarian/traditional systems)
    VillageHead,
    /// Governor (for Megaregions)
    Governor,
    /// Direct central administration (no local head)
    CentralAdministrator,
}

/// Administrative status of a region
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AdministrativeStatus {
    #[default]
    /// Normal self-governance
    Normal,
    /// Commissary administration (central government takeover due to debt crisis)
    CommissaryAdministration,
    /// Martial law (military administration)
    MartialLaw,
}

/// Regional budget (JST budget)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct RegionalBudget {
    /// Liquid reserves
    #[serde(default)]
    pub liquid_reserves: f64,

    /// Annual tax revenue collected
    #[serde(default)]
    pub tax_revenue: f64,

    /// Local property/land tax revenue
    #[serde(default)]
    pub property_tax: f64,

    /// Local service fees
    #[serde(default)]
    pub local_fees: f64,

    /// Transfer from central government (grants)
    #[serde(default)]
    pub central_grants: f64,

    /// Upward transfer to Megaregion (percentage of revenue)
    #[serde(default)]
    pub megaregion_transfer: f64,

    /// Upward transfer to Central Budget (percentage of revenue)
    #[serde(default)]
    pub central_transfer: f64,

    /// Local expenditures (infrastructure, services)
    #[serde(default)]
    pub local_expenditures: f64,

    /// Debt service payments
    #[serde(default)]
    pub debt_service: f64,

    /// Budget balance (revenue - expenditures - transfers)
    #[serde(default)]
    pub budget_balance: f64,

    /// Municipal land value (asset)
    #[serde(default)]
    pub municipal_land_value: f64,

    /// Any additional budget fields
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Regional debt structure
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct RegionalDebt {
    /// Total outstanding debt
    #[serde(default)]
    pub total_debt: f64,

    /// Municipal bonds issued
    #[serde(default)]
    pub municipal_bonds: Vec<MunicipalBond>,

    /// Debt-to-revenue ratio (warning threshold: 3.0, critical: 5.0)
    #[serde(default)]
    pub debt_to_revenue_ratio: f64,

    /// Credit rating (AAA to D)
    #[serde(default)]
    pub credit_rating: String,

    /// Years until debt maturity
    #[serde(default)]
    pub years_to_maturity: u32,
}

/// Municipal bond issue
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MunicipalBond {
    /// Bond identifier
    #[serde(default)]
    pub id: String,

    /// Principal amount
    #[serde(default)]
    pub principal: f64,

    /// Interest rate
    #[serde(default)]
    pub interest_rate: f64,

    /// Year issued
    #[serde(default)]
    pub issue_year: u32,

    /// Year of maturity
    #[serde(default)]
    pub maturity_year: u32,

    /// Bondholders
    #[serde(default)]
    pub holders: Vec<String>,
}

/// Megaregion governance structure (optional administrative layer)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MegaregionGovernance {
    /// Unique identifier for the megaregion government
    #[serde(default)]
    pub id: String,

    /// Megaregion governor
    #[serde(default)]
    pub governor: Leader,

    /// Megaregion budget
    #[serde(default)]
    pub budget: MegaregionBudget,

    /// Whether governor is appointed centrally or elected
    #[serde(default)]
    pub governor_appointed: bool,

    /// Administrative competence level (from national law)
    #[serde(default)]
    pub competence_level: MegaregionCompetence,
}

/// Megaregion administrative competence level
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MegaregionCompetence {
    #[default]
    /// Advisory only (no fiscal authority)
    Advisory,
    /// Limited fiscal authority (coordination only)
    Limited,
    /// Full fiscal authority (collects and redistributes)
    Full,
}

/// Megaregion budget (aggregated from regions)
///
/// # CRITICAL: No Upward Transfer to Central
/// Megaregions keep 100% of their regional transfers. They do NOT send
/// another cut to the Central Government. The Region already split its
/// revenue according to FiscalTransferConfig (Local + Megaregion + Central).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MegaregionBudget {
    /// Liquid reserves
    #[serde(default)]
    pub liquid_reserves: f64,

    /// Total transfers received from member regions
    #[serde(default)]
    pub regional_transfers: f64,

    /// Regional development spending
    #[serde(default)]
    pub development_expenditures: f64,

    /// Inter-regional coordination spending
    #[serde(default)]
    pub coordination_expenditures: f64,

    /// Budget balance
    #[serde(default)]
    pub budget_balance: f64,

    /// Any additional budget fields
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Phase 33: Initialize regional governance for a region that has none.
///
/// Creates a default `RegionalGovernance` with:
/// - A unique ID based on the region ID
/// - Mayor head type (democratic default)
/// - Empty budget (funds will flow from tax collection)
/// - Normal administrative status
/// - 4-year election cycle
pub fn initialize_regional_governance(region_id: &str, country_name: &str) -> RegionalGovernance {
    RegionalGovernance {
        id: format!("JST-{}-{}", country_name, region_id),
        head_type: RegionalHeadType::Mayor,
        head: Leader::default(),
        council: crate::politics::local_council::LocalCouncil::default(),
        budget: RegionalBudget::default(),
        debt: RegionalDebt::default(),
        admin_status: AdministrativeStatus::Normal,
        last_election_year: 0,
        years_to_next_election: 4,
        zoning_plans: crate::society::cadastre::ZoningPlanRegistry::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_regional_governance_creates_valid_structure() {
        let gov = initialize_regional_governance("REG-001", "TestCountry");
        assert_eq!(gov.id, "JST-TestCountry-REG-001");
        assert_eq!(gov.head_type, RegionalHeadType::Mayor);
        assert_eq!(gov.admin_status, AdministrativeStatus::Normal);
        assert_eq!(gov.years_to_next_election, 4);
        assert_eq!(gov.budget.liquid_reserves, 0.0);
        assert_eq!(gov.budget.tax_revenue, 0.0);
        assert_eq!(gov.debt.total_debt, 0.0);
    }

    #[test]
    fn test_regional_governance_default_is_valid() {
        let gov = RegionalGovernance::default();
        assert_eq!(gov.admin_status, AdministrativeStatus::Normal);
        assert_eq!(gov.head_type, RegionalHeadType::Mayor);
        assert_eq!(gov.budget.liquid_reserves, 0.0);
    }
}

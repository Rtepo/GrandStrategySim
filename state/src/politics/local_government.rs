//! Regional governance and fiscal structures for local government (JST)

use crate::politics::system::Leader;
use crate::politics::local_council::LocalCouncil;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Regional governance structure (JST - Jednostka Samorządu Terytorialnego)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct RegionalGovernance {
    /// Unique identifier for the regional government
    #[serde(rename = "id_jst", default)]
    pub id: String,
    
    /// Type of regional head (Mayor/Wójt/Governor)
    #[serde(rename = "typ_glowy", default)]
    pub head_type: RegionalHeadType,
    
    /// Current regional head
    #[serde(rename = "glowa_regionu", default)]
    pub head: Leader,
    
    /// Local council structure
    #[serde(rename = "rada_lokalna", default)]
    pub council: LocalCouncil,
    
    /// Regional budget
    #[serde(rename = "budzet_regionalny", default)]
    pub budget: RegionalBudget,
    
    /// Debt issued by the region
    #[serde(rename = "dlug_regionalny", default)]
    pub debt: RegionalDebt,
    
    /// Administrative status (Normal, Commissary Administration)
    #[serde(rename = "status_administracyjny", default)]
    pub admin_status: AdministrativeStatus,
    
    /// Date of last local election
    #[serde(rename = "ostatnie_wybory_lokalne", default)]
    pub last_election_year: u32,
    
    /// Years until next local election
    #[serde(rename = "lata_do_wyborow_lokalnych", default)]
    pub years_to_next_election: u32,
}

/// Type of regional head
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RegionalHeadType {
    #[default]
    /// Elected mayor (democratic systems)
    Mayor,
    /// Appointed wójt (authoritarian/traditional systems)
    Wójt,
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
    #[serde(rename = "rezerwy_liquidne", default)]
    pub liquid_reserves: f64,
    
    /// Annual tax revenue collected
    #[serde(rename = "przychody_podatkowe", default)]
    pub tax_revenue: f64,
    
    /// Local property/land tax revenue
    #[serde(rename = "podatek_nieruchomosci", default)]
    pub property_tax: f64,
    
    /// Local service fees
    #[serde(rename = "oplata_lokalna", default)]
    pub local_fees: f64,
    
    /// Transfer from central government (grants)
    #[serde(rename = "dotacje_rzadowe", default)]
    pub central_grants: f64,
    
    /// Upward transfer to Megaregion (percentage of revenue)
    #[serde(rename = "transfer_do_megaregionu", default)]
    pub megaregion_transfer: f64,
    
    /// Upward transfer to Central Budget (percentage of revenue)
    #[serde(rename = "transfer_do_centrum", default)]
    pub central_transfer: f64,
    
    /// Local expenditures (infrastructure, services)
    #[serde(rename = "wydatki_lokalne", default)]
    pub local_expenditures: f64,
    
    /// Debt service payments
    #[serde(rename = "obsuga_dlugu", default)]
    pub debt_service: f64,
    
    /// Budget balance (revenue - expenditures - transfers)
    #[serde(rename = "saldo_budzetowe", default)]
    pub budget_balance: f64,
    
    /// Municipal land value (asset)
    #[serde(rename = "wartosc_ziemi_miejskiej", default)]
    pub municipal_land_value: f64,
    
    /// Any additional budget fields
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Regional debt structure
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct RegionalDebt {
    /// Total outstanding debt
    #[serde(rename = "dlug_calkowity", default)]
    pub total_debt: f64,
    
    /// Municipal bonds issued
    #[serde(rename = "obligacje_miejskie", default)]
    pub municipal_bonds: Vec<MunicipalBond>,
    
    /// Debt-to-revenue ratio (warning threshold: 3.0, critical: 5.0)
    #[serde(rename = "wskaznik_dlugu", default)]
    pub debt_to_revenue_ratio: f64,
    
    /// Credit rating (AAA to D)
    #[serde(rename = "rating_kredytowy", default)]
    pub credit_rating: String,
    
    /// Years until debt maturity
    #[serde(rename = "lata_do_splaty", default)]
    pub years_to_maturity: u32,
}

/// Municipal bond issue
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MunicipalBond {
    /// Bond identifier
    #[serde(rename = "id_obligacji", default)]
    pub id: String,
    
    /// Principal amount
    #[serde(rename = "kwota_glowna", default)]
    pub principal: f64,
    
    /// Interest rate
    #[serde(rename = "oprocentowanie", default)]
    pub interest_rate: f64,
    
    /// Year issued
    #[serde(rename = "rok_emisji", default)]
    pub issue_year: u32,
    
    /// Year of maturity
    #[serde(rename = "rok_splaty", default)]
    pub maturity_year: u32,
    
    /// Bondholders
    #[serde(rename = "posiadacze", default)]
    pub holders: Vec<String>,
}

/// Megaregion governance structure (optional administrative layer)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MegaregionGovernance {
    /// Unique identifier for the megaregion government
    #[serde(rename = "id_megaregionu", default)]
    pub id: String,
    
    /// Megaregion governor
    #[serde(rename = "gubernator", default)]
    pub governor: Leader,
    
    /// Megaregion budget
    #[serde(rename = "budzet_megaregionu", default)]
    pub budget: MegaregionBudget,
    
    /// Whether governor is appointed centrally or elected
    #[serde(rename = "gubernator_mianowany", default)]
    pub governor_appointed: bool,
    
    /// Administrative competence level (from national law)
    #[serde(rename = "poziom_kompetencji", default)]
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
    #[serde(rename = "rezerwy_liquidne", default)]
    pub liquid_reserves: f64,
    
    /// Total transfers received from member regions
    #[serde(rename = "przychody_z_regionow", default)]
    pub regional_transfers: f64,
    
    /// Regional development spending
    #[serde(rename = "wydatki_rozwojowe", default)]
    pub development_expenditures: f64,
    
    /// Inter-regional coordination spending
    #[serde(rename = "wydatki_koordynacyjne", default)]
    pub coordination_expenditures: f64,
    
    /// Budget balance
    #[serde(rename = "saldo_budzetowe", default)]
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

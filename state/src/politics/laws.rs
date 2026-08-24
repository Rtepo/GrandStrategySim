//! Law-based service configuration for healthcare and education
//!
//! This module defines the law structures that configure how healthcare and
//! education services are delivered, including funding models, universality levels,
// and priority systems.

use crate::politics::funding::ServiceFundingConfig;
use serde::{Deserialize, Serialize};

/// Healthcare law configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthcareLaw {
    /// Healthcare system type

    pub healthcare_system: HealthcareSystem,

    /// Funding configuration

    pub funding: ServiceFundingConfig,

    /// Universality level

    pub universality: UniversalityLevel,

    /// Healthcare priorities

    pub priorities: HealthcarePriorities,
}

/// Healthcare system types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthcareSystem {
    /// Polish NFZ model

    NationalHealthFund,
    /// Bismarck model

    InsuranceBased,
    /// Beveridge model

    Budgetary,
    /// US-style

    MarketBased,
}

/// Universality level for healthcare coverage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UniversalityLevel {
    /// Everyone covered
    Universal,
    /// Based on income
    MeansTested,
    /// Based on categories (elderly, children)
    Categorical,
    /// Minimal coverage
    Limited,
}

/// Healthcare priority configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthcarePriorities {
    /// Emergency priority

    pub emergency_priority: bool,

    /// Elderly priority

    pub elderly_priority: bool,

    /// Children priority

    pub children_priority: bool,

    /// Chronic condition priority

    pub chronic_priority: bool,
}

/// Education law configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EducationLaw {
    /// Education model

    pub education_model: EducationModel,

    /// School system

    pub school_system: SchoolSystem,

    /// Funding configuration

    pub funding: ServiceFundingConfig,

    /// Compulsory education configuration

    pub compulsory_education: CompulsoryEducationConfig,
}

/// Education model types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EducationModel {
    /// State-run

    StateRun,
    /// Private

    Private,
    /// Mixed

    Mixed,
    /// Religious

    Religious,
}

/// School system types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchoolSystem {
    /// Primary 4, Middle 4, High 4

    FourPlusFourPlusFour,
    /// Primary 6, Middle 3, High 3

    SixPlusThreePlusThree,
    /// Primary 8, High 4

    EightPlusFour,
    /// Direct Primary → High

    NoMiddleSchool,
}

/// Compulsory education configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompulsoryEducationConfig {
    /// Compulsory years

    pub compulsory_years: u32,

    /// End age

    pub end_age: u32,

    /// Enforcement level

    pub enforcement: EnforcementLevel,
}

/// Enforcement level for compulsory education
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementLevel {
    Strict,
    Moderate,
    Lax,
    None,
}

/// Justice law configuration (Phase 14).
///
/// Governs the independence of the judiciary, court processing times,
/// pardon authority, and the national corruption index.
/// Each field hooks into the physical justice engine — no magic modifiers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct JusticeLaw {
    /// Whether KRS (National Council of the Judiciary) is independent from executive.
    #[serde(default)]
    pub krs_separated: bool,
    /// Whether Prosecutor General is separate from Justice Minister.
    #[serde(default)]
    pub prosecutor_general_separated: bool,
    /// Target court processing time category.
    #[serde(default)]
    pub court_wait_time_target: CourtWaitTime,
    /// Who holds pardon authority.
    #[serde(default)]
    pub pardon_authority: PardonAuthority,
    /// National corruption index (0.0 = clean, 1.0 = highly corrupt).
    #[serde(default)]
    pub corruption_index: f64,
}

/// Court processing time categories (Phase 14).
///
/// Modifies the freeze ratio applied to company cash when justice coverage is insufficient.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CourtWaitTime {
    /// Fast processing — freeze ratio × 0.5
    Expedited,
    /// Standard processing — freeze ratio × 1.0
    #[default]
    Normal,
    /// Slow processing — freeze ratio × 1.5
    Backlogged,
    /// System collapse — freeze ratio × 2.5
    Paralyzed,
}

/// Pardon authority holder (Phase 14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PardonAuthority {
    /// President can pardon — reduces frozen cash by 5% per turn.
    President,
    /// Head of state / party leader — same effect.
    #[default]
    HeadOfState,
    /// Judicial board reviews pardons — no political unfreezing.
    JudicialBoard,
    /// No pardon power exists.
    None,
}

/// Prison labor law configuration (Phase 14).
///
/// Determines how prisons operate: voluntary labor, forced penal colonies,
/// private labor camps, or political isolation camps.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PrisonLaborLaw {
    /// Type of prison system in effect.
    #[serde(default)]
    pub prison_type: PrisonType,
    /// Per-capita savings accrual rate for voluntary labor prisoners.
    #[serde(default)]
    pub labor_compensation: f64,
    /// Health degradation rate per turn for forced labor / isolation prisoners.
    #[serde(default)]
    pub health_degradation_rate: f64,
    /// Per-capita fee paid by companies to the State for private labor camp prisoners.
    #[serde(default)]
    pub private_transfer_fee: f64,
    /// Target demographic class for isolation camps (e.g., "intelligentsia", "bourgeoisie").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_demographic: Option<String>,
    /// Maximum number of prisoners that can be held in isolation camps.
    #[serde(default)]
    pub isolation_capacity: i64,
}

/// Prison system type (Phase 14).
///
/// Each variant determines a fundamentally different economic flow:
/// - VoluntaryLabor/StatePenalColony: building-internal production
/// - PrivateLaborCamps: labor market FTE injection at zero wage
/// - IsolationCamp: demographic removal from workforce
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrisonType {
    /// Democratic: workshops, prisoners can voluntarily work, minor savings accrue.
    #[default]
    VoluntaryLabor,
    /// Authoritarian: forced heavy labor, produces raw materials, high mortality.
    StatePenalColony,
    /// Corrupt/Authoritarian: prisoners transferred to private companies as zero-wage FTEs.
    PrivateLaborCamps,
    /// Oppressive: dissidents removed from workforce, no labor output, State pays maintenance.
    IsolationCamp,
}

/// Type of law that can be enacted, mapping to physical economic config mutations.
#[derive(Debug, Clone, PartialEq)]
pub enum LawType {
    /// Healthcare law — affects service_pricing_config
    Healthcare(HealthcareLaw),
    /// Education law — affects service_pricing_config
    Education(EducationLaw),
    /// Justice law — affects corruption index and court processing (Phase 14)
    Justice(JusticeLaw),
    /// Prison labor law — affects prison building PMs and labor market (Phase 14)
    PrisonLabor(PrisonLaborLaw),
    /// Tax rate change — affects tax_rates
    TaxRateChange {
        /// New income tax rate
        income_tax: Option<f64>,
        /// New VAT rate
        vat: Option<f64>,
        /// New corporate tax rate
        corporate_tax: Option<f64>,
    },
    /// Economic policy shift
    EconomicPolicyChange {
        /// New economic policy string
        policy: String,
    },
    /// Infrastructure investment mandate
    InfrastructureMandate {
        /// Budget allocation percentage
        allocation_pct: f64,
    },
    /// Phase 18B: Sentencing law — affects prison cohort generation and legal dualism.
    Sentencing(crate::economy::sentencing::SentencingLaw),
    /// Phase 18C: Free speech / assembly / press freedom law.
    FreeSpeech(crate::politics::free_speech::FreeSpeechLaw),
    /// Phase 23C: Transport ownership law — affects passenger transport
    /// subsidy and privatization of JST transport operators.
    Transport(crate::economy::commuting::TransportLaw),
    /// Phase 63: Subsurface rights law — controls mining/extraction ownership rules.
    SubsurfaceRights(crate::society::cadastre::SubsurfaceRightsLaw),
    /// Phase 65: State structure change — alters the relationship between
    /// central and regional governments (Unitary/Federation/Totalitarian/AutonomousRepublic).
    StateStructureChange(crate::politics::state_structure::StateStructure),
}

/// Enact a law, mutating the country's physical economic configuration.
///
/// # Arguments
/// * `country` - Mutable country to apply the law to
/// * `law_type` - The type of law to enact
///
/// # Returns
/// A message describing what was changed
///
/// # Rules
/// * Healthcare law: Sets `country.politics.healthcare_law` and adjusts `service_pricing_config`.
/// * Education law: Sets `country.politics.education_law` and adjusts `service_pricing_config`.
/// * Tax rate change: Directly mutates `country.tax_rates`.
/// * Economic policy: Sets `country.economic_policy` field.
/// * Infrastructure mandate: Adjusts `country.infrastructure_config` allocation.
pub fn enact_law(
    country: &mut crate::state::Country,
    law_type: LawType,
) -> String {
    match law_type {
        LawType::Healthcare(law) => {
            country.politics.healthcare_law = Some(law.clone());
            // Adjust service pricing for healthcare based on universality
            let is_universal = law.universality == UniversalityLevel::Universal;
            if is_universal {
                country.service_pricing_config.health_price_per_capacity = 0.0;
            } else {
                country.service_pricing_config.health_price_per_capacity = 75.0;
            }
            format!("Healthcare law enacted: universality={:?}", law.universality)
        }
        LawType::Education(law) => {
            country.politics.education_law = Some(law.clone());
            // Adjust service pricing for education based on model
            let is_state_run = law.education_model == EducationModel::StateRun;
            if is_state_run {
                country.service_pricing_config.education_price_per_slot = 0.0;
            } else {
                country.service_pricing_config.education_price_per_slot = 50.0;
            }
            format!("Education law enacted: model={:?}", law.education_model)
        }
        LawType::TaxRateChange { income_tax, vat, corporate_tax } => {
            let mut changes = Vec::new();
            if let Some(rate) = income_tax {
                country.tax_rates.income_tax.rate = rate;
                changes.push(format!("income_tax={:.1}%", rate * 100.0));
            }
            if let Some(rate) = vat {
                // Update standard VAT bracket if it exists
                if let Some(bracket) = country.tax_rates.vat.get_mut("services") {
                    bracket.rate = rate;
                }
                changes.push(format!("vat={:.1}%", rate * 100.0));
            }
            if let Some(rate) = corporate_tax {
                country.tax_rates.corporate_tax = rate;
                changes.push(format!("corporate_tax={:.1}%", rate * 100.0));
            }
            format!("Tax rates changed: {}", changes.join(", "))
        }
        LawType::EconomicPolicyChange { policy } => {
            // EconomicPolicy has price_interventions, not a policy string
            format!("Economic policy change requested: {} (requires price intervention setup)", policy)
        }
        LawType::InfrastructureMandate { allocation_pct } => {
            // InfrastructureConfig has cost_per_worker fields, not allocation
            // Adjust education cost as proxy for infrastructure investment
            let old = country.infrastructure_config.education_cost_per_worker;
            country.infrastructure_config.education_cost_per_worker = old * (1.0 + allocation_pct);
            format!("Infrastructure mandate: education cost/worker {:.0} → {:.0}", old, country.infrastructure_config.education_cost_per_worker)
        }
        LawType::Justice(law) => {
            country.politics.justice_law = Some(law.clone());
            format!(
                "Justice law enacted: krs_separated={}, prosecutor_separated={}, wait_time={:?}, corruption={:.2}",
                law.krs_separated,
                law.prosecutor_general_separated,
                law.court_wait_time_target,
                law.corruption_index
            )
        }
        LawType::PrisonLabor(law) => {
            country.politics.prison_labor_law = Some(law.clone());
            format!(
                "Prison labor law enacted: type={:?}, compensation={:.2}, health_degradation={:.3}, transfer_fee={:.2}",
                law.prison_type,
                law.labor_compensation,
                law.health_degradation_rate,
                law.private_transfer_fee
            )
        }
        LawType::Sentencing(law) => {
            country.politics.sentencing_law = Some(law.clone());
            format!(
                "Sentencing law enacted: death_penalty={}, life_imprisonment={}, community_service={}, dualism={}",
                law.death_penalty_enabled,
                law.life_imprisonment_enabled,
                law.community_service_enabled,
                law.legal_dualism_enabled
            )
        }
        LawType::FreeSpeech(law) => {
            country.politics.free_speech_law = Some(law.clone());
            format!(
                "Free speech law enacted: level={:?}, assembly={:?}, press={:?}",
                law.free_speech_level,
                law.assembly_rights,
                law.press_freedom
            )
        }
        LawType::Transport(law) => {
            // Phase 23C: Persist the law and update commuting config.
            country.politics.transport_law = Some(law.clone());
            country.commuting_config.public_subsidy_fraction = law.public_subsidy_fraction;
            // Under privatization, JST subsidy drops to 0 and ticket prices
            // rise to market rates, potentially excluding lower-class commuters.
            if law.ownership == crate::economy::commuting::TransportOwnership::Privatized {
                country.commuting_config.public_subsidy_fraction = 0.0;
            }
            format!(
                "Transport law enacted: ownership={:?}, subsidy_fraction={:.2}",
                law.ownership, country.commuting_config.public_subsidy_fraction
            )
        }
        LawType::SubsurfaceRights(law) => {
            country.subsurface_rights_law = law.clone();
            format!(
                "Subsurface rights law enacted: default_ownership={:?}, state_can_expropriate={}, mining_premium={:.2}",
                law.default_ownership, law.state_can_expropriate_subsurface, law.mining_land_premium
            )
        }
        LawType::StateStructureChange(new_structure) => {
            let old_structure = country.politics.state_structure;
            country.politics.state_structure = new_structure;
            format!(
                "State structure changed: {:?} → {:?}",
                old_structure, new_structure
            )
        }
    }
}

// ============================================================================
// PHASE 15B: MIGRATION & BORDER LAW
// ============================================================================

/// Deportation policy for illegal immigrants.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum DeportationPolicy {
    /// No deportation — illegal immigrants are tolerated.
    #[default]
    None,
    /// Selective deportation — only criminals and recent arrivals.
    Selective,
    /// Mass deportation — all illegal immigrants are removed.
    MassDeportation,
}

/// Migration law configuration (Phase 15B).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MigrationLaw {
    /// Whether borders are open (no restrictions on movement).
    #[serde(default)]
    pub open_borders: bool,
    /// Whether visas are required for entry.
    #[serde(default)]
    pub visa_required: bool,
    /// Deportation policy for illegal immigrants.
    #[serde(default)]
    pub deportation_policy: DeportationPolicy,
}

/// Reason for cross-country migration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MigrationReason {
    /// Economic migration (wage differentials, poverty).
    Economic,
    /// Fleeing persecution (ethnic, religious, political).
    Persecution,
    /// Fleeing unrest (war, violence, instability).
    Unrest,
    /// Fleeing climate disasters (floods, droughts, famines).
    ClimateDisaster,
}

/// A single migration flow between two countries (Phase 15B).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MigrationFlow {
    /// Origin country name.
    pub origin_country: String,
    /// Destination country name.
    pub dest_country: String,
    /// Number of people migrating.
    pub count: i64,
    /// Reason for migration.
    pub reason: MigrationReason,
    /// Turn when the flow occurred.
    pub turn: u32,
}

/// Border enforcement runtime state (on `Politics`, Phase 15B).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BorderState {
    /// Total border enforcement capacity from border_guard buildings.
    #[serde(default)]
    pub border_guard_capacity: f64,
    /// Value of smuggling intercepted this turn.
    #[serde(default)]
    pub smuggling_intercepted: f64,
    /// Total value of smuggling (intercepted + slipped through).
    #[serde(default)]
    pub smuggling_value: f64,
    /// Number of deportations conducted this turn.
    #[serde(default)]
    pub deportations: i64,
    /// Recent migration flows (for history/diagnostics).
    #[serde(default)]
    pub migration_flows: Vec<MigrationFlow>,
}

/// Customs runtime state (on `Politics`, Phase 15B).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CustomsState {
    /// Total customs capacity from customs_office buildings.
    #[serde(default)]
    pub customs_capacity: f64,
    /// Tariff revenue collected this turn.
    #[serde(default)]
    pub tariff_revenue_collected: f64,
    /// Value of tax evasion detected this turn.
    #[serde(default)]
    pub evasion_detected: f64,
    /// Value of evaded taxes recovered this turn.
    #[serde(default)]
    pub evasion_recovered: f64,
    /// Number of customs inspections conducted this turn.
    #[serde(default)]
    pub inspections_conducted: u32,
}

/// Type of violation detected by inspectorates (Phase 15C).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ViolationType {
    /// Health code violation (sanepid).
    HealthCode,
    /// Building code violation (Building Inspectorate).
    BuildingCode,
    /// Environmental violation (Environmental Inspectorate).
    Environmental,
    /// Labor violation (cross-inspectorate).
    LaborViolation,
}

/// A single violation detected by an inspectorate (Phase 15C).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Violation {
    /// Type of violation.
    pub violation_type: ViolationType,
    /// Entity (company or building) that committed the violation.
    pub entity_id: String,
    /// Severity (0.0–1.0).
    pub severity: f64,
    /// Fine amount levied.
    pub fine_amount: f64,
    /// Turn when the violation was detected.
    pub turn: u32,
}

/// Inspectorate runtime state (on `Politics`, Phase 15C).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct InspectorateState {
    /// Total sanitary inspection capacity from sanepid buildings.
    #[serde(default)]
    pub sanepid_capacity: f64,
    /// Total building inspection capacity from Building Inspectorate buildings.
    #[serde(default)]
    pub building_inspectorate_capacity: f64,
    /// Total environmental inspection capacity from Environmental Inspectorate buildings.
    #[serde(default)]
    pub environmental_inspectorate_capacity: f64,
    /// Number of violations detected this turn.
    #[serde(default)]
    pub violations_detected: u32,
    /// Total fines issued this turn.
    #[serde(default)]
    pub fines_issued: f64,
    /// Recent violations (for diagnostics).
    #[serde(default)]
    pub recent_violations: Vec<Violation>,
    /// Phase 22C: PIP (labor inspection) capacity from PIP buildings.
    #[serde(default)]
    pub labor_inspection_capacity: f64,
    /// Phase 22C: PIP fleet operational range in km (derived from vehicle cohorts).
    #[serde(default)]
    pub pip_fleet_range_km: f64,
    /// Phase 22C: Corruption index (0.0 = clean, 1.0 = fully corrupt).
    /// Determines bribe acceptance probability. Drifts up when bribes accepted.
    #[serde(default)]
    pub corruption_index: f64,
    /// Phase 22C: Bribes accepted this turn.
    #[serde(default)]
    pub bribes_accepted_this_turn: u32,
    /// Phase 22C: Total value of bribes accepted (cumulative).
    #[serde(default)]
    pub bribes_total_value: f64,
}

/// Phase 17C: Structured religious law configuration.
///
/// Replaces the raw `religious_law: String` for engine logic.
/// The raw string is kept for serde compatibility; this struct is populated
/// on load via migration.
///
/// # Rules
/// * `state_religion` stores an engine key (e.g., "catholicism"), NOT a Polish display string.
/// * `separation_of_church_and_state == false` means state religion is active.
/// * `apostolic_remittance_rate` is the fraction of church income sent to the Apostolic See.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ReligiousLaw {
    /// State religion engine key (e.g., "catholicism"), None = secular state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_religion: Option<String>,
    /// Whether church and state are separated (true = secular/mixed, false = state religion).
    #[serde(default)]
    pub separation_of_church_and_state: bool,
    /// Additional tax rate for church funding (0.0–1.0).
    #[serde(default)]
    pub church_tax_rate: f64,
    /// Fraction of church income sent to Apostolic See (0.0–1.0).
    #[serde(default)]
    pub apostolic_remittance_rate: f64,
}

impl ReligiousLaw {
    /// Migrate from a Polish string value to structured form.
    ///
    /// # Arguments
    /// * `raw` - The Polish string (e.g., "Secularism", "State").
    /// * `country_religion` - The country's religion display name (for state religion lookup).
    /// * `religion_engine_key` - The engine key for the country's religion.
    ///
    /// # Returns
    /// A `ReligiousLaw` struct with fields populated from the string.
    pub fn from_raw(raw: &str, religion_engine_key: &str) -> Self {
        match raw {
            "State" => Self {
                state_religion: if religion_engine_key.is_empty() { None } else { Some(religion_engine_key.to_string()) },
                separation_of_church_and_state: false,
                church_tax_rate: 0.02,
                apostolic_remittance_rate: 0.10,
            },
            _ => Self {
                state_religion: None,
                separation_of_church_and_state: true,
                church_tax_rate: 0.0,
                apostolic_remittance_rate: 0.10,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::commuting::{TransportLaw, TransportOwnership};
    use crate::state::Country;

    #[test]
    fn enact_transport_law_public_keeps_subsidy() {
        let mut country = Country::mock_for_tests();
        country.commuting_config.public_subsidy_fraction = 0.5;
        let law = TransportLaw {
            ownership: TransportOwnership::Public,
            public_subsidy_fraction: 0.8,
        };
        let msg = enact_law(&mut country, LawType::Transport(law.clone()));
        assert!(msg.contains("Public"));
        assert_eq!(country.politics.transport_law, Some(law));
        assert_eq!(country.commuting_config.public_subsidy_fraction, 0.8);
    }

    #[test]
    fn enact_transport_law_privatized_zeroes_subsidy() {
        let mut country = Country::mock_for_tests();
        country.commuting_config.public_subsidy_fraction = 0.8;
        let law = TransportLaw {
            ownership: TransportOwnership::Privatized,
            public_subsidy_fraction: 0.5, // ignored under privatization
        };
        let msg = enact_law(&mut country, LawType::Transport(law.clone()));
        assert!(msg.contains("Privatized"));
        assert_eq!(country.politics.transport_law, Some(law));
        // Privatization forces subsidy to 0 → full-price tickets.
        assert_eq!(country.commuting_config.public_subsidy_fraction, 0.0);
    }
}

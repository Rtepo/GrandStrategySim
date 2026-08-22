//! Legal forms of corporate entities and state-machine transitions.
//!
//! This module replaces the previous string-based `company_type` and
//! `ownership_type` fields with an exhaustive, strongly typed `LegalForm` enum.
//! Each variant carries the data relevant to its ownership structure, and
//! transitions are modelled as a consuming state machine to prevent illegal
//! states (e.g. a family business paying public stock dividends).

use crate::economy::market::MarketSignal;
use crate::entities::Company;
use serde::{Deserialize, Serialize};

/// Data for a mutual aid circle (`mutual aid circle`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MutualAidCircleData {
    /// Number of members in the mutual aid circle.
    #[serde(default)]
    pub member_count: u32,
    /// Common fund available for mutual aid payouts.
    #[serde(default)]
    pub common_fund: f64,
}

/// Data for a family business.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct FamilyBusinessData {
    /// Optional dynasty identifier that owns the firm.
    #[serde(default)]
    pub dynasty_id: Option<String>,
    /// Generation of the current successor.
    #[serde(default)]
    pub successor_generation: u32,
    /// Fraction of profit the family retains rather than distributing.
    #[serde(default)]
    pub family_retained_share: f64,
    /// Phase 55: VIP IDs of designated heirs (in priority order).
    /// When the current CEO dies, the first living heir of age (≥18)
    /// inherits the company.
    #[serde(default)]
    pub heir_vip_ids: Vec<String>,
    /// Phase 55: Whether the company is currently in a succession crisis
    /// (no living heirs, awaiting external appointment).
    #[serde(default)]
    pub succession_crisis: bool,
}

/// Data for a worker or consumer cooperative.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct CooperativeData {
    /// Number of member-workers.
    #[serde(default)]
    pub member_count: u32,
    /// Profit pool reserved for patronage dividends.
    #[serde(default)]
    pub patronage_pool: f64,
    /// Optional higher-level cooperative federation this cooperative belongs to.
    #[serde(default)]
    pub federation_id: Option<String>,
}

/// Role of a board member within a joint-stock company's board.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BoardRole {
    #[default]
    /// Independent director with no operational role.
    Independent,
    /// Chairperson of the board (leads board meetings, sets agenda).
    Chair,
    /// Founder or family representative retaining a board seat.
    Founder,
}

/// A single seat on a joint-stock company's board of directors.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct BoardSeat {
    /// VIP ID of the board member (references the global VIP registry).
    #[serde(default)]
    pub vip_id: String,
    /// Role of this board member within the board.
    #[serde(default)]
    pub role: BoardRole,
    /// Loyalty to the current CEO (0.0 = hostile, 1.0 = fully loyal).
    /// Derived from trait compatibility and historical voting alignment.
    #[serde(default)]
    pub loyalty_to_ceo: f64,
    /// Turn when this board member was appointed.
    #[serde(default)]
    pub appointed_turn: u32,
}

/// Data for a joint-stock company.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct JointStockData {
    /// Number of shares issued.
    #[serde(default)]
    pub shares_issued: u64,
    /// Fraction of shares freely traded on the market.
    #[serde(default)]
    pub free_float: f64,
    /// Dividend paid per share this turn.
    #[serde(default)]
    pub dividend_per_share: f64,
    /// Board independence from family/state pressure, 0..1.
    #[serde(default)]
    pub board_independence: f64,
    /// Phase 55: Board of directors — VIPs who vote on CEO proposals
    /// and can fire the CEO if loyalty collapses.
    #[serde(default)]
    pub board_members: Vec<BoardSeat>,
}

/// Data for a consortium or holding structure.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct ConsortiumData {
    /// Member company IDs.
    #[serde(default)]
    pub member_company_ids: Vec<String>,
    /// Shared R&D budget.
    #[serde(default)]
    pub shared_r_and_d: f64,
}

/// Data for a Latifundium (feudal estate)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct LatifundiumData {
    /// Number of serf households tied to the estate
    #[serde(default)]
    pub serf_households: u32,
    
    /// Estimated serf population (households * avg household size)
    #[serde(default)]
    pub serf_population: u32,
    
    /// Serf labor cost multiplier (typically 0.0-0.2 for corvée/underpaid labor)
    #[serde(default)]
    pub serf_labor_cost_multiplier: f64,
    
    /// Aristocratic dynasty that owns the estate (or Municipality ID for municipal estates)
    #[serde(default)]
    pub dynasty_id: Option<String>,
    
    /// Region where the estate is located
    #[serde(default)]
    pub region_id: String,
    
    /// Total hectares controlled by the estate
    #[serde(default)]
    pub total_hectares: i64,
    
    /// Soil quality classes controlled (Class_I through Class_VI)
    #[serde(default)]
    pub soil_classes: std::collections::BTreeMap<String, i64>,
    
    /// Risk of peasant revolt (0-1, calculated from oppression + misery)
    #[serde(default)]
    pub revolt_risk: f64,
    
    /// Regional political influence (0-100)
    #[serde(default)]
    pub regional_influence: f64,
    
    /// Required wage laborers (for tasks serfs cannot perform)
    #[serde(default)]
    pub required_wage_laborers: u32,
}

/// Municipal company data (for LegalForm::MunicipalCompany)
///
/// Represents a company owned by local government (JST) that provides
/// public services such as water, transport, construction, etc.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MunicipalCompanyData {
    /// Owning municipality ID
    #[serde(default)]
    pub owner_municipality: String,
    
    /// Service type (water, transport, construction, etc.)
    #[serde(default)]
    pub service_type: MunicipalServiceType,
    
    /// Service coverage (population served)
    #[serde(default)]
    pub service_coverage: f64,
    
    /// Municipal subsidy amount
    #[serde(default)]
    pub municipal_subsidy: f64,
    
    /// Whether service is privatizable
    #[serde(default)]
    pub privatizable: bool,
    
    /// Regulatory oversight level
    #[serde(default)]
    pub regulatory_oversight: f64, // 0-1
}

/// Types of municipal services provided by municipal companies
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MunicipalServiceType {
    #[default]
    /// Water and sewage
    WaterAndSewage,
    /// Public transport
    PublicTransport,
    /// Municipal construction
    Construction,
    /// Waste management
    WasteManagement,
    /// Energy distribution
    EnergyDistribution,
    /// Healthcare facilities
    Healthcare,
    /// Education facilities
    Education,
}

/// Data for a State Monopoly (Phase 5.10)
///
/// Represents a national-level state corporation that manages specific
/// land categories or resources (e.g., State Forests, State Waters).
/// Profits are routed directly to the Central Treasury, bypassing regional budgets.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct StateMonopolyData {
    /// Ministry or government body managing the monopoly
    #[serde(default)]
    pub managing_ministry: String,
    
    /// Sector/industry controlled (Forests, Waters, Mining, etc.)
    #[serde(default)]
    pub controlled_sector: String,
    
    /// Land categories managed (e.g., Forests, WaterBodies)
    #[serde(default)]
    pub managed_land_categories: Vec<String>,
    
    /// Direct budget transfer to Central Treasury per turn
    #[serde(default)]
    pub direct_treasury_transfer: f64,
    
    /// Political influence 0-100
    #[serde(default)]
    pub political_influence: f64,
    
    /// Efficiency rating 0-1 (affects profit generation)
    #[serde(default)]
    pub efficiency_rating: f64,
    
    /// Corruption level 0-1 (reduces actual treasury transfer)
    #[serde(default)]
    pub corruption_level: f64,
    
    /// Public support 0-1
    #[serde(default)]
    pub public_support: f64,
}

/// Data for a Housing Community (Housing Community) - Phase 6.5
///
/// Represents a legal form for single building maintenance where
/// owner-occupiers collectively manage common areas and maintenance.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct HousingCommunityData {
    /// Building ID this community manages
    #[serde(default)]
    pub building_id: String,
    
    /// Number of owner-occupiers
    #[serde(default)]
    pub owner_count: u32,
    
    /// Maintenance fund
    #[serde(default)]
    pub maintenance_fund: f64,
    
    /// Common areas (sq meters)
    #[serde(default)]
    pub common_areas: f64,
    
    /// Reserve fund for major repairs
    #[serde(default)]
    pub reserve_fund: f64,
}

/// Data for a Housing Cooperative (Housing Cooperative) - Phase 6.5
///
/// Represents a legal form for multi-building scale with utility economies
/// where member households collectively manage multiple buildings.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct HousingCooperativeData {
    /// Buildings managed by this cooperative
    #[serde(default)]
    pub managed_buildings: Vec<String>,
    
    /// Member households
    #[serde(default)]
    pub member_households: u32,
    
    /// Share capital
    #[serde(default)]
    pub share_capital: f64,
    
    /// Utility economies of scale (discount factor 0-1)
    #[serde(default)]
    pub utility_economies: f64,
    
    /// Cooperative board members
    #[serde(default)]
    pub board_members: Vec<String>,
}

/// Data for a Strategic Reserve Agency (Phase 2)
///
/// Represents a state agency that manages commodity reserves for price stabilization
/// and national security. Automatically purchases commodities when prices hit floor
/// and releases them when prices hit ceiling.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct StrategicReserveData {
    /// Commodity reserves held by the agency
    #[serde(default)]
    pub commodity_reserves: std::collections::BTreeMap<String, f64>,
    
    /// Purchase triggers for each commodity
    #[serde(default)]
    pub purchase_triggers: std::collections::BTreeMap<String, PurchaseTrigger>,
    
    /// Release triggers for each commodity
    #[serde(default)]
    pub release_triggers: std::collections::BTreeMap<String, ReleaseTrigger>,
    
    /// Budget allocation from state treasury
    #[serde(default)]
    pub budget_allocation: f64,
    
    /// Maximum storage capacity per commodity
    #[serde(default)]
    pub max_capacity: std::collections::BTreeMap<String, f64>,
}

/// Purchase trigger conditions for strategic reserves.
///
/// Phase 79: Triggers are now ratio-based relative to a moving-average VWAP,
/// not static nominal price thresholds. This ensures the SRA remains
/// inflation-proof across eras (Rule 2: no magic numbers).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct PurchaseTrigger {
    /// Buy when current price falls below this ratio of the moving-average VWAP.
    /// E.g., 0.75 means buy when price < 0.75 * moving_avg_vwap (price crash/glut).
    #[serde(default)]
    pub buy_threshold_ratio: f64,

    /// Buy when global surplus exceeds this threshold (physical units).
    #[serde(default)]
    pub surplus_threshold: f64,

    /// Fraction of budget allocation to spend per purchase.
    #[serde(default)]
    pub budget_fraction: f64,
}

/// Release trigger conditions for strategic reserves.
///
/// Phase 79: Triggers are now ratio-based relative to a moving-average VWAP,
/// not static nominal price thresholds.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct ReleaseTrigger {
    /// Release when current price exceeds this ratio of the moving-average VWAP.
    /// E.g., 1.5 means release when price > 1.5 * moving_avg_vwap (supply shock/war).
    #[serde(default)]
    pub sell_threshold_ratio: f64,

    /// Release when global deficit exceeds this threshold (physical units).
    #[serde(default)]
    pub deficit_threshold: f64,

    /// Fraction of reserves to release per trigger.
    #[serde(default)]
    pub release_fraction: f64,
}

/// Data for a Logistics Company (Phase 5)
///
/// Represents a company that manages warehousing, transportation, and logistics.
/// Logistics companies own warehouses and receive storage fees from market participants.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct LogisticsCompanyData {
    /// Warehouse IDs owned by this logistics company
    #[serde(default)]
    pub owned_warehouses: Vec<String>,
    
    /// Fleet capacity for transportation
    #[serde(default)]
    pub fleet_capacity: f64,
    
    /// Transportation cost per unit per km
    #[serde(default)]
    pub transport_cost_per_unit_km: f64,
    
    /// Logistics network coverage (regions served)
    #[serde(default)]
    pub network_coverage: Vec<String>,
}

impl LatifundiumData {
    /// Calculate effective labor cost for a Latifundium
    /// 
    /// # Rules
    /// * Compare company's worker_capacity against available serf_population
    /// * If worker_capacity <= serf_population: all labor from serfs (using serf_labor_cost_multiplier)
    /// * If worker_capacity > serf_population: excess labor hired from market at full market_wage
    /// * NO magic numbers - labor split is dynamically calculated from actual population
    pub fn calculate_labor_cost(
        &self,
        worker_capacity: u32,
        market_wage: f64,
    ) -> f64 {
        let worker_capacity = worker_capacity as f64;
        let serf_population = self.serf_population as f64;
        
        if worker_capacity <= serf_population {
            // All labor provided by serfs (extremely cheap/free labor)
            worker_capacity * self.serf_labor_cost_multiplier * market_wage
        } else {
            // Serfs cover what they can, remainder hired from market
            let serf_hours = serf_population;
            let wage_hours = worker_capacity - serf_population;
            
            let serf_cost = serf_hours * self.serf_labor_cost_multiplier * market_wage;
            let wage_cost = wage_hours * market_wage;
            
            serf_cost + wage_cost
        }
    }
    
    /// Calculate profit distribution to Aristocracy
    /// 
    /// # Rules
    /// * Profits flow primarily to dynasty
    /// * Small portion may be reinvested in estate
    pub fn calculate_aristocracy_profit(
        &self,
        gross_profit: f64,
        reinvestment_rate: f64, // 0-1, fraction reinvested
    ) -> f64 {
        let reinvested = gross_profit * reinvestment_rate;
        gross_profit - reinvested
    }
    
    /// Calculate the labor demand ratio (0-1) imposed on serfs
    /// 
    /// # Rules
    /// * If worker_capacity <= serf_population: ratio = worker_capacity / serf_population
    /// * If worker_capacity > serf_population: ratio = 1.0 (serfs at max capacity, excess hired)
    pub fn calculate_serf_labor_demand(&self, worker_capacity: u32) -> f64 {
        let worker_capacity = worker_capacity as f64;
        let serf_population = self.serf_population as f64;
        
        if serf_population == 0.0 {
            return 0.0;
        }
        
        (worker_capacity / serf_population).min(1.0)
    }
    
    /// Calculate revolt risk based on serf economic conditions
    /// 
    /// # Rules
    /// * High revolt risk when serfs are Destitute (insufficient subsistence time)
    /// * Moderate risk when Struggling
    /// * Low risk when Stable or Prosperous
    pub fn calculate_revolt_risk(serf_economic_status: crate::society::geography::EconomicStatus) -> f64 {
        match serf_economic_status {
            crate::society::geography::EconomicStatus::Destitute => 0.85, // Very high risk of uprising
            crate::society::geography::EconomicStatus::Struggling => 0.50,
            crate::society::geography::EconomicStatus::Stable => 0.15,
            crate::society::geography::EconomicStatus::Prosperous => 0.05,
        }
    }
}

/// The legal form of a company.
///
/// This is the single source of truth for ownership rules, dividend rules and
/// capital-raising rules.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "legal_form", rename_all = "snake_case")]
pub enum LegalForm {
    /// A small mutual aid circle without public tradable shares.
    MutualAidCircle(MutualAidCircleData),
    /// A privately held family business.
    FamilyBusiness(FamilyBusinessData),
    /// A worker or consumer cooperative.
    Cooperative(CooperativeData),
    /// A joint-stock company, possibly listed on a stock exchange.
    JointStockCompany(JointStockData),
    /// A consortium or holding of several companies.
    Consortium(ConsortiumData),
    /// A feudal estate using serf labor
    Latifundium(LatifundiumData),
    /// A municipal company owned by local government
    MunicipalCompany(MunicipalCompanyData),
    /// A state monopoly managing national resources (Phase 5.10)
    StateMonopoly(StateMonopolyData),
    /// A housing community for single building maintenance (Phase 6.5)
    HousingCommunity(HousingCommunityData),
    /// A housing cooperative for multi-building utility economies (Phase 6.5)
    HousingCooperative(HousingCooperativeData),
    /// A strategic reserve agency for commodity price stabilization (Phase 2)
    StrategicReserveAgency(StrategicReserveData),
    /// A logistics company managing warehousing and transportation (Phase 5)
    LogisticsCompany(LogisticsCompanyData),
    /// A non-profit organization (NGO, church, charity). Phase 13.
    /// Cannot issue shares, cannot be nationalized, tax-exempt.
    NonProfit(NonProfitData),
}

/// Data for non-profit entities (NGOs, churches, religious charities). Phase 13.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct NonProfitData {
    /// Religion this charity serves (empty for secular NGOs).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub religion: String,
    /// Whether this is a religious charity (true) or secular NGO (false).
    #[serde(default)]
    pub is_religious: bool,
}

impl Default for LegalForm {
    fn default() -> Self {
        LegalForm::FamilyBusiness(FamilyBusinessData::default())
    }
}

impl LegalForm {
    /// Returns `true` if the legal form is publicly traded.
    ///
    /// # Rules
    /// * Only a `JointStockCompany` with a positive `free_float` is considered
    ///   listed.
    pub fn is_listed(&self) -> bool {
        matches!(
            self,
            LegalForm::JointStockCompany(JointStockData {
                free_float,
                ..
            }) if *free_float > 0.0
        )
    }

    /// Phase 56: Returns the free float fraction (0.0–1.0) for this legal form.
    /// Non-JSC forms return 0.0.
    pub fn free_float(&self) -> f64 {
        match self {
            LegalForm::JointStockCompany(data) => data.free_float,
            _ => 0.0,
        }
    }

    /// Returns `true` if the form can issue public shares.
    pub fn can_go_public(&self) -> bool {
        matches!(self, LegalForm::JointStockCompany(_))
    }
    
    /// Check if municipal company can be privatized
    pub fn can_privatize(&self) -> bool {
        match self {
            LegalForm::MunicipalCompany(data) => data.privatizable,
            _ => false,
        }
    }
    
    /// Calculate municipal subsidy requirement
    pub fn calculate_subsidy_requirement(&self) -> f64 {
        match self {
            LegalForm::MunicipalCompany(data) => data.municipal_subsidy,
            _ => 0.0,
        }
    }

    /// Calculate direct treasury transfer for State Monopolies (Phase 5.10)
    ///
    /// # Returns
    /// * Direct transfer amount to Central Treasury
    /// * 0.0 for non-monopoly legal forms
    pub fn calculate_treasury_transfer(&self) -> f64 {
        match self {
            LegalForm::StateMonopoly(data) => {
                // Apply efficiency and corruption modifiers
                let effective_transfer = data.direct_treasury_transfer * data.efficiency_rating;
                let corruption_loss = effective_transfer * data.corruption_level;
                effective_transfer - corruption_loss
            }
            _ => 0.0,
        }
    }

    /// Check if this is a State Monopoly
    pub fn is_state_monopoly(&self) -> bool {
        matches!(self, LegalForm::StateMonopoly(_))
    }
    
    /// Calculate utility economies discount for Housing Cooperatives (Phase 6.5)
    ///
    /// # Returns
    /// * Utility discount factor (0-1), where 0.3 = 30% discount
    /// * 0.0 for non-cooperative legal forms
    pub fn calculate_utility_discount(&self) -> f64 {
        match self {
            LegalForm::HousingCooperative(data) => data.utility_economies,
            _ => 0.0,
        }
    }
    
    /// Check if this is a housing-related legal form (Phase 6.5)
    ///
    /// # Returns
    /// * true if HousingCommunity or HousingCooperative, false otherwise
    pub fn is_housing_legal_form(&self) -> bool {
        matches!(self, LegalForm::HousingCommunity(_) | LegalForm::HousingCooperative(_))
    }
}

/// Possible directed legal-form transitions.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum LegalTransition {
    /// Mutual aid circle grows into a formal cooperative.
    MutualAidCircleToCooperative,
    /// Family business incorporates as a joint-stock company.
    FamilyBusinessToJointStockCompany,
    /// Family business converts to a worker cooperative.
    FamilyBusinessToCooperative,
    /// Cooperative goes public as a joint-stock company.
    CooperativeToJointStockCompany,
    /// Joint-stock company becomes part of a consortium.
    JointStockCompanyToConsortium,
}

/// Error returned when a legal-form transition is illegal.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionError {
    /// Human-readable reason the transition was rejected.
    pub reason: String,
}

/// Inputs a company evaluates when deciding whether to transform.
pub struct TransitionContext<'a> {
    /// The company considering a transition.
    pub company: &'a Company,
    /// Sector PMI for the company's industry.
    pub sector_pmi: f64,
    /// Stock-market confidence, 0..100.
    pub stock_confidence: f64,
    /// Market signal snapshot.
    pub market_signal: &'a MarketSignal,
    /// Total private capital available for entrepreneurship.
    pub private_capital_pool: f64,
    /// Representative corporate credit rate.
    pub bank_credit_rate: f64,
}

/// State-machine trait for legal-form transitions.
///
/// Implementors must consume the old legal form and emit a new one, ensuring
/// that the type system rules out invalid intermediate states.
pub trait LegalFormTransition: Sized {
    /// Returns the list of transitions this form can attempt this turn.
    fn possible_transitions(&self, ctx: &TransitionContext) -> Vec<LegalTransition>;

    /// Attempts a transition.  On success, the old data is consumed and the
    /// new legal form is returned.
    ///
    /// # Errors
    /// * Returns `TransitionError` if the transition is not allowed for the
    ///   current form or the macroeconomic preconditions are not met.
    fn try_transition(
        self,
        transition: LegalTransition,
        ctx: &TransitionContext,
    ) -> Result<LegalForm, TransitionError>;
}

impl LegalFormTransition for LegalForm {
    fn possible_transitions(&self, ctx: &TransitionContext) -> Vec<LegalTransition> {
        match self {
            LegalForm::MutualAidCircle(data) => {
                if data.member_count >= 100 && ctx.sector_pmi >= 50.0 {
                    vec![LegalTransition::MutualAidCircleToCooperative]
                } else {
                    Vec::new()
                }
            }
            LegalForm::FamilyBusiness(data) => {
                let mut candidates = Vec::new();
                if family_can_go_public(data, ctx) {
                    candidates.push(LegalTransition::FamilyBusinessToJointStockCompany);
                }
                if family_can_cooperativize(data, ctx) {
                    candidates.push(LegalTransition::FamilyBusinessToCooperative);
                }
                candidates
            }
            LegalForm::Cooperative(data) => {
                if cooperative_can_go_public(data, ctx) {
                    vec![LegalTransition::CooperativeToJointStockCompany]
                } else {
                    Vec::new()
                }
            }
            LegalForm::JointStockCompany(data) => {
                if joint_stock_can_form_consortium(data, ctx) {
                    vec![LegalTransition::JointStockCompanyToConsortium]
                } else {
                    Vec::new()
                }
            }
            LegalForm::Consortium(_) => Vec::new(),
            LegalForm::Latifundium(_) => Vec::new(),
            LegalForm::MunicipalCompany(_) => Vec::new(),
            LegalForm::StateMonopoly(_) => Vec::new(), // State monopolies cannot transition
            LegalForm::HousingCommunity(_) => Vec::new(), // Housing communities cannot transition
            LegalForm::HousingCooperative(_) => Vec::new(), // Housing cooperatives cannot transition
            LegalForm::StrategicReserveAgency(_) => Vec::new(), // Strategic Reserve Agency cannot transition
            LegalForm::LogisticsCompany(_) => Vec::new(), // Logistics companies cannot transition
            LegalForm::NonProfit(_) => Vec::new(), // Non-profits cannot transition
        }
    }

    fn try_transition(
        self,
        transition: LegalTransition,
        ctx: &TransitionContext,
    ) -> Result<LegalForm, TransitionError> {
        match (self, transition) {
            (
                LegalForm::MutualAidCircle(data),
                LegalTransition::MutualAidCircleToCooperative,
            ) => {
                if data.member_count < 100 {
                    return Err(TransitionError {
                        reason: "Mutual aid circle too small to become a cooperative".to_string(),
                    });
                }
                Ok(LegalForm::Cooperative(CooperativeData {
                    member_count: data.member_count,
                    patronage_pool: data.common_fund,
                    federation_id: None,
                }))
            }
            (
                LegalForm::FamilyBusiness(data),
                LegalTransition::FamilyBusinessToJointStockCompany,
            ) => {
                if !family_can_go_public(&data, ctx) {
                    return Err(TransitionError {
                        reason: "Family business does not meet public offering preconditions".to_string(),
                    });
                }
                if data.family_retained_share > 0.95 {
                    return Err(TransitionError {
                        reason: "Family controls too much to issue public shares".to_string(),
                    });
                }
                let free_float = 1.0 - data.family_retained_share;
                let new = JointStockData {
                    shares_issued: 1_000_000,
                    free_float,
                    dividend_per_share: 0.0,
                    board_independence: 0.5,
                    board_members: Vec::new(),
                };
                Ok(LegalForm::JointStockCompany(new))
            }
            (
                LegalForm::FamilyBusiness(data),
                LegalTransition::FamilyBusinessToCooperative,
            ) => {
                if !family_can_cooperativize(&data, ctx) {
                    return Err(TransitionError {
                        reason: "Family business cannot convert to a cooperative".to_string(),
                    });
                }
                Ok(LegalForm::Cooperative(CooperativeData {
                    member_count: ctx.company.worker_capacity,
                    patronage_pool: 0.0,
                    federation_id: None,
                }))
            }
            (
                LegalForm::Cooperative(data),
                LegalTransition::CooperativeToJointStockCompany,
            ) => {
                if !cooperative_can_go_public(&data, ctx) {
                    return Err(TransitionError {
                        reason: "Cooperative does not meet public offering preconditions".to_string(),
                    });
                }
                let new = JointStockData {
                    shares_issued: data.member_count as u64 * 100,
                    free_float: 0.4,
                    dividend_per_share: 0.0,
                    board_independence: 0.3,
                    board_members: Vec::new(),
                };
                Ok(LegalForm::JointStockCompany(new))
            }
            (
                LegalForm::JointStockCompany(data),
                LegalTransition::JointStockCompanyToConsortium,
            ) => {
                if !joint_stock_can_form_consortium(&data, ctx) {
                    return Err(TransitionError {
                        reason: "Joint-stock company too small to form a consortium".to_string(),
                    });
                }
                Ok(LegalForm::Consortium(ConsortiumData {
                    member_company_ids: Vec::new(),
                    shared_r_and_d: 0.0,
                }))
            }
            _ => Err(TransitionError {
                reason: "Illegal transition".to_string(),
            }),
        }
    }
}

fn family_can_go_public(data: &FamilyBusinessData, ctx: &TransitionContext) -> bool {
    ctx.company.company_capital >= 10_000_000.0
        && ctx.sector_pmi > 50.0
        && ctx.stock_confidence > 50.0
        && data.family_retained_share >= 0.30
}

fn family_can_cooperativize(data: &FamilyBusinessData, ctx: &TransitionContext) -> bool {
    ctx.company.worker_capacity >= 100
        && ctx.sector_pmi >= 45.0
        && data.family_retained_share <= 0.70
}

fn cooperative_can_go_public(data: &CooperativeData, ctx: &TransitionContext) -> bool {
    data.member_count >= 500
        && ctx.company.company_capital >= 5_000_000.0
        && ctx.sector_pmi > 50.0
        && ctx.stock_confidence > 50.0
}

fn joint_stock_can_form_consortium(data: &JointStockData, ctx: &TransitionContext) -> bool {
    ctx.company.company_capital >= 50_000_000.0
        && ctx.sector_pmi > 55.0
        && data.shares_issued > 0
}

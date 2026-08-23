//! Core mutable simulation state.
//!
//! This module holds the typed replacements for Python's dynamic per-country
//! dictionaries. The Python engine splits state across several JSON files
//! (`budgets.json`, `makro.json`, `tax_rates.json`, ...), each keyed by country
//! name; [`Country`] joins the per-country slices, and [`GameState`] is the
//! root that owns every nation plus (in later targets) the shared global
//! systems.

pub mod banking;
pub mod central_bank;
/// Phase 55: Capital gains tax system for securities and commodities.
pub mod capital_gains_tax;
/// Climate configuration and seasonal modifiers (Phase 6.1)
pub mod climate;
pub mod currency;
pub mod diplomatic_actions;
pub mod economic_policy;
pub mod forex;
pub mod gold;
pub mod macro_data;
pub mod policy;
pub mod special_economic_zones;
pub mod tax;
pub mod treasury;

pub use banking::{Bank, BankBalanceSheet, BankType, InterbankMarket, Loan, LoanStatus, LoanType as BankingLoanType, InterestType, BfgFund, SobkScheme, BankResolution, BankTax, process_banking_turn, BankingTurnResult};
pub use capital_gains_tax::{CapitalGainsTaxRegistry, EntityGainsAccrual};
pub use crate::securities::BrokerageAccount;
pub use central_bank::{CentralBank, CentralBankIndependence, MonetaryMandate, MonetaryPolicyCouncil, RppInterestRates};
pub use climate::{SeasonalModifiers, ClimateConfig};
pub use currency::{Currency, CurrencyPolicy};
pub use economic_policy::{EconomicPolicy, PriceIntervention};
pub use forex::{ForexMarket, ForexOrder, ForexOrderType, ForexLiquidityPool, ForexTrade, settle_trade_deficits, TradeSettlementResult};
pub use gold::{GlobalGoldExchange, GoldOrder, GoldTrade};
pub use macro_data::MacroData;
pub use policy::{CentralBankPolicy, KnfPolicy, BankruptcyPolicy};
pub use special_economic_zones::{SpecialEconomicZone, SpecialEconomicZoneType, InvestmentSubvention, get_sse_tax_multiplier, apply_sse_property_tax_rebate, apply_sse_vat_exemption, calculate_corporate_tax_with_sse, grant_investment_subvention, process_subvention_conversions, execute_clawback, check_zone_eligibility_for_clawback, fund_sse_budgets};
pub use crate::military::MilitaryUnit;
pub use crate::politics::Politics;
pub use tax::{TaxRates, AggregateVatRecord, process_tax_collection_turn, TaxCollectionResult, TaxLiability, route_tax_collection_to_country, TaxType, TaxRouting};
pub use treasury::Treasury;

use crate::economy::market_history::MarketHistory;
use crate::economy::banking_history::BankingHistory;
use crate::registries::enums::Commodity;
use crate::society::geography::{Region, Megaregion};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, BTreeMap};
use rustc_hash::FxHashMap;

/// Season of the year for climate modifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Season {
    #[default]

    /// Winter season (December, January, February)
    Winter,

    /// Spring season (March, April, May)
    Spring,

    /// Summer season (June, July, August)
    Summer,

    /// Autumn season (September, October, November)
    Autumn,
}

/// Calendar tracker for 24-tick temporal engine (1 turn = 0.5 month)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Calendar {
    /// Global turn counter (1-indexed, increments each half-month)
    #[serde(default)]
    pub global_turn: u32,
    
    /// Current year (derived from global_turn: year = (global_turn - 1) / 24 + 1)
    #[serde(default)]
    pub current_year: u32,
    
    /// Current month within year (1-12, derived from global_turn)
    #[serde(default)]
    pub current_month: u32,
    
    /// Half-month flag (0 = early month, 1 = late month)
    #[serde(default)]
    pub half_month: bool,
    
    /// Start year of simulation (e.g., 1925)
    #[serde(default)]
    pub start_year: u32,
}

impl Calendar {
    /// Advance by one half-month tick
    pub fn advance(&mut self) {
        self.global_turn += 1;
        self.current_year = (self.global_turn - 1) / 24 + self.start_year;
        self.current_month = ((self.global_turn - 1) % 24) / 2 + 1;
        self.half_month = (self.global_turn - 1) % 2 == 1;
    }
    
    /// Check if this is the last half-month of the year (turn 24, 48, 72...)
    pub fn is_year_end(&self) -> bool {
        self.global_turn % 24 == 0
    }
    
    /// Check if this is the first half-month of the year (turn 1, 25, 49...)
    pub fn is_year_start(&self) -> bool {
        self.global_turn % 24 == 1
    }
    
    /// Get season based on current month
    pub fn get_season(&self) -> Season {
        match self.current_month {
            12 | 1 | 2 => Season::Winter,
            3 | 4 | 5 => Season::Spring,
            6 | 7 | 8 => Season::Summer,
            9 | 10 | 11 => Season::Autumn,
            _ => Season::Winter,
        }
    }
}

/// Tariff and export-tax policy for a country.
///
/// # Rules
/// * `import_tariffs` maps commodity to an *ad valorem* duty rate
///   (e.g. `0.20` means a 20% import tariff).
/// * `export_taxes` maps commodity to an *ad valorem* export tax rate.
/// * Missing commodities are treated as zero.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct TradePolicy {
    /// Import tariffs.
    #[serde(default)]
    pub import_tariffs: HashMap<Commodity, f64>,
    /// Export taxes.
    #[serde(default)]
    pub export_taxes: HashMap<Commodity, f64>,
    /// Price floors (Phase 5.5) - Minimum legal price per commodity.
    #[serde(default)]
    pub price_floors: HashMap<Commodity, f64>,
    /// Price ceilings (Phase 5.5) - Maximum legal price per commodity.
    #[serde(default)]
    pub price_ceilings: HashMap<Commodity, f64>,
    /// Phase 29: Import subsidies (negative tariff for strategic imports).
    #[serde(default)]
    pub import_subsidies: HashMap<Commodity, f64>,
    /// Phase 29: Export subsidies (payment to exporters).
    #[serde(default)]
    pub export_subsidies: HashMap<Commodity, f64>,
}

/// Rationing level for a commodity (Phase 4).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RationingLevel {
    /// No rationing - normal consumption.
    None,
    /// Reduced consumption (50% of normal).
    Reduced,
    /// Critical shortage (25% of normal).
    Critical,
    /// Emergency shortage (10% of normal).
    Emergency,
}

impl Default for RationingLevel {
    fn default() -> Self {
        RationingLevel::None
    }
}

/// Rationing system for managing commodity shortages (Phase 4).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct RationingSystem {
    /// Whether rationing is currently active.
    #[serde(default)]
    pub active: bool,
    /// Rationing levels per commodity.
    #[serde(default)]
    pub rationed_goods: BTreeMap<String, RationingLevel>,
    /// Per-capita consumption limits per commodity.
    #[serde(default)]
    pub per_capita_limits: BTreeMap<String, f64>,
    /// Enforcement strictness (0.0-1.0).
    #[serde(default)]
    pub enforcement_strictness: f64,
}

/// Emergency powers available to the state (Phase 4).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EmergencyPowers {
    /// Normal operations - no emergency powers.
    Normal,
    /// Excise taxes enabled for strategic goods.
    ExciseTaxesEnabled,
    /// Full rationing system activated.
    RationingEnabled,
    /// Martial law - complete state control.
    MartialLaw,
}

impl Default for EmergencyPowers {
    fn default() -> Self {
        EmergencyPowers::Normal
    }
}

/// Intelligence budget for espionage operations (Phase 10).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct IntelligenceBudget {
    /// Fiat allocated this turn (debited from liquid_reserves).
    #[serde(default)]
    pub current_budget: f64,
    /// Cumulative spent on operations.
    #[serde(default)]
    pub spent: f64,
}

/// A single nation — the join of its `budgets`, `makro`, and `tax_rates`
/// slices.
///
/// # Rules
/// * `name` is not stored in the per-country JSON payloads (it is the map key
///   in each save file); it is populated by the loader and defaults to empty
///   when a bare `Country` is deserialized.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Country {
    /// Canonical country name (map key in the Python save files).
    #[serde(default)]
    pub name: String,
    /// Financial and structural state (from `budgets.json`).
    pub budget: Treasury,
    /// Macroeconomic and social indicators (from `makro.json`).
    pub macro_indicators: MacroData,
    /// Taxation state (from `tax_rates.json`).
    pub tax_rates: TaxRates,
    /// Trade-policy state (defaulted when missing from the save).
    #[serde(default)]
    pub trade_policy: TradePolicy,
    /// Political state (from `polityka` inside `makro.json`).
    #[serde(default)]
    pub politics: Politics,
    /// Regional geography and governance
    #[serde(default)]
    pub regions: Vec<Region>,
    /// Megaregional groupings.
    #[serde(default)]
    pub megaregions: Vec<Megaregion>,
    /// Phase 3: Rebellion flag - true if this is a rebel proto-state.
    #[serde(default)]
    pub is_rebellion: bool,
    /// Phase 3: Mother country (if this is a rebellion).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mother_country: Option<String>,
    /// Phase 3: Type of rebellion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rebellion_type: Option<crate::politics::rebellions::RebellionType>,
    /// Phase 3: Rebellion goals (ideological demands).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rebellion_goals: Option<Vec<String>>,
    /// Phase 70: Hierarchical Order of Battle (replaces flat military_units).
    /// No serde default — breaks saves per Rule 10.
    pub order_of_battle: crate::military::oob::OrderOfBattle,
    /// Phase 5: Active military fronts.
    #[serde(default)]
    pub military_fronts: Vec<crate::military::Front>,
    /// Phase 3: Central military arms depot. Filled by B2B procurement deliveries.
    #[serde(default)]
    pub military_stockpile: FxHashMap<crate::registries::enums::Commodity, f64>,
    /// Phase 3: All combat and supply parameters. No magic numbers in logic.
    #[serde(default)]
    pub military_config: crate::military::config::MilitaryCombatConfig,
    /// Phase 69: War economy state — production decrees, conscription, war bonds.
    /// No serde default — breaks saves per Rule 10.
    pub war_economy: crate::military::war_economy::WarEconomyState,
    /// Phase 70: Countries this nation is currently at war with.
    pub at_war_with: Vec<String>,
    /// Phase 3: Pending B2B buy orders from Ministry of Defense.
    /// Created in Phase 8, merged into global OrderBook at start of next turn's Phase 6.4.
    #[serde(default)]
    pub pending_defense_orders: Vec<crate::economy::order_book::Bid>,
    /// Phase 4: Rationing system for commodity shortages.
    #[serde(default)]
    pub rationing_system: RationingSystem,
    /// Phase 4: Emergency powers status.
    #[serde(default)]
    pub emergency_powers: EmergencyPowers,
    /// Phase 33: Hysteresis counters for emergency powers escalation/de-escalation.
    /// Counts consecutive turns where escalation conditions are met.
    #[serde(default)]
    pub emergency_escalation_counter: u32,
    /// Phase 33: Counts consecutive turns where recovery conditions are met.
    #[serde(default)]
    pub emergency_deescalation_counter: u32,
    /// Phase 33: Ministry public service wage pool routed to the State Employer.
    /// Ministries add their Healthcare/Education budget here instead of directly
    /// debiting liquid_reserves. The State Employer reads and clears this pool
    /// when it is created, adding it to its funded_payroll.
    #[serde(default)]
    pub ministry_public_service_pool: f64,
    /// Phase 10: Intelligence budget for espionage operations.
    #[serde(default)]
    pub intelligence_budget: IntelligenceBudget,
    /// PHASE 4: Active lobbying operations
    #[serde(default)]
    pub active_lobbying_operations: Vec<crate::politics::lobbying::LobbyingOperation>,
    /// Stage D: Central Bank (one per country, or shared in currency unions).
    #[serde(default)]
    pub central_bank: CentralBank,
    /// Stage D: Reference to currency zone (if different from country default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency_zone: Option<String>,
    /// Stage D Phase 2: Interbank market for liquidity exchange.
    #[serde(default)]
    pub interbank_market: InterbankMarket,
    /// Stage D Phase 3: Mandatory deposit insurance fund.
    #[serde(default)]
    pub bfg_fund: BfgFund,
    /// Stage D Phase 3: Voluntary institutional protection scheme.
    #[serde(default)]
    pub sobk_scheme: SobkScheme,
    /// Stage D Phase 3: Bank resolution authority (bridge bank framework).
    #[serde(default)]
    pub bank_resolution: BankResolution,
    /// Stage D Phase 3: Bank tax (temporary macro-fiscal tool).
    #[serde(default)]
    pub bank_tax: BankTax,
    /// Phase D.4: National stock exchange with dual-liquidity trading.
    #[serde(default)]
    pub stock_exchange: crate::securities::StockExchange,
    /// Phase 24A.6: Pending dividend payments to be processed after apply_action.
    /// Each tuple is (owner_id, amount). Owner_id can be a company_id, fund_id,
    /// or "STATE"/"TREASURY" for state-owned shares.
    #[serde(default)]
    pub dividend_queue: Vec<(String, f64)>,
    /// Phase 24A.7: Pending IPO requests to be processed after apply_action.
    /// Each tuple is (company_id, shares_to_float, reserve_price).
    #[serde(default)]
    pub ipo_queue: Vec<(String, u64, f64)>,
    /// Phase 24A.8: Persistent auction pool for bankruptcy liquidation.
    /// Stores assets from bankrupt companies until they are sold or nationalized.
    #[serde(default)]
    pub bankruptcy_auction_pool: crate::corporate::BankruptcyAuctionPool,
    /// Phase 24A.9: Pending demolition requests.
    /// Each tuple is (company_id, building_id).
    #[serde(default)]
    pub demolition_queue: Vec<(String, String)>,
    /// Phase 24A.9: Pending production halt requests.
    /// Each tuple is (company_id, building_id).
    #[serde(default)]
    pub halt_queue: Vec<(String, String)>,
    /// Phase D.4: Financial Supervision Authority (KNF).
    #[serde(default)]
    pub knf: crate::securities::KNF,
    /// Phase 55: Capital gains tax registry for securities and commodities.
    /// Tracks per-entity accrued gains/losses and settles tax at fiscal year-end.
    #[serde(default)]
    pub capital_gains_tax: crate::state::capital_gains_tax::CapitalGainsTaxRegistry,
    /// Phase D.5: Sovereign default status - turns remaining in default.
    #[serde(default)]
    pub sovereign_default_turns_remaining: u32,
    /// Phase D.5: Foreign debt outstanding (for sovereign default calculation).
    #[serde(default)]
    pub foreign_debt: f64,
    /// Phase 6.2: Statutory minimum wage per FTE (currency units)
    /// None = laissez-faire economy (no minimum wage enforcement)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum_wage: Option<f64>,
    /// Phase 6.4: Economic policy for price interventions and subsidies
    #[serde(default)]
    pub economic_policy: EconomicPolicy,
    /// Phase 8: Advanced debt market (wholesale + retail + secondary).
    #[serde(default)]
    pub debt_market: crate::economy::debt_market::DebtMarket,
    /// Resurrection Phase 1: Cultural/religious institutions as economic actors
    #[serde(default)]
    pub cultural_institutions: Vec<crate::infrastructure::cultural::CulturalBuilding>,
    /// Resurrection Phase 1: Maritime infrastructure (shipyards, ports, docks)
    #[serde(default)]
    pub maritime_infrastructure: crate::infrastructure::maritime::MaritimeInfrastructure,
    /// Resurrection Phase 1: Cultural relief configuration (no magic numbers)
    #[serde(default)]
    pub cultural_relief_config: crate::infrastructure::cultural::CulturalReliefConfig,
    /// Resurrection Phase 1: Building condition configuration
    #[serde(default)]
    pub building_condition_config: crate::infrastructure::building_condition::BuildingConditionConfig,
    /// Resurrection Phase 1: Maritime configuration
    #[serde(default)]
    pub maritime_config: crate::infrastructure::maritime::MaritimeConfig,
    /// Resurrection Phase 2: Securities market configuration (no magic numbers).
    #[serde(default)]
    pub securities_config: crate::securities::SecuritiesMarketConfig,
    /// Resurrection Phase 2: CCP clearinghouse for derivative clearing.
    #[serde(default)]
    pub central_counterparty: crate::securities::CentralCounterparty,
    /// Resurrection Phase 2: Active MBS structures.
    #[serde(default)]
    pub mbs_pool: Vec<crate::securities::MortgageBackedSecurity>,
    /// Resurrection Phase 2: Active covered bonds.
    #[serde(default)]
    pub covered_bonds_issued: Vec<crate::securities::CoveredBond>,
    /// Resurrection Phase 2: Active CDS contracts.
    #[serde(default)]
    pub active_derivatives: Vec<crate::securities::CreditDefaultSwap>,
    /// Resurrection Phase 2: Active futures contracts.
    #[serde(default)]
    pub active_futures: Vec<crate::securities::FuturesContract>,
    /// Resurrection Phase 2: Trade finance bills of lading.
    #[serde(default)]
    pub bills_of_lading: Vec<crate::securities::BillOfLading>,
    /// Resurrection Phase 2: Working capital loans backed by bills of lading.
    #[serde(default)]
    pub working_capital_loans: Vec<crate::securities::WorkingCapitalLoan>,
    /// Phase 4: B2B order submission configuration (no magic numbers).
    #[serde(default)]
    pub b2b_order_config: crate::economy::b2b_config::B2bOrderConfig,
    /// Phase 4: Fishing and aquaculture configuration.
    #[serde(default)]
    pub fishing_config: crate::economy::fishing_config::FishingConfig,
    /// Phase 4: B2C service pricing configuration.
    #[serde(default)]
    pub service_pricing_config: crate::economy::service_config::ServicePricingConfig,
    /// Phase 4: Infrastructure funding configuration.
    #[serde(default)]
    pub infrastructure_config: crate::economy::infrastructure_config::InfrastructureConfig,
    /// Phase 4: Innovation trading and royalty configuration.
    #[serde(default)]
    pub innovation_config: crate::economy::innovation_config::InnovationConfig,
    /// Phase 4: Corporate technology configuration (extended with state patent + R&D cap).
    #[serde(default)]
    pub corporate_tech_config: crate::economy::corporate_config::CorporateTechConfig,
    /// Phase 4: Fish stocks by region.
    #[serde(default)]
    pub fish_stocks: Vec<crate::economy::fishing::FishStock>,
    /// Phase 4: Fish farms by region.
    #[serde(default)]
    pub fish_farms: Vec<crate::economy::fishing::FishFarm>,
    /// Phase 4: Fishing policies.
    #[serde(default)]
    pub fishing_policies: Vec<crate::economy::fishing::FishingPolicy>,
    /// Phase 5: Special Economic Zones active in this country.
    #[serde(default)]
    pub special_economic_zones: Vec<crate::state::special_economic_zones::SpecialEconomicZone>,
    /// Phase 6: Active conservation policies (national parks, landscape parks)
    #[serde(default)]
    pub conservation_policies: Vec<crate::politics::conservation::ConservationPolicy>,
    /// Phase 6: National parks
    #[serde(default)]
    pub national_parks: Vec<crate::politics::conservation::NationalPark>,
    /// Phase 6: Landscape parks
    #[serde(default)]
    pub landscape_parks: Vec<crate::politics::conservation::LandscapePark>,
    /// Phase 8: Utility pricing tariffs for electricity, heating, water, sewage.
    #[serde(default)]
    pub utility_pricing_config: crate::utilities::UtilityPricingConfig,
    /// Phase 8: Physical conversion factors and penalty parameters for utilities.
    #[serde(default)]
    pub utility_config: crate::utilities::UtilityConfig,
    /// Phase 9: Natural wonders in this country.
    #[serde(default)]
    pub natural_wonders: Vec<crate::society::tourism::NaturalWonder>,
    /// Phase 9: Tourism destinations (keyed by region_id).
    #[serde(default)]
    pub tourism_destinations: BTreeMap<String, crate::society::tourism::TourismDestination>,
    /// Phase 13: Active social programs enacted by the Ministry of Social Welfare.
    /// Persist across turns; re-evaluated by Ministry AI during political year.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub social_programs: Vec<crate::politics::social_programs::SocialProgram>,
    /// Phase 15A: Weather event state (active events, RNG seed).
    #[serde(default)]
    pub weather_state: crate::economy::weather::WeatherState,
    /// Phase 15A: Maintenance configuration for building condition.
    #[serde(default)]
    pub maintenance_config: crate::economy::maintenance::MaintenanceConfig,
    /// Phase 15C: State Forests timber management state.
    #[serde(default)]
    pub state_forest_state: crate::economy::state_forests::ForestDistrictState,
    /// Phase 17A: Religious authority scores per religion.
    #[serde(default)]
    pub religious_authority_state: crate::society::religious_authority::ReligiousAuthorityState,
    /// Phase 19: Generative investment goods, blueprints, fixed-asset cohorts,
    /// maintenance-as-a-service, obsolescence, and quality-driven markets.
    #[serde(default)]
    pub generative_goods_config: crate::economy::generative_goods_config::GenerativeGoodsConfig,
    /// Phase 21A: Geological formations with finite, depletable resource deposits.
    #[serde(default)]
    pub geological_formations: Vec<crate::society::geography::GeologicalFormation>,
    /// Phase 22A: Active construction tenders awaiting bid submission/award.
    #[serde(default)]
    pub phase22_tenders: Vec<crate::construction::ConstructionTender>,
    /// Phase 22D: Pending civil lawsuits.
    #[serde(default)]
    pub phase22_lawsuits: Vec<crate::economy::civil_lawsuits::CivilLawsuit>,
    /// Phase 22D: Pending KIO appeals.
    #[serde(default)]
    pub phase22_kio_appeals: Vec<crate::government::kio::KioAppeal>,
    /// Phase 23A: Freight logistics configuration (spatial friction parameters).
    #[serde(default)]
    pub freight_logistics_config: crate::economy::logistics::FreightLogisticsConfig,
    /// Phase 23A: Deferred trades awaiting freight capacity (retried next turn).
    #[serde(default)]
    pub deferred_trades: Vec<crate::economy::logistics::DeferredTrade>,
    /// Phase 23B: Transport network overlay (roads, rail, highways, canals).
    #[serde(default)]
    pub transport_networks: crate::economy::transport_networks::TransportNetworkOverlay,
    /// Phase 23C: Commuting configuration (ticket pricing, subsidy, frequency).
    #[serde(default)]
    pub commuting_config: crate::economy::commuting::CommutingConfig,
    /// Phase 29: Regional overflow fees (storage fees + perishability losses)
    /// per region, updated each turn after production. Used by logistics
    /// companies for ROI-driven warehouse construction decisions.
    #[serde(default)]
    pub regional_overflow_fees: std::collections::BTreeMap<String, f64>,
    /// Phase 38/41: Last tax collection result (now serialized for persistence).
    /// Used by the Finance tab to display tax revenue breakdown.
    /// Phase 41: Removed #[serde(skip)] so tax data survives save/reload.
    #[serde(default)]
    pub last_tax_result: Option<tax::TaxCollectionResult>,
    /// Phase 41: Accumulated transactional VAT from B2C clearing.
    /// Reset to 0.0 at the start of each turn, accumulated during B2C clearing,
    /// and read by process_tax_collection_turn for REPORTING ONLY (no second treasury credit).
    #[serde(default)]
    pub accumulated_vat: f64,
    /// Phase 58: Topological land cadastre (slotmap-backed ParcelChunks).
    /// Replaces the old aggregate LandRegistry. Source of truth for all land
    /// ownership, zoning, valuation, and legal certainty.
    #[serde(default)]
    pub cadastre: crate::society::cadastre::Cadastre,
    /// Phase 58: Hedonic valuation and cadastre cost configuration.
    #[serde(default)]
    pub cadastre_config: crate::society::cadastre::CadastreConfig,
    /// Phase 58: Per-region rolling land price history for FairMarketAverage
    /// compensation calculations during agrarian reform / expropriation.
    #[serde(default)]
    pub land_price_history: crate::society::cadastre::LandPriceHistoryRegistry,
    /// Phase 58: Arbitration court configuration (no hardcoded multipliers).
    #[serde(default)]
    pub arbitration_config: crate::society::cadastre::ArbitrationConfig,
    /// Phase 59: Arbitration court system (cases, compensation liabilities).
    #[serde(default)]
    pub arbitration_court: crate::society::cadastre::ArbitrationCourt,
    /// Phase 59: Border conflict registry (per-country, parcels frozen by disputes).
    #[serde(default)]
    pub border_conflicts: crate::society::cadastre::BorderConflictRegistry,
    /// Phase 59: Legal certainty dynamics configuration.
    #[serde(default)]
    pub legal_certainty_config: crate::society::cadastre::LegalCertaintyConfig,
    /// Phase 59: Negative externality configuration for incompatible zoning.
    #[serde(default)]
    pub externality_config: crate::society::cadastre::ExternalityConfig,
    /// Phase 59: National zoning quota set by the central government (player as PM).
    #[serde(default)]
    pub national_zoning_quota: crate::society::cadastre::NationalZoningQuota,
    /// Phase 63.3: National subsurface rights law (default by tradition, changeable via legislation).
    #[serde(default)]
    pub subsurface_rights_law: crate::society::cadastre::SubsurfaceRightsLaw,
    /// Phase 67: Global reputation for this country (-100 to +100).
    pub global_reputation: crate::international::reputation::GlobalReputation,
    /// Phase 67: Geopolitical doctrine guiding AI diplomatic behavior.
    pub geopolitical_doctrine: crate::international::ai_doctrines::GeopoliticalDoctrine,
    /// Phase 81: Power grid state — HV lines, LV/MV capacities, spot prices,
    /// load shedding tiers, and overproduction tiers per region.
    #[serde(default)]
    pub power_grid_state: crate::energy::PowerGridState,
}

impl Country {
    /// Create a mock country for testing purposes.
    ///
    /// # Returns
    /// A Country instance with default values suitable for unit tests
    pub fn mock_for_tests() -> Self {
        Self {
            name: String::new(),
            budget: Treasury::default(),
            macro_indicators: MacroData::default(),
            tax_rates: TaxRates::default(),
            trade_policy: TradePolicy::default(),
            politics: Politics::default(),
            regions: Vec::new(),
            megaregions: Vec::new(),
            is_rebellion: false,
            mother_country: None,
            rebellion_type: None,
            rebellion_goals: None,
            order_of_battle: crate::military::oob::OrderOfBattle::default(),
            military_fronts: Vec::new(),
            military_stockpile: rustc_hash::FxHashMap::default(),
            military_config: crate::military::config::MilitaryCombatConfig::default(),
            war_economy: crate::military::war_economy::WarEconomyState::default(),
            at_war_with: Vec::new(),
            pending_defense_orders: Vec::new(),
            rationing_system: RationingSystem::default(),
            emergency_powers: EmergencyPowers::default(),
            emergency_escalation_counter: 0,
            emergency_deescalation_counter: 0,
            ministry_public_service_pool: 0.0,
            intelligence_budget: IntelligenceBudget::default(),
            active_lobbying_operations: Vec::new(),
            central_bank: CentralBank::default(),
            currency_zone: None,
            interbank_market: InterbankMarket::default(),
            bfg_fund: BfgFund::default(),
            sobk_scheme: SobkScheme::default(),
            bank_resolution: BankResolution::default(),
            bank_tax: BankTax::default(),
            stock_exchange: crate::securities::StockExchange::default(),
            dividend_queue: Vec::new(), ipo_queue: Vec::new(), bankruptcy_auction_pool: crate::corporate::BankruptcyAuctionPool::default(), demolition_queue: Vec::new(), halt_queue: Vec::new(),
            knf: crate::securities::KNF::default(),
            capital_gains_tax: crate::state::capital_gains_tax::CapitalGainsTaxRegistry::default(),
            sovereign_default_turns_remaining: 0,
            foreign_debt: 0.0,
            minimum_wage: None,
            economic_policy: EconomicPolicy::default(),
            debt_market: crate::economy::debt_market::DebtMarket::default(),
            cultural_institutions: Vec::new(),
            maritime_infrastructure: crate::infrastructure::maritime::MaritimeInfrastructure::default(),
            cultural_relief_config: crate::infrastructure::cultural::CulturalReliefConfig::default(),
            building_condition_config: crate::infrastructure::building_condition::BuildingConditionConfig::default(),
            maritime_config: crate::infrastructure::maritime::MaritimeConfig::default(),
            securities_config: crate::securities::SecuritiesMarketConfig::default(),
            central_counterparty: crate::securities::CentralCounterparty::default(),
            mbs_pool: Vec::new(),
            covered_bonds_issued: Vec::new(),
            active_derivatives: Vec::new(),
            active_futures: Vec::new(),
            bills_of_lading: Vec::new(),
            working_capital_loans: Vec::new(),
            b2b_order_config: crate::economy::b2b_config::B2bOrderConfig::default(),
            fishing_config: crate::economy::fishing_config::FishingConfig::default(),
            service_pricing_config: crate::economy::service_config::ServicePricingConfig::default(),
            infrastructure_config: crate::economy::infrastructure_config::InfrastructureConfig::default(),
            innovation_config: crate::economy::innovation_config::InnovationConfig::default(),
            corporate_tech_config: crate::economy::corporate_config::CorporateTechConfig::default(),
            fish_stocks: Vec::new(),
            fish_farms: Vec::new(),
            fishing_policies: Vec::new(),
            utility_pricing_config: crate::utilities::UtilityPricingConfig::default(),
            utility_config: crate::utilities::UtilityConfig::default(),
            conservation_policies: Vec::new(),
            national_parks: Vec::new(),
            landscape_parks: Vec::new(),
            special_economic_zones: Vec::new(),
            natural_wonders: Vec::new(),
            tourism_destinations: BTreeMap::new(),
            social_programs: Vec::new(),
            weather_state: crate::economy::weather::WeatherState::default(),
            maintenance_config: crate::economy::maintenance::MaintenanceConfig::default(),
            state_forest_state: crate::economy::state_forests::ForestDistrictState::default(),
            religious_authority_state: crate::society::religious_authority::ReligiousAuthorityState::default(),
            generative_goods_config: crate::economy::generative_goods_config::GenerativeGoodsConfig::default(),
            geological_formations: Vec::new(),
            phase22_tenders: Vec::new(),
            phase22_lawsuits: Vec::new(),
            phase22_kio_appeals: Vec::new(),
            freight_logistics_config: crate::economy::logistics::FreightLogisticsConfig::default(),
            deferred_trades: Vec::new(),
            transport_networks: crate::economy::transport_networks::TransportNetworkOverlay::default(),
            commuting_config: crate::economy::commuting::CommutingConfig::default(),
            regional_overflow_fees: std::collections::BTreeMap::new(),
            last_tax_result: None,
            accumulated_vat: 0.0,
            cadastre: crate::society::cadastre::Cadastre::default(),
            cadastre_config: crate::society::cadastre::CadastreConfig::default(),
            land_price_history: crate::society::cadastre::LandPriceHistoryRegistry::default(),
            arbitration_config: crate::society::cadastre::ArbitrationConfig::default(),
            arbitration_court: crate::society::cadastre::ArbitrationCourt::default(),
            border_conflicts: crate::society::cadastre::BorderConflictRegistry::default(),
            legal_certainty_config: crate::society::cadastre::LegalCertaintyConfig::default(),
            externality_config: crate::society::cadastre::ExternalityConfig::default(),
            national_zoning_quota: crate::society::cadastre::NationalZoningQuota::default(),
            subsurface_rights_law: crate::society::cadastre::SubsurfaceRightsLaw::default(),
            global_reputation: crate::international::reputation::GlobalReputation::default(),
            geopolitical_doctrine: crate::international::ai_doctrines::GeopoliticalDoctrine::default(),
            power_grid_state: crate::energy::PowerGridState::default(),
        }
    }

    /// Test builder for Country with fluent configuration.
    pub fn test_builder() -> CountryBuilder {
        CountryBuilder::default()
    }

    /// Check if country is in sovereign default.
    pub fn is_in_default(&self) -> bool {
        self.sovereign_default_turns_remaining > 0
    }

    /// Trigger sovereign default (locks country out of international markets).
    pub fn trigger_sovereign_default(&mut self, turns: u32) {
        self.sovereign_default_turns_remaining = turns;
    }
    
    /// Decrement default counter each turn.
    pub fn process_default_turn(&mut self) {
        if self.sovereign_default_turns_remaining > 0 {
            self.sovereign_default_turns_remaining -= 1;
        }
    }
}

/// Root of the simulation world.
///
/// # Rules
/// * Stage 1 populates only [`GameState::countries`]; shared global systems
///   (market, diplomacy, currencies, regions) are preserved in
///   [`GameState::extra`] and will be promoted to typed fields in later
///   targets.
/// * Stage 5 promotes the currency map (`waluty.json`) to
///   [`GameState::currencies`].
/// * Phase E.1 adds global Forex and Gold markets.
/// * Phase F.1 adds save version to invalidate old Polish-keyed saves.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GameState {
    /// Save format version - incremented when breaking schema changes occur.
    /// Version 1: Initial English migration (Polish keys removed).
    /// Version 2: Phase 6.1 - 24-tick temporal engine added.
    #[serde(default)]
    pub save_version: u32,
    /// Phase 6.1: Calendar tracker for 24-tick temporal engine
    #[serde(default)]
    pub calendar: Calendar,
    /// Phase 6.1: Climate configuration for seasonal modifiers
    #[serde(default)]
    pub climate_config: ClimateConfig,
    /// Phase 6.4: Historical price registry for fallback prices
    #[serde(default)]
    pub market_history: MarketHistory,
    /// Phase 54: Rolling banking history for sparkline tooltips (per-country).
    #[serde(default)]
    pub banking_history: HashMap<String, BankingHistory>,
    /// All simulated nations, keyed by canonical country name.
    pub countries: HashMap<String, Country>,
    /// Shared currency zones, keyed by currency code.
    #[serde(default)]
    pub currencies: HashMap<String, Currency>,
    /// Phase E.1: Global Forex Market for currency trading.
    #[serde(default)]
    pub forex_market: ForexMarket,
    /// Phase E.1: Global Gold Exchange for physical gold trading.
    #[serde(default)]
    pub gold_exchange: GlobalGoldExchange,
    /// Phase E.1: Global vault registry for physical gold storage (entity_id -> gold_stored).
    #[serde(default)]
    pub vaults: BTreeMap<String, f64>,
    /// Phase 6: Leader trait registry (global, data-driven)
    #[serde(default)]
    pub trait_registry: Option<crate::politics::traits::TraitRegistry>,
    /// Phase 39: Deferred diplomatic action queue. Populated during parallel
    /// per-country turn processing (each country returns a Vec<DiplomaticAction>),
    /// then drained sequentially after the parallel block to avoid cross-country
    /// mutation during Rayon iteration.
    #[serde(default)]
    pub pending_diplomatic_actions: Vec<crate::state::diplomatic_actions::DiplomaticAction>,
    /// Phase 66: Foreign intelligence data per observer country.
    /// Keyed by observer country name → (target country name → ForeignIntelligence).
    #[serde(default)]
    pub foreign_intelligence: HashMap<String, HashMap<String, crate::international::fog_of_war::ForeignIntelligence>>,
    /// Phase 66: Fog of War configuration (intel rates, estimation errors).
    #[serde(default)]
    pub fog_of_war_config: crate::international::fog_of_war::FogOfWarConfig,
    /// Phase 66: Diplomatic configuration (spy risk, relation penalties, costs).
    #[serde(default)]
    pub diplomatic_config: crate::international::fog_of_war::DiplomaticConfig,
    /// Phase 67: Treaty registry (all treaties: active, pending, expired, abrogated).
    pub treaty_registry: crate::international::treaties::TreatyRegistry,
    /// Phase 67: Treaty configuration (negotiation speed, capacity costs).
    pub treaty_config: crate::international::treaties::TreatyConfig,
    /// Phase 67: Reputation configuration (penalties, recovery rate, thresholds).
    pub reputation_config: crate::international::reputation::ReputationConfig,
    /// Phase 67: AI doctrine configuration (thresholds for doctrine selection).
    pub doctrine_config: crate::international::ai_doctrines::DoctrineConfig,
    /// Phase 68: International organizations (World Forum + dynamic orgs).
    pub international_organizations: crate::international::organizations::OrganizationRegistry,
    /// Phase 68: Active and expired sanctions.
    pub active_sanctions: crate::international::sanctions::SanctionRegistry,
    /// Phase 68: Organization configuration (integration thresholds, fine rates).
    pub org_config: crate::international::organizations::OrgConfig,
    /// Phase 68: Sanction configuration (vote thresholds, durations).
    pub sanction_config: crate::international::sanctions::SanctionConfig,
    /// Not-yet-typed global systems, preserved losslessly.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl GameState {
    /// Creates an empty game state with no countries and no currencies.
    ///
    /// # Returns
    /// A [`GameState`] whose `countries` map, `currencies` map and `extra` bag
    /// are empty.
    pub fn new() -> Self {
        Self {
            save_version: 2,
            calendar: Calendar::default(),
            climate_config: ClimateConfig::default(),
            market_history: MarketHistory::default(),
            banking_history: HashMap::new(),
            countries: HashMap::new(),
            currencies: HashMap::new(),
            forex_market: ForexMarket::default(),
            gold_exchange: GlobalGoldExchange::default(),
            vaults: BTreeMap::new(),
            trait_registry: None,
            pending_diplomatic_actions: Vec::new(),
            foreign_intelligence: HashMap::new(),
            fog_of_war_config: crate::international::fog_of_war::FogOfWarConfig::default(),
            diplomatic_config: crate::international::fog_of_war::DiplomaticConfig::default(),
            treaty_registry: crate::international::treaties::TreatyRegistry::default(),
            treaty_config: crate::international::treaties::TreatyConfig::default(),
            reputation_config: crate::international::reputation::ReputationConfig::default(),
            doctrine_config: crate::international::ai_doctrines::DoctrineConfig::default(),
            international_organizations: crate::international::organizations::OrganizationRegistry::default(),
            active_sanctions: crate::international::sanctions::SanctionRegistry::default(),
            org_config: crate::international::organizations::OrgConfig::default(),
            sanction_config: crate::international::sanctions::SanctionConfig::default(),
            extra: Map::new(),
        }
    }
    
    /// Validates that the save version is compatible with the current engine.
    ///
    /// # Returns
    /// true if the save version is compatible, false otherwise.
    ///
    /// # Rules
    /// * Version 0: Legacy Polish-keyed saves (incompatible)
    /// * Version 1: English-migrated saves (current)
    pub fn is_save_compatible(&self) -> bool {
        self.save_version >= 1
    }
    
    /// Returns a human-readable error message if the save is incompatible.
    ///
    /// # Returns
    /// None if compatible, Some(error_message) if incompatible.
    pub fn save_compatibility_error(&self) -> Option<String> {
        if self.save_version < 1 {
            Some(format!(
                "Save version {} is incompatible with current engine (requires version 1+). \
                 This save uses Polish JSON keys and must be migrated using the migration script.",
                self.save_version
            ))
        } else {
            None
        }
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder pattern for creating test Country instances with fluent configuration.
#[derive(Default)]
pub struct CountryBuilder {
    name: String,
    budget: Option<Treasury>,
    macro_indicators: Option<MacroData>,
    tax_rates: Option<TaxRates>,
    trade_policy: Option<TradePolicy>,
    politics: Option<Politics>,
}

impl CountryBuilder {
    /// Set the country name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Set the treasury/budget.
    pub fn with_treasury(mut self, treasury: Treasury) -> Self {
        self.budget = Some(treasury);
        self
    }

    /// Configure the treasury with a closure.
    pub fn configure_treasury<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut Treasury),
    {
        let mut treasury = self.budget.unwrap_or_default();
        f(&mut treasury);
        self.budget = Some(treasury);
        self
    }

    /// Set the macro indicators.
    pub fn with_macro_indicators(mut self, macro_indicators: MacroData) -> Self {
        self.macro_indicators = Some(macro_indicators);
        self
    }

    /// Configure the macro indicators with a closure.
    pub fn configure_macro_indicators<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut MacroData),
    {
        let mut macro_indicators = self.macro_indicators.unwrap_or_default();
        f(&mut macro_indicators);
        self.macro_indicators = Some(macro_indicators);
        self
    }

    /// Set the tax rates.
    pub fn with_tax_rates(mut self, tax_rates: TaxRates) -> Self {
        self.tax_rates = Some(tax_rates);
        self
    }

    /// Configure the tax rates with a closure.
    pub fn configure_tax_rates<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut TaxRates),
    {
        let mut tax_rates = self.tax_rates.unwrap_or_default();
        f(&mut tax_rates);
        self.tax_rates = Some(tax_rates);
        self
    }

    /// Build the Country instance.
    pub fn build(self) -> Country {
        Country {
            name: self.name,
            budget: self.budget.unwrap_or_default(),
            macro_indicators: self.macro_indicators.unwrap_or_default(),
            tax_rates: self.tax_rates.unwrap_or_default(),
            trade_policy: self.trade_policy.unwrap_or_default(),
            politics: self.politics.unwrap_or_default(),
            regions: Vec::new(),
            megaregions: Vec::new(),
            is_rebellion: false,
            mother_country: None,
            rebellion_type: None,
            rebellion_goals: None,
            order_of_battle: crate::military::oob::OrderOfBattle::default(),
            military_fronts: Vec::new(),
            military_stockpile: rustc_hash::FxHashMap::default(),
            military_config: crate::military::config::MilitaryCombatConfig::default(),
            war_economy: crate::military::war_economy::WarEconomyState::default(),
            at_war_with: Vec::new(),
            pending_defense_orders: Vec::new(),
            rationing_system: RationingSystem::default(),
            emergency_powers: EmergencyPowers::default(),
            emergency_escalation_counter: 0,
            emergency_deescalation_counter: 0,
            ministry_public_service_pool: 0.0,
            intelligence_budget: IntelligenceBudget::default(),
            active_lobbying_operations: Vec::new(),
            central_bank: CentralBank::default(),
            currency_zone: None,
            interbank_market: InterbankMarket::default(),
            bfg_fund: BfgFund::default(),
            sobk_scheme: SobkScheme::default(),
            bank_resolution: BankResolution::default(),
            bank_tax: BankTax::default(),
            stock_exchange: crate::securities::StockExchange::default(),
            dividend_queue: Vec::new(), ipo_queue: Vec::new(), bankruptcy_auction_pool: crate::corporate::BankruptcyAuctionPool::default(), demolition_queue: Vec::new(), halt_queue: Vec::new(),
            knf: crate::securities::KNF::default(),
            capital_gains_tax: crate::state::capital_gains_tax::CapitalGainsTaxRegistry::default(),
            sovereign_default_turns_remaining: 0,
            foreign_debt: 0.0,
            minimum_wage: None,
            economic_policy: EconomicPolicy::default(),
            debt_market: crate::economy::debt_market::DebtMarket::default(),
            cultural_institutions: Vec::new(),
            maritime_infrastructure: crate::infrastructure::maritime::MaritimeInfrastructure::default(),
            cultural_relief_config: crate::infrastructure::cultural::CulturalReliefConfig::default(),
            building_condition_config: crate::infrastructure::building_condition::BuildingConditionConfig::default(),
            maritime_config: crate::infrastructure::maritime::MaritimeConfig::default(),
            securities_config: crate::securities::SecuritiesMarketConfig::default(),
            central_counterparty: crate::securities::CentralCounterparty::default(),
            mbs_pool: Vec::new(),
            covered_bonds_issued: Vec::new(),
            active_derivatives: Vec::new(),
            active_futures: Vec::new(),
            bills_of_lading: Vec::new(),
            working_capital_loans: Vec::new(),
            b2b_order_config: crate::economy::b2b_config::B2bOrderConfig::default(),
            fishing_config: crate::economy::fishing_config::FishingConfig::default(),
            service_pricing_config: crate::economy::service_config::ServicePricingConfig::default(),
            infrastructure_config: crate::economy::infrastructure_config::InfrastructureConfig::default(),
            innovation_config: crate::economy::innovation_config::InnovationConfig::default(),
            corporate_tech_config: crate::economy::corporate_config::CorporateTechConfig::default(),
            fish_stocks: Vec::new(),
            fish_farms: Vec::new(),
            fishing_policies: Vec::new(),
            special_economic_zones: Vec::new(),
            conservation_policies: Vec::new(),
            national_parks: Vec::new(),
            landscape_parks: Vec::new(),
            utility_pricing_config: crate::utilities::UtilityPricingConfig::default(),
            utility_config: crate::utilities::UtilityConfig::default(),
            natural_wonders: Vec::new(),
            tourism_destinations: BTreeMap::new(),
            social_programs: Vec::new(),
            weather_state: crate::economy::weather::WeatherState::default(),
            maintenance_config: crate::economy::maintenance::MaintenanceConfig::default(),
            state_forest_state: crate::economy::state_forests::ForestDistrictState::default(),
            religious_authority_state: crate::society::religious_authority::ReligiousAuthorityState::default(),
            generative_goods_config: crate::economy::generative_goods_config::GenerativeGoodsConfig::default(),
            geological_formations: Vec::new(),
            phase22_tenders: Vec::new(),
            phase22_lawsuits: Vec::new(),
            phase22_kio_appeals: Vec::new(),
            freight_logistics_config: crate::economy::logistics::FreightLogisticsConfig::default(),
            deferred_trades: Vec::new(),
            transport_networks: crate::economy::transport_networks::TransportNetworkOverlay::default(),
            commuting_config: crate::economy::commuting::CommutingConfig::default(),
            regional_overflow_fees: std::collections::BTreeMap::new(),
            last_tax_result: None,
            accumulated_vat: 0.0,
            cadastre: crate::society::cadastre::Cadastre::default(),
            cadastre_config: crate::society::cadastre::CadastreConfig::default(),
            land_price_history: crate::society::cadastre::LandPriceHistoryRegistry::default(),
            arbitration_config: crate::society::cadastre::ArbitrationConfig::default(),
            arbitration_court: crate::society::cadastre::ArbitrationCourt::default(),
            border_conflicts: crate::society::cadastre::BorderConflictRegistry::default(),
            legal_certainty_config: crate::society::cadastre::LegalCertaintyConfig::default(),
            externality_config: crate::society::cadastre::ExternalityConfig::default(),
            national_zoning_quota: crate::society::cadastre::NationalZoningQuota::default(),
            subsurface_rights_law: crate::society::cadastre::SubsurfaceRightsLaw::default(),
            global_reputation: crate::international::reputation::GlobalReputation::default(),
            geopolitical_doctrine: crate::international::ai_doctrines::GeopoliticalDoctrine::default(),
            power_grid_state: crate::energy::PowerGridState::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_game_state() {
        let gs = GameState::new();
        assert!(gs.countries.is_empty());
    }
}

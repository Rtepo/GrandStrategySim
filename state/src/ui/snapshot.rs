//! Phase 24E: Country snapshot aggregator.
//!
//! Produces a flat, UI-ready struct from `GameState`, `GlobalMarket`, and
//! `MarketHistory`. The TUI rendering layer consumes `CountrySnapshot` and
//! never touches raw simulation state directly, keeping the presentation
//! layer decoupled from the engine.

use crate::economy::market::market_history::MarketHistory;
use crate::economy::market::GlobalMarket;
use crate::entities::legal_form::LegalForm;
use crate::entities::Company;
use crate::politics::ideology::{classify, IdeologyCoordinates};
use crate::politics::local_government::{
    AdministrativeStatus, RegionalGovernance, RegionalHeadType,
};
use crate::registries::enums::Commodity;
use crate::registries::enums::Sector;
use crate::society::geography::RuralClass;
use crate::state::macro_data::{
    GdpBreakdown, InflationIndices, MoneySupplySnapshot, TelemetryHistory,
};
use crate::state::{Country, GameState};
use std::collections::BTreeMap;

/// Ideology Step 6: Derive the current simulation year from a `Country` for
/// `classify()` zeitgeist gating.
///
/// `Country` does not carry the calendar (that lives on `GameState`), so we
/// extract the year from the last telemetry sample. If no telemetry has been
/// recorded yet (e.g., freshly generated save), we return `u32::MAX` so that
/// ALL ideology centroids are eligible for classification — this is the
/// safest display default (no anachronistic filtering).
fn country_year(country: &Country) -> u32 {
    country
        .macro_indicators
        .telemetry_history
        .samples
        .last()
        .map(|s| s.year)
        .unwrap_or(u32::MAX)
}

/// Same extraction for the global turn counter — used to project pipeline
/// estimates (earliest enactment turn) onto queued bills. `0` when no
/// telemetry has been recorded yet.
fn country_turn(country: &Country) -> u32 {
    country
        .macro_indicators
        .telemetry_history
        .samples
        .last()
        .map(|s| s.turn)
        .unwrap_or(0)
}

/// Human-readable display name for a `Sector` enum variant.
fn sector_display_name(sector: Sector) -> String {
    match sector {
        Sector::Mining => "Mining",
        Sector::Agriculture => "Agriculture",
        Sector::HeavyIndustry => "Heavy Industry",
        Sector::LightIndustry => "Light Industry",
        Sector::ArmamentsIndustry => "Armaments Industry",
        Sector::LocalServices => "Local Services",
        Sector::ExportServices => "Export Services",
        Sector::Construction => "Construction",
        Sector::Energy => "Energy",
        Sector::PublicServices => "Public Services",
        Sector::MedicalServices => "Medical Services",
        Sector::EducationalServices => "Educational Services",
        Sector::TransportLogistics => "Transport & Logistics",
        Sector::PublicAdministration => "Public Administration",
        Sector::Banking => "Banking",
        Sector::MediaAndEntertainment => "Media & Entertainment",
        Sector::WasteManagement => "Waste Management",
        Sector::Hospitality => "Hospitality",
        Sector::NGO => "NGO",
        Sector::Religion => "Religion",
        Sector::MaintenanceWorkshops => "Maintenance Workshops",
        Sector::Government => "Government",
        Sector::SportsRecreation => "Sports & Recreation",
    }
    .to_string()
}

/// Human-readable display name for a `LegalForm` enum variant.
fn legal_form_display(legal_form: &LegalForm) -> String {
    match legal_form {
        LegalForm::JointStockCompany(_) => "Joint-Stock Company",
        LegalForm::StateMonopoly(_) => "State Monopoly",
        LegalForm::FamilyBusiness(_) => "Family Business",
        LegalForm::Cooperative(_) => "Cooperative",
        LegalForm::Latifundium(_) => "Latifundium",
        LegalForm::Consortium(_) => "Consortium",
        LegalForm::MunicipalCompany(_) => "Municipal Company",
        LegalForm::HousingCommunity(_) => "Housing Community",
        LegalForm::HousingCooperative(_) => "Housing Cooperative",
        LegalForm::StrategicReserveAgency(_) => "Strategic Reserve Agency",
        LegalForm::LogisticsCompany(_) => "Logistics Company",
        LegalForm::NonProfit(_) => "Non-Profit",
        LegalForm::MutualAidCircle(_) => "Mutual Aid Circle",
        LegalForm::Guild(_) => "Craft Guild",
    }
    .to_string()
}

// ============================================================================
// SNAPSHOT STRUCTS
// ============================================================================

/// Per-commodity market data for the Market tab.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CommodityRow {
    pub name: String,
    pub vwap: f64,
    pub base_price: f64,
    /// Bugfix Sprint: UI net surplus, computed dynamically as
    /// `supply_volume − demand_volume + net_trade` (NOT from `market.net_surplus`,
    /// which is the raw B2B order book surplus used by the clearing engine).
    pub net_surplus: f64,
    /// Bugfix Sprint: Net trade (imports − exports, physical units).
    /// Positive = net importer, negative = net exporter.
    pub net_trade: f64,
    /// Phase 27: ToT (turn-over-turn) % change of net_surplus.
    pub tot_balance_change: f64,
    /// Phase 27: true if this commodity has any market activity (vwap, last_trade, or surplus).
    pub active: bool,
    /// Phase 43: Total sell order volume (supply).
    pub supply_volume: f64,
    /// Phase 43: Total buy order volume (demand).
    pub demand_volume: f64,
}

/// Per-sector aggregation for the Sector Overview table.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct SectorRow {
    pub sector_name: String,
    pub company_count: usize,
    /// Percentage share of total wages paid (proxy for GDP share).
    pub pct_gdp_share: f64,
    /// Total employment (sum of fulfilled_fte). Phase 77: Integer headcount.
    pub total_employment: u32,
    /// Average wage in this sector.
    pub average_wage: f64,
    /// Phase 28: PMI (Purchasing Managers Index) for this sector (0-100).
    pub pmi: f64,
    /// Phase 28: Turn-over-Turn percentage change in employment.
    pub employment_tot: Option<f64>,
    /// Phase 28: Turn-over-Turn percentage change in average wage.
    pub wage_tot: Option<f64>,
}

/// Infrastructure link summary.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct InfrastructureRow {
    pub link_id: String,
    pub condition: f64,
    pub capacity: f64,
}

/// Geological deposit summary.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct DepositRow {
    pub formation: String,
    pub deposit_id: String,
    pub current_reserves: f64,
    pub estimated_reserves: f64,
    pub quality: f64,
    pub depletion_pct: f64,
    /// Phase 37: Number of active miners/companies extracting from this deposit.
    pub active_miners: u32,
}

/// Tender summary for the Construction tab.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct TenderRow {
    pub id: String,
    /// Phase 40: Human-readable tender name.
    pub name: String,
    pub project_type: String,
    /// Phase 40: Estimated cost (value).
    pub value: f64,
    pub status: String,
    pub awarded: bool,
    /// Phase 40: Contractor company ID (if awarded).
    pub contractor: String,
}

/// KIO appeal summary.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct KioAppealRow {
    pub id: String,
    pub appellant: String,
    pub status: String,
}

/// Building defect summary.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct DefectRow {
    pub building_id: String,
    pub region_id: String,
    pub condition: f64,
    pub structural_defect: f64,
}

/// Shadow economy summary.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ShadowEconomySummary {
    pub total_hidden_fte: f64,
    pub total_pit_evaded: f64,
    pub shadow_gdp: f64,
    /// D.7.2: Inspectorate raid stats for the Shadow Economy tab.
    pub shadow_raids_conducted: u32,
    pub shadow_fines_collected: f64,
    pub shadow_illegals_deported: i64,
    pub shadow_total_pit_evaded: f64,
}

/// OHS / casualty summary.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct OhsSummary {
    pub total_deceased: i64,
    pub total_disabled: i64,
    pub total_unable_to_work_fte: f64,
    pub ohs_accidents_on_projects: u32,
}

/// Labor market summary.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct LaborSummary {
    pub unemployment_rate: f64,
    pub employed_total: f64,
    pub unemployed: f64,
    pub workforce: f64,
    pub average_wage: f64,
    /// Emergency Stabilization: Total furloughed workers across all companies.
    /// Distinct from unemployed — furloughed workers are retained by companies
    /// and excluded from active labor clearing.
    pub furloughed_total: f64,
    /// Phase 89: Peasant population (FreePeasant + Serf classes) across all regions.
    /// These are subsistence farmers operating outside the corporate structure
    /// on Smallholder parcels — NOT corporate agricultural wage laborers.
    pub peasant_population: f64,
    /// Phase 89: Peasant share of total national population (percentage).
    pub peasant_pct: f64,
    /// Phase 18S: Sports/recreation capacity per capita (Rule 17 — full-stack
    /// accountability). Exposed from HealthStatistics so the UI can render
    /// sports infrastructure access without reading internal state.
    pub sports_capacity_per_capita: f64,
}

/// ToT (Turn-over-Turn) and YoY (Year-over-Year) percentage deltas
/// for key macro indicators. `None` means no historical data available.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct TelemetryDeltas {
    /// GDP ToT delta (percent).
    pub gdp_tot: Option<f64>,
    /// GDP YoY delta (percent).
    pub gdp_yoy: Option<f64>,
    /// CPI ToT delta (percent).
    pub cpi_tot: Option<f64>,
    /// CPI YoY delta (percent).
    pub cpi_yoy: Option<f64>,
    /// PPI ToT delta (percent).
    pub ppi_tot: Option<f64>,
    /// PPI YoY delta (percent).
    pub ppi_yoy: Option<f64>,
    /// M3 ToT delta (percent).
    pub m3_tot: Option<f64>,
    /// M3 YoY delta (percent).
    pub m3_yoy: Option<f64>,
    /// Unemployment ToT delta (percentage points).
    pub unemployment_tot: Option<f64>,
    /// Unemployment YoY delta (percentage points).
    pub unemployment_yoy: Option<f64>,
    /// Shadow GDP ToT delta (percent).
    pub shadow_gdp_tot: Option<f64>,
    /// Shadow GDP YoY delta (percent).
    pub shadow_gdp_yoy: Option<f64>,
    /// Corruption ToT delta (absolute change).
    pub corruption_tot: Option<f64>,
    /// Corruption YoY delta (absolute change).
    pub corruption_yoy: Option<f64>,
    /// Population ToT delta (percent).
    pub population_tot: Option<f64>,
    /// Population YoY delta (percent).
    pub population_yoy: Option<f64>,
    /// Average wage ToT delta (percent).
    pub wage_tot: Option<f64>,
    /// Average wage YoY delta (percent).
    pub wage_yoy: Option<f64>,
    /// Phase 90: Peasant population ToT delta (percent).
    pub peasant_population_tot: Option<f64>,
    /// Phase 90: Peasant population YoY delta (percent).
    pub peasant_population_yoy: Option<f64>,
    /// Phase 90: Furloughed workers ToT delta (percent).
    pub furloughed_tot: Option<f64>,
    /// Phase 90: Furloughed workers YoY delta (percent).
    pub furloughed_yoy: Option<f64>,
    /// Phase 90: GDP per capita ToT delta (percent).
    pub gdp_per_capita_tot: Option<f64>,
    /// Phase 90: GDP per capita YoY delta (percent).
    pub gdp_per_capita_yoy: Option<f64>,
}

/// The flat, UI-ready projection of a single country at a point in time.
/// NOTE: This is an internal builder type used by the snapshot aggregation
/// functions. Tauri IPC commands use the smaller targeted response types
/// (e.g., `MacroIndicatorsResponse`, `VipPageResponse`) instead.
/// `TS` is not derived here because it references core engine types
/// (`GdpBreakdown`, `InflationIndices`, `MoneySupplySnapshot`) that are
/// frozen and do not derive `TS`.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct CountrySnapshot {
    // Identity
    pub name: String,
    pub turn: u32,
    pub year: u32,

    // Macro & Finance
    pub gdp_breakdown: GdpBreakdown,
    pub inflation_indices: InflationIndices,
    pub money_supply: MoneySupplySnapshot,
    pub treasury: TreasurySummary,
    pub central_bank_rate: f64,

    // Market & Logistics
    pub commodities: Vec<CommodityRow>,
    pub infrastructure: Vec<InfrastructureRow>,
    pub freight_config_summary: String,

    // Phase 26: Sector overview
    pub sectors: Vec<SectorRow>,

    // Construction & Geology
    pub tenders: Vec<TenderRow>,
    pub kio_appeals: Vec<KioAppealRow>,
    pub defects: Vec<DefectRow>,
    pub deposits: Vec<DepositRow>,

    // Society & Justice
    pub shadow_economy: ShadowEconomySummary,
    pub ohs: OhsSummary,
    pub labor: LaborSummary,
    pub corruption_index: f64,
    pub justice_coverage: f64,
    pub population: u64,
    pub sovereign_default_turns: u32,

    // Phase 24F: ToT/YoY deltas
    pub deltas: TelemetryDeltas,

    // Phase 32: Government & Parliament
    pub government: GovernmentSnapshot,
    pub parliament: ParliamentSnapshot,

    // Phase 34: Regions — geographic inequality and local government data
    pub regions: Vec<RegionRow>,

    // Phase 35: Finance tab
    pub finance: FinanceSnapshot,

    // Phase 49: VIP explorer (paginated)
    pub vips_page: Vec<VipDossierRow>,
    pub vip_total_count: usize,
    pub vip_dossier: Option<VipDossier>,

    // Phase 49: Banking tab (paginated bank list)
    pub banks_page: Vec<BankRow>,
    pub bank_total_count: usize,

    // Phase 49: Company explorer (paginated)
    pub companies_page: Vec<CompanyRow>,
    pub company_total_count: usize,
    pub company_detail: Option<CompanyDetail>,

    // Phase 49: Region drill-down (on-demand)
    pub region_detail: Option<RegionDetail>,

    // Phase 53: Megaregion drill-down (on-demand)
    pub megaregion_detail: Option<MegaregionDetail>,

    // Phase 49: Advisory council + royal dynasty for Parliament tab
    pub advisory_council: Option<AdvisoryCouncilSnapshot>,
    pub royal_dynasty: Option<RoyalDynastySnapshot>,
    pub government_form: String,
}

/// Phase 34: A region row for the Regions tab.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct RegionRow {
    pub id: String,
    pub display_name: String,
    pub megaregion: String,
    /// Phase 61.3: Megaregion ID for drill-down lookup (separate from display name).
    pub megaregion_id: String,
    pub population: i64,
    pub regional_gdp: f64,
    pub gdp_per_capita: f64,
    pub has_governance: bool,
    /// v4.22 WS3: human-readable governance mode — "Autonomous Council" |
    /// "Appointed Governor" | "Commissary Admin" | "Martial Law" | "None".
    pub governance_mode: String,
    pub liquid_reserves: f64,
}

/// v4.22 WS3: derive the display governance mode from real governance state —
/// commissary/martial administration first (status overrides head type), then
/// elected councils (Mayor + council) vs centrally appointed heads.
fn regional_governance_mode(g: Option<&RegionalGovernance>) -> String {
    let Some(g) = g else {
        return "None".to_string();
    };
    match g.admin_status {
        AdministrativeStatus::CommissaryAdministration => "Commissary Admin".to_string(),
        AdministrativeStatus::MartialLaw => "Martial Law".to_string(),
        AdministrativeStatus::Normal => match g.head_type {
            RegionalHeadType::Mayor => "Autonomous Council".to_string(),
            RegionalHeadType::CentralAdministrator => "Appointed Governor".to_string(),
            RegionalHeadType::VillageHead | RegionalHeadType::Governor => {
                "Appointed Governor".to_string()
            }
        },
    }
}

/// Treasury fields for the Finance tab.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct TreasurySummary {
    pub gdp: f64,
    pub population: u64,
    pub liquid_reserves: f64,
    pub private_capital: f64,
    pub infrastructure_level: f64,
    pub savings: f64,
}

/// Phase 35: Finance tab data — treasury, ministries, tax, debt, CB, banks,
/// consumer debt, and shadow economy. Reuses existing snapshot data rather
/// than creating disconnected accounting.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct FinanceSnapshot {
    // Treasury
    pub treasury_reserves: f64,
    pub gdp: f64,
    // Ministry allocations
    pub ministry_total_allocated: f64,
    pub ministry_total_spent: f64,
    pub ministry_total_cash: f64,
    // Tax revenue (from last collection)
    pub pit_revenue: f64,
    pub cit_revenue: f64,
    pub vat_revenue: f64,
    pub wealth_tax_revenue: f64,
    pub capital_gains_revenue: f64,
    // Phase 39: Customs and state property revenue
    pub customs_revenue: f64,
    pub state_property_revenue: f64,
    // Agent 4 — Phase 6: Smuggling diagnostics (Rule 17 — UI visibility).
    // These are diagnostic-only values (no fiat flows). Role-gating per
    // Rule 11 is handled by the frontend based on the player's position.
    pub smuggling_value: f64,
    pub smuggling_intercepted: f64,
    pub smuggling_tariff_loss: f64,
    pub smuggling_recovered_tariffs: f64,
    // Phase 89: Property tax revenue (aggregated from regional governments)
    pub property_tax_revenue: f64,
    // Phase 38: Tax rates for display in the Finance tab
    pub pit_rate: f64,
    pub cit_rate: f64,
    pub vat_rate: f64,
    pub wealth_tax_rate: f64,
    pub capital_gains_rate: f64,
    // Public debt
    pub total_public_debt: f64,
    pub debt_service: f64,
    pub weighted_avg_interest_rate: f64,
    // Phase 37: Debt holder breakdown
    pub debt_held_by_banks: f64,
    pub debt_held_by_central_bank: f64,
    pub debt_held_by_funds: f64,
    pub debt_held_by_citizens: f64,
    /// Phase 87+: Foreign-held sovereign debt. Computed from the authoritative
    /// debt_market.outstanding_securities holders with SecurityHolderType::ForeignEntity.
    pub debt_held_by_foreign: f64,
    // Central Bank
    pub m0: f64,
    pub m3: f64,
    pub cb_reference_rate: f64,
    // Phase 40: Additional CB rate parameters for Finance tab Detail column.
    pub cb_lombard_rate: f64,
    pub cb_discount_rate: f64,
    pub cb_rediscount_rate: f64,
    pub cb_deposit_rate: f64,
    pub cb_fx_reserves_total: f64,
    pub cb_gold_reserves: f64,
    pub cb_reserve_requirement_ratio: f64,
    pub cb_omo_holdings: f64,
    pub cb_liquidity_injected: f64,
    pub cb_last_omo_turn: u32,
    pub cb_last_omo_amount: f64,
    // Banking
    pub total_bank_reserves: f64,
    pub total_bank_deposits: f64,
    pub total_bank_loans: f64,
    pub total_consumer_debt: f64,
    pub dspw_bank_count: u32,
    // Shadow economy
    pub shadow_gdp: f64,
    pub pit_evaded: f64,
    // Phase 42: FX basket — top 3 foreign currencies held by Central Bank.
    pub fx_basket: Vec<FxBasketEntry>,
    /// Phase 54: Ministry expenditure breakdown for the Finance tab.
    pub ministry_expenditure_breakdown: Vec<MinistryExpenditureEntry>,
}

/// Phase 42: A single entry in the Central Bank's FX reserves basket.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct FxBasketEntry {
    pub currency: String,
    pub amount: f64,
    pub exchange_rate: f64,
}

/// Global snapshot containing all countries + global market state.
/// NOTE: Internal builder type. Not exported via `ts-rs` because it
/// contains `CountrySnapshot` which references frozen core types.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct GlobalSnapshot {
    pub turn: u32,
    pub year: u32,
    pub countries: BTreeMap<String, CountrySnapshot>,
    /// Phase 66: Diplomacy snapshot for the player's country.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diplomacy: Option<DiplomacySnapshot>,
    /// Phase 66: Foreign countries with Fog of War applied.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub foreign_countries: Vec<ForeignCountryRow>,
    /// Phase R10: Foreign sector balance — the external spending pool used
    /// by tourism and remittances. Visible to the player for transparency
    /// on cross-border capital flows.
    #[serde(default)]
    pub foreign_sector_balance: f64,
}

// ============================================================================
// PHASE 66: DIPLOMACY & FOG OF WAR SNAPSHOTS
// ============================================================================

/// Phase 66/67: Diplomacy snapshot for a single country's diplomatic view.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct DiplomacySnapshot {
    /// Player country name.
    pub country: String,
    /// Bilateral relations with all known countries.
    pub relations: Vec<RelationRow>,
    /// Diplomats posted by this country.
    pub diplomats: Vec<DiplomatRow>,
    /// Intelligence on foreign nations.
    pub foreign_intelligence: Vec<ForeignIntelRow>,
    /// Phase 67: Active and pending treaties involving this country.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub treaties: Vec<TreatyRow>,
    /// Phase 67: Global reputation score (-100 to +100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reputation: Option<f64>,
    /// Phase 67: Geopolitical doctrine label.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doctrine: Option<String>,
    /// Phase 68: International organizations the country belongs to.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub organizations: Vec<InternationalOrgRow>,
    /// Phase 68: Active sanctions against this country.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sanctions_against: Vec<SanctionRow>,
    /// v4.22.1 WS5.3: Localized wars declared through diplomatic friction.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub declared_wars: Vec<DeclaredWarRow>,
    /// v4.22.1 WS5.3: Bilateral friction levels (tension toward war).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tensions: Vec<TensionRow>,
}

/// v4.22.1 WS5.3: A localized war declared through diplomatic friction.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct DeclaredWarRow {
    pub aggressor: String,
    pub defender: String,
    pub start_turn: u32,
    pub reason: String,
}

/// v4.22.1 WS5.3: A bilateral friction level — the pair's tension level and
/// provocation count driving the AI war-escalation pipeline.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TensionRow {
    pub country_a: String,
    pub country_b: String,
    pub level: f64,
    pub provocations: u32,
}

/// Phase 66: A single bilateral relation row.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct RelationRow {
    /// Partner country name.
    pub partner: String,
    /// Relations score (-100 to 100).
    pub relations: i64,
    /// Frozen turns remaining.
    pub frozen_turns: i64,
    /// Whether free trade is active.
    pub free_trade: bool,
    /// Whether a customs union is active.
    pub customs_union: bool,
    /// Whether an embargo is in effect.
    pub embargo: bool,
    /// Treaty description.
    pub treaty_description: String,
}

/// Phase 66: A posted diplomat row.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct DiplomatRow {
    /// VIP ID.
    pub vip_id: String,
    /// VIP name.
    pub name: String,
    /// Host country.
    pub host_country: String,
    /// Post type label (Ambassador, Consul, Spy).
    pub post_type: String,
    /// Turn assigned.
    pub assigned_turn: u32,
    /// VIP traits.
    pub traits: Vec<String>,
}

/// Phase 66: Intelligence on a foreign country (Fog of War filtered).
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ForeignIntelRow {
    /// Target country name.
    pub country: String,
    /// Intel level label.
    pub intel_level: String,
    /// Estimated GDP range (low, high), or null if Unknown.
    pub estimated_gdp: Option<(f64, f64)>,
    /// Estimated military range (low, high), or null if Unknown.
    pub estimated_military: Option<(u32, u32)>,
    /// Estimated treasury range (low, high), or null if Unknown.
    pub estimated_treasury: Option<(f64, f64)>,
    /// Turn when intel was last updated.
    pub last_intel_turn: u32,
}

/// Phase 66: A foreign country row with Fog of War applied.
/// Hidden stats are `None` when intel level is Unknown.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ForeignCountryRow {
    /// Country name.
    pub name: String,
    /// Demonym (e.g., "Bactrians").
    pub demonym: String,
    /// Cultural group.
    pub cultural_group: String,
    /// Intel level label.
    pub intel_level: String,
    /// Estimated GDP range, or null if Unknown.
    pub estimated_gdp: Option<(f64, f64)>,
    /// Estimated military size range, or null if Unknown.
    pub estimated_military: Option<(u32, u32)>,
    /// Estimated treasury range, or null if Unknown.
    pub estimated_treasury: Option<(f64, f64)>,
    /// Relations score with player country (-100 to 100), if known.
    pub relations: Option<i64>,
    /// Whether government form is known.
    pub government_known: bool,
    /// Government form label (if known).
    pub government_form: Option<String>,
}

/// Phase 67: A treaty row for the diplomacy snapshot.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TreatyRow {
    /// Treaty ID.
    pub id: String,
    /// Treaty name.
    pub name: String,
    /// Status label (Proposed, Negotiating, Active, etc.).
    pub status: String,
    /// Participant country names.
    pub participants: Vec<String>,
    /// Clause labels.
    pub clauses: Vec<String>,
    /// Negotiation progress (0.0 to 1.0).
    pub negotiation_progress: f64,
    /// Diplomatic capacity cost.
    pub diplomatic_capacity_cost: u32,
    /// Turn initiated.
    pub initiated_turn: u32,
    /// Turn signed (if active/expired/abrogated).
    pub signed_turn: Option<u32>,
    /// Duration in turns.
    pub duration_turns: u32,
    /// Initiating country.
    pub initiator: String,
}

/// Phase 68: An international organization row for the diplomacy snapshot.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct InternationalOrgRow {
    /// Organization ID.
    pub id: String,
    /// Organization name.
    pub name: String,
    /// Integration level label.
    pub integration_level: String,
    /// Voting mechanism label.
    pub voting_mechanism: String,
    /// Member state names.
    pub member_states: Vec<String>,
    /// Number of active directives.
    pub directive_count: usize,
    /// v4.22.1 WS5.2: debated resolutions with per-country roll calls.
    pub directives: Vec<DirectiveRow>,
    /// Founded turn.
    pub founded_turn: u32,
    /// Active sanctions enacted by this organization.
    pub sanctions: Vec<SanctionRow>,
}

/// v4.22.1 WS5.2: A debated International Assembly resolution — status,
/// vote tally, and per-member roll call for the IA panel.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct DirectiveRow {
    pub id: String,
    pub title: String,
    pub status: String,
    pub mandate_type: String,
    pub enacted_turn: u32,
    pub compliance_deadline: u32,
    pub votes_for: usize,
    pub votes_against: usize,
    pub votes_abstain: usize,
    /// (member state, "For"|"Against"|"Abstain") roll call, sorted by member.
    pub votes: Vec<(String, String)>,
}

/// Phase 68: A sanction row for the diplomacy snapshot.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SanctionRow {
    /// Sanction ID.
    pub id: String,
    /// Sanctioned country.
    pub target_country: String,
    /// Sanctioning organization name.
    pub sanctioning_org: String,
    /// Sanction type label.
    pub sanction_type: String,
    /// Turn enacted.
    pub enacted_turn: u32,
    /// Duration in turns.
    pub duration_turns: u32,
    /// Reason.
    pub reason: String,
    /// Whether the sanction is still active.
    pub is_active: bool,
}

// ============================================================================
// PHASE 85: FACTIONAL DOMAINS, COTTAGE INDUSTRY, AND GUILDS
// ============================================================================

/// Phase 85: Factional domain row for the FactionalDomainsPage.
/// Role-gated (Rule 11): foreign observers see only faction_type and population.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct FactionalDomainRow {
    /// Domain ID
    pub id: String,
    /// Parent region name
    pub region_name: String,
    /// Faction type display name
    pub faction_type: String,
    /// Population within the domain
    pub population: i64,
    /// Governing faction entity name (classified for foreign observers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub governing_faction: Option<String>,
    /// Entry tariff rate (classified for foreign observers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_tariff_rate: Option<f64>,
    /// Feudal dues rate (classified for foreign observers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feudal_dues_rate: Option<f64>,
    /// Tithe rate (classified for foreign observers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tithe_rate: Option<f64>,
    /// Whether commercial zoning is allowed
    pub allows_commercial_zoning: bool,
    /// Education slots (passive, clergy domains)
    pub education_slots: u32,
    /// Health capacity (passive, clergy domains)
    pub health_capacity: f64,
    /// Number of controlled parcels
    pub parcel_count: usize,
}

/// Phase 85: Cottage industry summary for MarketPage and FactionalDomainsPage.
/// Role-gated (Rule 11): foreign observers see only aggregate estimates.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CottageIndustrySummary {
    /// Total FTE allocated to cottage production
    pub total_cottage_fte: f64,
    /// Output by commodity name
    pub output_by_commodity: Vec<CottageOutputEntry>,
    /// Raw material demand by commodity name
    pub raw_input_demand: Vec<CottageInputEntry>,
}

/// Phase 85: Single cottage output entry.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CottageOutputEntry {
    pub commodity: String,
    pub volume: f64,
}

/// Phase 85: Single cottage raw input entry.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CottageInputEntry {
    pub commodity: String,
    pub demand: f64,
}

/// Phase 85: Guild row for the GuildsPage.
/// Role-gated (Rule 11): foreign observers see only public registry data.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct GuildRow {
    /// Guild company ID
    pub id: String,
    /// Guild name
    pub name: String,
    /// Production sector
    pub sector: String,
    /// Number of member workshops
    pub member_count: usize,
    /// Welfare fund amount (classified for foreign observers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub welfare_fund: Option<f64>,
    /// Dividend per member last turn (classified for foreign observers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dividend_per_member: Option<f64>,
    /// Quality standard (financial premium)
    pub quality_standard: f64,
    /// Whether the guild has a state charter
    pub has_charter: bool,
    /// Jurisdiction domain ID
    pub jurisdiction_domain_id: String,
}

/// Phase 85: Factional domains snapshot for the FactionalDomainsPage.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct FactionalDomainsSnapshot {
    pub domains: Vec<FactionalDomainRow>,
    pub cottage_summary: CottageIndustrySummary,
}

/// Phase 85: Guilds snapshot for the GuildsPage.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct GuildsSnapshot {
    pub guilds: Vec<GuildRow>,
}

// ============================================================================
// R2: Cooperative Sector — federations, patronage, cooperative banks
// ============================================================================

/// R2: Cooperative federation row for the CooperativeSectorSnapshot.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CooperativeFederationRow {
    /// Federation ID.
    pub id: String,
    /// Number of member cooperatives.
    pub member_count: usize,
    /// Shared administrative fund (cash).
    pub admin_fund: f64,
    /// Total outstanding joint debt.
    pub total_debt: f64,
    /// Whether the federation is active.
    pub active: bool,
}

/// R2: Cooperative company row for the CooperativeSectorSnapshot.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CooperativeRow {
    /// Company ID.
    pub id: String,
    /// Company name.
    pub name: String,
    /// Number of member-workers.
    pub member_count: u32,
    /// Patronage pool (accumulated patronage for distribution).
    pub patronage_pool: f64,
    /// Federation ID (if any).
    pub federation_id: Option<String>,
    /// Company capital.
    pub company_capital: f64,
}

/// R2: Cooperative sector snapshot for the CooperativeSectorPage.
/// Exposes cooperative companies, federations, and patronage state.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CooperativeSectorSnapshot {
    /// All cooperative companies.
    pub cooperatives: Vec<CooperativeRow>,
    /// All cooperative federations.
    pub federations: Vec<CooperativeFederationRow>,
    /// Total patronage distributed this turn (sum of patronage_pool).
    pub total_patronage: f64,
    /// Number of cooperative banks.
    pub cooperative_bank_count: usize,
}

// ============================================================================
// Phase 87+: Organizations — consolidated view for Guilds, Unions, Movements
// ============================================================================

/// Phase 87+: Organization category for the consolidated Organizations page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
#[serde(rename_all = "snake_case")]
pub enum OrganizationCategory {
    #[default]
    Guild,
    TradeUnion,
    PoliticalMovement,
    ChamberOfCommerce,
}

/// Phase 87+: A single organization row in the consolidated list.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct OrganizationRow {
    pub id: String,
    pub name: String,
    pub category: OrganizationCategory,
    pub sector: String,
    pub region_id: String,
    pub member_count: usize,
    /// Funds (classified for foreign observers — stripped in build function).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub funds: Option<f64>,
    pub activity_summary: String,
}

/// Phase 87+: Organizations snapshot for the consolidated OrganizationsPage.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct OrganizationsSnapshot {
    pub organizations: Vec<OrganizationRow>,
}

/// Phase 87+: Guild-specific detail data.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct GuildDetailData {
    pub sector: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub welfare_fund: Option<f64>,
    pub welfare_contribution_rate: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dividend_per_member: Option<f64>,
    pub quality_standard: f64,
    pub has_charter: bool,
    pub jurisdiction_domain_id: String,
    pub member_workshop_ids: Vec<String>,
    pub master_class_ids: Vec<String>,
    pub guild_raw_inventory: Vec<(String, f64)>,
}

/// Phase 87+: Union-specific detail data.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct UnionDetailData {
    pub sector: String,
    pub scale_level: String,
    pub budget: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strike_fund: Option<f64>,
    pub political_power: f64,
    pub militancy: f64,
    pub wage_demand: f64,
    pub safety_demand: f64,
    pub on_strike: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leader_vip_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leader_name: Option<String>,
    pub member_company_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_strike_turn: Option<u32>,
}

/// Phase 87+: Political movement-specific detail data.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MovementDetailData {
    pub movement_type: String,
    pub initiating_class: String,
    pub start_turn: u32,
    pub expected_duration: u32,
    pub intensity: f64,
    pub participant_count: i64,
    pub union_backed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backing_union_id: Option<String>,
    pub strike_fund_per_participant: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_company_ids: Option<Vec<String>>,
    pub status: String,
    pub demands: Vec<String>,
}

/// Phase 87+: Organization detail — category-specific detail page data.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct OrganizationDetail {
    pub id: String,
    pub name: String,
    pub category: OrganizationCategory,
    pub region_id: String,
    pub member_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub funds: Option<f64>,
    pub activity_summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_detail: Option<GuildDetailData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub union_detail: Option<UnionDetailData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub movement_detail: Option<MovementDetailData>,
}

/// Phase 85: Build a factional domains snapshot for a country.
/// Role-gated (Rule 11): strips classified data for foreign observers.
pub fn build_factional_domains_snapshot(
    country: &crate::state::Country,
    is_classified: bool,
) -> FactionalDomainsSnapshot {
    let mut domains = Vec::new();

    for region in &country.regions {
        for domain in region.micro_regions.values() {
            let faction_type_str = format!("{:?}", domain.faction_type);

            domains.push(FactionalDomainRow {
                id: domain.id.clone(),
                region_name: region.display_name.clone(),
                faction_type: faction_type_str,
                population: domain.population,
                governing_faction: if is_classified {
                    None
                } else {
                    domain.governing_faction_id.clone()
                },
                entry_tariff_rate: if is_classified {
                    None
                } else {
                    Some(domain.local_laws.entry_tariff_rate)
                },
                feudal_dues_rate: if is_classified {
                    None
                } else {
                    Some(domain.local_laws.feudal_dues_rate)
                },
                tithe_rate: if is_classified {
                    None
                } else {
                    Some(domain.local_laws.tithe_rate)
                },
                allows_commercial_zoning: domain.local_laws.allows_commercial_zoning,
                education_slots: domain.education_slots,
                health_capacity: domain.health_capacity,
                parcel_count: domain.controlled_parcel_ids.len(),
            });
        }
    }

    // Build cottage summary from class demographics
    let mut total_cottage_fte = 0.0;
    let mut output_map: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    let mut input_map: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();

    for region in &country.regions {
        for demo in region.class_demographics.rural_classes.values() {
            total_cottage_fte += demo.cottage_fte_allocated;
            for (commodity, volume) in &demo.cottage_output {
                let name = format!("{:?}", commodity);
                *output_map.entry(name).or_insert(0.0) += volume;
            }
            for (commodity, qty) in &demo.cottage_raw_inventory {
                let name = format!("{:?}", commodity);
                *input_map.entry(name).or_insert(0.0) += qty;
            }
        }
    }

    // Add noise for foreign observers (Rule 11)
    if is_classified {
        total_cottage_fte *= 1.2; // ±20% estimate
    }

    let output_by_commodity = output_map
        .into_iter()
        .map(|(commodity, volume)| CottageOutputEntry { commodity, volume })
        .collect();
    let raw_input_demand = input_map
        .into_iter()
        .map(|(commodity, demand)| CottageInputEntry { commodity, demand })
        .collect();

    FactionalDomainsSnapshot {
        domains,
        cottage_summary: CottageIndustrySummary {
            total_cottage_fte,
            output_by_commodity,
            raw_input_demand,
        },
    }
}

/// Phase 85: Build a guilds snapshot for a country.
/// Role-gated (Rule 11): strips financial data for foreign observers.
pub fn build_guilds_snapshot(
    companies: &[crate::entities::Company],
    is_classified: bool,
) -> GuildsSnapshot {
    let mut guilds = Vec::new();

    for company in companies {
        if let crate::entities::LegalForm::Guild(data) = &company.legal_form {
            guilds.push(GuildRow {
                id: company.id.clone(),
                name: company.name.clone(),
                sector: data.guild_sector.clone(),
                member_count: data.member_workshop_ids.len(),
                welfare_fund: if is_classified {
                    None
                } else {
                    Some(data.welfare_fund)
                },
                dividend_per_member: if is_classified {
                    None
                } else {
                    // v4.21 WS4: real per-turn dividend paid per member
                    // workshop, persisted by Phase 85A.3 after the
                    // double-entry class credits.
                    company
                        .extra
                        .get("guild_last_dividend_per_member")
                        .and_then(|v| v.as_f64())
                },
                quality_standard: data.quality_standard,
                has_charter: data.has_charter,
                jurisdiction_domain_id: data.jurisdiction_domain_id.clone(),
            });
        }
    }

    GuildsSnapshot { guilds }
}

/// R2: Build a cooperative sector snapshot for a country.
/// Exposes cooperative companies, federations, and patronage state.
/// Role-gated (Rule 11): strips financial data for foreign observers.
pub fn build_cooperative_sector_snapshot(
    companies: &[crate::entities::Company],
    country: &crate::state::Country,
    is_classified: bool,
) -> CooperativeSectorSnapshot {
    let mut cooperatives = Vec::new();
    let mut total_patronage = 0.0_f64;
    let mut cooperative_bank_count = 0usize;

    for company in companies {
        if let crate::entities::LegalForm::Cooperative(data) = &company.legal_form {
            cooperatives.push(CooperativeRow {
                id: company.id.clone(),
                name: company.name.clone(),
                member_count: data.member_count,
                patronage_pool: if is_classified {
                    0.0
                } else {
                    data.patronage_pool
                },
                federation_id: data.federation_id.clone(),
                company_capital: if is_classified {
                    0.0
                } else {
                    company.company_capital
                },
            });
            total_patronage += data.patronage_pool;
        }
        // Count cooperative banks
        if company.bank_type == Some(crate::state::banking::BankType::Cooperative) {
            cooperative_bank_count += 1;
        }
    }

    let federations = country
        .cooperative_federations
        .iter()
        .map(|f| CooperativeFederationRow {
            id: f.id.clone(),
            member_count: f.member_ids.len(),
            admin_fund: if is_classified { 0.0 } else { f.admin_fund },
            total_debt: if is_classified {
                0.0
            } else {
                f.total_outstanding_debt()
            },
            active: f.active,
        })
        .collect();

    CooperativeSectorSnapshot {
        cooperatives,
        federations,
        total_patronage: if is_classified { 0.0 } else { total_patronage },
        cooperative_bank_count,
    }
}

/// Phase 87+: Build a consolidated organizations snapshot for a country.
/// Aggregates Guilds, Trade Unions, and Political Movements into a single list.
/// Role-gated (Rule 11): strips financial data for foreign observers.
pub fn build_organizations_snapshot(
    country: &crate::state::Country,
    companies: &[crate::entities::Company],
    unions: &[crate::entities::union::Union],
    is_classified: bool,
) -> OrganizationsSnapshot {
    let mut organizations = Vec::new();

    // Guilds
    for company in companies {
        if let crate::entities::LegalForm::Guild(data) = &company.legal_form {
            organizations.push(OrganizationRow {
                id: company.id.clone(),
                name: company.name.clone(),
                category: OrganizationCategory::Guild,
                sector: data.guild_sector.clone(),
                region_id: company.region_id.clone(),
                member_count: data.member_workshop_ids.len(),
                funds: if is_classified {
                    None
                } else {
                    Some(data.welfare_fund)
                },
                activity_summary: format!(
                    "Guild with {} workshops, quality {:.2}",
                    data.member_workshop_ids.len(),
                    data.quality_standard
                ),
            });
        }
    }

    // Trade Unions
    for union in unions {
        // v4.19 WS5: members are the unionized *workers*, not the firm
        // count — `company_ids.len()` produced the cloned "Members: 154"
        // where 154 was actually the number of companies in the sector.
        let member_workers: usize = companies
            .iter()
            .filter(|c| {
                c.union_id.as_deref() == Some(union.id.as_str())
                    || union.company_ids.contains(&c.id)
            })
            .map(|c| c.fulfilled_fte as usize)
            .sum();
        let member_firms = union.company_ids.len();
        organizations.push(OrganizationRow {
            id: union.id.clone(),
            name: union.name.clone(),
            category: OrganizationCategory::TradeUnion,
            sector: format!("{:?}", union.sector),
            region_id: union.region_id.clone(),
            member_count: member_workers,
            funds: if is_classified {
                None
            } else {
                Some(union.budget + union.strike_fund)
            },
            activity_summary: if union.on_strike {
                "ON STRIKE".to_string()
            } else {
                format!(
                    "{} workers across {} member companies",
                    member_workers, member_firms
                )
            },
        });
    }

    // Political Movements
    for movement in &country.politics.mass_movements {
        organizations.push(OrganizationRow {
            id: movement.id.clone(),
            name: format!(
                "{} in {}",
                movement_type_label(&movement.movement_type),
                movement.region_id
            ),
            category: OrganizationCategory::PoliticalMovement,
            sector: movement.initiating_class.clone(),
            region_id: movement.region_id.clone(),
            member_count: movement.participant_count as usize,
            funds: if is_classified {
                None
            } else {
                Some(movement.strike_fund_per_participant * movement.participant_count as f64)
            },
            activity_summary: format!(
                "Intensity {:.2}, {} participants, {:?}",
                movement.intensity, movement.participant_count, movement.status
            ),
        });
    }

    OrganizationsSnapshot { organizations }
}

/// Phase 87+: Build an organization detail snapshot.
/// Category-specific data is populated based on the organization type.
/// Role-gated (Rule 11): strips classified data for foreign observers.
/// Human-readable label for a mass-movement type (used in Organizations UI).
fn movement_type_label(t: &crate::politics::mass_movements::MassMovementType) -> &'static str {
    use crate::politics::mass_movements::MassMovementType::*;
    match t {
        IndustrialStrike => "Industrial Strike",
        Riot => "Riot",
        PeacefulProtest => "Peaceful Protest",
        Occupation => "Occupation",
        Boycott => "Boycott",
    }
}

pub fn build_organization_detail(
    country: &crate::state::Country,
    companies: &[crate::entities::Company],
    unions: &[crate::entities::union::Union],
    category: &str,
    id: &str,
    is_classified: bool,
) -> Option<OrganizationDetail> {
    match category {
        "guild" => {
            let company = companies.iter().find(|c| c.id == id)?;
            let data = match &company.legal_form {
                crate::entities::LegalForm::Guild(d) => d,
                _ => return None,
            };
            Some(OrganizationDetail {
                id: company.id.clone(),
                name: company.name.clone(),
                category: OrganizationCategory::Guild,
                region_id: company.region_id.clone(),
                member_count: data.member_workshop_ids.len(),
                funds: if is_classified {
                    None
                } else {
                    Some(data.welfare_fund)
                },
                activity_summary: format!("Craft Guild — {} sector", data.guild_sector),
                guild_detail: Some(GuildDetailData {
                    sector: data.guild_sector.clone(),
                    welfare_fund: if is_classified {
                        None
                    } else {
                        Some(data.welfare_fund)
                    },
                    welfare_contribution_rate: data.welfare_contribution_rate,
                    // v4.21 WS4: real per-turn dividend paid per member
                    // workshop, persisted by Phase 85A.3.
                    dividend_per_member: if is_classified {
                        None
                    } else {
                        company
                            .extra
                            .get("guild_last_dividend_per_member")
                            .and_then(|v| v.as_f64())
                    },
                    quality_standard: data.quality_standard,
                    has_charter: data.has_charter,
                    jurisdiction_domain_id: data.jurisdiction_domain_id.clone(),
                    member_workshop_ids: data.member_workshop_ids.clone(),
                    master_class_ids: data.master_class_ids.clone(),
                    guild_raw_inventory: data
                        .guild_raw_inventory
                        .iter()
                        .map(|(c, q)| (format!("{:?}", c), *q))
                        .collect(),
                }),
                ..Default::default()
            })
        }
        "trade_union" => {
            let union = unions.iter().find(|u| u.id == id)?;
            let leader_name = union.leader_vip_id.as_ref().and_then(|lid| {
                country
                    .politics
                    .vip_registry
                    .as_ref()
                    .and_then(|r| r.get(lid))
                    .map(|v| v.full_name.clone())
            });
            let member_workers: usize = companies
                .iter()
                .filter(|c| {
                    c.union_id.as_deref() == Some(union.id.as_str())
                        || union.company_ids.contains(&c.id)
                })
                .map(|c| c.fulfilled_fte as usize)
                .sum();
            Some(OrganizationDetail {
                id: union.id.clone(),
                name: union.name.clone(),
                category: OrganizationCategory::TradeUnion,
                region_id: union.region_id.clone(),
                member_count: member_workers,
                funds: if is_classified {
                    None
                } else {
                    Some(union.budget + union.strike_fund)
                },
                activity_summary: if union.on_strike {
                    "ON STRIKE".to_string()
                } else {
                    format!("{} unionized workers", member_workers)
                },
                union_detail: Some(UnionDetailData {
                    sector: format!("{:?}", union.sector),
                    scale_level: format!("{:?}", union.scale_level),
                    budget: union.budget,
                    strike_fund: if is_classified {
                        None
                    } else {
                        Some(union.strike_fund)
                    },
                    political_power: union.political_power,
                    militancy: union.militancy,
                    wage_demand: union.wage_demand,
                    safety_demand: union.safety_demand,
                    on_strike: union.on_strike,
                    leader_vip_id: union.leader_vip_id.clone(),
                    leader_name,
                    member_company_ids: union.company_ids.iter().cloned().collect(),
                    last_strike_turn: union.last_strike_turn,
                }),
                ..Default::default()
            })
        }
        "political_movement" => {
            let movement = country
                .politics
                .mass_movements
                .iter()
                .find(|m| m.id == id)?;
            Some(OrganizationDetail {
                id: movement.id.clone(),
                name: format!("{:?} in {}", movement.movement_type, movement.region_id),
                category: OrganizationCategory::PoliticalMovement,
                region_id: movement.region_id.clone(),
                member_count: movement.participant_count as usize,
                funds: if is_classified {
                    None
                } else {
                    Some(movement.strike_fund_per_participant * movement.participant_count as f64)
                },
                activity_summary: format!("Status: {:?}", movement.status),
                movement_detail: Some(MovementDetailData {
                    movement_type: format!("{:?}", movement.movement_type),
                    initiating_class: movement.initiating_class.clone(),
                    start_turn: movement.start_turn,
                    expected_duration: movement.expected_duration,
                    intensity: movement.intensity,
                    participant_count: movement.participant_count,
                    union_backed: movement.union_backed,
                    backing_union_id: movement.union_id.clone(),
                    strike_fund_per_participant: movement.strike_fund_per_participant,
                    target_company_ids: if is_classified {
                        None
                    } else {
                        Some(movement.target_companies.clone())
                    },
                    status: format!("{:?}", movement.status),
                    demands: movement.demands.clone(),
                }),
                ..Default::default()
            })
        }
        _ => None,
    }
}

/// Phase 85B: A single City Region row in the Cities snapshot.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CityRegionRow {
    /// City region ID.
    pub id: String,
    /// Display name.
    pub display_name: String,
    /// Parent (original rural) region ID.
    pub parent_region_id: String,
    /// Population.
    pub population: i64,
    /// Turn when the city emancipated.
    pub emancipated_turn: u32,
    /// Number of parcels under city control.
    pub parcel_count: usize,
    /// Annexation cooldown remaining (turns).
    pub annexation_cooldown: u32,
    /// City treasury liquid reserves (classified for foreign observers).
    pub treasury_reserves: f64,
    /// Development level (0.0-1.0).
    pub development_level: f64,
}

/// Phase 85B: Cities snapshot for the CitiesPage.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CitiesSnapshot {
    pub cities: Vec<CityRegionRow>,
}

/// Phase 85B: Build a cities snapshot for a country.
/// Role-gated (Rule 11): strips treasury data for foreign observers.
pub fn build_cities_snapshot(
    country: &crate::state::Country,
    is_classified: bool,
) -> CitiesSnapshot {
    let mut cities = Vec::new();

    for region in &country.regions {
        if !region.is_city() {
            continue;
        }
        let meta = region.city_metadata.as_ref();
        let treasury_reserves = if is_classified {
            0.0 // Strip classified financial data for foreign observers.
        } else {
            region.treasury.liquid_reserves
        };
        cities.push(CityRegionRow {
            id: region.id.clone(),
            display_name: region.display_name.clone(),
            parent_region_id: meta.map(|m| m.parent_region_id.clone()).unwrap_or_default(),
            population: region.population,
            emancipated_turn: meta.map(|m| m.emancipated_turn).unwrap_or(0),
            parcel_count: region.parcel_ids.len(),
            annexation_cooldown: meta.map(|m| m.annexation_cooldown).unwrap_or(0),
            treasury_reserves,
            development_level: region.development_level,
        });
    }

    CitiesSnapshot { cities }
}

/// Phase 66: Build a diplomacy snapshot for a country.
pub fn build_diplomacy_snapshot(
    state: &crate::state::GameState,
    country_name: &str,
    diplomacy: &std::collections::HashMap<
        String,
        std::collections::HashMap<String, crate::international::DiplomaticRelation>,
    >,
) -> DiplomacySnapshot {
    let mut relations = Vec::new();
    if let Some(partners) = diplomacy.get(country_name) {
        let mut sorted_partners: Vec<_> = partners.iter().collect();
        sorted_partners.sort_by_key(|(k, _)| *k);
        for (partner, rel) in sorted_partners {
            relations.push(RelationRow {
                partner: partner.clone(),
                relations: rel.relations,
                frozen_turns: rel.frozen_turns,
                free_trade: rel.free_trade,
                customs_union: rel.customs_union,
                embargo: rel.ban_import || rel.ban_export,
                treaty_description: rel.treaty_description.clone(),
            });
        }
    }

    let mut diplomats = Vec::new();
    if let Some(country) = state.countries.get(country_name) {
        if let Some(registry) = &country.politics.vip_registry {
            let mut posted_vips: Vec<_> = registry
                .vips
                .values()
                .filter(|v| v.diplomatic_post.is_some())
                .collect();
            posted_vips.sort_by_key(|v| v.id.clone());
            for vip in posted_vips {
                let post = vip.diplomatic_post.as_ref().unwrap();
                diplomats.push(DiplomatRow {
                    vip_id: vip.id.clone(),
                    name: vip.full_name.clone(),
                    host_country: post.host_country.clone(),
                    post_type: format!("{:?}", post.post_type),
                    assigned_turn: post.assigned_turn,
                    traits: vip.traits.clone(),
                });
            }
        }
    }

    let mut foreign_intelligence = Vec::new();
    if let Some(intel_map) = state.foreign_intelligence.get(country_name) {
        let mut sorted_intel: Vec<_> = intel_map.iter().collect();
        sorted_intel.sort_by_key(|(k, _)| *k);
        for (target, intel) in sorted_intel {
            foreign_intelligence.push(ForeignIntelRow {
                country: target.clone(),
                intel_level: intel.intel_level.as_str().to_string(),
                estimated_gdp: intel.estimated_gdp,
                estimated_military: intel.estimated_military,
                estimated_treasury: intel.estimated_treasury,
                last_intel_turn: intel.last_intel_turn,
            });
        }
    }

    // Phase 67: Build treaty rows for this country
    let mut treaties = Vec::new();
    for treaty in state.treaty_registry.treaties_for_country(country_name) {
        treaties.push(TreatyRow {
            id: treaty.id.clone(),
            name: treaty.name.clone(),
            status: treaty.status.as_str().to_string(),
            participants: treaty.participants.clone(),
            clauses: treaty
                .clauses
                .iter()
                .map(|c| c.as_str().to_string())
                .collect(),
            negotiation_progress: treaty.negotiation_progress,
            diplomatic_capacity_cost: treaty.diplomatic_capacity_cost,
            initiated_turn: treaty.initiated_turn,
            signed_turn: treaty.signed_turn,
            duration_turns: treaty.duration_turns,
            initiator: treaty.initiator.clone(),
        });
    }

    // Phase 67: Get reputation and doctrine
    let (reputation, doctrine) = if let Some(country) = state.countries.get(country_name) {
        (
            Some(country.global_reputation.score),
            Some(country.geopolitical_doctrine.as_str().to_string()),
        )
    } else {
        (None, None)
    };

    // Phase 68: Build organization rows for this country
    let current_turn = state.calendar.global_turn;
    let mut organizations = Vec::new();
    for org in state
        .international_organizations
        .orgs_for_country(country_name)
    {
        let org_sanctions: Vec<SanctionRow> = state
            .active_sanctions
            .sanctions
            .iter()
            .filter(|s| s.sanctioning_org == org.id && s.is_active_at(current_turn))
            .map(|s| SanctionRow {
                id: s.id.clone(),
                target_country: s.target_country.clone(),
                sanctioning_org: org.name.clone(),
                sanction_type: s.sanction_type.as_str().to_string(),
                enacted_turn: s.enacted_turn,
                duration_turns: s.duration_turns,
                reason: s.reason.clone(),
                is_active: s.is_active_at(current_turn),
            })
            .collect();
        let directives: Vec<DirectiveRow> = org
            .directives
            .iter()
            .map(|d| DirectiveRow {
                id: d.id.clone(),
                title: d.title.clone(),
                status: d.status.clone(),
                mandate_type: d.mandate_type.as_str().to_string(),
                enacted_turn: d.enacted_turn,
                compliance_deadline: d.compliance_deadline,
                votes_for: d.votes.values().filter(|v| v.as_str() == "For").count(),
                votes_against: d
                    .votes
                    .values()
                    .filter(|v| v.as_str() == "Against")
                    .count(),
                votes_abstain: d
                    .votes
                    .values()
                    .filter(|v| v.as_str() == "Abstain")
                    .count(),
                votes: d
                    .votes
                    .iter()
                    .map(|(m, v)| (m.clone(), v.clone()))
                    .collect(),
            })
            .collect();
        organizations.push(InternationalOrgRow {
            id: org.id.clone(),
            name: org.name.clone(),
            integration_level: org.integration_level.as_str().to_string(),
            voting_mechanism: org.voting_mechanism.as_str().to_string(),
            member_states: org.member_states.clone(),
            directive_count: org.directives.len(),
            directives,
            founded_turn: org.founded_turn,
            sanctions: org_sanctions,
        });
    }

    // Phase 68: Build sanctions against this country
    let sanctions_against: Vec<SanctionRow> = state
        .active_sanctions
        .active_sanctions_against(country_name, current_turn)
        .into_iter()
        .map(|s| SanctionRow {
            id: s.id.clone(),
            target_country: s.target_country.clone(),
            sanctioning_org: s.sanctioning_org.clone(),
            sanction_type: s.sanction_type.as_str().to_string(),
            enacted_turn: s.enacted_turn,
            duration_turns: s.duration_turns,
            reason: s.reason.clone(),
            is_active: true,
        })
        .collect();

    // v4.22.1 WS5.3: declared localized wars + bilateral friction levels.
    let declared_wars: Vec<DeclaredWarRow> = state
        .declared_wars
        .iter()
        .map(|w| DeclaredWarRow {
            aggressor: w.aggressor.clone(),
            defender: w.defender.clone(),
            start_turn: w.start_turn,
            reason: format!("{:?}", w.reason),
        })
        .collect();
    let mut tensions: Vec<TensionRow> = state
        .bilateral_tensions
        .iter()
        .filter_map(|(key, t)| {
            let mut parts = key.splitn(2, '|');
            Some(TensionRow {
                country_a: parts.next()?.to_string(),
                country_b: parts.next()?.to_string(),
                level: t.level,
                provocations: t.provocation_count,
            })
        })
        .filter(|t| t.level > 0.0)
        .collect();
    tensions.sort_by(|a, b| {
        b.level
            .partial_cmp(&a.level)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    DiplomacySnapshot {
        country: country_name.to_string(),
        relations,
        diplomats,
        foreign_intelligence,
        treaties,
        reputation,
        doctrine,
        organizations,
        sanctions_against,
        declared_wars,
        tensions,
    }
}

/// Phase 66: Build foreign country rows with Fog of War applied.
/// The player country gets full data; all others get filtered.
pub fn build_foreign_country_rows(
    state: &crate::state::GameState,
    player_country: &str,
    diplomacy: &std::collections::HashMap<
        String,
        std::collections::HashMap<String, crate::international::DiplomaticRelation>,
    >,
) -> Vec<ForeignCountryRow> {
    let mut rows = Vec::new();
    let mut sorted_names: Vec<&String> = state
        .countries
        .keys()
        .filter(|n| *n != player_country)
        .collect();
    sorted_names.sort();

    for name in sorted_names {
        let country = match state.countries.get(name) {
            Some(c) => c,
            None => continue,
        };

        // Get intel level for this country from player's perspective
        let intel = state
            .foreign_intelligence
            .get(player_country)
            .and_then(|m| m.get(name))
            .cloned()
            .unwrap_or_else(crate::international::fog_of_war::ForeignIntelligence::unknown);

        let fog_result = crate::international::fog_of_war::apply_fog_of_war(
            country.budget.gdp,
            country.order_of_battle.unit_count() as u32,
            country.budget.liquid_reserves,
            country.budget.population as f64,
            &intel,
        );

        // Get relations if known
        let relations = diplomacy
            .get(player_country)
            .and_then(|p| p.get(name))
            .map(|r| r.relations);

        let government_form = if intel.government_known {
            Some(country.politics.government_form.display_name().to_string())
        } else {
            None
        };

        rows.push(ForeignCountryRow {
            name: name.clone(),
            demonym: country.macro_indicators.demonym.clone(),
            cultural_group: country.macro_indicators.cultural_group.clone(),
            intel_level: fog_result.intel_level.as_str().to_string(),
            estimated_gdp: fog_result.gdp,
            estimated_military: fog_result.military,
            estimated_treasury: fog_result.treasury,
            relations,
            government_known: intel.government_known,
            government_form,
        });
    }

    rows
}

// ============================================================================
// PHASE 32: GOVERNMENT & PARLIAMENT SNAPSHOTS
// ============================================================================

/// Ideology Step 6: UI projection of `IdeologyCoordinates` for 3-axis radar
/// chart visualization (Directive 17 — full-stack accountability).
///
/// This is a plain DTO with no behavioral methods. It is `Option` on every
/// consumer struct so the backend can strip it for role-gated fog-of-war
/// (Directive 11 — the player is not omniscient; classified stats are
/// obfuscated for non-VIP entities).
#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct IdeologyCoordinatesDto {
    /// -1.0 = total collectivization, +1.0 = laissez-faire capitalism
    pub economy: f64,
    /// -1.0 = totalitarian, +1.0 = anarchic liberty
    pub liberty: f64,
    /// -1.0 = revolutionary futurism, +1.0 = entrenched traditionalism
    pub tradition: f64,
}

impl From<IdeologyCoordinates> for IdeologyCoordinatesDto {
    fn from(c: IdeologyCoordinates) -> Self {
        Self {
            economy: c.economy,
            liberty: c.liberty,
            tradition: c.tradition,
        }
    }
}

/// Government tab data.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct GovernmentSnapshot {
    pub head_of_state_name: String,
    /// v4.22.1 WS2.2: VIP registry id of the head of state — enables the
    /// clickable dossier link on the Government card.
    pub head_of_state_vip_id: Option<String>,
    pub head_of_state_role: String,
    pub pm_name: String,
    pub pm_party: String,
    pub pm_ideology: String,
    /// Ideology Step 6: Ruling party's ideological coordinates for 3-axis
    /// radar chart (Directive 17). `Option` for fog-of-war stripping (Rule 11).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinates: Option<IdeologyCoordinatesDto>,
    pub cabinet: Vec<MinisterRow>,
    pub state_of_emergency: Option<EmergencySnapshot>,
    pub political_capital: f64,
    /// Phase 41: Named VIPs moved from ParliamentSnapshot to GovernmentSnapshot.
    pub vips: Vec<VipRow>,
    /// Phase 54: Government form string for monarchy detection in the UI.
    pub government_form: String,
    /// Phase 54: Royal dynasty snapshot (only populated for monarchies).
    pub royal_dynasty: Option<RoyalDynastySnapshot>,
    /// D.7.1: Inspectorate stats for the Government tab.
    pub inspectorate_sanepid_capacity: f64,
    pub inspectorate_building_capacity: f64,
    pub inspectorate_environmental_capacity: f64,
    pub inspectorate_labor_capacity: f64,
    pub inspectorate_violations_detected: u32,
    pub inspectorate_fines_issued: f64,
    pub inspectorate_corruption_index: f64,
    pub inspectorate_bribes_accepted: u32,
    pub inspectorate_bribes_total_value: f64,
    pub inspectorate_pip_fleet_range_km: f64,
}

/// A minister row for the Government tab.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MinisterRow {
    pub ministry_name: String,
    pub minister_name: String,
    pub party: String,
    pub ideology: String,
    /// Ideology Step 6: Minister's party ideological coordinates (Directive 17).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinates: Option<IdeologyCoordinatesDto>,
    pub allocated_cash: f64,
    pub spent_cash: f64,
    /// Phase 35: Current cash pocket available for spending.
    pub ministry_cash: f64,
}

/// State of Emergency snapshot.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct EmergencySnapshot {
    pub active: bool,
    pub reason: String,
    pub turns_remaining: u32,
    pub parliament_suspended: bool,
}

/// Parliament tab data.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ParliamentSnapshot {
    pub chambers: Vec<ChamberSnapshot>,
    pub clubs: Vec<ClubRow>,
    pub recent_votes: Vec<VoteRow>,
    pub legislative_queue: Vec<QueueRow>,
    pub suspended: bool,
    /// Phase 34: Named VIPs (Head of State, PM, Ministers, Speakers).
    /// Phase 42: Kept for backwards compat but always empty — VIPs are in GovernmentSnapshot.
    pub vips: Vec<VipRow>,
    /// Phase 42: Committees with chairs and member counts.
    pub committees: Vec<CommitteeRow>,
    /// v4.20: Regime-aware display name for the legislature ("Parliament",
    /// "Royal Council", "National Salvation Committee",
    /// "Supreme Religious Council", "People's Assembly", ...).
    pub legislature_name: String,
    /// v4.20: Regime type key (Democracy, AbsoluteMonarchy,
    /// MilitaryDictatorship, Theocracy, OnePartyState, ...).
    pub regime_type: String,
}

/// Phase 42: A committee row for the Parliament tab.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CommitteeRow {
    pub name: String,
    pub committee_type: String,
    pub chair: String,
    pub chair_party: String,
    pub member_count: usize,
    pub bills_under_review: usize,
    pub partisan_bias: f64,
    /// v4.18: The bills actually sitting in this committee right now, with
    /// their pipeline stage/progress and the committee's recommendation.
    pub bills: Vec<CommitteeBillRow>,
    /// v4.22.1 WS2.5: Per-party seat composition of the committee membership.
    pub party_seats: Vec<(String, u32)>,
}

/// v4.18: A bill currently under committee review, for the Committees tab.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CommitteeBillRow {
    pub bill_id: String,
    pub title: String,
    /// Pipeline stage label (Committee, FloorVote, ...).
    pub stage: String,
    /// Turn the committee review is scheduled to complete.
    pub completion_turn: Option<u32>,
    /// The committee's recommendation modifier — the same deterministic
    /// coalition-share value the pipeline records on the bill at completion.
    pub recommendation: f64,
    /// Sponsor/proposer of the bill.
    pub initiator: String,
}

/// Phase 34: A named VIP row for the Parliament tab.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct VipRow {
    pub full_name: String,
    pub party: String,
    pub role: String,
    pub ideology: String,
    /// Ideology Step 6: VIP ideological coordinates for radar chart (Directive 17).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinates: Option<IdeologyCoordinatesDto>,
    pub age: u32,
}

/// A chamber snapshot for the Parliament tab.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ChamberSnapshot {
    pub name: String,
    pub total_seats: u32,
    pub speaker_name: String,
    pub speaker_club: String,
    pub seat_distribution: Vec<(String, u32)>,
}

/// A parliamentary club row.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ClubRow {
    pub name: String,
    pub seats: u32,
    pub ideology: String,
    /// Ideology Step 6: Club/party ideological coordinates for radar chart
    /// (Directive 17). Replaces the planned `PartyRow.coordinates` — `ClubRow`
    /// is the party representation in the UI snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coordinates: Option<IdeologyCoordinatesDto>,
    pub is_splinter: bool,
    pub discipline: f64,
    /// Phase 54: Chairperson VIP ID (if assigned).
    pub chairperson_id: Option<String>,
    /// Phase 54: Chairperson display name.
    pub chairperson_name: String,
    /// v4.20: Deterministic party visual identity — persistent hex color
    /// assigned at party genesis, shared across Hemicycle/Elections/Clubs.
    pub color_hex: String,
    /// v4.20: Ideology badge key (e.g. "conservative", "socialist") resolved
    /// to an icon by the frontend.
    pub ideology_icon: String,
    /// v4.21 WS1: Offices controlled by this club — chamber speakerships,
    /// committee chairmanships, and cabinet portfolios held by its party.
    pub offices: Vec<ClubOfficeRow>,
    /// v4.21 WS1: Bills this club sponsored/initiated (in-flight and
    /// completed), recovered from the live session and recorded votes.
    pub initiatives: Vec<ClubInitiativeRow>,
    /// v4.21 WS1: Recent chamber votes carrying this club's tally — the
    /// club's whip line plus the seats that broke it (discipline rebels).
    pub vote_history: Vec<ClubVoteHistoryRow>,
}

/// v4.21 WS1: One office held by a parliamentary club — a speakership,
/// committee chairmanship, or cabinet portfolio. Every field is resolved
/// from the presidium/committee/ministry records, never synthesized.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ClubOfficeRow {
    /// Office title: "Speaker", "Deputy Speaker", "Committee Chair",
    /// "Prime Minister", "Minister".
    pub office: String,
    /// Body the office belongs to — chamber name, committee name, or
    /// ministry name.
    pub body: String,
    /// Officeholder display name (VIP name, or the recorded role+party
    /// fallback when the seat is unnamed).
    pub holder: String,
}

/// v4.21 WS1: A bill the club sponsored — live-session bills plus completed
/// bills recovered from `VoteRecord.detail` (which outlives the bill's
/// removal from `LegislativeSession::active_bills`).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ClubInitiativeRow {
    pub bill_id: String,
    pub title: String,
    /// Current pipeline stage label or terminal stamp — "In Committee",
    /// "Executive Review", "Enacted", "Rejected by …".
    pub status: String,
    /// Turn the bill was introduced.
    pub introduced_turn: u32,
    /// Turn the law took effect; None while pending or rejected.
    pub enacted_turn: Option<u32>,
}

/// v4.21 WS1: One recorded chamber vote carrying this club's tally — the
/// club's whip line and the seats that broke it.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ClubVoteHistoryRow {
    pub bill_id: String,
    pub bill_title: String,
    /// Chamber display name that cast this vote (Sejm, Senate, ...).
    pub chamber_name: String,
    pub turn: u32,
    /// The club's official line on the bill: "For" for government-side
    /// clubs (ruling party + coalition, whipped yes by the vote model);
    /// the plurality direction of the club's own recorded tally otherwise.
    pub stance: String,
    pub votes_for: u32,
    pub votes_against: u32,
    pub abstentions: u32,
    /// Party-discipline rebels — seats in the club's tally cast against its
    /// line (the split the vote model records when a whipped club fails its
    /// discipline roll).
    pub rebels: u32,
    /// Whether the bill passed this chamber's vote.
    pub passed: bool,
}

/// A recent vote row.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct VoteRow {
    pub bill_id: String,
    pub bill_title: String,
    /// Chamber that recorded the vote: "lower" or "upper".
    pub chamber: String,
    /// Chamber display name (Sejm, Senate, ...).
    pub chamber_name: String,
    pub votes_for: u32,
    pub votes_against: u32,
    pub abstentions: u32,
    pub passed: bool,
    pub turn: u32,
    /// v4.18: Per-club For/Against/Abstain tallies recorded at vote time.
    pub club_breakdown: Vec<ClubVoteRow>,
    /// v4.18: Bill provisions/effects snapshot taken at vote time.
    pub detail: Option<BillDetailRow>,
}

/// v4.18: Per-club vote tally within one chamber vote.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ClubVoteRow {
    pub club_id: String,
    pub votes_for: u32,
    pub votes_against: u32,
    pub abstentions: u32,
}

/// v4.18: One provision inside a bill detail — typed effect data.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ProvisionRow {
    /// Clause description written at drafting time.
    pub description: String,
    /// `BillProvision` variant name (e.g. "TaxRateChange").
    pub kind: String,
    /// Concrete effect text formatted from the provision's real fields.
    pub detail: String,
    /// Clause budget impact (signed).
    pub budget_impact: f64,
    /// Sunset turn if the provision expires.
    pub sunset_turn: Option<u32>,
    /// Whether a committee chair mutated this clause.
    pub mutated: bool,
    /// Committee mutation notes.
    pub mutation_notes: Vec<String>,
}

/// v4.18: Bill detail payload for the expandable drawer — provisions,
/// sponsor, effective turn.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct BillDetailRow {
    /// Sponsor/proposer — the club/party that introduced the bill.
    pub initiator: String,
    /// BillType snake_case classification.
    pub bill_type: String,
    /// Legislative weight (Ordinary/Organic/Constitutional).
    pub weight: String,
    /// Stage label at capture time (or terminal outcome).
    pub stage: String,
    pub introduction_turn: u32,
    /// Turn the law took effect (enactment). None while pending.
    pub enacted_turn: Option<u32>,
    /// Earliest enactment turn given the current pipeline stage.
    pub earliest_enactment_turn: Option<u32>,
    /// Committee id while under review.
    pub committee: Option<String>,
    /// Typed provisions from the bill's core clauses.
    pub provisions: Vec<ProvisionRow>,
    /// v4.20: Terminal/procedural badge — "Passed", "Rejected by Upper
    /// House", "Senate Veto Overridden", "Presidential Veto", etc.
    pub status_badge: String,
    /// v4.20: Thematic tag for color-coding — "Budget", "Justice", "Labor",
    /// "Infrastructure", "Education", "Defense", "Other".
    pub theme_tag: String,
}

/// A legislative queue row.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct QueueRow {
    pub bill_id: String,
    pub bill_title: String,
    pub stage: String,
    pub initiator: String,
    /// Committee id while under review.
    pub committee: Option<String>,
    /// v4.18: Live bill detail for the expandable drawer.
    pub detail: Option<BillDetailRow>,
}

// ============================================================================
// PHASE 49: VIEW QUERY (PAGINATION / FILTERING)
// ============================================================================

/// Pagination request for a scrollable list view.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct PageQuery {
    /// Index of the first item to include (0-based).
    pub offset: usize,
    /// Maximum number of items to include in the page.
    pub limit: usize,
}

impl PageQuery {
    /// Create a page query with the given offset and a default limit of 30.
    pub fn new(offset: usize) -> Self {
        Self { offset, limit: 30 }
    }

    /// Create a page query with a custom limit.
    pub fn with_limit(offset: usize, limit: usize) -> Self {
        Self {
            offset,
            limit: limit.max(1),
        }
    }

    /// Apply this page to a slice: skip `offset`, take `limit`.
    pub fn apply<'a, T>(&self, items: &'a [T]) -> &'a [T] {
        let start = self.offset.min(items.len());
        let end = (start + self.limit.max(1)).min(items.len());
        &items[start..end]
    }
}

/// Filter for the VIP explorer list.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct VipFilter {
    /// If true, include dead VIPs (marked with †).
    pub show_dead: bool,
    /// If non-empty, only include VIPs whose name contains this substring (case-insensitive).
    pub search: String,
    /// Phase 54: If non-empty, only include VIPs with a role matching this label.
    pub role_filter: String,
}

/// Filter for the Company explorer list.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CompanyFilter {
    /// If non-empty, only include companies whose name contains this substring (case-insensitive).
    pub search: String,
    /// If non-empty, only include companies in this sector (display name).
    pub sector_filter: String,
    /// Phase 54: If non-empty, only include companies in this region (region ID).
    pub region_filter: String,
}

/// View parameters passed from the TUI `App` to the snapshot builder.
/// Controls pagination and filtering so the snapshot only contains the
/// visible page of data, not the entire registry.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ViewQuery {
    /// VIP explorer pagination.
    pub vip_page: PageQuery,
    /// VIP explorer filter.
    pub vip_filter: VipFilter,
    /// If set, build a full dossier for this VIP ID.
    pub vip_dossier_id: Option<String>,
    /// Company explorer pagination.
    pub company_page: PageQuery,
    /// Company explorer filter.
    pub company_filter: CompanyFilter,
    /// If set, build a full detail for the company with this ID.
    pub company_detail_id: Option<String>,
    /// Banking tab bank list pagination.
    pub bank_page: PageQuery,
    /// If set, build a full region drill-down detail for this region ID.
    pub region_drilldown_id: Option<String>,
    /// Phase 53: If set, build a full megaregion drill-down detail for this megaregion ID.
    pub megaregion_drilldown_id: Option<String>,
}

// ============================================================================
// PHASE 49: VIP DOSSIER ROW
// ============================================================================

/// A row in the VIP explorer list (compact form for table display).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct VipDossierRow {
    pub id: String,
    pub full_name: String,
    pub roles: String,
    pub age: u32,
    pub health: f64,
    pub faction: String,
    pub influence: u32,
    pub is_dead: bool,
    pub main_trait: String,
    pub ideology: String,
    /// Phase 54: Company name if this VIP is a CEO (for tooltip display).
    pub company_name: Option<String>,
    /// Phase 92: Gender for visual indicators.
    pub gender: String,
    /// Phase 92: Portrait seed for deterministic avatar generation.
    pub portrait_seed: String,
}

/// A full VIP dossier for the detail view.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct VipDossier {
    pub id: String,
    pub full_name: String,
    pub gender: String,
    pub age: u32,
    pub health: f64,
    pub incapacity: String,
    pub traits: Vec<String>,
    pub main_trait: String,
    pub ideology: String,
    pub religion: String,
    pub nationality: String,
    pub dynasty: Option<String>,
    pub roles: Vec<String>,
    pub base_influence: u32,
    pub faction: String,
    pub born_turn: u32,
    pub is_dead: bool,
    pub death_turn: Option<u32>,
    pub death_cause: Option<String>,
    pub acting_replacement_id: Option<String>,
}

// ============================================================================
// PHASE 49: BANK ROW
// ============================================================================

/// A row in the Banking tab's commercial bank list.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct BankRow {
    pub id: String,
    pub name: String,
    pub bank_type: String,
    pub reserves: f64,
    pub deposits: f64,
    pub loans: f64,
    pub securities: f64,
    pub is_dspw: bool,
    pub ldr: f64,
}

/// v4.19 WS2: Full bank detail for the expandable row panel — balance-sheet
/// breakdown, regulatory ratios, and a per-turn income statement derived
/// from the live ledger (interest run-rates = outstanding balances ×
/// contract/standing-facility rates; no synthetic accrual figures).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct BankDetail {
    pub id: String,
    pub name: String,
    pub bank_type: String,
    pub is_dspw: bool,
    // Assets
    pub reserves_at_central_bank: f64,
    pub cb_deposit_facility_balance: f64,
    pub interbank_loans_given: f64,
    pub corporate_loans: f64,
    pub consumer_loans: f64,
    pub securities: f64,
    pub real_estate: f64,
    pub mbs_holdings_value: f64,
    // Liabilities
    pub deposits: f64,
    pub cb_lombard_loans: f64,
    pub interbank_loans_taken: f64,
    pub issued_bonds: f64,
    // Equity + totals
    pub tier_1_capital: f64,
    pub total_assets: f64,
    pub total_liabilities: f64,
    // Regulatory ratios
    pub ldr: f64,
    /// Capital adequacy: tier_1_capital / total_assets.
    pub car: f64,
    /// reserves_at_central_bank / deposits (1.0 when depositless).
    pub reserve_ratio: f64,
    // Income statement (current-turn run-rate)
    pub interest_income: f64,
    pub interest_expense: f64,
    pub net_interest_margin: f64,
    /// Outstanding principal on loans past Current status.
    pub non_performing_loans: f64,
    pub operating_wages: f64,
    pub net_profit: f64,
}

// ============================================================================
// PHASE 49: COMPANY ROW + DETAIL
// ============================================================================

/// A row in the Company explorer list (compact form for table display).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CompanyRow {
    pub id: String,
    pub name: String,
    pub sector: String,
    pub region: String,
    pub fulfilled_fte: u32,
    pub average_wage: f64,
    pub seasonal_state: String,
    pub wage_arrears: f64,
}

/// A full company detail for the detail view.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CompanyDetail {
    pub id: String,
    pub name: String,
    pub sector: String,
    pub region: String,
    pub legal_form: String,
    pub ceo_vip_id: Option<String>,
    /// Phase 54: Resolved CEO display name from the VIP registry.
    pub ceo_name: Option<String>,
    /// Phase 54: Resolved CEO ideology from the VIP registry.
    pub ceo_ideology: Option<String>,
    pub union_id: Option<String>,
    pub fulfilled_fte: u32,
    pub fte_demand: u32,
    pub average_wage: f64,
    pub seasonal_state: String,
    pub furloughed_workers_count: f64,
    pub wage_arrears: f64,
    pub building_count: usize,
    pub available_cash: f64,
    /// Phase 87+: Financial summary (last turn, quarter, year).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub financial_summary: Option<CompanyFinancialSummary>,
}

/// Phase 87+: A single financial record (income, expenses, net profit).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CompanyFinancialRecord {
    pub income: f64,
    pub expenses: f64,
    pub net_profit: f64,
    /// Phase 90: Wage expense (paid + accrued arrears) for transparency.
    /// This is a subset of `expenses`, not an additional deduction.
    pub wage_expense: f64,
    /// Phase 91: Wage arrears (unpaid portion of payroll obligation).
    /// Accumulates when company cannot afford full payroll.
    pub wage_arrears: f64,
}

/// Phase 87+: Financial summary aggregating last turn, quarter (3 turns), and year (24 turns).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CompanyFinancialSummary {
    pub last_turn: CompanyFinancialRecord,
    pub last_quarter: CompanyFinancialRecord,
    pub last_year: CompanyFinancialRecord,
}

// ============================================================================
// PHASE 55: GOVERNANCE & OWNERSHIP DTOs
// ============================================================================

/// Phase 55: Board member row for governance display.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct BoardMemberRow {
    pub vip_id: String,
    pub name: String,
    pub role: String,
    pub loyalty_to_ceo: f64,
    pub appointed_turn: u32,
    pub age: u32,
    pub main_trait: String,
}

/// Phase 55: Governance detail for a company (board, succession, financial metrics).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct GovernanceDetail {
    pub company_id: String,
    pub company_name: String,
    pub is_listed: bool,
    pub shares_count: u64,
    pub share_price: f64,
    pub market_cap: f64,
    pub eps: f64,
    pub pe_ratio: f64,
    pub dividend_yield: f64,
    pub open_price: f64,
    pub close_price: f64,
    pub board_members: Vec<BoardMemberRow>,
    pub has_board: bool,
    pub board_independence: f64,
    pub is_family_business: bool,
    pub successor_generation: u32,
    pub heir_count: u32,
    pub succession_crisis: bool,
    pub heirs: Vec<BoardMemberRow>,
}

// ============================================================================
// PHASE 56: STOCK EXCHANGE DTOs
// ============================================================================

/// Phase 56: Market index snapshot for the stock exchange UI.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MarketIndexSnapshot {
    pub value: f64,
    pub change_pct: f64,
    pub history: Vec<f64>,
    pub total_market_cap: f64,
    pub total_volume: u64,
    pub advancing: u32,
    pub declining: u32,
    pub volatility: f64,
}

/// Phase 56: Sector index snapshot.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct SectorIndexSnapshot {
    pub sector: String,
    pub value: f64,
    pub history: Vec<f64>,
}

/// Phase 56: Listed company row for the stock exchange table.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ListedCompanyRow {
    pub company_id: String,
    pub name: String,
    pub sector: String,
    pub share_price: f64,
    pub change_pct: f64,
    pub market_cap: f64,
    pub pe_ratio: f64,
    pub dividend_yield: f64,
    pub volume: u64,
    pub open_price: f64,
    pub close_price: f64,
    pub spread: f64,
    pub ceo_name: Option<String>,
    pub ceo_vip_id: Option<String>,
}

/// Phase 56: Recent trade row for the stock exchange UI.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct TradeRow {
    pub instrument_id: String,
    pub buyer_id: String,
    pub seller_id: String,
    pub quantity: u64,
    pub price: f64,
    pub turn: u32,
}

/// Phase 56: Commodity spot price row.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CommoditySpotRow {
    pub commodity_id: String,
    pub spot_price: f64,
    pub open_interest: u64,
    pub history: Vec<f64>,
}

/// Phase 56: Commodity futures row.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CommodityFuturesRow {
    pub commodity_id: String,
    pub delivery_turn: u32,
    pub futures_price: f64,
    pub spot_price: f64,
    pub basis: f64,
}

/// Phase 56: Full stock exchange response for the securities UI.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct StockExchangeResponse {
    pub main_index: MarketIndexSnapshot,
    pub sector_indices: Vec<SectorIndexSnapshot>,
    pub listed_companies: Vec<ListedCompanyRow>,
    pub recent_trades: Vec<TradeRow>,
    pub trading_halted: bool,
    pub commodity_spot: Vec<CommoditySpotRow>,
    pub commodity_futures: Vec<CommodityFuturesRow>,
}

/// Phase 56: Paginated listed company page response.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ListedCompanyPageResponse {
    pub rows: Vec<ListedCompanyRow>,
    pub total_count: usize,
}

/// Phase 56: Detailed market view for a single listed company.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ListedCompanyDetail {
    pub company_id: String,
    pub name: String,
    pub sector: String,
    pub share_price: f64,
    pub market_cap: f64,
    pub pe_ratio: f64,
    pub dividend_yield: f64,
    pub eps: f64,
    pub open_price: f64,
    pub close_price: f64,
    pub spread: f64,
    pub volume: u64,
    pub shares_count: u64,
    pub free_float: f64,
    pub recent_trades: Vec<TradeRow>,
}

// ============================================================================
// PHASE 57: FUND & KNF DTOs
// ============================================================================

/// Phase 57: Fund row for the investment funds UI.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct FundRow {
    pub fund_id: String,
    pub name: String,
    pub fund_type: String,
    /// v4.18 WS4: mandate risk profile label (Aggressive / Balanced /
    /// Conservative / Index).
    pub risk_profile: String,
    pub nav_per_share: f64,
    pub total_aum: f64,
    pub manager_name: String,
    pub manager_vip_id: String,
    pub manager_trait: String,
    pub shares_outstanding: u64,
    pub ytd_return: f64,
    /// Top holdings as (display_name, market_value) — resolved instrument
    /// names, never raw `EQUITY:`/`BOND:` ids.
    pub top_holdings: Vec<(String, f64)>,
}

/// v4.18 WS4: One resolved holding row on the fund detail view — the
/// engine's instrument id plus its human-readable display name and the
/// exchange listing code it trades under.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct FundHoldingRow {
    /// Raw instrument/security id (`EQUITY:<id>`, `BOND:<id>`, `mbs:<id>:<p>`,
    /// or a treasury security id for ledger bond holdings).
    pub instrument_id: String,
    /// Resolved display name (e.g. "Acme Steel (Shares)", "Nova Bank
    /// Covered Bond #2", "Treasury Bond TBOND-2024-003").
    pub display_name: String,
    /// Exchange listing code — the id the order book trades under (the
    /// exchange has no separate ticker registry).
    pub ticker: Option<String>,
    /// Units held (shares / tranche units / face-value lots).
    pub quantity: u64,
    /// Quantity-weighted average cost basis per unit.
    pub avg_cost: f64,
    /// Current mark-to-market value of the position.
    pub market_value: f64,
    /// Position weight as a fraction of fund AUM (0–1).
    pub weight_pct: f64,
}

/// Phase 57: Detailed fund view for the fund detail panel.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct FundDetail {
    pub fund_id: String,
    pub name: String,
    pub fund_type: String,
    /// v4.18 WS4: mandate risk profile label.
    pub risk_profile: String,
    pub nav_per_share: f64,
    pub total_aum: f64,
    pub manager_name: String,
    pub manager_vip_id: String,
    pub manager_trait: String,
    pub shares_outstanding: u64,
    pub ytd_return: f64,
    pub leverage_ratio: f64,
    pub management_fee: f64,
    pub performance_fee: f64,
    /// Allocation pie data as (display_name, market_value).
    pub top_holdings: Vec<(String, f64)>,
    /// Resolved portfolio rows with instrument names.
    pub portfolio_holdings: Vec<FundHoldingRow>,
}

/// Phase 57: KNF audit finding row for the regulatory UI.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct KnfFindingRow {
    pub entity_id: String,
    pub entity_name: String,
    pub violation_type: String,
    pub severity: u8,
    pub turn: u32,
    pub description: String,
    pub penalty: f64,
}

/// Phase 57: Capital gains tax summary row.
/// v4.18 WS4: `entity_name` is the resolved human-readable label (company
/// name, `"Class — Region (rural|urban)"` for household investors, VIP
/// name), `entity_id` keeps the raw registry key.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CapitalGainsTaxRow {
    pub entity_id: String,
    pub entity_name: String,
    pub realized_gains: f64,
    pub realized_losses: f64,
    /// Settled rows: tax actually paid. Pending rows: accrued estimate.
    pub tax_owed: f64,
    pub carried_forward_losses: f64,
}

/// Phase 57: Capital gains tax summary response.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CapitalGainsTaxSummary {
    /// Settled per-entity rows for the most recent completed fiscal year —
    /// `Σ rows.tax_owed == total_tax_collected`.
    pub rows: Vec<CapitalGainsTaxRow>,
    /// Current fiscal year's unsettled accruals (estimated tax at the
    /// configured rate — not yet collected).
    pub pending_rows: Vec<CapitalGainsTaxRow>,
    /// Cash actually collected in the last fiscal-year settlement.
    pub total_tax_collected: f64,
    pub annual_tax_history: Vec<f64>,
    /// Fiscal year of each `annual_tax_history` entry (X axis).
    pub annual_tax_history_years: Vec<u32>,
    /// The fiscal year `rows` covers.
    pub last_settled_year: Option<u32>,
    /// The securities CGT rate applied (for the "estimated" pending column).
    pub securities_cgt_rate: f64,
}

// ============================================================================
// PHASE 49: REGION DETAIL (DRILL-DOWN)
// ============================================================================

/// Phase 82: Thermal grid snapshot for UI display.
///
/// Role-gated: only visible to regional/national government roles.
/// Foreign observers do not see detailed pipe infrastructure data.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ThermalGridSnapshot {
    /// Total pipe network length (km).
    pub pipe_network_km: f64,
    /// Pipe condition (0.0 = collapsed, 1.0 = pristine).
    pub pipe_condition: f64,
    /// Transmission loss per km (fraction/km).
    pub loss_per_km: f64,
    /// Current effective heat supply after losses (GJ).
    pub effective_heat_supply_gj: f64,
    /// Maximum connectable buildings.
    pub max_connectable_buildings: usize,
    /// Number of active heating plants.
    pub active_heating_plants: usize,
    /// Average delivery distance (km).
    pub average_delivery_distance_km: f64,
    /// Transmission loss fraction (0.0 = no loss, 1.0 = total loss).
    pub transmission_loss_fraction: f64,
}

/// Phase 82: Smog/air pollution snapshot for UI display.
///
/// Public information — air quality is visible to all roles.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct SmogSnapshot {
    /// Accumulated smog concentration (0.0 = clean, 100.0 = lethal).
    pub smog_level: f64,
    /// Per-turn emissions from standalone heating (mass units).
    pub standalone_emissions: f64,
    /// Per-turn emissions from centralized plants (mass units).
    pub centralized_emissions: f64,
    /// Per-turn emissions from industrial production (mass units).
    pub industrial_emissions: f64,
    /// Smog mortality multiplier (1.0 = normal, 1.5 = 50% increase).
    pub mortality_multiplier: f64,
}

// ============================================================================
// PHASE 83: SANITATION EPIC SNAPSHOTS (Rule 17 — Full-Stack Accountability)
// ============================================================================

/// Phase 83: Water reserve snapshot for UI display.
///
/// Role-gating (Rule 11):
/// - Foreign observers: do not see this (resource data is classified)
/// - Domestic non-municipal: see surface water quality only
/// - Municipal/government: see full snapshot including groundwater volumes
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct WaterReserveSnapshot {
    /// Groundwater volume (liters).
    pub groundwater_volume: f64,
    /// Groundwater quality (0.0-1.0).
    pub groundwater_quality: f64,
    /// Surface water volume (liters).
    pub surface_water_volume: f64,
    /// Surface water quality (0.0-1.0).
    pub surface_water_quality: f64,
}

/// Phase 83: Water grid snapshot for UI display.
///
/// Role-gating (Rule 11):
/// - Foreign observers: do not see this
/// - Domestic non-municipal: see current_quality only
/// - Municipal/government: see full snapshot including pipe infrastructure
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct WaterGridSnapshot {
    /// Total pipe network length (km).
    pub pipe_network_km: f64,
    /// Pipe condition (0.0 = collapsed, 1.0 = pristine).
    pub pipe_condition: f64,
    /// PARADIGM SHIFT: Current water quality in the grid (0.0-1.0).
    pub current_quality: f64,
    /// Current throughput (liters/turn).
    pub throughput_liters: f64,
    /// Effective water delivered after transmission losses (liters).
    pub effective_water_delivered: f64,
    /// Maximum connectable buildings.
    pub max_connectable_buildings: usize,
    /// Number of active water treatment plants.
    pub active_water_plants: usize,
    /// Transmission loss fraction (0.0 = no loss, 1.0 = total loss).
    pub transmission_loss_fraction: f64,
}

/// Phase 83: Sewer grid snapshot for UI display.
///
/// Role-gating (Rule 11):
/// - Foreign observers: do not see this
/// - Domestic non-municipal: see current_quality only
/// - Municipal/government: see full snapshot including pipe infrastructure
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct SewerGridSnapshot {
    /// Total pipe network length (km).
    pub pipe_network_km: f64,
    /// Pipe condition (0.0 = collapsed, 1.0 = pristine).
    pub pipe_condition: f64,
    /// Current sewage quality (typically ~0.05 = blackwater).
    pub current_quality: f64,
    /// Current throughput (liters/turn).
    pub throughput_liters: f64,
    /// Water delivered to wastewater treatment plants (liters).
    pub water_delivered_to_treatment: f64,
    /// Leaked water mass (liters lost through cracked pipes).
    pub leaked_water_mass: f64,
    /// Maximum connectable buildings.
    pub max_connectable_buildings: usize,
    /// Number of active wastewater treatment plants.
    pub active_wastewater_plants: usize,
}

/// Phase 84: Waste grid snapshot for UI display.
///
/// Role-gating (Rule 11):
/// - Foreign observers: see only collection coverage and pollution levels
/// - Domestic non-municipal: see collection coverage and landfill utilization
/// - Municipal/government: see full snapshot including route infrastructure,
///   uncollected waste by category, and separation efficiency
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct WasteGridSnapshot {
    /// Collection route network length (km).
    pub collection_route_km: f64,
    /// Route condition (0.0 = collapsed, 1.0 = pristine).
    pub route_condition: f64,
    /// Per-turn collection capacity (tons).
    pub collection_capacity: f64,
    /// Total uncollected waste mass (tons).
    pub total_uncollected: f64,
    /// Active collection method separation efficiency (0.0-1.0).
    pub separation_efficiency: f64,
    /// Landfill capacity utilization (0.0-1.0).
    pub landfill_utilization: f64,
    /// Methane capture rate (0.0-1.0).
    pub methane_capture_rate: f64,
}

/// Phase 84: Landfill snapshot for UI display.
///
/// Role-gating (Rule 11):
/// - Foreign observers: see only utilization and is_full status
/// - Municipal/government: see full snapshot including liner integrity,
///   leachate capture, gas capture, and stored waste by category
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct LandfillSnapshot {
    /// Total capacity (tons).
    pub total_capacity: f64,
    /// Remaining capacity (tons).
    pub remaining_capacity: f64,
    /// Total stored waste mass (tons).
    pub total_stored: f64,
    /// Liner integrity (0.0 = breached, 1.0 = intact).
    pub liner_integrity: f64,
    /// Leachate collection efficiency (0.0-1.0).
    pub leachate_capture: f64,
    /// Gas capture efficiency (0.0-1.0).
    pub gas_capture: f64,
    /// LOGISTICAL BOUND 2: True when landfill is full and rejecting waste.
    pub is_full: bool,
    /// Utilization fraction (0.0-1.0).
    pub utilization: f64,
}

/// Phase 84: Waste pollution snapshot for UI display.
///
/// Role-gating (Rule 11): Pollution levels are public information.
/// Source breakdown (burning vs dumping vs uncollected) is municipal only.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct WastePollutionSnapshot {
    /// Smog mass from trash burning (public).
    pub burning_emissions: f64,
    /// Biohazard mass from illegal dumping (public).
    pub dumping_biohazard: f64,
    /// Biohazard mass from uncollected waste (public).
    pub uncollected_biohazard: f64,
    /// Leachate leakage mass (municipal only).
    pub leachate_leakage: f64,
}

/// Phase 84: Recycling snapshot for UI display.
///
/// Role-gating (Rule 11):
/// - Public: see recovered commodity totals only
/// - Municipal/government: see full plant inventory and yields
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct RecyclingSnapshot {
    /// Number of active recycling facilities.
    pub active_recycling_plants: usize,
    /// Number of active separation plants.
    pub active_separation_plants: usize,
    /// Number of active WtE plants.
    pub active_wte_plants: usize,
    /// Total waste recycled this turn (tons).
    pub total_recycled: f64,
    /// Total waste incinerated this turn (tons).
    pub total_incinerated: f64,
    /// Ash generated this turn (tons) — CRITICAL FIX 2.
    pub ash_generated: f64,
}

/// Phase 83: Biohazard snapshot for UI display.
///
/// Role-gating (Rule 11):
/// - Foreign observers: see only biohazard_level (public health data)
/// - Domestic non-municipal: see biohazard_level and mortality_multiplier
/// - Municipal/government: see full snapshot including source breakdown
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct BiohazardSnapshot {
    /// Accumulated biological contamination (0.0 = clean, 100.0 = epidemic).
    pub biohazard_level: f64,
    /// Per-turn biohazard from standalone sanitation (open defecation, etc.).
    pub standalone_biohazard: f64,
    /// Per-turn biohazard from sewage overflow/leakage.
    pub sewage_overflow_biohazard: f64,
    /// Per-turn biohazard from industrial wastewater.
    pub industrial_biohazard: f64,
    /// PARADIGM SHIFT: Per-turn biohazard from low-quality water consumption.
    pub low_quality_water_biohazard: f64,
    /// Biohazard mortality multiplier (1.0 = normal, 2.0 = epidemic).
    pub mortality_multiplier: f64,
}

/// Full region drill-down data for the Regions tab modal.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct RegionDetail {
    pub region_id: String,
    pub display_name: String,
    pub development_level: f64,
    pub admin_status: String,
    pub head_name: String,
    pub head_type: String,
    /// Phase 54: VIP ID of the regional head (for hover cards).
    pub head_vip_id: Option<String>,
    pub council_factions: Vec<(String, u32)>,
    /// Phase 54: Total council seats/mandates (sum of all factions).
    pub total_council_seats: u32,
    pub budget_reserves: f64,
    pub budget_tax_revenue: f64,
    pub budget_property_tax: f64,
    pub budget_expenditures: f64,
    pub budget_balance: f64,
    pub debt_total: f64,
    pub debt_to_revenue_ratio: f64,
    pub credit_rating: String,
    pub active_mandates: Vec<MandateSummary>,
    /// v4.20 WS4: Regional council legislative record — resolutions debated
    /// and voted by the council each turn (oldest → newest).
    #[serde(default)]
    pub council_resolutions: Vec<CouncilResolutionRow>,
    /// v4.20 WS4: Per-councilor roll-call records for the resolutions above.
    #[serde(default)]
    pub council_roll_calls: Vec<CouncilRollCallRow>,
    /// v4.21 WS3: Sitting councilors for the expandable faction summary —
    /// includes members with no recorded roll-call votes.
    #[serde(default)]
    pub council_members: Vec<crate::ui::snapshot_regions::CouncilorRow>,
    pub infrastructure_avg_condition: f64,
    pub sector_employment: Vec<(String, f64)>,
    pub durable_cohorts: Vec<DurableCohortSummary>,
    /// Phase 82: Thermal grid snapshot (role-gated — only visible to
    /// regional/national government roles, not to foreign observers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thermal_grid: Option<ThermalGridSnapshot>,
    /// Phase 82: Smog/pollution snapshot (visible to all roles — air quality
    /// is public information).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smog: Option<SmogSnapshot>,
    /// Phase 83: Water reserve snapshot (role-gated — municipal/government only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub water_reserves: Option<WaterReserveSnapshot>,
    /// Phase 83: Water grid snapshot (role-gated — municipal/government only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub water_grid: Option<WaterGridSnapshot>,
    /// Phase 83: Sewer grid snapshot (role-gated — municipal/government only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sewer_grid: Option<SewerGridSnapshot>,
    /// Phase 83: Biohazard snapshot (role-gated — biohazard_level is public,
    /// source breakdown is municipal/government only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub biohazard: Option<BiohazardSnapshot>,
    /// Phase 84: Waste grid snapshot (role-gated — municipal/government only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waste_grid: Option<WasteGridSnapshot>,
    /// Phase 84: Landfill snapshot (role-gated — municipal/government only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub landfill: Option<LandfillSnapshot>,
    /// Phase 84: Waste pollution snapshot (public — pollution is public info).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waste_pollution: Option<WastePollutionSnapshot>,
    /// Phase 84: Recycling snapshot (role-gated — municipal/government only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recycling: Option<RecyclingSnapshot>,
    /// Phase 87+: Climate profile (public — geographic information).
    pub climate_profile: String,
    /// Phase 87+: Arable land max (public — geographic information).
    pub arable_land_max: i64,
    /// Phase 87+: Arable land used (public — geographic information).
    pub arable_land_used: i64,
    /// Phase 87+: Geological deposits (role-gated — undiscovered deposits
    /// filtered out for foreign observers per Directive 11).
    pub geological_deposits: Vec<GeologicalDepositRow>,
}

/// Phase 87+: Geological deposit row for the Land Resources tab.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct GeologicalDepositRow {
    pub commodity: String,
    pub formation_name: String,
    pub estimated_reserves: f64,
    pub current_reserves: f64,
    pub extraction_rate: f64,
    pub utilization_rate: f64,
    pub active_mine_count: u32,
    pub discovered: bool,
}

/// Summary of an unfunded mandate for the region drill-down.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MandateSummary {
    pub description: String,
    pub required_spending: f64,
    pub central_funding: f64,
    pub funding_gap: f64,
    pub status: String,
    /// Region this mandate targets (empty = legacy national record).
    pub region_id: String,
    /// Imposing bill's id (provenance; empty for legacy).
    pub source_bill_id: String,
    /// Turn the mandate was imposed.
    pub imposed_turn: u32,
    /// v4.21 WS3: What the mandate physically covers (provision-derived).
    #[serde(default)]
    pub scope: String,
    /// v4.21 WS3: The real-world effect the mandated spending produces.
    #[serde(default)]
    pub effects: String,
    /// v4.21 WS3: Cash disbursed to the service provider so far.
    #[serde(default)]
    pub disbursed: f64,
    /// v4.21 WS3: Portion paid by the central-grant leg (Treasury→JST→pool).
    #[serde(default)]
    pub central_disbursed: f64,
    /// v4.21 WS3: Portion paid by the regional-treasury leg (JST→pool).
    #[serde(default)]
    pub regional_disbursed: f64,
    /// v4.21 WS3: "Central Grant" | "Regional Treasury" | combined —
    /// explicit funding source derived from the mandate's cash legs.
    #[serde(default)]
    pub funding_source: String,
    /// v4.21 WS3: `disbursed / required` progress fraction (0–1).
    #[serde(default)]
    pub progress: f64,
    /// v4.21 WS3: Nominal per-turn tranche (`required / MANDATE_DISBURSE_TURNS`).
    #[serde(default)]
    pub per_turn_disbursement: f64,
}

/// v4.20 WS4: A resolution debated and voted by a regional council —
/// mirrors `CouncilResolution` on `politics::local_council::LocalCouncil`
/// and the `CouncilResolutionRow` api.ts contract.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CouncilResolutionRow {
    pub title: String,
    pub resolution_type: String,
    pub status: String,
    pub votes_for: u32,
    pub votes_against: u32,
    pub abstentions: u32,
    pub turn: u32,
    /// v4.23 WS7: Region whose council held the vote — populated for
    /// megaregion assembly aggregation (member-region unions share titles);
    /// empty for single-region rows. serde(default) tolerates old payloads.
    #[serde(default)]
    pub region_id: String,
    /// v4.23 WS7: What the ordinance actually does — derived from the
    /// resolution type and, for adopted spending resolutions, the linked
    /// execution mandate's real appropriation/scope/effects.
    #[serde(default)]
    pub description: String,
}

/// v4.23 WS7: Human-readable description of what a council resolution
/// actually does, derived from its real type plus the linked execution
/// mandate (adopted spending resolutions impose an `UnfundedMandate` whose
/// `source_bill_id` is `COUNCIL-RES-{region_id}-{mandate_key}` — real
/// appropriation amount, scope and effects). Rejected resolutions describe
/// the proposal that was declined. Deterministic; never fabricated.
fn council_resolution_description(
    resolution_type: &str,
    status: &str,
    region_id: &str,
    mandates: &[crate::politics::local_legislation::UnfundedMandate],
) -> String {
    // Agenda `mandate_key` per resolution type — must mirror the keys used
    // by `politics::local_legislation::process_regional_council_sessions`.
    let mandate_key = match resolution_type {
        "infrastructure" => "infra",
        other => other,
    };
    let source_id = format!("COUNCIL-RES-{region_id}-{mandate_key}");
    let mandate = mandates.iter().find(|m| m.source_bill_id == source_id);

    let base = match resolution_type {
        "infrastructure" => {
            "Appropriation ordinance funding regional roads, water, and \
             public-works upgrades toward the 60-point JST service standard."
        }
        "sanitation" => {
            "Emergency appropriation for waste-collection routes, landfill \
             capacity, and sewerage to clear the sanitation backlog."
        }
        "budget" => {
            "Austerity ordinance — on adoption the council cuts local \
             expenditures by 5% immediately to stabilize the budget."
        }
        _ => "Ordinance debated and voted by the regional council.",
    };

    let mut desc = base.to_string();
    match (status, mandate) {
        ("adopted", Some(m)) => {
            desc.push_str(&format!(
                " Appropriates {} — {} {}",
                crate::ui::snapshot_press::fmt_money(m.required_spending_per_region),
                m.scope,
                m.effects
            ));
        }
        ("adopted", None) => {
            if resolution_type != "budget" {
                desc.push_str(
                    " The execution mandate has completed or is being funded \
                     through the normal council-funding process.",
                );
            }
        }
        ("rejected", _) => {
            desc.push_str(" The council declined the proposal.");
        }
        _ => {}
    }
    desc
}

/// v4.20 WS4: Per-councilor vote record for a council resolution — mirrors
/// `CouncilRollCall` and the `CouncilRollCallRow` api.ts contract. `vip_id`
/// is an extra field beyond the contract (the frontend tolerates extras).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CouncilRollCallRow {
    pub resolution_title: String,
    pub member_name: String,
    pub faction: String,
    pub vote: String,
    pub turn: u32,
    /// Extra: councilor persistent id (VIP identifier where registered).
    pub vip_id: String,
}

/// Summary of a household durable cohort for the region drill-down.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct DurableCohortSummary {
    pub commodity: String,
    pub count: f64,
    pub avg_condition: f64,
    pub quality: f64,
    pub durability: f64,
}

/// Phase 53: Megaregion drill-down data for the Regions tab.
/// Analogous to `RegionDetail` but for the megaregion administrative layer.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MegaregionDetail {
    pub megaregion_id: String,
    pub display_name: String,
    pub country: String,
    pub member_region_ids: Vec<String>,
    pub member_region_count: usize,
    pub total_population: i64,
    pub total_gdp: f64,
    pub governor_name: String,
    pub governor_appointed: bool,
    pub competence_level: String,
    pub budget_reserves: f64,
    pub regional_transfers: f64,
    pub development_expenditures: f64,
    pub coordination_expenditures: f64,
    pub budget_balance: f64,
    /// v4.20 WS4: Council legislative record aggregated from member
    /// regions' councils — the megaregion has no council of its own
    /// (`MegaregionGovernance` is governor-led), so its assembly record is
    /// the union of member-region council resolutions (real data only;
    /// api.ts `MegaregionDetail` lacks these fields — extra JSON keys are
    /// tolerated).
    #[serde(default)]
    pub council_resolutions: Vec<CouncilResolutionRow>,
    /// v4.20 WS4: Roll calls of member-region councilors.
    #[serde(default)]
    pub council_roll_calls: Vec<CouncilRollCallRow>,
    /// v4.21 WS3: Assembly composition — faction seat totals aggregated
    /// from member-region councils (the megaregion has no council of its
    /// own; this is real governance state, not fabricated seats).
    #[serde(default)]
    pub council_factions: Vec<(String, u32)>,
    /// v4.21 WS3: Sum of `council_factions` seats.
    #[serde(default)]
    pub total_council_seats: u32,
    /// v4.21 WS3: Sitting councilors of member-region councils (assembly
    /// membership for the expandable faction summary).
    #[serde(default)]
    pub council_members: Vec<crate::ui::snapshot_regions::CouncilorRow>,
    /// v4.21 WS3: Sector employment aggregated over member regions
    /// (Σ building.current_employment per GDP sector).
    #[serde(default)]
    pub sector_employment: Vec<(String, f64)>,
    /// v4.21 WS3: Unfunded mandates targeting member regions.
    #[serde(default)]
    pub active_mandates: Vec<MandateSummary>,
}

// ============================================================================
// PHASE 49: ADVISORY COUNCIL + ROYAL DYNASTY SNAPSHOTS
// ============================================================================

/// Advisory council snapshot for the Parliament tab (autocracies).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct AdvisoryCouncilSnapshot {
    pub council_type: String,
    pub members: Vec<CouncilMemberRow>,
    pub aggregate_loyalty: f64,
    pub coup_risk_threshold: f64,
    pub coup_cooldown_until_turn: u32,
}

/// A single council member row.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CouncilMemberRow {
    pub vip_id: String,
    pub name: String,
    pub faction: String,
    pub loyalty: f64,
    pub influence: u32,
}

/// Royal dynasty snapshot for the Parliament tab (monarchies).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct RoyalDynastySnapshot {
    pub dynasty_name: String,
    pub current_monarch_id: Option<String>,
    pub current_monarch_name: String,
    pub current_regent_id: Option<String>,
    pub current_regent_name: String,
    pub regency_active: bool,
    pub members: Vec<DynastyMemberRow>,
    /// Phase 86: Genealogy event history.
    pub marriage_history: Vec<MarriageEventRow>,
    pub birth_history: Vec<BirthEventRow>,
}

/// A single dynasty member row.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct DynastyMemberRow {
    pub vip_id: String,
    pub name: String,
    pub relation: String,
    pub age: u32,
    pub succession_order: u32,
    pub is_heir_apparent: bool,
    /// Phase 86: Genealogy links.
    pub father_vip_id: Option<String>,
    pub mother_vip_id: Option<String>,
    pub spouse_vip_id: Option<String>,
    pub children_vip_ids: Vec<String>,
    pub is_dead: bool,
    pub death_cause: Option<String>,
    /// Phase 92: Gender for visual indicators.
    pub gender: String,
    /// Phase 92: Portrait seed for deterministic avatar generation.
    pub portrait_seed: String,
}

/// Phase 86: A royal marriage event row for UI display.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MarriageEventRow {
    pub turn: u32,
    pub spouse1_name: String,
    pub spouse2_name: String,
    pub significance: String,
    pub foreign_dynasty: Option<String>,
}

/// Phase 86: A royal birth event row for UI display.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct BirthEventRow {
    pub turn: u32,
    pub child_name: String,
    pub father_name: String,
    pub mother_name: String,
    pub is_legitimate: bool,
}

// ============================================================================
// TAURI IPC RESPONSE WRAPPER TYPES
// ============================================================================

/// Response for paginated VIP queries.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct VipPageResponse {
    pub rows: Vec<VipDossierRow>,
    pub total_count: usize,
}

/// Response for paginated company queries.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CompanyPageResponse {
    pub rows: Vec<CompanyRow>,
    pub total_count: usize,
}

/// Response for paginated bank queries.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct BankPageResponse {
    pub rows: Vec<BankRow>,
    pub total_count: usize,
}

/// Response for the advance_turn command.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct TurnResult {
    pub turn: u32,
    pub year: u32,
    pub status: String,
}

/// Response for the get_game_status command.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct GameStatus {
    pub has_game: bool,
    pub turn: u32,
    pub year: u32,
    /// Emergency Stabilization: Current month (1-12).
    pub month: u32,
    /// Emergency Stabilization: Current season name (Winter/Spring/Summer/Autumn).
    pub season: String,
    pub processing: bool,
    pub countries: Vec<String>,
    /// v4.22 WS2.1: per-country regime legislature name (e.g. "Royal
    /// Council", "National Salvation Junta") for the sidebar tab label.
    #[serde(default)]
    pub legislature_names: std::collections::BTreeMap<String, String>,
}

/// Response for the get_parliament command.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ParliamentResponse {
    pub parliament: ParliamentSnapshot,
    pub advisory_council: Option<AdvisoryCouncilSnapshot>,
    pub royal_dynasty: Option<RoyalDynastySnapshot>,
    pub government_form: String,
}

// ============================================================================
// v4.19 WS4: CONSTITUTION & LAWS + ELECTIONS SNAPSHOTS
// ============================================================================

/// Constitutional parameters for the "Constitution & Laws" tab — every
/// field read from `politics.constitution` / `politics` (no projection).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ConstitutionSnapshot {
    pub exists: bool,
    pub regime_type: String,
    pub chambers: u32,
    pub upper_house_name: Option<String>,
    pub electoral_system: String,
    pub election_threshold: f64,
    /// Government-form term length in years (4/5 democratic, 999 non-elective).
    pub term_length_years: u32,
    pub presidential_veto: bool,
    pub constitutional_tribunal: String,
    pub suffrage_system: String,
    pub budget_failure_consequence: String,
    pub change_mechanism: String,
}

/// One active statute/policy configuration row.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ActiveLawRow {
    /// Policy domain (Labor, Health, Education, Justice, Migration, ...).
    pub domain: String,
    /// Display name of the law/policy.
    pub name: String,
    /// Current active parameters (e.g. the configured regime value).
    pub summary: String,
}

/// A bill that completed the pipeline and was enacted.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct EnactedStatuteRow {
    pub bill_id: String,
    pub title: String,
    pub initiator: String,
    pub bill_type: String,
    pub enacted_turn: Option<u32>,
    /// Provisions/effects recorded at vote time.
    pub provisions: Vec<String>,
}

/// Response for the get_constitution_laws command.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ConstitutionLawsResponse {
    pub constitution: ConstitutionSnapshot,
    /// Live policy configurations (what the laws currently DO).
    pub active_laws: Vec<ActiveLawRow>,
    /// Bills enacted this term — the parliamentary statute book.
    pub enacted_statutes: Vec<EnactedStatuteRow>,
}

/// A party's standing for the Elections tab.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ElectionPartyRow {
    pub party_id: String,
    pub ideology: String,
    /// Current support share (0-1).
    pub support: f64,
    /// Seats currently held in the lower chamber.
    pub seats: u32,
    pub is_coalition_member: bool,
    pub is_ruling: bool,
    /// v4.20: Deterministic party color, shared with ClubRow.color_hex.
    pub color_hex: String,
    /// v4.20: Ideology badge key resolved to an icon by the frontend.
    pub ideology_icon: String,
}

/// One executed campaign action.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CampaignActionRow {
    pub party_id: String,
    pub action: String,
    pub turn: u32,
    pub is_black_money: bool,
}

/// Response for the get_elections command.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ElectionsResponse {
    pub government_form: String,
    /// ElectionState label: Idle / PreCampaign / ActiveCampaign / ElectionDay.
    pub election_state: String,
    /// Years remaining on the current parliamentary term.
    pub years_to_elections: u32,
    /// Estimated turns until the next election (years_to_elections × 24).
    pub estimated_turns_remaining: u32,
    /// v4.20: LIVE countdown — decremented every turn by the engine until the
    /// election fires at 0. Replaces the static estimate for display.
    pub turns_remaining: u32,
    /// v4.20: Scheduled turn index on which the next election executes.
    pub scheduled_election_turn: u32,
    /// Calendar year of the next scheduled election.
    pub next_election_year: u32,
    pub head_of_state_name: String,
    pub head_of_state_title: String,
    pub campaign_duration_turns: u32,
    /// Party standings (support share + current seats) — the "polling" table.
    pub parties: Vec<ElectionPartyRow>,
    /// Campaign actions executed this cycle (real `campaign_executions`).
    pub campaign_actions: Vec<CampaignActionRow>,
    /// Current lower-chamber seat distribution (latest election result).
    pub current_seats: Vec<(String, u32)>,
    /// v4.22.1 WS2.3: Completed elections, latest last (results + rounds).
    pub election_history: Vec<ElectionRecordRow>,
    /// v4.22.1 WS2.3: Rolling 24-turn support history for the trend chart.
    pub support_history: Vec<SupportSnapshotRow>,
    /// v4.22.1 WS2.3: Party backing the current head of state (ruling party).
    pub head_of_state_party: String,
}

/// v4.22.1 WS2.3: One completed election for the historical sub-view.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ElectionRecordRow {
    pub turn: u32,
    pub year: u32,
    /// (party name, vote share, seats won).
    pub results: Vec<(String, f64, u32)>,
    /// (round, candidate, backing party, vote share, is_winner).
    pub presidential_rounds: Vec<(u32, String, String, f64, bool)>,
}

/// v4.22.1 WS2.3: One turn's party support readings.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct SupportSnapshotRow {
    pub turn: u32,
    /// (party name, support).
    pub support: Vec<(String, f64)>,
}

/// Response for the get_macro_indicators command.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MacroIndicatorsResponse {
    pub gdp: f64,
    pub gdp_per_capita: f64,
    pub population: u64,
    pub unemployment_rate: f64,
    pub inflation_rate: f64,
    pub average_wage: f64,
    pub money_supply_m0: f64,
    pub money_supply_m3: f64,
    pub consumption: f64,
    /// Imputed consumption — in-kind/subsistence flows valued at VWAP. Kept
    /// as its own GDP component so C+I+G+NX+imputed visibly reconciles with
    /// the headline (v4.18's 890M-vs-2.33B gap), while `consumption` stays
    /// the monetized series that V418-2 smoothing governs.
    pub imputed_consumption: f64,
    pub investment: f64,
    pub government_spending: f64,
    pub net_exports: f64,
    pub cpi: f64,
    pub ppi: f64,
    pub deltas: TelemetryDeltas,
    /// Emergency Stabilization: Total furloughed workers across all companies.
    /// Distinct from unemployed — furloughed workers are retained by companies
    /// and excluded from active labor clearing.
    pub furloughed_total: f64,
    /// Phase 89: Peasant population (FreePeasant + Serf classes) across all regions.
    /// These are subsistence farmers operating outside the corporate structure
    /// on Smallholder parcels — NOT corporate agricultural wage laborers.
    pub peasant_population: f64,
    /// Phase 89: Peasant share of total national population (percentage).
    pub peasant_pct: f64,
}

/// Phase 24F: One point of the per-country macro history ring buffer,
/// projected for the frontend macro chart. Sourced from
/// `MacroData::telemetry_history` — the authoritative per-country series,
/// so country switches render that country's own history rather than
/// accumulating cross-country points client-side.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MacroHistoryPoint {
    pub turn: u32,
    pub year: u32,
    pub gdp: f64,
    pub cpi: f64,
    pub ppi: f64,
}

/// Response for the get_banking_aggregates command.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct BankingAggregates {
    pub total_bank_reserves: f64,
    pub total_bank_deposits: f64,
    pub total_bank_loans: f64,
    pub total_consumer_debt: f64,
    pub dspw_bank_count: u32,
    pub central_bank_rate: f64,
    pub m0: f64,
    pub m3: f64,
    pub cb_fx_reserves_total: f64,
    pub cb_gold_reserves: f64,
}

/// Phase 54: Banking history response for sparkline tooltips.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct BankingHistoryResponse {
    pub turns: Vec<u32>,
    pub total_reserves: Vec<f64>,
    pub total_deposits: Vec<f64>,
    pub total_loans: Vec<f64>,
}

/// Phase 54: A single ministry expenditure category entry for the Finance tab.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MinistryExpenditureEntry {
    pub category: String,
    pub amount: f64,
    pub share_pct: f64,
}

/// Phase 54: A region option for the Companies tab region filter dropdown.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct RegionOption {
    pub value: String,
    pub label: String,
}

/// Phase 54: A role option for the VIPs tab role filter dropdown.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct RoleOption {
    pub value: String,
    pub label: String,
}

// ============================================================================
// AGGREGATION FUNCTIONS
// ============================================================================

/// Build a `CountrySnapshot` from a `Country` and global market data.
///
/// # Arguments
/// * `country` - The country to snapshot.
/// * `market_history` - Global market history (VWAP, base prices).
/// * `market` - Global market (net surplus per commodity).
/// * `buildings` - All buildings (for defect aggregation).
///
/// # Returns
/// A flat `CountrySnapshot` ready for TUI rendering.
pub fn build_country_snapshot(
    country: &Country,
    market_history: &MarketHistory,
    market: &GlobalMarket,
    buildings: &[crate::entities::Building],
    companies: &[Company],
    view: &ViewQuery,
    calendar_year: u32,
) -> CountrySnapshot {
    let macro_data = &country.macro_indicators;

    // Commodities: build rows for all 140 commodities.
    // Stabilization Sprint: Exclude local grid utilities (Energy, Heat, Water,
    // WasteUtility) — these are managed by physical grids, not the B2B market.
    // v4.22 WS1: hide commodities not yet invented (Software/Radio/Plastics/
    // OfficeMachinery/Batteries at 1900) — they surface once the calendar
    // reaches `Commodity::introduction_year`.
    let commodities: Vec<CommodityRow> = Commodity::all()
        .iter()
        .filter(|&&c| !c.is_local_utility())
        .filter(|&&c| calendar_year >= c.introduction_year())
        .map(|&c| {
            let name = format!("{:?}", c);
            let vwap = market_history
                .vwap_per_commodity
                .get(&c)
                .copied()
                .unwrap_or(0.0);
            let base_price = market_history
                .global_base_prices
                .get(&c)
                .copied()
                .unwrap_or(0.0);
            // Phase 43: Raw supply/demand volumes for the Market UI.
            let supply_volume = market.supply_volume.get(&c).copied().unwrap_or(0.0);
            let demand_volume = market.demand_volume.get(&c).copied().unwrap_or(0.0);
            // Bugfix Sprint: Net trade (imports − exports) for this commodity.
            let net_trade = market.net_trade.get(&c).copied().unwrap_or(0.0);
            // Bugfix Sprint: Compute the UI net_surplus dynamically as
            // supply − demand + net_trade. This is NOT market.net_surplus
            // (which is the raw B2B order book surplus used by clearing.rs).
            let net_surplus = supply_volume - demand_volume + net_trade;
            // Phase 33: Compute real ToT % change from stored previous-turn surplus.
            let prev_surplus = market_history
                .prev_net_surplus
                .get(&c)
                .copied()
                .unwrap_or(0.0);
            let tot_balance_change = if prev_surplus.abs() > 0.01 {
                ((net_surplus - prev_surplus) / prev_surplus.abs()) * 100.0
            } else if net_surplus.abs() > 0.01 {
                100.0 // New activity when there was none before.
            } else {
                0.0
            };
            // Phase 27: Mark commodity as active if any market activity exists.
            let active = vwap > 0.0
                || net_surplus.abs() > 0.01
                || supply_volume > 0.01
                || demand_volume > 0.01;
            CommodityRow {
                name,
                vwap,
                base_price,
                net_surplus,
                net_trade,
                tot_balance_change,
                active,
                supply_volume,
                demand_volume,
            }
        })
        .collect();

    // Infrastructure: summarize all network links.
    let infrastructure: Vec<InfrastructureRow> = country
        .transport_networks
        .links
        .iter()
        .map(|(id, link)| InfrastructureRow {
            link_id: id.clone(),
            condition: link.condition,
            capacity: 0.0, // capacity not directly on NetworkLink in current schema
        })
        .collect();

    // Tenders
    let tenders: Vec<TenderRow> = country
        .phase22_tenders
        .iter()
        .map(|t| TenderRow {
            id: t.id.clone(),
            name: if t.tender_name.is_empty() {
                t.id.clone()
            } else {
                t.tender_name.clone()
            },
            project_type: format!("{:?}", t.project_type),
            value: t.estimated_cost,
            status: format!("{:?}", t.status),
            awarded: t.awarded_bid.is_some(),
            contractor: t.awarded_bid.clone().unwrap_or_default(),
        })
        .collect();

    // KIO appeals
    let kio_appeals: Vec<KioAppealRow> = country
        .phase22_kio_appeals
        .iter()
        .map(|a| KioAppealRow {
            id: a.id.clone(),
            appellant: a.appellant_id.clone(),
            status: if a.resolution_turn > 0 {
                if a.upheld {
                    "upheld".to_string()
                } else {
                    "rejected".to_string()
                }
            } else {
                "pending".to_string()
            },
        })
        .collect();

    // Building defects (only buildings with condition < 0.5 or defect > 0.0)
    let defects: Vec<DefectRow> = buildings
        .iter()
        .filter(|b| b.structural_defect > 0.0 || b.condition < 0.5)
        .map(|b| DefectRow {
            building_id: b.id.clone(),
            region_id: b.region_id.clone(),
            condition: b.condition,
            structural_defect: b.structural_defect,
        })
        .collect();

    // Geological deposits
    // Phase 37: Count active miners per deposit by matching building.deposit_id.
    let deposit_miner_counts: std::rc::Rc<std::collections::HashMap<String, u32>> = {
        let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for b in buildings {
            if let Some(ref did) = b.deposit_id {
                *counts.entry(did.clone()).or_insert(0) += 1;
            }
        }
        std::rc::Rc::new(counts)
    };
    let deposits: Vec<DepositRow> = country
        .geological_formations
        .iter()
        .flat_map(|f| {
            let formation_name = f.name.clone();
            let formation_id = f.id.clone();
            let counts = std::rc::Rc::clone(&deposit_miner_counts);
            f.resource_deposits.iter().map(move |(dep_id, dep)| {
                // Phase 41: Buildings store deposit_id as "formation.id/deposit_key",
                // not "formation.name/deposit_key". Use formation.id for the lookup.
                let full_id = format!("{}/{}", formation_id, dep_id);
                DepositRow {
                    formation: formation_name.clone(),
                    deposit_id: dep_id.clone(),
                    current_reserves: dep.current_reserves.max(0.0),
                    estimated_reserves: dep.estimated_reserves,
                    quality: dep.quality,
                    // Depletion % of the original estimate, clamped so a
                    // malformed deposit can never render <0% or >100%.
                    depletion_pct: if dep.estimated_reserves > 0.0 {
                        ((1.0 - dep.current_reserves / dep.estimated_reserves)
                            * 100.0)
                            .clamp(0.0, 100.0)
                    } else {
                        0.0
                    },
                    active_miners: counts.get(&full_id).copied().unwrap_or(0),
                }
            })
        })
        .collect();

    // Shadow economy
    let shadow_economy = ShadowEconomySummary {
        total_hidden_fte: country
            .politics
            .shadow_economy_state
            .as_ref()
            .map(|s| s.total_hidden_fte)
            .unwrap_or(0.0),
        total_pit_evaded: country
            .politics
            .shadow_economy_state
            .as_ref()
            .map(|s| s.total_pit_evaded)
            .unwrap_or(0.0),
        shadow_gdp: macro_data.gdp_breakdown.shadow_gdp,
        // D.7.2: Inspectorate raid stats
        shadow_raids_conducted: country
            .politics
            .shadow_economy_state
            .as_ref()
            .map(|s| s.raids_conducted)
            .unwrap_or(0),
        shadow_fines_collected: country
            .politics
            .shadow_economy_state
            .as_ref()
            .map(|s| s.fines_collected)
            .unwrap_or(0.0),
        shadow_illegals_deported: country
            .politics
            .shadow_economy_state
            .as_ref()
            .map(|s| s.illegals_deported)
            .unwrap_or(0),
        shadow_total_pit_evaded: country
            .politics
            .shadow_economy_state
            .as_ref()
            .map(|s| s.total_pit_evaded)
            .unwrap_or(0.0),
    };

    // OHS casualties: aggregate from class demographics across all regions.
    let mut total_deceased: i64 = 0;
    let mut total_disabled: i64 = 0;
    let mut total_unable_to_work_fte: f64 = 0.0;
    for region in &country.regions {
        for demo in region.class_demographics.rural_classes.values() {
            total_deceased += demo.deceased;
            total_disabled += demo.active_disabled;
            total_unable_to_work_fte += demo.unable_to_work;
        }
        for demo in region.class_demographics.urban_classes.values() {
            total_deceased += demo.deceased;
            total_disabled += demo.active_disabled;
            total_unable_to_work_fte += demo.unable_to_work;
        }
    }
    // OHS accidents on construction projects
    let ohs_accidents_on_projects: u32 = buildings
        .iter()
        .filter_map(|b| b.active_project.as_ref())
        .map(|p| p.ohs_accidents)
        .sum();

    let ohs = OhsSummary {
        total_deceased,
        total_disabled,
        total_unable_to_work_fte,
        ohs_accidents_on_projects,
    };

    // Phase 89: Peasant population — FreePeasant + Serf class populations
    // across all regions. These are subsistence farmers operating outside
    // the corporate structure on Smallholder parcels. Explicitly excludes
    // LandlessLaborer (rural wage laborers) and Aristocracy (landowning elite).
    let mut peasant_population: f64 = 0.0;
    for region in &country.regions {
        if let Some(free_peasant) = region.class_demographics.rural_classes.get(&RuralClass::FreePeasant) {
            peasant_population += free_peasant.population as f64;
        }
        if let Some(serf) = region.class_demographics.rural_classes.get(&RuralClass::Serf) {
            peasant_population += serf.population as f64;
        }
    }
    let total_pop = country.budget.population as f64;
    let peasant_pct = if total_pop > 0.0 {
        peasant_population / total_pop * 100.0
    } else {
        0.0
    };

    // Labor market
    // v4.18 WS1: Furloughed total is summed from the live company roster at
    // snapshot-build time — `macro_data.labor_market.furloughed_total` is
    // refreshed end-of-turn from the same source, and the company-level
    // field is the authoritative one (mid-turn writes race the furlough
    // phases that run later in the turn).
    let furloughed_total = crate::economy::telemetry::sum_company_furloughed(companies);
    let labor = LaborSummary {
        unemployment_rate: macro_data.labor_market.unemployment_rate,
        employed_total: macro_data.labor_market.employed_total,
        unemployed: macro_data.labor_market.unemployed,
        workforce: macro_data.labor_market.employed_total + macro_data.labor_market.unemployed,
        average_wage: macro_data.average_wage,
        furloughed_total,
        peasant_population,
        peasant_pct,
        sports_capacity_per_capita: macro_data.health_statistics.sports_capacity_per_capita,
    };

    // Corruption index
    let corruption_index = country
        .politics
        .inspectorate_state
        .as_ref()
        .map(|ist| ist.corruption_index)
        .unwrap_or(0.0);

    // Justice coverage
    let justice_coverage = country
        .politics
        .justice_state
        .as_ref()
        .map(|js| js.justice_coverage)
        .unwrap_or(0.0);

    // Treasury summary
    let treasury = TreasurySummary {
        gdp: country.budget.gdp,
        population: country.budget.population,
        liquid_reserves: country.budget.liquid_reserves,
        private_capital: country.budget.private_capital,
        infrastructure_level: country.budget.infrastructure_level,
        savings: 0.0, // aggregated from regions if needed
    };

    // Aggregate citizen savings from all regions
    let mut total_savings: f64 = 0.0;
    for region in &country.regions {
        for demo in region.class_demographics.rural_classes.values() {
            total_savings += demo.savings;
        }
        for demo in region.class_demographics.urban_classes.values() {
            total_savings += demo.savings;
        }
    }
    let mut treasury = treasury;
    treasury.savings = total_savings;

    // Phase 24F/90: Compute ToT/YoY deltas from telemetry history.
    // v4.18 WS1: reuse the live company-sum furlough total computed above
    // for the labor summary (single authoritative aggregation).
    let population_f64 = country.budget.population as f64;
    let gdp_per_capita = if population_f64 > 0.0 {
        macro_data.gdp_breakdown.official_gdp / population_f64
    } else {
        0.0
    };
    let deltas = compute_deltas(
        &macro_data.telemetry_history,
        macro_data,
        corruption_index,
        population_f64,
        peasant_population,
        furloughed_total,
        gdp_per_capita,
    );

    // Phase 26: Aggregate companies by sector for the Sector Overview table.
    let sectors = aggregate_sectors(companies, country);

    // Phase 34: Build region rows for the [8] Regions tab.
    // v4.18 WS1: `Region::gdp` is the expenditure-side regional account
    // written each turn by `reconcile_regional_gdp` — floored at 0 (inventory
    // drawdowns are not negative GDP) and reconciled so Σ region.gdp equals
    // national official GDP to floating-point exactness. The previous
    // production-side `regional_value_added` path displayed gross output
    // (not value added) at ~120× national scale and could go negative on
    // stock drawdown; it remains available as a diagnostic in geography.rs.
    let regions: Vec<RegionRow> = country
        .regions
        .iter()
        .map(|r| {
            // Derive megaregion membership by searching country.megaregions.
            let megaregion_info = country
                .megaregions
                .iter()
                .find(|mg| mg.regions.contains(&r.id))
                .map(|mg| (mg.name.clone(), mg.id.clone()))
                .unwrap_or_else(|| ("Unassigned".to_string(), String::new()));
            let regional_gdp = r.gdp.max(0.0);
            let gdp_per_capita = if r.population > 0 {
                regional_gdp / r.population as f64
            } else {
                0.0
            };
            RegionRow {
                id: r.id.clone(),
                display_name: if r.display_name.is_empty() {
                    r.id.clone()
                } else {
                    r.display_name.clone()
                },
                megaregion: megaregion_info.0,
                megaregion_id: megaregion_info.1,
                population: r.population,
                regional_gdp,
                gdp_per_capita,
                has_governance: r.governance.is_some(),
                governance_mode: regional_governance_mode(r.governance.as_ref()),
                liquid_reserves: r
                    .governance
                    .as_ref()
                    .map(|g| g.budget.liquid_reserves)
                    .unwrap_or(0.0),
            }
        })
        .collect();

    CountrySnapshot {
        name: country.name.clone(),
        turn: 0, // filled by caller from GameState calendar
        year: 0, // filled by caller
        gdp_breakdown: macro_data.gdp_breakdown.clone(),
        inflation_indices: macro_data.inflation_indices.clone(),
        money_supply: macro_data.money_supply.clone(),
        treasury,
        central_bank_rate: country.central_bank.interest_rates.reference_rate,
        commodities,
        infrastructure,
        freight_config_summary: format!(
            "rate={:.2}/tkm max_defer={}t",
            country.freight_logistics_config.base_freight_rate,
            country.freight_logistics_config.max_deferred_turns,
        ),
        sectors,
        tenders,
        kio_appeals,
        defects,
        deposits,
        shadow_economy,
        ohs,
        labor,
        corruption_index,
        justice_coverage,
        population: country.budget.population,
        sovereign_default_turns: country.sovereign_default_turns_remaining,
        deltas,
        government: build_government_snapshot(country, country_year(country)),
        parliament: build_parliament_snapshot(country, country_year(country)),
        regions,
        finance: build_finance_snapshot(country, companies),
        vips_page: build_vip_page(country, companies, view),
        vip_total_count: count_vips(country, view),
        vip_dossier: build_vip_dossier(country, view),
        banks_page: build_bank_page(country, companies, view),
        bank_total_count: count_banks(companies),
        companies_page: build_company_page(country, companies, view),
        company_total_count: count_companies(companies, view),
        company_detail: build_company_detail(country, companies, view),
        region_detail: build_region_detail(country, buildings, view),
        megaregion_detail: build_megaregion_detail(country, buildings, view),
        advisory_council: build_advisory_council_snapshot(country),
        royal_dynasty: build_royal_dynasty_snapshot(country),
        government_form: country.politics.government_form.display_name().to_string(),
    }
}

// ============================================================================
// PHASE 49: PAGINATED VIP / BANK / COMPANY BUILDERS
// ============================================================================

/// Build a paginated, filtered page of VIP rows from the VipRegistry.
fn build_vip_page(
    country: &Country,
    companies: &[Company],
    view: &ViewQuery,
) -> Vec<VipDossierRow> {
    let registry = match &country.politics.vip_registry {
        Some(r) => r,
        None => return Vec::new(),
    };

    // Phase 54: Build a lookup from CEO VIP ID to company name.
    let ceo_to_company: std::collections::HashMap<&str, &str> = companies
        .iter()
        .filter_map(|c| {
            c.ceo_vip_id
                .as_ref()
                .map(|id| (id.as_str(), c.name.as_str()))
        })
        .collect();

    // Collect and filter VIPs from both living and deceased lists.
    let search_lower = view.vip_filter.search.to_lowercase();
    let role_filter = &view.vip_filter.role_filter;
    let filter_fn = |v: &&crate::politics::vip_registry::Vip| {
        if !view.vip_filter.show_dead && v.is_dead {
            return false;
        }
        if !search_lower.is_empty() && !v.full_name.to_lowercase().contains(&search_lower) {
            return false;
        }
        // Phase 54/91: Role filter — check if any role's canonical_name() matches.
        // Phase 91: Use canonical_name() (e.g., "PrimeMinister") instead of
        // as_str() (e.g., "Prime Minister") for consistent filtering.
        if !role_filter.is_empty() {
            let has_role = v
                .roles
                .iter()
                .any(|r| r.canonical_name() == role_filter.as_str());
            if !has_role {
                return false;
            }
        }
        true
    };
    let mut all_vips: Vec<&crate::politics::vip_registry::Vip> =
        registry.vips.values().filter(filter_fn).collect();
    if view.vip_filter.show_dead {
        all_vips.extend(registry.deceased.iter().filter(filter_fn));
    }

    // Sort by name for stable pagination.
    all_vips.sort_by(|a, b| a.full_name.cmp(&b.full_name));

    // Apply pagination.
    all_vips
        .into_iter()
        .skip(view.vip_page.offset)
        .take(view.vip_page.limit.max(1))
        .map(|v| {
            let roles_str = if v.roles.is_empty() {
                "Private Citizen".to_string()
            } else {
                v.roles
                    .iter()
                    .map(|r| r.as_str().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let company_name = v
                .roles
                .contains(&crate::politics::vip_registry::VipRoleExtended::Ceo)
                .then(|| ceo_to_company.get(v.id.as_str()).map(|s| s.to_string()))
                .flatten();
            VipDossierRow {
                id: v.id.clone(),
                full_name: if v.is_dead {
                    format!("{} †", v.full_name)
                } else {
                    v.full_name.clone()
                },
                roles: roles_str,
                age: v.age,
                health: v.health.aggregate(),
                faction: v.faction.clone(),
                influence: v.base_influence,
                is_dead: v.is_dead,
                main_trait: v.main_trait.clone(),
                ideology: v.ideology.clone(),
                company_name,
                gender: v.gender.clone(),
                portrait_seed: v.portrait_seed.clone(),
            }
        })
        .collect()
}

/// Count total VIPs matching the current filter (for scroll indicators).
fn count_vips(country: &Country, view: &ViewQuery) -> usize {
    let registry = match &country.politics.vip_registry {
        Some(r) => r,
        None => return 0,
    };
    let search_lower = view.vip_filter.search.to_lowercase();
    let filter_fn = |v: &&crate::politics::vip_registry::Vip| {
        if !view.vip_filter.show_dead && v.is_dead {
            return false;
        }
        if !search_lower.is_empty() && !v.full_name.to_lowercase().contains(&search_lower) {
            return false;
        }
        // Phase 54/91: Role filter — use canonical_name() for consistent filtering.
        if !view.vip_filter.role_filter.is_empty() {
            let has_role = v
                .roles
                .iter()
                .any(|r| r.canonical_name() == view.vip_filter.role_filter.as_str());
            if !has_role {
                return false;
            }
        }
        true
    };
    let living = registry.vips.values().filter(filter_fn).count();
    let dead = if view.vip_filter.show_dead {
        registry.deceased.iter().filter(filter_fn).count()
    } else {
        0
    };
    living + dead
}

/// Build a full dossier for the VIP specified in `view.vip_dossier_id`.
fn build_vip_dossier(country: &Country, view: &ViewQuery) -> Option<VipDossier> {
    let vip_id = view.vip_dossier_id.as_ref()?;
    let registry = country.politics.vip_registry.as_ref()?;
    // Look in living VIPs first, then deceased.
    let v = registry
        .get(vip_id)
        .or_else(|| registry.deceased.iter().find(|d| d.id == *vip_id))?;
    Some(VipDossier {
        id: v.id.clone(),
        full_name: v.full_name.clone(),
        gender: v.gender.clone(),
        age: v.age,
        health: v.health.aggregate(),
        incapacity: format!("{:?}", v.incapacity),
        traits: v.traits.clone(),
        main_trait: v.main_trait.clone(),
        ideology: v.ideology.clone(),
        religion: v.religion.clone(),
        nationality: v.nationality.clone(),
        dynasty: v.dynasty.clone(),
        roles: if v.roles.is_empty() {
            vec!["Private Citizen".to_string()]
        } else {
            v.roles.iter().map(|r| r.as_str().to_string()).collect()
        },
        base_influence: v.base_influence,
        faction: v.faction.clone(),
        born_turn: v.born_turn,
        is_dead: v.is_dead,
        death_turn: v.death_turn,
        death_cause: v.death_cause.as_ref().map(|c| format!("{:?}", c)),
        acting_replacement_id: v.acting_replacement_id.clone(),
    })
}

/// Build a paginated page of bank rows from companies.
fn build_bank_page(_country: &Country, companies: &[Company], view: &ViewQuery) -> Vec<BankRow> {
    let mut banks: Vec<&Company> = companies.iter().filter(|c| c.bank_type.is_some()).collect();
    banks.sort_by(|a, b| a.name.cmp(&b.name));

    banks
        .into_iter()
        .skip(view.bank_page.offset)
        .take(view.bank_page.limit.max(1))
        .filter_map(|c| {
            let bs = c.balance_sheet.as_ref()?;
            let loans: f64 = bs.loans_issued.iter().map(|l| l.outstanding_balance).sum();
            let ldr = if bs.deposits > 0.0 {
                loans / bs.deposits
            } else {
                0.0
            };
            Some(BankRow {
                id: c.id.clone(),
                name: c.name.clone(),
                bank_type: format!("{:?}", c.bank_type.as_ref()?),
                reserves: bs.reserves_at_central_bank,
                deposits: bs.deposits,
                loans,
                securities: bs.securities,
                is_dspw: c.is_dspw,
                ldr,
            })
        })
        .collect()
}

/// v4.19 WS2: Full detail for one bank (expandable row panel). Every figure
/// is read from the live balance sheet / ledger — interest flows are the
/// per-turn run-rate implied by outstanding balances and contract rates.
pub fn build_bank_detail(
    country: &Country,
    companies: &[Company],
    bank_id: &str,
) -> Option<BankDetail> {
    use crate::state::banking::LoanStatus;
    let c = companies.iter().find(|c| c.id == bank_id && c.bank_type.is_some())?;
    let bs = c.balance_sheet.as_ref()?;
    let rates = &country.central_bank.interest_rates;

    let corporate_loans: f64 = bs.loans_issued.iter().map(|l| l.outstanding_balance).sum();
    let non_performing_loans: f64 = bs
        .loans_issued
        .iter()
        .filter(|l| matches!(l.status, LoanStatus::Overdue | LoanStatus::Default))
        .map(|l| l.outstanding_balance)
        .sum();
    let mbs_holdings_value: f64 = bs
        .mbs_holdings
        .iter()
        .map(|m| m.total_underlying_notional)
        .sum();
    let interbank_given: f64 = bs.interbank_loans_given.values().sum();
    let interbank_taken: f64 = bs.interbank_loans_taken.values().sum();

    // Per-turn interest run-rate: loan interest accrues at each loan's
    // contract rate; liabilities accrue at the CB standing-facility corridor
    // (deposits at the deposit rate, Lombard at the penalty rate, interbank
    // at the reference rate, issued bonds at reference + 1% credit spread).
    let interest_income: f64 = bs
        .loans_issued
        .iter()
        .map(|l| l.outstanding_balance * l.interest_rate)
        .sum::<f64>()
        + bs.consumer_loans_outstanding * (rates.reference_rate + 0.04)
        + interbank_given * rates.reference_rate;
    let interest_expense: f64 = bs.deposits * rates.deposit_rate
        + bs.cb_lombard_loans * rates.lombard_rate
        + interbank_taken * rates.reference_rate
        + bs.issued_bonds * (rates.reference_rate + 0.01);
    let interest_bearing_assets =
        corporate_loans + bs.consumer_loans_outstanding + interbank_given + bs.securities;
    let net_interest_margin = if interest_bearing_assets > 0.0 {
        (interest_income - interest_expense) / interest_bearing_assets
    } else {
        0.0
    };
    let ldr = if bs.deposits > 0.0 {
        (corporate_loans + bs.consumer_loans_outstanding) / bs.deposits
    } else {
        0.0
    };
    let total_assets = bs.total_assets();
    let car = if total_assets > 0.0 {
        bs.tier_1_capital / total_assets
    } else {
        0.0
    };
    let reserve_ratio = if bs.deposits > 0.0 {
        bs.reserves_at_central_bank / bs.deposits
    } else {
        1.0
    };

    Some(BankDetail {
        id: c.id.clone(),
        name: c.name.clone(),
        bank_type: format!("{:?}", c.bank_type.as_ref()?),
        is_dspw: c.is_dspw,
        reserves_at_central_bank: bs.reserves_at_central_bank,
        cb_deposit_facility_balance: bs.cb_deposit_facility_balance,
        interbank_loans_given: interbank_given,
        corporate_loans,
        consumer_loans: bs.consumer_loans_outstanding,
        securities: bs.securities,
        real_estate: bs.real_estate,
        mbs_holdings_value,
        deposits: bs.deposits,
        cb_lombard_loans: bs.cb_lombard_loans,
        interbank_loans_taken: interbank_taken,
        issued_bonds: bs.issued_bonds,
        tier_1_capital: bs.tier_1_capital,
        total_assets,
        total_liabilities: bs.total_liabilities(),
        ldr,
        car,
        reserve_ratio,
        interest_income,
        interest_expense,
        net_interest_margin,
        non_performing_loans,
        operating_wages: c.wages_paid_this_turn + c.arrears_accrued_this_turn,
        net_profit: c.realized_profit_this_turn,
    })
}

/// v4.19 WS4: Constitution parameters + the live statute/policy book for
/// the "Constitution & Laws" tab. All rows come from real `politics`
/// fields — no projected or synthetic statute lists.
pub fn build_constitution_laws(country: &Country) -> ConstitutionLawsResponse {
    let p = &country.politics;
    let c = &p.constitution;
    let constitution = ConstitutionSnapshot {
        exists: c.exists,
        regime_type: p.government_form.display_name().to_string(),
        chambers: p.government_form.chambers(),
        upper_house_name: c.upper_house.as_ref().map(|u| u.name.clone()),
        electoral_system: if p.election_method.is_empty() {
            "None (non-elective regime)".to_string()
        } else {
            p.election_method.clone()
        },
        election_threshold: p.election_threshold,
        term_length_years: p.government_form.election_cycle(),
        presidential_veto: c.presidential_veto,
        constitutional_tribunal: c.constitutional_tribunal.clone(),
        suffrage_system: crate::politics::system::humanize_debug_token(&format!(
            "{:?}",
            c.suffrage_system.suffrage_type
        )),
        budget_failure_consequence: crate::politics::system::humanize_debug_token(&format!(
            "{:?}",
            c.budget_failure_consequence
        )),
        change_mechanism: c.change_mechanism.clone(),
    };

    // Live statute/policy rows — every entry is a real Politics field and
    // its currently-active parameters.
    let mut active_laws: Vec<ActiveLawRow> = Vec::new();
    let mut push = |domain: &str, name: &str, summary: String| {
        if !summary.is_empty() {
            active_laws.push(ActiveLawRow {
                domain: domain.to_string(),
                name: name.to_string(),
                summary,
            });
        }
    };
    push("Civil Rights", "Civil Rights Regime", p.civil_rights_law.clone());
    push("Civil Rights", "Emancipation Regime", p.emancipation_law.clone());
    push("Labor", "Labor Relations Regime", p.labor_law.clone());
    push("Labor", "Union Regime", p.union_law.clone());
    push("Labor", "Strike Regime", p.strike_law.clone());
    push("Religion", "Religious Regime", p.religious_law.clone());
    if let Some(ref l) = p.child_labor_law {
        push(
            "Labor",
            "Child Labor Law",
            crate::politics::system::humanize_debug_token(&format!("{l:?}")),
        );
    }
    if let Some(ref l) = p.healthcare_law {
        push(
            "Health",
            "Healthcare Act",
            format!(
                "{} — {} coverage",
                crate::politics::system::humanize_debug_token(&format!(
                    "{:?}",
                    l.healthcare_system
                )),
                crate::politics::system::humanize_debug_token(&format!(
                    "{:?}",
                    l.universality
                ))
            ),
        );
    }
    if let Some(ref l) = p.education_law {
        push(
            "Education",
            "Education Act",
            format!(
                "{} — {} school system",
                crate::politics::system::humanize_debug_token(&format!(
                    "{:?}",
                    l.education_model
                )),
                crate::politics::system::humanize_debug_token(&format!(
                    "{:?}",
                    l.school_system
                ))
            ),
        );
    }
    if let Some(ref l) = p.justice_law {
        push(
            "Justice",
            "Justice Reform",
            format!(
                "pardon: {}, KRS separated: {}, corruption idx {:.2}",
                crate::politics::system::humanize_debug_token(&format!(
                    "{:?}",
                    l.pardon_authority
                )),
                l.krs_separated,
                l.corruption_index
            ),
        );
    }
    if let Some(ref l) = p.prison_labor_law {
        push(
            "Justice",
            "Prison Labor Law",
            format!(
                "{} — compensation {:.2}",
                crate::politics::system::humanize_debug_token(&format!(
                    "{:?}",
                    l.prison_type
                )),
                l.labor_compensation
            ),
        );
    }
    if let Some(ref l) = p.sentencing_law {
        push(
            "Justice",
            "Sentencing Code",
            format!(
                "death penalty: {}, life imprisonment: {}, community service: {}",
                l.death_penalty_enabled, l.life_imprisonment_enabled, l.community_service_enabled
            ),
        );
    }
    if let Some(ref l) = p.amnesty_law {
        if l.active {
            push(
                "Justice",
                "Amnesty Program",
                format!(
                    "rate {:.2}, fee {:.0}, target {}",
                    l.legalization_rate,
                    l.legalization_fee,
                    crate::politics::system::humanize_debug_token(&format!(
                        "{:?}",
                        l.target_status
                    ))
                ),
            );
        }
    }
    if let Some(ref l) = p.migration_law {
        push(
            "Migration",
            "Migration Law",
            format!(
                "open borders: {}, visa required: {}, deportation: {}",
                l.open_borders,
                l.visa_required,
                crate::politics::system::humanize_debug_token(&format!(
                    "{:?}",
                    l.deportation_policy
                ))
            ),
        );
    }
    if let Some(ref l) = p.religious_law_struct {
        push(
            "Religion",
            "Religious Law",
            format!(
                "state religion: {}, separation: {}, church tax {:.2}",
                l.state_religion
                    .as_deref()
                    .map(crate::politics::system::humanize_debug_token)
                    .unwrap_or_else(|| "none".into()),
                l.separation_of_church_and_state,
                l.church_tax_rate
            ),
        );
    }
    if p.nuclear_law {
        push("Energy", "Nuclear Program Act", "authorized".to_string());
    }

    // Enacted statutes — bills that completed the pipeline this term, with
    // the terminal stage/enactment turn stamped by `mark_bill_terminal`.
    let mut enacted_statutes: Vec<EnactedStatuteRow> = Vec::new();
    if let Some(ref parl) = p.parliament_struct {
        for chamber in &parl.chambers {
            for record in &chamber.recent_votes {
                let Some(ref detail) = record.detail else {
                    continue;
                };
                if detail.stage != "Enacted" {
                    continue;
                }
                if enacted_statutes.iter().any(|r| r.bill_id == record.bill_id) {
                    continue;
                }
                enacted_statutes.push(EnactedStatuteRow {
                    bill_id: record.bill_id.clone(),
                    title: record.bill_title.clone(),
                    initiator: detail.initiator.clone(),
                    bill_type: detail.bill_type.clone(),
                    enacted_turn: detail.enacted_turn,
                    provisions: detail
                        .provisions
                        .iter()
                        .map(|pr| pr.description.clone())
                        .collect(),
                });
            }
        }
        enacted_statutes.sort_by_key(|r| r.bill_id.clone());
    }

    ConstitutionLawsResponse {
        constitution,
        active_laws,
        enacted_statutes,
    }
}

/// v4.19 WS4: Elections & Campaigns snapshot — countdown from
/// `years_to_elections` (the live term counter), party standings from real
/// `support` values and current seats, and executed campaign actions.
pub fn build_elections(country: &Country, current_year: u32) -> ElectionsResponse {
    let p = &country.politics;
    let election_state = match &p.election_state {
        crate::politics::campaign::ElectionState::Idle => "Idle".to_string(),
        crate::politics::campaign::ElectionState::PreCampaign {
            turns_until_start,
            registration_deadline,
        } => format!(
            "Pre-Campaign (starts in {} turns, registration by turn {})",
            turns_until_start, registration_deadline
        ),
        crate::politics::campaign::ElectionState::ActiveCampaign {
            turns_remaining, ..
        } => format!("Active Campaign ({} turns remaining)", turns_remaining),
        crate::politics::campaign::ElectionState::ElectionDay => "Election Day".to_string(),
        crate::politics::campaign::ElectionState::PostElectionResolution { turn } => {
            format!("Post-Election Resolution (turn {})", turn)
        }
    };
    let current_seats: Vec<(String, u32)> = p
        .parliament_struct
        .as_ref()
        .map(|parl| {
            let mut v: Vec<(String, u32)> =
                parl.lower_seat_map().into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            v
        })
        .unwrap_or_else(|| {
            let mut v: Vec<(String, u32)> = p.parliament.iter().map(|(k, v)| (k.clone(), *v)).collect();
            v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            v
        });

    let ruling = p
        .parliament
        .iter()
        .max_by_key(|(_, s)| *s)
        .map(|(k, _)| k.clone());
    // v4.20 T1: Normalize support — the engine stores percent-scale values
    // (sum ≈ 100) but the contract requires a 0..1 fraction summing to ~1.0
    // across active parties.
    let total_support: f64 = p.active_parties.values().map(|x| x.support).sum();
    let mut parties: Vec<ElectionPartyRow> = p
        .active_parties
        .iter()
        .map(|(pid, party)| {
            let (color_hex, ideology_icon) =
                crate::politics::system::club_visual_identity(
                    pid,
                    &party.ideology,
                    &party.coordinates,
                    &p.active_parties,
                );
            ElectionPartyRow {
                party_id: pid.clone(),
                ideology: party.ideology.clone(),
                support: if total_support > 0.0 {
                    (party.support / total_support).clamp(0.0, 1.0)
                } else {
                    0.0
                },
                seats: p
                    .parliament_struct
                    .as_ref()
                    .map(|parl| *parl.lower_seat_map().get(pid).unwrap_or(&0))
                    .or_else(|| p.parliament.get(pid).copied())
                    .unwrap_or(0),
                is_coalition_member: p.coalition.contains(pid),
                is_ruling: ruling.as_deref() == Some(pid.as_str()),
                color_hex,
                ideology_icon,
            }
        })
        .collect();
    parties.sort_by(|a, b| {
        b.support
            .partial_cmp(&a.support)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.party_id.cmp(&b.party_id))
    });

    let campaign_actions: Vec<CampaignActionRow> = p
        .campaign_executions
        .iter()
        .map(|e| CampaignActionRow {
            party_id: e.party_id.clone(),
            action: crate::politics::system::humanize_debug_token(&format!(
                "{:?}",
                e.action
            )),
            turn: e.execution_turn,
            is_black_money: e.is_black_money,
        })
        .collect();

    ElectionsResponse {
        government_form: p.government_form.display_name().to_string(),
        election_state,
        years_to_elections: p.years_to_elections,
        estimated_turns_remaining: p.years_to_elections.saturating_mul(24),
        turns_remaining: p.turns_until_election,
        scheduled_election_turn: p.scheduled_election_turn,
        next_election_year: current_year.saturating_add(p.years_to_elections),
        head_of_state_name: if !p.head_of_state.name.is_empty() {
            p.head_of_state.name.clone()
        } else {
            "(vacant)".to_string()
        },
        head_of_state_title: p.government_form.display_name().to_string(),
        campaign_duration_turns: p.campaign_duration_turns,
        parties,
        campaign_actions,
        current_seats,
        election_history: p
            .election_history
            .iter()
            .map(|e| ElectionRecordRow {
                turn: e.turn,
                year: e.year,
                results: e.results.clone(),
                presidential_rounds: e
                    .presidential_rounds
                    .iter()
                    .flat_map(|r| {
                        let round = r.round;
                        let winner = r.winner.clone();
                        r.candidates
                            .iter()
                            .map(move |(cand, party, share)| {
                                (round, cand.clone(), party.clone(), *share, *cand == winner)
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect(),
            })
            .collect(),
        support_history: p
            .party_support_history
            .iter()
            .map(|s| SupportSnapshotRow {
                turn: s.turn,
                support: s.support.clone(),
            })
            .collect(),
        head_of_state_party: p.ruling_party.clone(),
    }
}

/// Count total banks (for scroll indicators).
fn count_banks(companies: &[Company]) -> usize {
    companies.iter().filter(|c| c.bank_type.is_some()).count()
}

/// Build a paginated, filtered page of company rows.
fn build_company_page(
    country: &Country,
    companies: &[Company],
    view: &ViewQuery,
) -> Vec<CompanyRow> {
    let search_lower = view.company_filter.search.to_lowercase();
    let sector_filter = &view.company_filter.sector_filter;
    let region_filter = &view.company_filter.region_filter;
    let parsed_sector: Option<Sector> = if sector_filter.is_empty() {
        None
    } else {
        serde_json::from_value(serde_json::Value::String(sector_filter.clone())).ok()
    };
    let mut filtered: Vec<&Company> = companies
        .iter()
        .filter(|c| {
            if !search_lower.is_empty() && !c.name.to_lowercase().contains(&search_lower) {
                return false;
            }
            if let Some(ref ps) = parsed_sector {
                if c.sector != *ps {
                    return false;
                }
            }
            // Phase 54: Region filter.
            if !region_filter.is_empty() && c.region_id != *region_filter {
                return false;
            }
            true
        })
        .collect();
    filtered.sort_by(|a, b| a.name.cmp(&b.name));

    filtered
        .into_iter()
        .skip(view.company_page.offset)
        .take(view.company_page.limit.max(1))
        .map(|c| CompanyRow {
            id: c.id.clone(),
            name: c.name.clone(),
            sector: sector_display_name(c.sector),
            region: country
                .regions
                .iter()
                .find(|r| r.id == c.region_id)
                .map(|r| {
                    if r.display_name.is_empty() {
                        r.id.clone()
                    } else {
                        r.display_name.clone()
                    }
                })
                .unwrap_or_else(|| c.region_id.clone()),
            fulfilled_fte: c.fulfilled_fte,
            average_wage: c.offered_wage_per_fte,
            seasonal_state: if c.furloughed_workers_count > 0.0 {
                "Furloughed".to_string()
            } else {
                "Active".to_string()
            },
            wage_arrears: c.wage_arrears,
        })
        .collect()
}

/// Count total companies matching the current filter.
fn count_companies(companies: &[Company], view: &ViewQuery) -> usize {
    let search_lower = view.company_filter.search.to_lowercase();
    let sector_filter = &view.company_filter.sector_filter;
    let region_filter = &view.company_filter.region_filter;
    let parsed_sector: Option<Sector> = if sector_filter.is_empty() {
        None
    } else {
        serde_json::from_value(serde_json::Value::String(sector_filter.clone())).ok()
    };
    companies
        .iter()
        .filter(|c| {
            if !search_lower.is_empty() && !c.name.to_lowercase().contains(&search_lower) {
                return false;
            }
            if let Some(ref ps) = parsed_sector {
                if c.sector != *ps {
                    return false;
                }
            }
            // Phase 54: Region filter.
            if !region_filter.is_empty() && c.region_id != *region_filter {
                return false;
            }
            true
        })
        .count()
}

/// Build a full company detail for the company with `view.company_detail_id`.
/// Phase 87+: Compute a financial summary from the company's financial history.
/// Aggregates last turn, last quarter (3 turns), and last year (24 turns).
fn compute_financial_summary(history: &[serde_json::Value]) -> CompanyFinancialSummary {
    let parse_record = |v: &serde_json::Value| -> CompanyFinancialRecord {
        // Phase 88: Fixed field mapping. The actual financial history records
        // (pushed at manager.rs:788-799) use "revenue", "operating_costs",
        // "interest", "taxes", and "net_profit" — NOT "income"/"expenses".
        let income = v.get("revenue").and_then(|i| i.as_f64()).unwrap_or(0.0);
        let operating = v
            .get("operating_costs")
            .and_then(|e| e.as_f64())
            .unwrap_or(0.0);
        let interest = v.get("interest").and_then(|e| e.as_f64()).unwrap_or(0.0);
        let taxes = v.get("taxes").and_then(|e| e.as_f64()).unwrap_or(0.0);
        let wage_expense = v
            .get("wage_expense")
            .and_then(|w| w.as_f64())
            .unwrap_or(0.0);
        let wage_arrears = v
            .get("wage_arrears")
            .and_then(|w| w.as_f64())
            .unwrap_or(0.0);
        let expenses = operating + interest + taxes;
        let net_profit = v
            .get("net_profit")
            .and_then(|n| n.as_f64())
            .unwrap_or(income - expenses);
        CompanyFinancialRecord {
            income,
            expenses,
            net_profit,
            wage_expense,
            wage_arrears,
        }
    };

    let aggregate = |records: &[&serde_json::Value]| -> CompanyFinancialRecord {
        if records.is_empty() {
            return CompanyFinancialRecord::default();
        }
        let (income, expenses, wage_expense, wage_arrears) =
            records
                .iter()
                .fold((0.0, 0.0, 0.0, 0.0), |(acc_i, acc_e, acc_w, acc_a), v| {
                    let r = parse_record(v);
                    (
                        acc_i + r.income,
                        acc_e + r.expenses,
                        acc_w + r.wage_expense,
                        acc_a + r.wage_arrears,
                    )
                });
        let n = records.len() as f64;
        CompanyFinancialRecord {
            income: income / n,
            expenses: expenses / n,
            net_profit: (income - expenses) / n,
            wage_expense: wage_expense / n,
            wage_arrears: wage_arrears / n,
        }
    };

    let last_turn = history.last().map(parse_record).unwrap_or_default();

    let quarter_records: Vec<&serde_json::Value> = history.iter().rev().take(3).collect();
    let last_quarter = aggregate(&quarter_records);

    let year_records: Vec<&serde_json::Value> = history.iter().rev().take(24).collect();
    let last_year = aggregate(&year_records);

    CompanyFinancialSummary {
        last_turn,
        last_quarter,
        last_year,
    }
}

fn build_company_detail(
    country: &Country,
    companies: &[Company],
    view: &ViewQuery,
) -> Option<CompanyDetail> {
    let target_id = view.company_detail_id.as_ref()?;
    let c = companies.iter().find(|c| c.id == *target_id)?;

    // Phase 54: Resolve CEO name and ideology from the VIP registry.
    let (ceo_name, ceo_ideology) = if let Some(ref ceo_id) = c.ceo_vip_id {
        if let Some(ref registry) = country.politics.vip_registry {
            if let Some(ceo_vip) = registry.get(ceo_id) {
                (
                    Some(ceo_vip.full_name.clone()),
                    Some(ceo_vip.ideology.clone()),
                )
            } else {
                (None, None)
            }
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    // Phase 87+: Compute financial summary from financial_history.
    let financial_summary = if c.financial_history.is_empty() {
        None
    } else {
        Some(compute_financial_summary(&c.financial_history))
    };

    Some(CompanyDetail {
        id: c.id.clone(),
        name: c.name.clone(),
        sector: sector_display_name(c.sector),
        region: country
            .regions
            .iter()
            .find(|r| r.id == c.region_id)
            .map(|r| {
                if r.display_name.is_empty() {
                    r.id.clone()
                } else {
                    r.display_name.clone()
                }
            })
            .unwrap_or_else(|| c.region_id.clone()),
        legal_form: legal_form_display(&c.legal_form),
        ceo_vip_id: c.ceo_vip_id.clone(),
        ceo_name,
        ceo_ideology,
        union_id: c.union_id.clone(),
        fulfilled_fte: c.fulfilled_fte,
        fte_demand: c.target_fte_demand,
        average_wage: c.offered_wage_per_fte,
        seasonal_state: if c.furloughed_workers_count > 0.0 {
            "Furloughed".to_string()
        } else {
            "Active".to_string()
        },
        furloughed_workers_count: c.furloughed_workers_count,
        wage_arrears: c.wage_arrears,
        building_count: c.building_ids.len(),
        available_cash: c.available_cash,
        financial_summary,
    })
}

/// Build region drill-down detail on-demand for the selected region.
fn build_region_detail(
    country: &Country,
    buildings: &[crate::entities::Building],
    view: &ViewQuery,
) -> Option<RegionDetail> {
    let region_id = view.region_drilldown_id.as_ref()?;
    let region = country.regions.iter().find(|r| r.id == *region_id)?;

    // Build council factions from local governance.
    let council_factions = if let Some(ref gov) = region.governance {
        let fd = &gov.council.faction_distribution;
        vec![
            ("Populares".to_string(), fd.populares_count),
            ("Moderates".to_string(), fd.moderates_count),
            ("Optimates".to_string(), fd.optimates_count),
        ]
    } else {
        Vec::new()
    };

    // Phase 54: Total council seats (sum of all factions).
    let total_council_seats: u32 = council_factions.iter().map(|(_, s)| *s).sum();

    // Phase 54: Resolve head VIP ID from the registry by matching head name.
    let head_name_for_lookup = region
        .governance
        .as_ref()
        .map(|g| g.head.name.clone())
        .unwrap_or_default();
    let head_vip_id = if !head_name_for_lookup.is_empty() {
        country
            .politics
            .vip_registry
            .as_ref()
            .and_then(|r| r.get_by_name(&head_name_for_lookup).map(|v| v.id.clone()))
    } else {
        None
    };

    // Build budget/debt from local governance.
    let (
        budget_reserves,
        budget_tax_revenue,
        budget_property_tax,
        budget_expenditures,
        budget_balance,
        debt_total,
        debt_to_revenue_ratio,
        credit_rating,
        admin_status,
        head_name,
        head_type,
    ) = if let Some(ref gov) = region.governance {
        let rev = gov.budget.tax_revenue + gov.budget.property_tax;
        let balance = gov.budget.budget_balance;
        let ratio = if rev > 0.0 {
            gov.debt.total_debt / rev
        } else {
            0.0
        };
        (
            gov.budget.liquid_reserves,
            gov.budget.tax_revenue,
            gov.budget.property_tax,
            gov.budget.local_expenditures,
            balance,
            gov.debt.total_debt,
            ratio,
            gov.debt.credit_rating.clone(),
            format!("{:?}", gov.admin_status),
            gov.head.name.clone(),
            format!("{:?}", gov.head_type),
        )
    } else {
        (
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            String::new(),
            "None".to_string(),
            String::new(),
            "None".to_string(),
        )
    };

    // Active mandates (mandates are national, not per-region, so show all).
    // v4.19 WS5c: Show mandates targeting THIS region (new records carry a
    // real region_id), plus legacy unscoped national mandates.
    let active_mandates = country
        .politics
        .active_mandates
        .iter()
        .filter(|m| m.region_id.is_empty() || m.region_id == *region_id)
        .map(crate::ui::snapshot_regions::build_mandate_summary)
        .collect();

    // v4.20 WS4: Council legislative record — resolutions + roll calls are
    // persisted on `gov.council` by
    // `politics::local_legislation::process_regional_council_sessions`
    // (condition-driven agenda → faction/trait-weighted vote per real
    // councilor → recorded roll calls).
    let council_resolutions: Vec<CouncilResolutionRow> = region
        .governance
        .as_ref()
        .map(|g| {
            g.council
                .resolutions
                .iter()
                .map(|r| CouncilResolutionRow {
                    title: r.title.clone(),
                    resolution_type: r.resolution_type.clone(),
                    status: r.status.clone(),
                    votes_for: r.votes_for,
                    votes_against: r.votes_against,
                    abstentions: r.abstentions,
                    turn: r.turn,
                    region_id: String::new(),
                    description: council_resolution_description(
                        &r.resolution_type,
                        &r.status,
                        region_id,
                        &country.politics.active_mandates,
                    ),
                })
                .collect()
        })
        .unwrap_or_default();
    let council_roll_calls: Vec<CouncilRollCallRow> = region
        .governance
        .as_ref()
        .map(|g| {
            g.council
                .roll_calls
                .iter()
                .map(|rc| CouncilRollCallRow {
                    resolution_title: rc.resolution_title.clone(),
                    member_name: rc.member_name.clone(),
                    faction: rc.faction.clone(),
                    vote: rc.vote.clone(),
                    turn: rc.turn,
                    vip_id: rc.vip_id.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    // v4.21 WS3: Sitting councilors — the expandable faction summary lists
    // every member (including those with no recorded roll-call vote).
    let council_members = region
        .governance
        .as_ref()
        .map(|g| crate::ui::snapshot_regions::council_member_rows(&g.council, ""))
        .unwrap_or_default();

    // Infrastructure average condition.
    let infra_links: Vec<&crate::economy::transport_networks::NetworkLink> = country
        .transport_networks
        .links
        .values()
        .filter(|l| l.region_a == *region_id || l.region_b == *region_id)
        .collect();
    let infrastructure_avg_condition = if infra_links.is_empty() {
        0.0
    } else {
        infra_links.iter().map(|l| l.condition).sum::<f64>() / infra_links.len() as f64
    };

    // v4.19 WS5c: Real sector employment — sum `current_employment` (actual
    // staffed FTE) across the region's buildings, grouped by GDP sector.
    // The previous capacity_utilization proxy showed percentages, not jobs.
    let mut sector_fte: std::collections::BTreeMap<String, f64> =
        std::collections::BTreeMap::new();
    for building in buildings
        .iter()
        .filter(|b| b.region_id == *region_id)
    {
        *sector_fte.entry(format!("{:?}", building.sector)).or_insert(0.0) +=
            building.current_employment as f64;
    }
    let sector_employment: Vec<(String, f64)> = sector_fte.into_iter().collect();

    // Durable cohorts from class demographics.
    let durable_cohorts = region
        .class_demographics
        .rural_classes
        .values()
        .chain(region.class_demographics.urban_classes.values())
        .flat_map(|demo| {
            // `count` is stored per-capita; the UI summary reports absolute
            // units held by the class.
            let pop = demo.population.max(0) as f64;
            demo.household_durables
                .iter()
                .map(move |cohort| (cohort, pop))
        })
        .map(|(cohort, pop)| DurableCohortSummary {
            commodity: format!("{:?}", cohort.commodity),
            count: cohort.count * pop,
            avg_condition: cohort.condition,
            quality: cohort.quality,
            durability: cohort.durability,
        })
        .collect();

    Some(RegionDetail {
        region_id: region_id.clone(),
        display_name: region.display_name.clone(),
        development_level: region.development_level,
        admin_status,
        head_name,
        head_type,
        head_vip_id,
        council_factions,
        total_council_seats,
        budget_reserves,
        budget_tax_revenue,
        budget_property_tax,
        budget_expenditures,
        budget_balance,
        debt_total,
        debt_to_revenue_ratio,
        credit_rating,
        active_mandates,
        council_resolutions,
        council_roll_calls,
        council_members,
        infrastructure_avg_condition,
        sector_employment,
        durable_cohorts,
        thermal_grid: Some(ThermalGridSnapshot {
            pipe_network_km: region.thermal_grid.pipe_network_km,
            pipe_condition: region.thermal_grid.pipe_condition,
            loss_per_km: region.thermal_grid.loss_per_km,
            effective_heat_supply_gj: region
                .capacity_pool
                .get(&crate::infrastructure::CapacityType::DistrictHeating)
                .copied()
                .unwrap_or(0.0),
            max_connectable_buildings: region
                .thermal_grid
                .max_connectable_buildings(region.development_level),
            active_heating_plants: 0, // Computed from buildings at query time
            average_delivery_distance_km: region.thermal_grid.average_delivery_distance_km(1),
            transmission_loss_fraction: region.thermal_grid.transmission_loss(1),
        }),
        smog: Some(SmogSnapshot {
            smog_level: region.local_pollution.smog_level,
            standalone_emissions: region.local_pollution.standalone_emissions,
            centralized_emissions: region.local_pollution.centralized_emissions,
            industrial_emissions: region.local_pollution.industrial_emissions,
            mortality_multiplier: crate::environment::smog::smog_mortality_multiplier(
                region.local_pollution.smog_level,
            ),
        }),
        // Phase 83: Water reserve snapshot (role-gated by caller).
        water_reserves: Some(WaterReserveSnapshot {
            groundwater_volume: region.water_reserves.groundwater_volume,
            groundwater_quality: region.water_reserves.groundwater_quality,
            surface_water_volume: region.water_reserves.surface_water_volume,
            surface_water_quality: region.water_reserves.surface_water_quality,
        }),
        // Phase 83: Water grid snapshot (role-gated by caller).
        water_grid: Some(WaterGridSnapshot {
            pipe_network_km: region.water_network.pipe_network_km,
            pipe_condition: region.water_network.pipe_condition,
            current_quality: region.water_network.current_quality,
            throughput_liters: region.water_network.throughput_liters,
            effective_water_delivered: region.water_network.effective_water_delivered(0),
            max_connectable_buildings: region
                .water_network
                .max_connectable_buildings(region.development_level),
            active_water_plants: 0, // Computed from buildings at query time
            transmission_loss_fraction: region.water_network.transmission_loss(0),
        }),
        // Phase 83: Sewer grid snapshot (role-gated by caller).
        sewer_grid: Some(SewerGridSnapshot {
            pipe_network_km: region.sewer_network.pipe_network_km,
            pipe_condition: region.sewer_network.pipe_condition,
            current_quality: region.sewer_network.current_quality,
            throughput_liters: region.sewer_network.throughput_liters,
            water_delivered_to_treatment: region.sewer_network.water_delivered_to_treatment(0),
            leaked_water_mass: region.sewer_network.leaked_water_mass(0),
            max_connectable_buildings: region
                .sewer_network
                .max_connectable_buildings(region.development_level),
            active_wastewater_plants: 0, // Computed from buildings at query time
        }),
        // Phase 83: Biohazard snapshot (role-gated by caller).
        biohazard: Some(BiohazardSnapshot {
            biohazard_level: region.local_pollution.biohazard_level,
            standalone_biohazard: region.local_pollution.standalone_biohazard,
            sewage_overflow_biohazard: region.local_pollution.sewage_overflow_biohazard,
            industrial_biohazard: region.local_pollution.industrial_biohazard,
            low_quality_water_biohazard: region.local_pollution.low_quality_water_biohazard,
            mortality_multiplier: crate::environment::smog::biohazard_mortality_multiplier(
                region.local_pollution.biohazard_level,
            ),
        }),
        // Phase 84: Waste grid snapshot (role-gated by caller).
        waste_grid: Some(WasteGridSnapshot {
            collection_route_km: region.waste_grid.collection_route_km,
            route_condition: region.waste_grid.route_condition,
            collection_capacity: region.waste_grid.collection_capacity,
            total_uncollected: region.waste_grid.total_uncollected(),
            separation_efficiency: region.waste_grid.separation_efficiency,
            landfill_utilization: region.waste_grid.landfill_utilization,
            methane_capture_rate: region.waste_grid.methane_capture_rate,
        }),
        // Phase 84: Landfill snapshot (role-gated by caller — computed from
        // the first landfill building in the region, if any).
        landfill: None, // Populated by caller from buildings query
        // Phase 84: Waste pollution snapshot (public — pollution is public info).
        waste_pollution: Some(WastePollutionSnapshot {
            burning_emissions: region.local_pollution.waste_burning_emissions,
            dumping_biohazard: region.local_pollution.waste_dumping_biohazard,
            uncollected_biohazard: region.local_pollution.uncollected_waste_biohazard,
            leachate_leakage: 0.0, // Computed from landfill buildings
        }),
        // Phase 84: Recycling snapshot (role-gated by caller).
        recycling: None, // Populated by caller from buildings query
        // Phase 87+: Climate profile (public — geographic information).
        climate_profile: format!("{:?}", region.climate_profile),
        // Phase 87+: Arable land (public — geographic information).
        arable_land_max: region.arable_land_max,
        arable_land_used: region.arable_land_used,
        // Phase 87+: Geological deposits from region resources.
        // Role-gated by caller — undiscovered deposits filtered for foreign observers.
        geological_deposits: build_geological_deposit_rows(region, buildings),
    })
}

/// Phase 87+/88/89: Build geological deposit rows from a region's resources map.
/// Extracts geological reserves information for the Resources tab.
///
/// Phase 88: Resource keys are now vein IDs (not commodity strings), so the
/// commodity must be read from the value object's "commodity" field.
/// Non-geological resources (freshwater, forests) are skipped because they
/// don't have a "commodity" field.
///
/// Phase 89: `active_mine_count` is now computed by counting buildings in this
/// region whose `deposit_id` matches the resource key (vein ID or composite ID).
/// This is the authoritative count — the old approach read a field from the
/// resource JSON that was never written, always returning 0.
fn build_geological_deposit_rows(
    region: &crate::society::geography::Region,
    buildings: &[crate::entities::Building],
) -> Vec<GeologicalDepositRow> {
    let mut rows = Vec::new();
    for (resource_key, value) in &region.resources {
        if let serde_json::Value::Object(map) = value {
            // Phase 88: Skip non-geological resources (no "commodity" field).
            let commodity = match map.get("commodity").and_then(|v| v.as_str()) {
                Some(c) => c.to_string(),
                None => continue, // freshwater, forests, etc.
            };
            let reserves = map
                .get("geological_reserves")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            if reserves <= 0.0 {
                continue;
            }
            let current = map
                .get("current_reserves")
                .and_then(|v| v.as_f64())
                .unwrap_or(reserves)
                .clamp(0.0, reserves);
            let extraction_rate = map
                .get("extraction_rate")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                .max(0.0);
            let formation_name = map
                .get("formation_name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .to_string();
            // Phase 89 + v4.19: Match mines by `deposit_id` against every
            // vein-id form — worldgen may key this resource entry by vein.id
            // while buildings store composite_id (or vice versa). The
            // per-turn sync stamps both forms on the entry.
            let vein_id = map.get("vein_id").and_then(|v| v.as_str());
            let composite_id = map.get("composite_id").and_then(|v| v.as_str());
            let mines: Vec<&crate::entities::Building> = buildings
                .iter()
                .filter(|b| {
                    b.region_id == region.id
                        && b.deposit_id.as_deref().is_some_and(|d| {
                            d == resource_key.as_str()
                                || Some(d) == vein_id
                                || Some(d) == composite_id
                        })
                })
                .collect();
            let active_mines = mines.len() as u32;
            // v4.19 WS5: Utilization = actual extraction / nameplate mine
            // capacity. Per-mine capacity mirrors the production path:
            // output_per_1k × (worker_capacity × scale / 1000) × efficiency
            // for the vein's commodity. Depletion % remains visible via
            // Estimated vs Current reserves.
            let capacity: f64 = mines
                .iter()
                .map(|b| {
                    let per_1k = b
                        .active_method
                        .outputs
                        .iter()
                        .find(|(c, _)| format!("{:?}", c) == commodity)
                        .map(|(_, q)| *q)
                        .unwrap_or(0.0);
                    let cap_fte = (b.worker_capacity as f64)
                        * (b.scale_factor.max(1) as f64);
                    per_1k * (cap_fte / 1000.0) * b.active_method.efficiency.max(0.0)
                })
                .sum();
            let utilization = if capacity > 0.0 {
                (extraction_rate / capacity).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let discovered = map
                .get("discovered")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            rows.push(GeologicalDepositRow {
                commodity,
                formation_name,
                estimated_reserves: reserves,
                current_reserves: current,
                extraction_rate,
                utilization_rate: utilization,
                active_mine_count: active_mines,
                discovered,
            });
        }
    }
    rows
}

/// Phase 53: Build megaregion drill-down detail on-demand for the selected megaregion.
fn build_megaregion_detail(
    country: &Country,
    buildings: &[crate::entities::Building],
    view: &ViewQuery,
) -> Option<MegaregionDetail> {
    let megaregion_id = view.megaregion_drilldown_id.as_ref()?;
    let megaregion = country
        .megaregions
        .iter()
        .find(|m| m.id == *megaregion_id)?;

    // Aggregate population and GDP from member regions.
    let member_regions: Vec<&crate::society::geography::Region> = country
        .regions
        .iter()
        .filter(|r| megaregion.regions.contains(&r.id))
        .collect();
    let total_population: i64 = member_regions.iter().map(|r| r.population).sum();
    let total_gdp: f64 = member_regions.iter().map(|r| r.gdp).sum();

    // Extract governance fields if available.
    let (
        governor_name,
        governor_appointed,
        competence_level,
        budget_reserves,
        regional_transfers,
        development_expenditures,
        coordination_expenditures,
        budget_balance,
    ) = if let Some(ref gov) = megaregion.governance {
        (
            gov.governor.name.clone(),
            gov.governor_appointed,
            format!("{:?}", gov.competence_level),
            gov.budget.liquid_reserves,
            gov.budget.regional_transfers,
            gov.budget.development_expenditures,
            gov.budget.coordination_expenditures,
            gov.budget.budget_balance,
        )
    } else {
        (
            String::new(),
            false,
            "None".to_string(),
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
        )
    };

    // v4.20 WS4: Aggregate council legislative records from member regions
    // (member order is the megaregion's stored region list — deterministic).
    let mut council_resolutions: Vec<CouncilResolutionRow> = Vec::new();
    let mut council_roll_calls: Vec<CouncilRollCallRow> = Vec::new();
    let mut council_members: Vec<crate::ui::snapshot_regions::CouncilorRow> = Vec::new();
    for region in &member_regions {
        if let Some(ref gov) = region.governance {
            council_resolutions.extend(gov.council.resolutions.iter().map(|r| {
                CouncilResolutionRow {
                    title: r.title.clone(),
                    resolution_type: r.resolution_type.clone(),
                    status: r.status.clone(),
                    votes_for: r.votes_for,
                    votes_against: r.votes_against,
                    abstentions: r.abstentions,
                    turn: r.turn,
                    region_id: region.id.clone(),
                    description: council_resolution_description(
                        &r.resolution_type,
                        &r.status,
                        &region.id,
                        &country.politics.active_mandates,
                    ),
                }
            }));
            council_roll_calls.extend(gov.council.roll_calls.iter().map(|rc| {
                CouncilRollCallRow {
                    resolution_title: rc.resolution_title.clone(),
                    member_name: rc.member_name.clone(),
                    faction: rc.faction.clone(),
                    vote: rc.vote.clone(),
                    turn: rc.turn,
                    vip_id: rc.vip_id.clone(),
                }
            }));
            // v4.21 WS3: the megaregional Assembly's membership is the union
            // of member-region councils — real governance state, tagged by
            // home region so the UI can group assembly members.
            council_members.extend(
                crate::ui::snapshot_regions::council_member_rows(&gov.council, &region.id),
            );
        }
    }

    // v4.21 WS3: Assembly composition — faction seat totals aggregated
    // from member-region councils (real `LocalCouncil.faction_distribution`,
    // not fabricated assembly seats).
    let council_factions = crate::ui::snapshot_regions::aggregate_faction_seats(
        member_regions
            .iter()
            .filter_map(|r| r.governance.as_ref().map(|g| &g.council)),
    );
    let total_council_seats: u32 = council_factions.iter().map(|(_, s)| *s).sum();

    // v4.21 WS3: Sector employment across member regions — Σ
    // building.current_employment per GDP sector, same measure as the
    // region-level `sector_employment` tab.
    let mut sector_fte: std::collections::BTreeMap<String, f64> =
        std::collections::BTreeMap::new();
    for building in buildings
        .iter()
        .filter(|b| megaregion.regions.contains(&b.region_id))
    {
        *sector_fte.entry(format!("{:?}", building.sector)).or_insert(0.0) +=
            building.current_employment as f64;
    }
    let sector_employment: Vec<(String, f64)> = sector_fte.into_iter().collect();

    // v4.21 WS3: Mandates targeting member regions (legacy unscoped
    // national records included — they apply to every governed region).
    let active_mandates: Vec<MandateSummary> = country
        .politics
        .active_mandates
        .iter()
        .filter(|m| m.region_id.is_empty() || megaregion.regions.contains(&m.region_id))
        .map(crate::ui::snapshot_regions::build_mandate_summary)
        .collect();

    Some(MegaregionDetail {
        megaregion_id: megaregion.id.clone(),
        display_name: if megaregion.name.is_empty() {
            megaregion.id.clone()
        } else {
            megaregion.name.clone()
        },
        country: megaregion.country.clone(),
        member_region_ids: megaregion.regions.clone(),
        member_region_count: megaregion.regions.len(),
        total_population,
        total_gdp,
        governor_name,
        governor_appointed,
        competence_level,
        budget_reserves,
        regional_transfers,
        development_expenditures,
        coordination_expenditures,
        budget_balance,
        council_resolutions,
        council_roll_calls,
        council_factions,
        total_council_seats,
        council_members,
        sector_employment,
        active_mandates,
    })
}

/// Build advisory council snapshot for autocracies.
fn build_advisory_council_snapshot(country: &Country) -> Option<AdvisoryCouncilSnapshot> {
    let council = country.politics.advisory_council.as_ref()?;
    let registry = &country.politics.vip_registry;
    let members = council
        .members
        .iter()
        .map(|m| {
            let name = registry
                .as_ref()
                .and_then(|r| r.get(&m.vip_id))
                .map(|v| v.full_name.clone())
                .unwrap_or_else(|| m.vip_id.clone());
            CouncilMemberRow {
                vip_id: m.vip_id.clone(),
                name,
                faction: m.faction.clone(),
                loyalty: m.loyalty,
                influence: m.influence as u32,
            }
        })
        .collect();
    Some(AdvisoryCouncilSnapshot {
        council_type: format!("{:?}", council.council_type),
        members,
        aggregate_loyalty: council.aggregate_loyalty,
        coup_risk_threshold: council.coup_risk_threshold,
        coup_cooldown_until_turn: council.coup_cooldown_until_turn,
    })
}

/// Build royal dynasty snapshot for monarchies.
fn build_royal_dynasty_snapshot(country: &Country) -> Option<RoyalDynastySnapshot> {
    let dynasty = country.politics.royal_dynasty.as_ref()?;
    let registry = &country.politics.vip_registry;
    let resolve_name = |vip_id: &str| -> String {
        registry
            .as_ref()
            .and_then(|r| r.get(vip_id))
            .map(|v| v.full_name.clone())
            .unwrap_or_else(|| vip_id.to_string())
    };
    let members = dynasty
        .members
        .iter()
        .map(|m| {
            let vip = registry.as_ref().and_then(|r| r.get(&m.vip_id));
            DynastyMemberRow {
                vip_id: m.vip_id.clone(),
                name: resolve_name(&m.vip_id),
                relation: format!("{:?}", m.relation),
                age: vip.map(|v| v.age).unwrap_or(0),
                succession_order: m.succession_order,
                is_heir_apparent: m.is_heir_apparent,
                // Phase 86: Genealogy links.
                father_vip_id: m.father_vip_id.clone(),
                mother_vip_id: m.mother_vip_id.clone(),
                spouse_vip_id: m.spouse_vip_id.clone(),
                children_vip_ids: m.children_vip_ids.clone(),
                is_dead: vip.map(|v| v.is_dead).unwrap_or(false),
                death_cause: m.death_cause.as_ref().map(|c| format!("{:?}", c)),
                // Phase 92: Gender and portrait seed for visual indicators.
                gender: vip.map(|v| v.gender.clone()).unwrap_or_default(),
                portrait_seed: vip.map(|v| v.portrait_seed.clone()).unwrap_or_default(),
            }
        })
        .collect();
    let current_monarch_name = dynasty
        .current_monarch_id
        .as_ref()
        .map(|id| resolve_name(id))
        .unwrap_or_default();
    let current_regent_name = dynasty
        .current_regent_id
        .as_ref()
        .map(|id| resolve_name(id))
        .unwrap_or_default();

    // Phase 86: Build marriage and birth event history rows.
    let marriage_history = dynasty
        .marriage_history
        .iter()
        .map(|m| MarriageEventRow {
            turn: m.turn,
            spouse1_name: resolve_name(&m.spouse1_vip_id),
            spouse2_name: resolve_name(&m.spouse2_vip_id),
            significance: format!("{:?}", m.political_significance),
            foreign_dynasty: m.foreign_dynasty.clone(),
        })
        .collect();
    let birth_history = dynasty
        .birth_history
        .iter()
        .map(|b| BirthEventRow {
            turn: b.turn,
            child_name: resolve_name(&b.child_vip_id),
            father_name: resolve_name(&b.father_vip_id),
            mother_name: resolve_name(&b.mother_vip_id),
            is_legitimate: b.is_legitimate,
        })
        .collect();

    Some(RoyalDynastySnapshot {
        dynasty_name: dynasty.dynasty_name.clone(),
        current_monarch_id: dynasty.current_monarch_id.clone(),
        current_monarch_name,
        current_regent_id: dynasty.current_regent_id.clone(),
        current_regent_name,
        regency_active: dynasty.regency_active,
        members,
        marriage_history,
        birth_history,
    })
}

/// Phase 35: Build the Finance tab snapshot from existing country data.
fn build_finance_snapshot(country: &Country, companies: &[Company]) -> FinanceSnapshot {
    let macro_data = &country.macro_indicators;
    let treasury_reserves = country.budget.liquid_reserves;
    let gdp = country.budget.gdp;

    // Ministry totals
    let (ministry_total_allocated, ministry_total_spent, ministry_total_cash) =
        if let Some(ref config) = country.politics.ministry_config {
            config
                .ministries
                .iter()
                .fold((0.0, 0.0, 0.0), |(a, s, c), m| {
                    (
                        a + m.allocated_cash,
                        s + crate::politics::ministries::ministry_spent(m),
                        c + m.ministry_cash,
                    )
                })
        } else {
            (0.0, 0.0, 0.0)
        };

    // Central Bank
    let cb = &country.central_bank;
    let m0 = macro_data.money_supply.m0;
    let m3 = macro_data.money_supply.m3;

    // Banking aggregates
    let mut total_bank_reserves = 0.0_f64;
    let mut total_bank_deposits = 0.0_f64;
    let mut total_bank_loans = 0.0_f64;
    let mut total_consumer_debt = 0.0_f64;
    let mut dspw_bank_count = 0_u32;
    for c in companies {
        if c.bank_type.is_some() {
            if let Some(ref bs) = c.balance_sheet {
                total_bank_reserves += bs.reserves_at_central_bank;
                total_bank_deposits += bs.deposits;
                total_bank_loans += bs
                    .loans_issued
                    .iter()
                    .map(|l| l.outstanding_balance)
                    .sum::<f64>();
            }
            total_consumer_debt += c
                .consumer_loans
                .iter()
                .map(|l| l.outstanding_principal)
                .sum::<f64>();
            if c.is_dspw {
                dspw_bank_count += 1;
            }
        }
    }

    // Phase 87+: Public debt — use the authoritative debt_market as the single
    // source of truth. The previous code double-counted retail bonds by adding
    // retail_bonds_total to debt_market_total (which already includes retail).
    // Holder breakdown is now computed from debt_market.outstanding_securities
    // holders, not stale entity-level aggregates.
    let total_public_debt = country.debt_market.total_outstanding_debt;
    let weighted_avg_interest_rate = country.debt_market.weighted_avg_interest_rate;

    // Phase 87+: Compute holder breakdown from the authoritative security holder ledger.
    use crate::economy::finance::debt_market::SecurityHolderType;
    let mut debt_held_by_banks = 0.0_f64;
    let mut debt_held_by_central_bank = 0.0_f64;
    let mut debt_held_by_funds = 0.0_f64;
    let mut debt_held_by_citizens = 0.0_f64;
    let mut debt_held_by_foreign = 0.0_f64;
    for security in &country.debt_market.outstanding_securities {
        if security.is_matured {
            continue;
        }
        for holder in &security.holders {
            match holder.holder_type {
                SecurityHolderType::CommercialBank
                | SecurityHolderType::UniversalBank
                | SecurityHolderType::InvestmentBank
                | SecurityHolderType::PrimaryDealer => {
                    debt_held_by_banks += holder.quantity;
                }
                SecurityHolderType::CentralBank => {
                    debt_held_by_central_bank += holder.quantity;
                }
                SecurityHolderType::OpenEndFund
                | SecurityHolderType::ClosedEndFund
                | SecurityHolderType::HedgeFund => {
                    debt_held_by_funds += holder.quantity;
                }
                SecurityHolderType::RetailSavingsBond => {
                    debt_held_by_citizens += holder.quantity;
                }
                SecurityHolderType::ForeignEntity => {
                    debt_held_by_foreign += holder.quantity;
                }
            }
        }
    }
    // Add retail savings bonds (separate non-tradable instrument, not in outstanding_securities).
    // total_outstanding_debt = wholesale (outstanding_securities) + retail (retail_bonds).
    // The holder iteration above counts citizen-held wholesale securities;
    // this adds the retail savings bond principal + arrears.
    let retail_total: f64 = country
        .debt_market
        .retail_bonds
        .iter()
        .map(|b| b.face_value + b.arrears)
        .sum();
    debt_held_by_citizens += retail_total;

    // Phase 87+: Debug invariant — holder sum must equal total debt.
    let holder_sum = debt_held_by_banks
        + debt_held_by_central_bank
        + debt_held_by_funds
        + debt_held_by_citizens
        + debt_held_by_foreign;
    debug_assert!(
        (holder_sum - total_public_debt).abs() < 1.0,
        "Debt holder sum {} != total_public_debt {}",
        holder_sum,
        total_public_debt
    );

    // Shadow economy
    let shadow_gdp = macro_data.gdp_breakdown.shadow_gdp;
    let pit_evaded = country
        .politics
        .shadow_economy_state
        .as_ref()
        .map(|s| s.total_pit_evaded)
        .unwrap_or(0.0);

    // Phase 38: Read last tax collection result for the Finance tab.
    let last_tax = country.last_tax_result.as_ref();
    let pit_revenue = last_tax.map(|t| t.pit_collected).unwrap_or(0.0);
    let cit_revenue = last_tax.map(|t| t.cit_collected).unwrap_or(0.0);
    let vat_revenue = last_tax.map(|t| t.vat_collected).unwrap_or(0.0);
    let wealth_tax_revenue = last_tax.map(|t| t.wealth_tax_collected).unwrap_or(0.0);
    let capital_gains_revenue = last_tax
        .map(|t| t.capital_gains_tax_collected)
        .unwrap_or(0.0);
    // Phase 39: Customs and state property revenue
    let customs_revenue = last_tax.map(|t| t.customs_revenue).unwrap_or(0.0);
    let state_property_revenue = last_tax.map(|t| t.state_property_revenue).unwrap_or(0.0);
    // Agent 4 — Phase 6: Smuggling diagnostics for UI exposure (Rule 17).
    let smuggling_result = country.last_smuggling_result.as_ref();
    let smuggling_value = smuggling_result.map(|r| r.smuggling_value).unwrap_or(0.0);
    let smuggling_intercepted = smuggling_result.map(|r| r.intercepted_value).unwrap_or(0.0);
    let smuggling_tariff_loss = smuggling_result.map(|r| r.tariff_loss).unwrap_or(0.0);
    let smuggling_recovered_tariffs = smuggling_result.map(|r| r.recovered_tariffs).unwrap_or(0.0);
    // Phase 89: Property tax revenue aggregated from regional governments.
    let property_tax_revenue = last_tax.map(|t| t.property_tax_collected).unwrap_or(0.0);

    // Phase 38: Read tax rates for display.
    let pit_rate = country.tax_rates.income_tax.rate;
    let cit_rate = country.tax_rates.corporate_tax;
    let vat_rate = {
        // Average of all VAT brackets
        let brackets: Vec<f64> = country.tax_rates.vat.values().map(|b| b.rate).collect();
        if brackets.is_empty() {
            0.0
        } else {
            brackets.iter().sum::<f64>() / brackets.len() as f64
        }
    };
    let wealth_tax_rate = country
        .tax_rates
        .wealth_tax
        .brackets
        .last()
        .map(|b| b.rate)
        .unwrap_or(0.0);
    let capital_gains_rate = country
        .tax_rates
        .capital_gains_tax
        .brackets
        .last()
        .map(|b| b.rate)
        .unwrap_or(0.0);

    // Phase 42: FX basket — top 3 foreign currencies by reserve amount.
    let domestic_ccy = &country.macro_indicators.currency;
    let mut fx_entries: Vec<(String, f64)> = cb
        .fx_reserves
        .iter()
        .filter(|(k, _)| *k != domestic_ccy)
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    fx_entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let fx_basket: Vec<FxBasketEntry> = fx_entries
        .iter()
        .take(3)
        .map(|(cur, amt)| FxBasketEntry {
            currency: cur.clone(),
            amount: *amt,
            exchange_rate: 1.0, // Placeholder — exchange rates loaded from currencies.json at runtime
        })
        .collect();

    FinanceSnapshot {
        treasury_reserves,
        gdp,
        ministry_total_allocated,
        ministry_total_spent,
        ministry_total_cash,
        pit_revenue,
        cit_revenue,
        vat_revenue,
        wealth_tax_revenue,
        capital_gains_revenue,
        customs_revenue,
        state_property_revenue,
        smuggling_value,
        smuggling_intercepted,
        smuggling_tariff_loss,
        smuggling_recovered_tariffs,
        property_tax_revenue,
        pit_rate,
        cit_rate,
        vat_rate,
        wealth_tax_rate,
        capital_gains_rate,
        total_public_debt,
        // v4.21 WS2: real sovereign debt service — the EWMA run-rate of
        // coupon+principal payments recorded by `process_debt_service`
        // each turn (raw per-turn figure is lumpy on annual coupons; the
        // EWMA is its honest run-rate). Falls back to 0 only when the
        // engine has never serviced debt.
        debt_service: country
            .budget
            .extra
            .get("debt_service_run_rate")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        weighted_avg_interest_rate,
        debt_held_by_banks,
        debt_held_by_central_bank,
        debt_held_by_funds,
        debt_held_by_citizens,
        debt_held_by_foreign,
        m0,
        m3,
        cb_reference_rate: cb.interest_rates.reference_rate,
        cb_lombard_rate: cb.interest_rates.lombard_rate,
        cb_discount_rate: cb.interest_rates.discount_rate,
        cb_rediscount_rate: cb.interest_rates.rediscount_rate,
        cb_deposit_rate: cb.interest_rates.deposit_rate,
        cb_fx_reserves_total: cb.fx_reserves.values().sum(),
        cb_gold_reserves: cb.physical_gold_reserves,
        cb_reserve_requirement_ratio: cb.reserve_requirement_ratio,
        cb_omo_holdings: cb.omo_bond_holdings,
        cb_liquidity_injected: cb.liquidity_injected,
        cb_last_omo_turn: cb.omo_last_operation_turn,
        cb_last_omo_amount: cb.omo_last_operation_amount,
        total_bank_reserves,
        total_bank_deposits,
        total_bank_loans,
        total_consumer_debt,
        dspw_bank_count,
        shadow_gdp,
        pit_evaded,
        fx_basket,
        ministry_expenditure_breakdown: build_ministry_expenditure_breakdown(country),
    }
}

/// Phase 54: Build ministry expenditure breakdown from ministry config.
/// Aggregates spent cash by ministry into categories for the Finance tab.
fn build_ministry_expenditure_breakdown(country: &Country) -> Vec<MinistryExpenditureEntry> {
    let config = match &country.politics.ministry_config {
        Some(c) => c,
        None => return Vec::new(),
    };

    let mut entries: Vec<(String, f64)> = config
        .ministries
        .iter()
        .map(|m| {
            let category = m
                .name
                .strip_prefix("Ministry of ")
                .unwrap_or(&m.name)
                .to_string();
            // v4.18 WS3: read through `ministry_spent` — the floored
            // accessor — so the Finance tab can never show a negative
            // spend figure even if a refund path drifts the raw field.
            (category, crate::politics::ministries::ministry_spent(m))
        })
        .collect();

    // Sort by amount descending.
    entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let total: f64 = entries.iter().map(|(_, amt)| *amt).sum();
    entries
        .into_iter()
        .map(|(category, amount)| {
            let share_pct = if total > 0.0 {
                (amount / total) * 100.0
            } else {
                0.0
            };
            MinistryExpenditureEntry {
                category,
                amount,
                share_pct,
            }
        })
        .collect()
}

/// Phase 32: Build the Government tab snapshot.
fn build_government_snapshot(country: &Country, year: u32) -> GovernmentSnapshot {
    let politics = &country.politics;

    // Head of State.
    let head_of_state_name = if !politics.head_of_state.name.is_empty() {
        politics.head_of_state.name.clone()
    } else {
        "(vacant)".to_string()
    };

    // PM from the canonical cabinet party — v4.18 WS3: resolve through
    // `canonical_pm_party_id` (same accessor `form_government` and
    // `parliament::build_vips` use) so the header PM can never disagree with
    // the cabinet record or the Key VIPs PrimeMinister row (production:
    // header "Paolo Ricci" vs VIP "Fernando Mancini").
    let canonical_pm_party = crate::politics::ministries::canonical_pm_party_id(politics);
    let ruling_party = politics.active_parties.get(canonical_pm_party);
    let pm_name = ruling_party
        .map(|p| {
            if !p.leader.name.is_empty() {
                p.leader.name.clone()
            } else {
                // Phase 34: Fallback to party name instead of "(unnamed)".
                format!("Leader ({})", canonical_pm_party)
            }
        })
        .unwrap_or_else(|| "(vacant)".to_string());

    // Ideology Step 6: Derive PM ideology label from `classify()` using the
    // ruling party's authoritative coordinates (not the stored string).
    let pm_ideology = ruling_party
        .map(|p| classify(p.coordinates, year).0.to_string())
        .unwrap_or_default();

    // Ideology Step 6: Ruling party coordinates for radar chart (Directive 17).
    let pm_coordinates = ruling_party.map(|p| IdeologyCoordinatesDto::from(p.coordinates));

    // Cabinet from ministry_config.
    let cabinet: Vec<MinisterRow> = if let Some(ref config) = politics.ministry_config {
        config
            .ministries
            .iter()
            .map(|m| {
                let minister_party = politics.active_parties.get(&m.minister_party);
                // Ideology Step 6: Derive ideology label from `classify()` using
                // the minister's party coordinates (not the stored string).
                let ideology = minister_party
                    .map(|p| classify(p.coordinates, year).0.to_string())
                    .unwrap_or_default();
                // Ideology Step 6: Minister party coordinates for radar chart.
                let minister_coords = minister_party
                    .map(|p| IdeologyCoordinatesDto::from(p.coordinates));
                MinisterRow {
                    // Phase 34: Strip "Ministry of " prefix for cleaner UI display.
                    ministry_name: m
                        .name
                        .clone()
                        .strip_prefix("Ministry of ")
                        .unwrap_or(&m.name)
                        .to_string(),
                    // Phase 34: Use resolve_minister_name fallback instead of "(unnamed)".
                    minister_name: if m.minister_name.is_empty() {
                        // Try to get the party leader's name as fallback.
                        politics
                            .active_parties
                            .get(&m.minister_party)
                            .map(|p| {
                                if !p.leader.name.is_empty() {
                                    p.leader.name.clone()
                                } else {
                                    format!("Minister ({})", m.minister_party)
                                }
                            })
                            .unwrap_or_else(|| format!("Minister ({})", m.minister_party))
                    } else {
                        m.minister_name.clone()
                    },
                    party: m.minister_party.clone(),
                    ideology,
                    coordinates: minister_coords,
                    allocated_cash: m.allocated_cash,
                    // v4.18 WS3: floored accessor — `SPENT` is a real
                    // outflow total and can never render negative.
                    spent_cash: crate::politics::ministries::ministry_spent(m),
                    ministry_cash: m.ministry_cash,
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    // State of Emergency.
    let state_of_emergency = politics.state_of_emergency.as_ref().and_then(|soe| {
        if soe.active {
            Some(EmergencySnapshot {
                active: true,
                reason: soe.reason.clone(),
                turns_remaining: soe.turns_remaining,
                parliament_suspended: soe.parliament_suspended,
            })
        } else {
            None
        }
    });

    // v4.22.1 WS2.2: resolve the head of state's VIP id so the UI can link
    // the name to the full dossier.
    let hos_name = politics.head_of_state.name.clone();
    let head_of_state_vip_id = country
        .politics
        .vip_registry
        .as_ref()
        .and_then(|r| {
            r.vips
                .values()
                .find(|v| !v.is_dead && v.full_name == hos_name)
                .map(|v| v.id.clone())
        });

    GovernmentSnapshot {
        head_of_state_name,
        head_of_state_vip_id,
        head_of_state_role: politics.government_form.display_name().to_string(),
        pm_name,
        // v4.18 WS3: report the canonical PM party (same source as pm_name).
        pm_party: canonical_pm_party.to_string(),
        pm_ideology,
        coordinates: pm_coordinates,
        cabinet,
        state_of_emergency,
        political_capital: politics.political_capital,
        // Phase 41: VIPs moved from Parliament to Government tab.
        vips: build_vip_rows(country, year),
        // Phase 54: Government form + royal dynasty for monarchy sub-tab.
        government_form: politics.government_form.display_name().to_string(),
        royal_dynasty: build_royal_dynasty_snapshot(country),
        // D.7.1: Inspectorate stats from InspectorateState
        inspectorate_sanepid_capacity: politics.inspectorate_state.as_ref().map(|ist| ist.sanepid_capacity).unwrap_or(0.0),
        inspectorate_building_capacity: politics.inspectorate_state.as_ref().map(|ist| ist.building_inspectorate_capacity).unwrap_or(0.0),
        inspectorate_environmental_capacity: politics.inspectorate_state.as_ref().map(|ist| ist.environmental_inspectorate_capacity).unwrap_or(0.0),
        inspectorate_labor_capacity: politics.inspectorate_state.as_ref().map(|ist| ist.labor_inspection_capacity).unwrap_or(0.0),
        inspectorate_violations_detected: politics.inspectorate_state.as_ref().map(|ist| ist.violations_detected).unwrap_or(0),
        inspectorate_fines_issued: politics.inspectorate_state.as_ref().map(|ist| ist.fines_issued).unwrap_or(0.0),
        inspectorate_corruption_index: politics.inspectorate_state.as_ref().map(|ist| ist.corruption_index).unwrap_or(0.0),
        inspectorate_bribes_accepted: politics.inspectorate_state.as_ref().map(|ist| ist.bribes_accepted_this_turn).unwrap_or(0),
        inspectorate_bribes_total_value: politics.inspectorate_state.as_ref().map(|ist| ist.bribes_total_value).unwrap_or(0.0),
        inspectorate_pip_fleet_range_km: politics.inspectorate_state.as_ref().map(|ist| ist.pip_fleet_range_km).unwrap_or(0.0),
    }
}

/// Phase 41: Build VIP rows from the parliament struct for the Government tab.
fn build_vip_rows(country: &Country, year: u32) -> Vec<VipRow> {
    if let Some(ref parl) = country.politics.parliament_struct {
        parl.vips
            .iter()
            .map(|v| {
                // Ideology Step 6: Derive ideology label from `classify()` using
                // the VIP's authoritative coordinates (not the stored string).
                let ideology = classify(v.coordinates, year).0.to_string();
                VipRow {
                    full_name: v.full_name.clone(),
                    party: v.party.clone(),
                    role: format!("{:?}", v.role),
                    ideology,
                    // Ideology Step 6: VIP coordinates for radar chart (Directive 17).
                    coordinates: Some(IdeologyCoordinatesDto::from(v.coordinates)),
                    age: v.age,
                }
            })
            .collect()
    } else {
        Vec::new()
    }
}

/// v4.18: Convert a `BillDetail` (recorded at vote time or projected live
/// for queued bills) into the flat UI row shape.
fn bill_detail_row(d: &crate::politics::parliament::BillDetail) -> BillDetailRow {
    BillDetailRow {
        initiator: d.initiator.clone(),
        bill_type: d.bill_type.clone(),
        weight: d.weight.clone(),
        stage: d.stage.clone(),
        introduction_turn: d.introduction_turn,
        enacted_turn: d.enacted_turn,
        earliest_enactment_turn: d.earliest_enactment_turn,
        committee: d.committee.clone(),
        provisions: d
            .provisions
            .iter()
            .map(|p| ProvisionRow {
                description: p.description.clone(),
                kind: p.kind.clone(),
                detail: p.detail.clone(),
                budget_impact: p.budget_impact,
                sunset_turn: p.sunset_turn,
                mutated: p.mutated,
                mutation_notes: p.mutation_notes.clone(),
            })
            .collect(),
        // v4.20: legacy saves carry empty badge/tag — fall back to the stage
        // label / bill-type mapping so the drawer never shows blanks.
        status_badge: if d.status_badge.is_empty() {
            d.stage.clone()
        } else {
            d.status_badge.clone()
        },
        theme_tag: if d.theme_tag.is_empty() {
            bill_theme_tag_from_type(&d.bill_type)
        } else {
            d.theme_tag.clone()
        },
    }
}

/// v4.20: Theme tag for a `BillType::as_str()` id — same vocabulary as
/// `BillType::theme_tag()` ("Budget", "Justice", "Labor",
/// "Infrastructure", "Education", "Defense", "Other").
fn bill_theme_tag_from_type(bill_type: &str) -> String {
    match bill_type {
        "budget" | "emergency_fiscal" | "emergency_bonds" | "tax_reform" => "Budget",
        "justice" | "sentencing" => "Justice",
        "labor" => "Labor",
        "infrastructure" | "transport" => "Infrastructure",
        "education" => "Education",
        _ => "Other",
    }
    .to_string()
}

/// Phase 32: Build the Parliament tab snapshot.
fn build_parliament_snapshot(country: &Country, year: u32) -> ParliamentSnapshot {
    let politics = &country.politics;

    if let Some(ref parl) = politics.parliament_struct {
        // Chambers. v4.20: seat distributions are sorted (seats desc, id asc)
        // — HashMap iteration order must not leak into serialized snapshots.
        let chambers: Vec<ChamberSnapshot> = parl
            .chambers
            .iter()
            .map(|c| {
                let mut seat_distribution: Vec<(String, u32)> =
                    c.seats.iter().map(|(k, v)| (k.clone(), *v)).collect();
                seat_distribution
                    .sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                ChamberSnapshot {
                    name: c.name.clone(),
                    total_seats: c.total_seats,
                    speaker_name: c.presidium.speaker.full_name.clone(),
                    speaker_club: c.presidium.speaker_club.clone(),
                    seat_distribution,
                }
            })
            .collect();

        // Clubs.
        let clubs: Vec<ClubRow> = parl
            .clubs
            .iter()
            .map(|c| {
                // Ideology Step 6: Derive ideology label from `classify()` using
                // the club's authoritative coordinates (not the stored string).
                let ideology = classify(c.coordinates, year).0.to_string();
                // v4.20 T2: persistent party color/icon — parent party's stored
                // identity, or a deterministic derivation for splinter clubs.
                let (color_hex, ideology_icon) =
                    crate::politics::system::club_visual_identity(
                        &c.id,
                        &ideology,
                        &c.coordinates,
                        &politics.active_parties,
                    );
                ClubRow {
                    name: c.name.clone(),
                    seats: c.seats,
                    ideology,
                    // Ideology Step 6: Club/party coordinates for radar chart (Directive 17).
                    coordinates: Some(IdeologyCoordinatesDto::from(c.coordinates)),
                    is_splinter: c.is_splinter,
                    discipline: c.discipline,
                    chairperson_id: c.chairperson_id.clone(),
                    chairperson_name: c.chairperson_name.clone(),
                    color_hex,
                    ideology_icon,
                    // v4.21 WS1: expandable Clubs-tab sections — controlled
                    // offices, sponsored bills, and per-vote whip/rebel data,
                    // all resolved from real state below.
                    offices: club_offices(c, parl, politics),
                    initiatives: club_initiatives(c, parl, politics),
                    vote_history: club_vote_history(c, parl, politics),
                }
            })
            .collect();

        // Recent votes from BOTH chambers — each VoteRecord is tagged with
        // the chamber that cast it so the UI can show lower/upper tallies.
        let recent_votes: Vec<VoteRow> = parl
            .chambers
            .iter()
            .flat_map(|c| {
                c.recent_votes.iter().map(move |v| VoteRow {
                    bill_id: v.bill_id.clone(),
                    bill_title: v.bill_title.clone(),
                    chamber: c.id.clone(),
                    chamber_name: c.name.clone(),
                    votes_for: v.votes_for,
                    votes_against: v.votes_against,
                    abstentions: v.abstentions,
                    passed: v.passed,
                    turn: v.turn,
                    club_breakdown: v
                        .club_breakdown
                        .iter()
                        .map(|t| ClubVoteRow {
                            club_id: t.club_id.clone(),
                            votes_for: t.votes_for,
                            votes_against: t.votes_against,
                            abstentions: t.abstentions,
                        })
                        .collect(),
                    detail: v.detail.as_ref().map(bill_detail_row),
                })
            })
            .collect();

        let has_upper = parl.upper_chamber().is_some();
        let current_turn = country_turn(country);

        // Legislative queue from lower chamber — each row carries a live
        // detail projection for the expandable drawer.
        let legislative_queue: Vec<QueueRow> = parl
            .lower_chamber()
            .map(|c| {
                c.legislative_queue
                    .iter()
                    .map(|id| {
                        // Try to get bill details from legislative session.
                        let bill = politics
                            .legislative_session
                            .as_ref()
                            .and_then(|s| s.active_bills.get(id));
                        let (title, stage, initiator, committee, detail) = bill
                            .map(|b| {
                                (
                                    b.title.clone(),
                                    b.stage.label().to_string(),
                                    b.initiator.clone(),
                                    b.committee.clone(),
                                    Some(crate::politics::bill_lifecycle::build_bill_detail(
                                        b,
                                        current_turn,
                                        has_upper,
                                    )),
                                )
                            })
                            .unwrap_or_else(|| {
                                (
                                    "(unknown)".to_string(),
                                    "(unknown)".to_string(),
                                    String::new(),
                                    None,
                                    None,
                                )
                            });
                        QueueRow {
                            bill_id: id.clone(),
                            bill_title: title,
                            stage,
                            initiator,
                            committee,
                            detail: detail.as_ref().map(bill_detail_row),
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Phase 41: VIPs moved to GovernmentSnapshot.
        // Phase 42 + v4.18: Populate committees — with the live roster of
        // bills under review, their stage/progress, and the recommendation.
        let lower_seats: std::collections::HashMap<String, u32> = parl.lower_seat_map();
        let preview_recommendation =
            crate::politics::bill_lifecycle::committee_recommendation_modifier(
                &lower_seats,
                &politics.coalition,
                &politics.ruling_party,
            );
        let committees: Vec<CommitteeRow> = politics
            .committee_system
            .as_ref()
            .map(|cs| {
                cs.committees
                    .values()
                    .map(|c| {
                        // Bills under review: the reconciled committee roster,
                        // resolved against the live session for details.
                        let bills: Vec<CommitteeBillRow> = c
                            .bills_under_review
                            .iter()
                            .filter_map(|bid| {
                                politics
                                    .legislative_session
                                    .as_ref()
                                    .and_then(|s| s.active_bills.get(bid))
                                    .map(|b| CommitteeBillRow {
                                        bill_id: bid.clone(),
                                        title: b.title.clone(),
                                        stage: b.stage.label().to_string(),
                                        completion_turn: b.committee_completion_turn,
                                        // The recorded modifier once set;
                                        // otherwise the deterministic preview
                                        // of the value that will be recorded.
                                        recommendation: if b.committee_modifier != 0.0 {
                                            b.committee_modifier
                                        } else {
                                            preview_recommendation
                                        },
                                        initiator: b.initiator.clone(),
                                    })
                            })
                            .collect();
                        // Chair display: resolve the named VIP when assigned;
                        // `Committee::chair` is always the chair's PARTY id.
                        let chair_name = c
                            .chair_vip_id
                            .as_ref()
                            .and_then(|id| {
                                politics
                                    .vip_registry
                                    .as_ref()
                                    .and_then(|r| r.get(id))
                                    .map(|v| v.full_name.clone())
                            })
                            .unwrap_or_else(|| c.chair.clone());
                        CommitteeRow {
                            name: c.name.clone(),
                            committee_type: crate::politics::system::humanize_debug_token(
                                &format!("{:?}", c.committee_type),
                            ),
                            chair: chair_name,
                            chair_party: c.chair.clone(),
                            member_count: c.members.values().map(|v| *v as usize).sum(),
                            bills_under_review: c.bills_under_review.len(),
                            partisan_bias: c.partisan_bias,
                            bills,
                            party_seats: {
                                let mut ps: Vec<(String, u32)> = c
                                    .members
                                    .iter()
                                    .map(|(party, seats)| (party.clone(), *seats))
                                    .collect();
                                ps.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                                ps
                            },
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        ParliamentSnapshot {
            chambers,
            clubs,
            recent_votes,
            legislative_queue,
            suspended: parl.suspended,
            vips: Vec::new(), // Phase 41: VIPs now in GovernmentSnapshot
            committees,
            legislature_name: politics.government_form.legislature_name().to_string(),
            regime_type: politics.government_form.regime_type_key().to_string(),
        }
    } else {
        // No parliament struct — still report the regime-appropriate
        // legislature identity so the UI can label the (empty) tab.
        ParliamentSnapshot {
            legislature_name: politics.government_form.legislature_name().to_string(),
            regime_type: politics.government_form.regime_type_key().to_string(),
            ..Default::default()
        }
    }
}

/// v4.21 WS1: Offices controlled by one club — chamber speakerships,
/// committee chairmanships, and cabinet portfolios. Every row is resolved
/// from the presidium/committee/ministry records; holders fall back to the
/// recorded role+party label when the seat has no named VIP.
fn club_offices(
    club: &crate::politics::parliament::ParliamentaryClub,
    parl: &crate::politics::parliament::Parliament,
    politics: &crate::politics::system::Politics,
) -> Vec<ClubOfficeRow> {
    // Seat-map keys are club ids; normal clubs share the party id while
    // splinter clubs get their own `Splinter_*` key. Match both id and
    // display name so recorded officeholders always resolve.
    let matches = |id: &str| id == club.id || id == club.name;
    let mut offices = Vec::new();

    // Chamber presidiums — speaker and deputy speakers carry a party id.
    for chamber in &parl.chambers {
        if matches(&chamber.presidium.speaker_club) {
            offices.push(ClubOfficeRow {
                office: "Speaker".to_string(),
                body: chamber.name.clone(),
                holder: chamber.presidium.speaker.full_name.clone(),
            });
        }
        for deputy in &chamber.presidium.deputy_speakers {
            if matches(&deputy.party) {
                offices.push(ClubOfficeRow {
                    office: "Deputy Speaker".to_string(),
                    body: chamber.name.clone(),
                    holder: deputy.full_name.clone(),
                });
            }
        }
    }

    // Committee chairmanships — `Committee::chair` is the chair's party id;
    // resolve the named VIP exactly like the CommitteeRow emit above.
    if let Some(ref cs) = politics.committee_system {
        for committee in cs.committees.values() {
            if matches(&committee.chair) {
                let holder = committee
                    .chair_vip_id
                    .as_ref()
                    .and_then(|id| {
                        politics
                            .vip_registry
                            .as_ref()
                            .and_then(|r| r.get(id))
                            .map(|v| v.full_name.clone())
                    })
                    .unwrap_or_else(|| committee.chair.clone());
                offices.push(ClubOfficeRow {
                    office: "Committee Chair".to_string(),
                    body: committee.name.clone(),
                    holder,
                });
            }
        }
    }

    // Premiership — resolved through the canonical PM-party accessor (same
    // source as the Government tab's pm_party), named via the parliament VIP
    // roster or the party leader.
    let pm_party = crate::politics::ministries::canonical_pm_party_id(politics);
    if matches(pm_party) {
        let holder = parl
            .vips
            .iter()
            .find(|v| v.role == crate::politics::parliament::VipRole::PrimeMinister)
            .map(|v| v.full_name.clone())
            .or_else(|| {
                politics
                    .active_parties
                    .get(pm_party)
                    .map(|p| p.leader.name.clone())
                    .filter(|n| !n.is_empty())
            })
            .unwrap_or_else(|| format!("PM ({})", pm_party));
        offices.push(ClubOfficeRow {
            office: "Prime Minister".to_string(),
            body: "Cabinet".to_string(),
            holder,
        });
    }

    // Cabinet portfolios — `Ministry::minister_party` is a party id.
    if let Some(ref config) = politics.ministry_config {
        for m in &config.ministries {
            if matches(&m.minister_party) {
                offices.push(ClubOfficeRow {
                    office: "Minister".to_string(),
                    // Same display convention as MinisterRow — strip the
                    // "Ministry of " prefix.
                    body: m
                        .name
                        .strip_prefix("Ministry of ")
                        .unwrap_or(&m.name)
                        .to_string(),
                    holder: if m.minister_name.is_empty() {
                        format!("Minister ({})", m.minister_party)
                    } else {
                        m.minister_name.clone()
                    },
                });
            }
        }
    }

    // Deterministic order — committees/ministries iterate HashMaps/Vecs that
    // must not leak storage order into serialized snapshots.
    offices.sort_by(|a, b| a.office.cmp(&b.office).then_with(|| a.body.cmp(&b.body)));
    offices
}

/// v4.21 WS1: Bills this club sponsored — live-session bills plus completed
/// bills recovered from recorded chamber votes (`VoteRecord.detail`
/// outlives the bill's removal from `active_bills`). Deduped by bill id;
/// the recorded (possibly terminal) status wins over the in-flight label.
fn club_initiatives(
    club: &crate::politics::parliament::ParliamentaryClub,
    parl: &crate::politics::parliament::Parliament,
    politics: &crate::politics::system::Politics,
) -> Vec<ClubInitiativeRow> {
    let matches = |id: &str| id == club.id || id == club.name;
    let mut by_id: std::collections::HashMap<String, ClubInitiativeRow> =
        std::collections::HashMap::new();

    // In-flight bills from the live legislative session.
    if let Some(ref session) = politics.legislative_session {
        for bill in session.active_bills.values() {
            if !matches(&bill.initiator) {
                continue;
            }
            by_id.insert(
                bill.id.clone(),
                ClubInitiativeRow {
                    bill_id: bill.id.clone(),
                    title: bill.title.clone(),
                    status: bill.stage.label().to_string(),
                    introduced_turn: bill.introduction_turn,
                    enacted_turn: None,
                },
            );
        }
    }

    // Completed bills — `mark_bill_terminal` stamps the final outcome
    // ("Enacted", "Rejected by …") onto the recorded detail.
    for chamber in &parl.chambers {
        for record in &chamber.recent_votes {
            let Some(ref detail) = record.detail else {
                continue;
            };
            if !matches(&detail.initiator) {
                continue;
            }
            let status = if !detail.status_badge.is_empty() {
                detail.status_badge.clone()
            } else if !detail.stage.is_empty() {
                detail.stage.clone()
            } else {
                "Recorded".to_string()
            };
            let row = ClubInitiativeRow {
                bill_id: record.bill_id.clone(),
                title: record.bill_title.clone(),
                status,
                introduced_turn: detail.introduction_turn,
                enacted_turn: detail.enacted_turn,
            };
            // Prefer the recorded status — it carries terminal stamps the
            // live-session stage label lacks.
            by_id.insert(record.bill_id.clone(), row);
        }
    }

    let mut initiatives: Vec<ClubInitiativeRow> = by_id.into_values().collect();
    initiatives.sort_by(|a, b| {
        b.introduced_turn
            .cmp(&a.introduced_turn)
            .then_with(|| a.bill_id.cmp(&b.bill_id))
    });
    initiatives
}

/// v4.21 WS1: Recent chamber votes carrying this club's tally, with the
/// club's whip line and its discipline rebels. Government-side clubs
/// (ruling party or coalition member — the same `is_coalition` test
/// `calculate_floor_vote` applies) are whipped FOR the agenda; every other
/// club's line is the plurality direction of its own recorded tally, since
/// MPs are anonymized seat pools and the tally is the finest real
/// granularity the engine keeps. `rebels` counts seats cast against the
/// line — nonzero when a whipped club fails its discipline roll and its
/// bloc splits into Against/Abstain.
fn club_vote_history(
    club: &crate::politics::parliament::ParliamentaryClub,
    parl: &crate::politics::parliament::Parliament,
    politics: &crate::politics::system::Politics,
) -> Vec<ClubVoteHistoryRow> {
    let matches = |id: &str| id == club.id || id == club.name;
    let whipped_for = matches(&politics.ruling_party)
        || politics.coalition.iter().any(|p| matches(p));
    let mut rows = Vec::new();
    for chamber in &parl.chambers {
        for record in &chamber.recent_votes {
            let Some(tally) = record.club_breakdown.iter().find(|t| matches(&t.club_id)) else {
                continue;
            };
            let stance = if whipped_for {
                "For"
            } else if tally.votes_for >= tally.votes_against && tally.votes_for >= tally.abstentions {
                "For"
            } else if tally.votes_against >= tally.abstentions {
                "Against"
            } else {
                "Abstain"
            };
            let total = tally.votes_for + tally.votes_against + tally.abstentions;
            let loyal = match stance {
                "For" => tally.votes_for,
                "Against" => tally.votes_against,
                _ => tally.abstentions,
            };
            rows.push(ClubVoteHistoryRow {
                bill_id: record.bill_id.clone(),
                bill_title: record.bill_title.clone(),
                chamber_name: chamber.name.clone(),
                turn: record.turn,
                stance: stance.to_string(),
                votes_for: tally.votes_for,
                votes_against: tally.votes_against,
                abstentions: tally.abstentions,
                rebels: total.saturating_sub(loyal),
                passed: record.passed,
            });
        }
    }
    rows.sort_by(|a, b| b.turn.cmp(&a.turn).then_with(|| a.bill_id.cmp(&b.bill_id)));
    rows
}

/// Phase 26: Aggregate companies by sector for the Sector Overview table.
///
/// Computes per-sector: company count, total employment (fulfilled_fte),
/// average wage, and percentage share of total wages (proxy for GDP share).
fn aggregate_sectors(companies: &[Company], country: &Country) -> Vec<SectorRow> {
    use std::collections::HashMap;

    // Accumulate per-sector: (count, total_fte, total_wages)
    // Phase 77: total_fte is u32 (integer headcount), total_wages is f64
    let mut by_sector: HashMap<Sector, (usize, u32, f64)> = HashMap::new();
    for c in companies {
        let entry = by_sector.entry(c.sector).or_insert((0, 0, 0.0));
        entry.0 += 1;
        entry.1 += c.fulfilled_fte;
        entry.2 += c.offered_wage_per_fte * c.fulfilled_fte as f64;
    }

    let total_wages: f64 = by_sector.values().map(|(_, _, w)| *w).sum();

    // Phase 28: Extract PMI and previous-turn data from country.budget.sectors.
    let sector_pmi: HashMap<Sector, f64> = country
        .budget
        .sectors
        .iter()
        .filter_map(|(sector, share)| {
            share
                .extra
                .get("pmi")
                .and_then(|v| v.as_f64())
                .map(|pmi| (*sector, pmi))
        })
        .collect();

    // Phase 28: Read previous-turn employment and wage for ToT computation.
    let prev_employment: HashMap<Sector, f64> = country
        .budget
        .sectors
        .iter()
        .filter_map(|(sector, share)| {
            share
                .extra
                .get("_prev_employment")
                .and_then(|v| v.as_f64())
                .map(|e| (*sector, e))
        })
        .collect();
    let prev_avg_wage: HashMap<Sector, f64> = country
        .budget
        .sectors
        .iter()
        .filter_map(|(sector, share)| {
            share
                .extra
                .get("_prev_avg_wage")
                .and_then(|v| v.as_f64())
                .map(|w| (*sector, w))
        })
        .collect();

    let mut rows: Vec<SectorRow> = by_sector
        .iter()
        .map(|(sector, (count, fte, wages))| {
            let fte_f = *fte as f64;
            let avg_wage = if fte_f > 0.0 { *wages / fte_f } else { 0.0 };
            let pct_share = if total_wages > 0.0 {
                (*wages / total_wages) * 100.0
            } else {
                0.0
            };
            let pmi = sector_pmi.get(sector).copied().unwrap_or(50.0);

            // Phase 28: Compute ToT deltas vs previous turn.
            let employment_tot = prev_employment.get(sector).and_then(|prev| {
                if *prev > 0.0 {
                    Some(((fte_f - *prev) / *prev) * 100.0)
                } else {
                    None
                }
            });
            let wage_tot = prev_avg_wage.get(sector).and_then(|prev| {
                if *prev > 0.0 {
                    Some(((avg_wage - *prev) / *prev) * 100.0)
                } else {
                    None
                }
            });

            SectorRow {
                sector_name: format!("{:?}", sector),
                company_count: *count,
                pct_gdp_share: pct_share,
                total_employment: *fte,
                average_wage: avg_wage,
                pmi,
                employment_tot,
                wage_tot,
            }
        })
        .collect();

    // Sort by GDP share descending.
    rows.sort_by(|a, b| {
        b.pct_gdp_share
            .partial_cmp(&a.pct_gdp_share)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows
}

/// Compute ToT and YoY deltas from the telemetry history buffer.
///
/// ToT = percent change from the previous turn.
/// YoY = percent change from 24 turns ago (1 year).
/// For unemployment and corruption, the delta is in absolute terms
/// (percentage points / index points) rather than percent change.
fn compute_deltas(
    history: &TelemetryHistory,
    md: &crate::state::MacroData,
    corruption_index: f64,
    population: f64,
    peasant_population: f64,
    furloughed_total: f64,
    gdp_per_capita: f64,
) -> TelemetryDeltas {
    let g = &md.gdp_breakdown;
    let inf = &md.inflation_indices;
    let ms = &md.money_supply;

    TelemetryDeltas {
        gdp_tot: history.tot_pct(g.official_gdp, |s| s.official_gdp),
        gdp_yoy: history.yoy_pct(g.official_gdp, |s| s.official_gdp),
        cpi_tot: history.tot_pct(inf.cpi_index, |s| s.cpi_index),
        cpi_yoy: history.yoy_pct(inf.cpi_index, |s| s.cpi_index),
        ppi_tot: history.tot_pct(inf.ppi_index, |s| s.ppi_index),
        ppi_yoy: history.yoy_pct(inf.ppi_index, |s| s.ppi_index),
        m3_tot: history.tot_pct(ms.m3, |s| s.m3),
        m3_yoy: history.yoy_pct(ms.m3, |s| s.m3),
        // Unemployment: absolute delta (percentage points), not percent change.
        unemployment_tot: history
            .previous_turn()
            .map(|s| md.labor_market.unemployment_rate - s.unemployment_pct),
        unemployment_yoy: history
            .one_year_ago()
            .map(|s| md.labor_market.unemployment_rate - s.unemployment_pct),
        shadow_gdp_tot: history.tot_pct(g.shadow_gdp, |s| s.shadow_gdp),
        shadow_gdp_yoy: history.yoy_pct(g.shadow_gdp, |s| s.shadow_gdp),
        // Corruption: absolute delta (index points).
        corruption_tot: history
            .previous_turn()
            .map(|s| corruption_index - s.corruption_index),
        corruption_yoy: history
            .one_year_ago()
            .map(|s| corruption_index - s.corruption_index),
        population_tot: history.tot_pct(population, |s| s.population as f64),
        population_yoy: history.yoy_pct(population, |s| s.population as f64),
        wage_tot: history.tot_pct(md.average_wage, |s| s.average_wage),
        wage_yoy: history.yoy_pct(md.average_wage, |s| s.average_wage),
        // Phase 90: Peasant population, furloughed, and GDP/capita deltas.
        peasant_population_tot: history.tot_pct(peasant_population, |s| s.peasant_population),
        peasant_population_yoy: history.yoy_pct(peasant_population, |s| s.peasant_population),
        furloughed_tot: history.tot_pct(furloughed_total, |s| s.furloughed_total),
        furloughed_yoy: history.yoy_pct(furloughed_total, |s| s.furloughed_total),
        gdp_per_capita_tot: history.tot_pct(gdp_per_capita, |s| s.gdp_per_capita),
        gdp_per_capita_yoy: history.yoy_pct(gdp_per_capita, |s| s.gdp_per_capita),
    }
}

// ============================================================================
// PHASE 60: CADASTRE / LAND / COURTS DTOs
// ============================================================================

// Build a `GlobalSnapshot` from the full `GameState` and market data.
//
// # Arguments
// * `state` - The full game state.
// * `market_history` - Global market history.
// * `market` - Global market.
// * `buildings` - All buildings (grouped by country via owner/region).
//
// # Returns
// A `GlobalSnapshot` with per-country snapshots.

/// Phase 60: Cadastre summary row per region (public data — visible to all players).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CadastreSummaryRow {
    pub region_id: String,
    pub region_name: String,
    pub total_hectares: f64,
    pub total_value: f64,
    pub avg_legal_certainty: f64,
    pub avg_infrastructure_access: f64,
    pub zoning_distribution: Vec<CadastreZoningEntry>,
    pub owner_distribution: Vec<CadastreOwnerEntry>,
    pub border_conflicts: u32,
    pub foreign_ownership_pct: f64,
}

/// Phase 60: Zoning distribution entry (designation + percentage).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CadastreZoningEntry {
    pub designation: String,
    pub percentage: f64,
}

/// Phase 60: Owner distribution entry (owner_type + percentage).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CadastreOwnerEntry {
    pub owner_type: String,
    pub percentage: f64,
}

/// Phase 60: Zoning plan row (public data).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ZoningPlanRow {
    pub region_id: String,
    pub region_name: String,
    pub plan_id: String,
    pub enacted_turn: u32,
    pub implementation_progress: f64,
    pub target_distribution: Vec<CadastreZoningEntry>,
    pub governor_name: String,
    pub governor_trait: String,
}

/// Phase 60: Court backlog row per region (public data).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CourtBacklogRow {
    pub region_id: String,
    pub region_name: String,
    pub pending_cases: u32,
    pub border_conflicts: u32,
    pub arbitration_cases: u32,
    pub avg_processing_turns: f64,
    pub court_status: String,
    /// Whether the court is in crisis (severely backlogged or paralyzed).
    pub is_crisis: bool,
}

/// Phase 60: Arbitration case row (public data).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ArbitrationCaseRow {
    pub case_id: String,
    pub plaintiff_name: String,
    pub plaintiff_type: String,
    pub compensation_claimed: f64,
    pub status: String,
    pub filed_turn: u32,
    pub state_strength_assessment: f64,
}

/// Phase 60: Parcel detail row (public data).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ParcelDetailRow {
    pub parcel_id: String,
    pub region_id: String,
    pub size_hectares: f64,
    pub soil_class: String,
    pub zoning: String,
    pub owner_type: String,
    pub owner_name: String,
    pub legal_certainty: f64,
    pub infrastructure_access: f64,
    pub current_value: f64,
    pub is_frozen: bool,
    pub is_border_zone: bool,
}

/// Phase 60: Ministry land report (role-gated — internal government document).
/// Only visible to players controlling a VIP in a top executive role.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MinistryLandReportDTO {
    pub report_turn: u32,
    pub total_land_value: f64,
    pub total_hectares: f64,
    pub foreign_ownership_pct: f64,
    pub total_border_conflicts: u32,
    pub total_arbitration_cases: u32,
    pub total_arbitration_exposure: f64,
    pub regional_summaries: Vec<MinistryRegionalSummaryDTO>,
    pub delay_note: String,
}

/// Phase 60: Per-region summary in the ministry report.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MinistryRegionalSummaryDTO {
    pub region_id: String,
    pub total_hectares: f64,
    pub total_value: f64,
    pub avg_legal_certainty: f64,
    pub border_conflicts: u32,
    pub foreign_ownership_pct: f64,
    pub court_backlog: f64,
}

/// Phase 60: Response wrapper for cadastre summary query.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CadastreSummaryResponse {
    pub rows: Vec<CadastreSummaryRow>,
}

/// Phase 60: Response wrapper for zoning plans query.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ZoningPlansResponse {
    pub rows: Vec<ZoningPlanRow>,
}

/// Phase 60: Response wrapper for court backlog query.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CourtBacklogResponse {
    pub rows: Vec<CourtBacklogRow>,
    /// Whether any region is in court crisis (for pulsating red warning).
    pub has_crisis: bool,
}

/// Phase 60: Response wrapper for arbitration cases query.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ArbitrationCasesResponse {
    pub rows: Vec<ArbitrationCaseRow>,
    pub total_exposure: f64,
    pub state_strength: f64,
}

pub fn build_global_snapshot(
    state: &GameState,
    market_history: &MarketHistory,
    market: &GlobalMarket,
    buildings_by_country: &BTreeMap<String, Vec<crate::entities::Building>>,
    companies_by_country: &BTreeMap<String, Vec<Company>>,
    view: &ViewQuery,
) -> GlobalSnapshot {
    let turn = state.calendar.global_turn;
    let year = state.calendar.current_year;

    let mut countries = BTreeMap::new();
    for (name, country) in &state.countries {
        let buildings = buildings_by_country
            .get(name)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let companies = companies_by_country
            .get(name)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let mut snap = build_country_snapshot(
            country,
            market_history,
            market,
            buildings,
            companies,
            view,
            year,
        );
        snap.turn = turn;
        snap.year = year;
        countries.insert(name.clone(), snap);
    }

    GlobalSnapshot {
        turn,
        year,
        countries,
        diplomacy: None,
        foreign_countries: Vec::new(),
        foreign_sector_balance: market.foreign_sector_balance,
    }
}

// ============================================================================
// Phase 73: Military & Crisis Dashboard DTOs
// ============================================================================

/// A single active war/front row for the Military dashboard.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct WarRow {
    /// Front ID.
    pub front_id: String,
    /// Front name.
    pub front_name: String,
    /// Countries involved.
    pub involved_countries: Vec<String>,
    /// Regions in the front.
    pub regions: Vec<String>,
    /// War exhaustion per country.
    pub war_exhaustion: Vec<(String, f64)>,
    /// Number of battles fought.
    pub battle_count: usize,
    /// Last battle result (if any).
    pub last_battle_result: String,
}

/// A single battle row for the Military dashboard.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct BattleRow {
    /// Battle ID.
    pub battle_id: String,
    /// Location (region).
    pub location: String,
    /// Attacker country.
    pub attacker: String,
    /// Defender country.
    pub defender: String,
    /// Battle result.
    pub result: String,
    /// Attacker casualties.
    pub attacker_casualties: i64,
    /// Defender casualties.
    pub defender_casualties: i64,
    /// Turn.
    pub turn: u32,
}

/// A single devastation row for the heatmap.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct DevastationRow {
    /// Region ID.
    pub region_id: String,
    /// Region display name.
    pub region_name: String,
    /// Average devastation index (0.0 = pristine, 1.0 = total ruin).
    pub devastation_index: f64,
    /// Number of parcels.
    pub parcel_count: usize,
    /// Number of parcels above destruction threshold.
    pub destroyed_parcels: usize,
    /// Number of parcels above damage threshold.
    pub damaged_parcels: usize,
}

/// Army composition for a single country.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct ArmyCompositionRow {
    /// Country name.
    pub country: String,
    /// Total manpower across all units.
    pub total_manpower: i64,
    /// Number of armies in the OOB.
    pub army_count: usize,
    /// Number of divisions.
    pub division_count: usize,
    /// Number of regiments.
    pub regiment_count: usize,
    /// Total unit count.
    pub unit_count: usize,
    /// Unit counts by type.
    pub units_by_type: Vec<(String, usize)>,
    /// Total manpower by type.
    pub manpower_by_type: Vec<(String, i64)>,
    /// v4.22: Per-region unit/manpower breakdown keyed by unit `location`
    /// (Country→Region grouping for the UI). Regions are country-local IDs.
    pub region_breakdown: Vec<RegionArmyRow>,
}

/// Per-region slice of a country's order of battle.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct RegionArmyRow {
    /// Country name (grouping key).
    pub country: String,
    /// Region ID where the units are currently located.
    pub region_id: String,
    /// Number of units located in this region.
    pub unit_count: usize,
    /// Total manpower located in this region.
    pub manpower: i64,
}

/// War morale for a single country's demographic class.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MoraleRow {
    /// Country name.
    pub country: String,
    /// Region ID.
    pub region_id: String,
    /// Class name (e.g., "FreePeasant").
    pub class_name: String,
    /// Class group for Country→Region UI grouping ("rural" or "urban").
    pub class_group: String,
    /// War morale (0.0–100.0), EWMA-seeked toward living-condition targets.
    pub war_morale: f64,
    /// Mental health (0.0–100.0), slower-moving counterpart of war morale.
    pub mental_health: f64,
    /// Population.
    pub population: i64,
    /// Whether strikes are active — a thresholded outcome of low war morale
    /// PLUS labor organization (union affiliation, a strong Trade Unions
    /// interest group, or an active IndustrialStrike movement), never a
    /// pinned default.
    pub strikes_active: bool,
    /// Whether desertions are active.
    pub desertions_active: bool,
    /// Whether this class carries a formal trade-union affiliation.
    pub unionized: bool,
}

/// A single disaster event for the Peacetime Disasters panel.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct DisasterRow {
    /// Disaster event ID.
    pub event_id: String,
    /// Disaster type.
    pub disaster_type: String,
    /// Region where it occurred.
    pub region_id: String,
    /// Turn.
    pub turn: u32,
    /// Devastation impact.
    pub devastation_impact: f64,
    /// Casualties.
    pub casualties: i64,
}

/// POW camp statistics for a single country.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct PowCampRow {
    /// Country name (captor).
    pub country: String,
    /// Total prisoners.
    pub total_prisoners: i64,
    /// Number of distinct prisoner groups.
    pub prisoner_groups: usize,
    /// Prisoners by origin country.
    pub prisoners_by_origin: Vec<(String, i64)>,
    /// Average health of POWs.
    pub average_health: f64,
    /// POWs assigned to forced labor.
    pub forced_labor_assigned: i64,
}

/// Full military dashboard response.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MilitaryDashboardResponse {
    /// All active wars/fronts.
    pub wars: Vec<WarRow>,
    /// Recent battles across all fronts.
    pub recent_battles: Vec<BattleRow>,
    /// Devastation heatmap data per region.
    pub devastation_map: Vec<DevastationRow>,
    /// Army composition for each country.
    pub army_compositions: Vec<ArmyCompositionRow>,
    /// War morale for each country's demographic classes.
    pub war_morale: Vec<MoraleRow>,
    /// Recent peacetime disaster events.
    pub disasters: Vec<DisasterRow>,
    /// POW camp statistics.
    pub pow_camps: Vec<PowCampRow>,
}

/// Builds the full military dashboard response.
pub fn build_military_dashboard(state: &GameState) -> MilitaryDashboardResponse {
    let mut wars = Vec::new();
    let mut recent_battles = Vec::new();
    let mut army_compositions = Vec::new();
    let mut war_morale = Vec::new();
    let mut pow_camps = Vec::new();

    for (country_name, country) in &state.countries {
        // Active wars/fronts
        for front in &country.military_fronts {
            let battle_count = front.battles.len();
            let last_battle_result = front
                .battles
                .last()
                .map(|b| format!("{:?}", b.result))
                .unwrap_or_else(|| "No battles".to_string());

            wars.push(WarRow {
                front_id: front.id.clone(),
                front_name: front.name.clone(),
                involved_countries: front.involved_countries.clone(),
                regions: front.regions.clone(),
                war_exhaustion: front
                    .war_exhaustion
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect(),
                battle_count,
                last_battle_result,
            });

            // Recent battles (last 5 per front)
            for battle in front.battles.iter().rev().take(5) {
                recent_battles.push(BattleRow {
                    battle_id: battle.id.clone(),
                    location: battle.location.clone(),
                    attacker: battle.attacker.clone(),
                    defender: battle.defender.clone(),
                    result: format!("{:?}", battle.result),
                    attacker_casualties: battle.attacker_casualties.total(),
                    defender_casualties: battle.defender_casualties.total(),
                    turn: battle.turn,
                });
            }
        }

        // Army composition
        let oob = &country.order_of_battle;
        let total_manpower = oob.total_manpower();
        let army_count = oob.armies.len();
        let division_count: usize = oob.armies.iter().map(|a| a.divisions.len()).sum();
        let regiment_count: usize = oob
            .armies
            .iter()
            .flat_map(|a| a.divisions.iter())
            .map(|d| d.regiments.len())
            .sum();
        let unit_count = oob.unit_count();

        // Count units by type
        let mut units_by_type: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut manpower_by_type: std::collections::BTreeMap<String, i64> =
            std::collections::BTreeMap::new();
        for unit in oob.flatten() {
            let type_name = format!("{:?}", unit.unit_type);
            *units_by_type.entry(type_name.clone()).or_insert(0) += 1;
            *manpower_by_type.entry(type_name).or_insert(0) += unit.manpower;
        }

        // v4.22: Region breakdown for Country→Region grouping — units are
        // bucketed by their live `location` field (real OOB state).
        let mut region_units: std::collections::BTreeMap<String, (usize, i64)> =
            std::collections::BTreeMap::new();
        for unit in oob.flatten() {
            let entry = region_units.entry(unit.location.clone()).or_default();
            entry.0 += 1;
            entry.1 += unit.manpower;
        }
        let region_breakdown: Vec<RegionArmyRow> = region_units
            .into_iter()
            .map(|(region_id, (count, manpower))| RegionArmyRow {
                country: country_name.clone(),
                region_id,
                unit_count: count,
                manpower,
            })
            .collect();

        army_compositions.push(ArmyCompositionRow {
            country: country_name.clone(),
            total_manpower,
            army_count,
            division_count,
            regiment_count,
            unit_count,
            units_by_type: units_by_type.into_iter().collect(),
            manpower_by_type: manpower_by_type.into_iter().collect(),
            region_breakdown,
        });

        // War morale per class — regions is a Vec<Region>.
        //
        // v4.22: `strikes_active` is a thresholded OUTCOME — low war morale
        // AND labor organization — not a bare morale threshold (the old code
        // flagged every class below 30, which combined with the pinned
        // baseline made all 848 rows read "STRIKES"). Organization is read
        // from real state: the class's own `union_affiliation`, the national
        // "Trade Unions" interest-group weight, or an active/forming
        // IndustrialStrike movement targeting that class in that region.
        let strike_threshold = country.morale_config.strike_threshold;
        let desertion_threshold = country.morale_config.desertion_threshold;
        let national_union_weight = country
            .politics
            .interest_groups
            .get("Trade Unions")
            .map(|g| g.total_political_weight)
            .unwrap_or(0.0);
        for region in &country.regions {
            let region_movements: Vec<&crate::politics::mass_movements::MassMovement> = country
                .politics
                .mass_movements
                .iter()
                .filter(|m| {
                    m.region_id == region.id
                        && matches!(
                            m.movement_type,
                            crate::politics::mass_movements::MassMovementType::IndustrialStrike
                        )
                        && matches!(
                            m.status,
                            crate::politics::mass_movements::MassMovementStatus::Active
                                | crate::politics::mass_movements::MassMovementStatus::Forming
                                | crate::politics::mass_movements::MassMovementStatus::Negotiating
                        )
                })
                .collect();
            let mut push_morale_row = |class_name: String,
                                       class_group: &str,
                                       demo: &crate::society::geography::ClassDemographics| {
                let class_strike_movement = region_movements
                    .iter()
                    .any(|m| m.initiating_class == class_name);
                let unionized = demo.union_affiliation.is_some();
                let labor_organized = unionized || national_union_weight >= 5.0;
                war_morale.push(MoraleRow {
                    country: country_name.clone(),
                    region_id: region.id.clone(),
                    class_name,
                    class_group: class_group.to_string(),
                    war_morale: demo.war_morale,
                    mental_health: demo.mental_health,
                    population: demo.population,
                    strikes_active: class_strike_movement
                        || (demo.war_morale < strike_threshold && labor_organized),
                    desertions_active: demo.war_morale < desertion_threshold,
                    unionized,
                });
            };
            for (class_name, demo) in &region.class_demographics.rural_classes {
                push_morale_row(class_name.to_string(), "rural", demo);
            }
            for (class_name, demo) in &region.class_demographics.urban_classes {
                push_morale_row(class_name.to_string(), "urban", demo);
            }
        }

        // POW camp stats — Phase 86.5B: Now wired from country.pow_camp.
        let pow_camp = &country.pow_camp;
        let total_prisoners = pow_camp.prisoners.len() as i64;
        let prisoner_groups = pow_camp
            .prisoners
            .iter()
            .map(|p| p.origin_country.clone())
            .collect::<std::collections::HashSet<_>>()
            .len();
        let forced_labor_assigned = pow_camp
            .prisoners
            .iter()
            .filter(|p| p.status == crate::military::pows::PowStatus::ForcedLabor)
            .count() as i64;
        let prisoners_by_origin: Vec<(String, i64)> = {
            let mut map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
            for p in &pow_camp.prisoners {
                *map.entry(p.origin_country.clone()).or_insert(0) += 1;
            }
            map.into_iter().collect()
        };
        let average_health = if !pow_camp.prisoners.is_empty() {
            pow_camp
                .prisoners
                .iter()
                .map(|p| p.productivity_factor)
                .sum::<f64>()
                / pow_camp.prisoners.len() as f64
        } else {
            0.0
        };
        if total_prisoners > 0 {
            pow_camps.push(PowCampRow {
                country: country_name.clone(),
                total_prisoners,
                prisoner_groups,
                prisoners_by_origin,
                average_health,
                forced_labor_assigned,
            });
        }
    }

    // Devastation map — aggregate per region per country
    let mut devastation_map = Vec::new();
    for (country_name, country) in &state.countries {
        let cadastre = &country.cadastre;
        for region in &country.regions {
            let parcel_ids = &region.parcel_ids;
            if parcel_ids.is_empty() {
                continue;
            }
            let mut total_devastation = 0.0;
            let mut parcel_count = 0usize;
            let mut destroyed = 0usize;
            let mut damaged = 0usize;
            for pid in parcel_ids {
                if let Some(p) = cadastre.get(*pid) {
                    total_devastation += p.devastation_index;
                    parcel_count += 1;
                    if p.devastation_index >= 0.7 {
                        destroyed += 1;
                    } else if p.devastation_index >= 0.3 {
                        damaged += 1;
                    }
                }
            }
            if parcel_count > 0 {
                devastation_map.push(DevastationRow {
                    region_id: format!("{}:{}", country_name, region.id),
                    region_name: if region.display_name.is_empty() {
                        region.id.clone()
                    } else {
                        region.display_name.clone()
                    },
                    devastation_index: total_devastation / parcel_count as f64,
                    parcel_count,
                    destroyed_parcels: destroyed,
                    damaged_parcels: damaged,
                });
            }
        }
    }

    MilitaryDashboardResponse {
        wars,
        recent_battles,
        devastation_map,
        army_compositions,
        war_morale,
        disasters: Vec::new(), // Disasters are transient — would need a log
        pow_camps,
    }
}

// ============================================================================
// Phase 81: Energy Grid Snapshot
// ============================================================================

/// Phase 81: Energy grid snapshot for the Energy dashboard.
///
/// Phase 86.5B: Municipal AI investment plan snapshot.
/// Shows the AI's investment decisions for water, sanitation, heating, and waste infrastructure.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MunicipalPlanRow {
    /// Region ID for the planned investment.
    pub region_id: String,
    /// Investment domain (heating, water, sanitation, waste).
    pub domain: String,
    /// Planned investment amount (fiat).
    pub planned_amount: f64,
    /// Priority (0.0–1.0, higher = more urgent).
    pub priority: f64,
    /// Crisis flag (true if triggered by mortality risk).
    pub is_crisis: bool,
}

/// Phase 86.5B: Municipal AI snapshot for a country.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MunicipalAiSnapshot {
    /// Total planned investment across all domains.
    pub total_planned_investment: f64,
    /// Per-region investment plans.
    pub plans: Vec<MunicipalPlanRow>,
    /// Available budget for municipal investments.
    pub available_budget: f64,
    /// Number of regions with crisis-level needs.
    pub crisis_regions: u32,
}

/// Phase 86.5B: Build a municipal AI snapshot for a country.
pub fn build_municipal_ai_snapshot(country: &Country) -> MunicipalAiSnapshot {
    let plan = &country.municipal_infrastructure_plan;
    let mut plans = Vec::new();
    let mut crisis_count = 0u32;

    // Heating plan (no is_crisis field — use passes_cost_benefit_gate as priority signal)
    if plan.thermal_plan.estimated_capex > 0.0 {
        let is_crisis = !plan.thermal_plan.passes_cost_benefit_gate
            && plan.thermal_plan.expected_mortality_reduction_value > 0.0;
        plans.push(MunicipalPlanRow {
            region_id: "national".to_string(),
            domain: "heating".to_string(),
            planned_amount: plan.thermal_plan.estimated_capex,
            priority: if is_crisis { 1.0 } else { 0.5 },
            is_crisis,
        });
        if is_crisis {
            crisis_count += 1;
        }
    }

    // Electrical plan
    if plan.electrical_plan.estimated_capex > 0.0 {
        plans.push(MunicipalPlanRow {
            region_id: "national".to_string(),
            domain: "electrical".to_string(),
            planned_amount: plan.electrical_plan.estimated_capex,
            priority: if plan.electrical_plan.is_crisis {
                1.0
            } else {
                0.5
            },
            is_crisis: plan.electrical_plan.is_crisis,
        });
        if plan.electrical_plan.is_crisis {
            crisis_count += 1;
        }
    }

    // Water plan
    if plan.water_plan.estimated_capex > 0.0 {
        plans.push(MunicipalPlanRow {
            region_id: "national".to_string(),
            domain: "water".to_string(),
            planned_amount: plan.water_plan.estimated_capex,
            priority: if plan.water_plan.is_crisis { 1.0 } else { 0.5 },
            is_crisis: plan.water_plan.is_crisis,
        });
        if plan.water_plan.is_crisis {
            crisis_count += 1;
        }
    }

    // Sanitation plan
    if plan.sanitation_plan.estimated_capex > 0.0 {
        plans.push(MunicipalPlanRow {
            region_id: "national".to_string(),
            domain: "sanitation".to_string(),
            planned_amount: plan.sanitation_plan.estimated_capex,
            priority: if plan.sanitation_plan.is_crisis {
                1.0
            } else {
                0.5
            },
            is_crisis: plan.sanitation_plan.is_crisis,
        });
        if plan.sanitation_plan.is_crisis {
            crisis_count += 1;
        }
    }

    // Waste plan
    if plan.waste_plan.estimated_capex > 0.0 {
        plans.push(MunicipalPlanRow {
            region_id: "national".to_string(),
            domain: "waste".to_string(),
            planned_amount: plan.waste_plan.estimated_capex,
            priority: if plan.waste_plan.is_crisis { 1.0 } else { 0.5 },
            is_crisis: plan.waste_plan.is_crisis,
        });
        if plan.waste_plan.is_crisis {
            crisis_count += 1;
        }
    }

    MunicipalAiSnapshot {
        total_planned_investment: plan.total_capex,
        plans,
        available_budget: plan.available_budget,
        crisis_regions: crisis_count,
    }
}

/// Role-gated: foreign observers see only public aggregate data (national
/// supply/demand/capacity), not detailed plant counts, spot prices, or
/// interconnector flows. The backend strips classified fields before
/// serialization based on the observer's role and country.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct EnergyGridSnapshot {
    /// National total electricity supply (MW).
    pub national_supply_mw: f64,
    /// National total electricity demand (MW).
    pub national_demand_mw: f64,
    /// National total nameplate capacity (MW).
    pub national_nameplate_capacity_mw: f64,
    /// National overproduction tier (string representation).
    pub national_overproduction_tier: String,
    /// National load-shed tier (string representation).
    pub national_load_shed_tier: String,
    /// Regional energy info (detail level depends on observer role).
    pub regions: Vec<RegionEnergyInfo>,
    /// Active power plant counts by type (zero-count types omitted).
    pub active_power_plants: Vec<PlantTypeCount>,
    /// National generation mix (fraction by plant type).
    pub generation_mix: Vec<GenerationMixEntry>,
    /// Average national grid condition (0.0 = collapsed, 1.0 = pristine).
    pub average_grid_condition: f64,
    /// Interconnector flows (only visible to authorized domestic observers).
    pub interconnector_flows: Vec<InterconnectorFlowInfo>,
    /// Whether this snapshot is classified (foreign observer).
    pub is_classified: bool,
    /// Phase 81 Wave 2: Spot market clearing info per region.
    /// Empty for foreign observers (classified).
    #[serde(default)]
    pub spot_market: Vec<SpotMarketInfo>,
    /// Phase 81 Wave 2: Active PPA count and total contracted MW.
    /// Empty for foreign observers (classified).
    #[serde(default)]
    pub ppa_summary: Vec<PpaInfo>,
}

/// Phase 81: Regional energy information.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct RegionEnergyInfo {
    /// Region ID.
    pub region_id: String,
    /// Region display name.
    pub region_name: String,
    /// Regional supply (MW) — raw generated supply before grid-cap clamping.
    pub supply_mw: f64,
    /// Bugfix Sprint: Effective supply (MW) after LV/MV grid capacity clamping.
    /// This is the value used for load-shed tiering. When this < demand,
    /// load shedding occurs even if raw supply > demand (grid bottleneck).
    pub effective_supply_mw: f64,
    /// Regional demand (MW).
    pub demand_mw: f64,
    /// Maximum production capacity (MW).
    pub max_production_capacity_mw: f64,
    /// Average spot price (currency per MWh).
    /// `None` for foreign observers (classified).
    pub average_spot_price: Option<f64>,
    /// Load-shed tier (string).
    pub load_shed_tier: String,
    /// Overproduction tier (string).
    pub overproduction_tier: String,
    /// Grid condition (0.0–1.0).
    pub grid_condition: f64,
}

/// Phase 81: Active power plant count by type.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct PlantTypeCount {
    /// Plant type (string representation).
    pub plant_type: String,
    /// Number of active plants of this type.
    pub count: usize,
    /// Total nameplate capacity (MW) for this plant type.
    pub total_capacity_mw: f64,
}

/// Phase 81: Generation mix entry (fraction of total supply by plant type).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct GenerationMixEntry {
    /// Plant type (string representation).
    pub plant_type: String,
    /// Fraction of total national supply (0.0–1.0).
    pub fraction: f64,
}

/// Phase 81: Interconnector flow information.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct InterconnectorFlowInfo {
    /// Source region ID.
    pub from_region: String,
    /// Destination region ID.
    pub to_region: String,
    /// Flow magnitude (MW). Positive = from→to, negative = to→from.
    pub flow_mw: f64,
    /// Line condition (0.0–1.0).
    pub condition: f64,
    /// Transmission loss fraction (0.0–1.0).
    pub loss_fraction: f64,
}

/// Phase 81 Wave 2: Spot market clearing info for a region.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct SpotMarketInfo {
    /// Region ID.
    pub region_id: String,
    /// Clearing price per MWh (currency).
    pub clearing_price: f64,
    /// Number of plants dispatched in merit order.
    pub dispatched_plant_count: usize,
    /// Total dispatched MW.
    pub total_dispatched_mw: f64,
}

/// Phase 81 Wave 2: PPA summary info.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct PpaInfo {
    /// PPA ID.
    pub ppa_id: String,
    /// Seller company ID.
    pub seller_company_id: String,
    /// Buyer company ID.
    pub buyer_company_id: String,
    /// Fixed price per MWh.
    pub fixed_price_per_mwh: f64,
    /// Contracted capacity (MW).
    pub contracted_mw: f64,
    /// Remaining turns until expiration.
    pub remaining_turns: u32,
}

/// Phase 81: Build an `EnergyGridSnapshot` from a country's power grid state.
///
/// Applies role-gating: if the observer is foreign (not from this country),
/// spot prices and interconnector flows are stripped (set to `None`/empty).
pub fn build_energy_grid_snapshot(
    country: &crate::state::Country,
    buildings: &[crate::entities::Building],
    observer_country: Option<&str>,
    _observer_role: Option<&str>,
) -> EnergyGridSnapshot {
    use crate::energy::types::{LoadShedTier, OverproductionTier, PowerPlantType};
    use std::collections::HashMap;

    let grid = &country.power_grid_state;
    let is_foreign = observer_country.is_none_or(|c| c != country.name);

    // Aggregate national supply/demand/capacity from grid state.
    let mut national_supply = 0.0_f64;
    let mut national_demand = 0.0_f64;
    let mut national_capacity = 0.0_f64;

    // Collect plant counts by type.
    let mut plant_counts: HashMap<PowerPlantType, (usize, f64)> = HashMap::new();

    // Count plants from buildings.
    for building in buildings {
        if building.sector != crate::registries::enums::Sector::Energy {
            continue;
        }
        if let Some(meta) = crate::energy::grid::get_plant_metadata(building) {
            let entry = plant_counts.entry(meta.plant_type).or_insert((0, 0.0));
            entry.0 += 1;
            entry.1 += meta.nameplate_capacity_mw;
            national_capacity += meta.nameplate_capacity_mw;
        }
    }

    // Build regional info.
    let mut regions: Vec<RegionEnergyInfo> = Vec::new();
    for region in &country.regions {
        let supply = grid
            .region_supply_mw
            .get(&region.id)
            .copied()
            .unwrap_or(0.0);
        let demand = grid
            .region_demand_mw
            .get(&region.id)
            .copied()
            .unwrap_or(0.0);
        let max_cap = grid
            .region_max_capacity_mw
            .get(&region.id)
            .copied()
            .unwrap_or(0.0);
        let spot = grid.spot_prices.get(&region.id).copied().unwrap_or(0.0);
        let shed = grid
            .load_shed_tiers
            .get(&region.id)
            .copied()
            .unwrap_or(LoadShedTier::Normal);
        let overprod = grid
            .overproduction_tiers
            .get(&region.id)
            .copied()
            .unwrap_or(OverproductionTier::Normal);
        let lv_cond = grid
            .region_lv_condition
            .get(&region.id)
            .copied()
            .unwrap_or(1.0);
        let mv_cond = grid
            .region_mv_condition
            .get(&region.id)
            .copied()
            .unwrap_or(1.0);
        let grid_cond = lv_cond.min(mv_cond);

        // Bugfix Sprint: Compute effective supply (after grid capacity clamping)
        // to expose the grid bottleneck that causes Blackout despite surplus.
        let lv_cap = grid
            .region_lv_capacity
            .get(&region.id)
            .copied()
            .unwrap_or(0.0);
        let mv_cap = grid
            .region_mv_capacity
            .get(&region.id)
            .copied()
            .unwrap_or(0.0);
        let grid_cap = lv_cap.min(mv_cap);
        let effective_supply = supply.min(grid_cap);

        national_supply += supply;
        national_demand += demand;

        regions.push(RegionEnergyInfo {
            region_id: region.id.clone(),
            region_name: if region.display_name.is_empty() {
                region.id.clone()
            } else {
                region.display_name.clone()
            },
            supply_mw: supply,
            effective_supply_mw: effective_supply,
            demand_mw: demand,
            max_production_capacity_mw: max_cap,
            average_spot_price: if is_foreign { None } else { Some(spot) },
            load_shed_tier: format!("{:?}", shed),
            overproduction_tier: format!("{:?}", overprod),
            grid_condition: grid_cond,
        });
    }

    // Build plant type counts (omit zero-count types).
    let mut active_power_plants: Vec<PlantTypeCount> = plant_counts
        .into_iter()
        .filter(|(_, (count, _))| *count > 0)
        .map(|(pt, (count, cap))| PlantTypeCount {
            plant_type: format!("{:?}", pt),
            count,
            total_capacity_mw: cap,
        })
        .collect();
    active_power_plants.sort_by(|a, b| a.plant_type.cmp(&b.plant_type));

    // Build generation mix.
    let total_supply = national_supply.max(0.001);
    let mut generation_mix: Vec<GenerationMixEntry> = Vec::new();
    // TODO: Track per-type generation. For now, use capacity as proxy.
    for plant in &active_power_plants {
        generation_mix.push(GenerationMixEntry {
            plant_type: plant.plant_type.clone(),
            fraction: plant.total_capacity_mw / total_supply,
        });
    }

    // Build interconnector flows (classified for foreign observers).
    let interconnector_flows: Vec<InterconnectorFlowInfo> = if is_foreign {
        Vec::new()
    } else {
        grid.hv_lines
            .iter()
            .map(|line| {
                let loss = crate::energy::grid::transmission_loss(line);
                InterconnectorFlowInfo {
                    from_region: line.from_region.clone(),
                    to_region: line.to_region.clone(),
                    flow_mw: line.current_flow_mw,
                    condition: line.condition,
                    loss_fraction: loss,
                }
            })
            .collect()
    };

    // Calculate average grid condition.
    let avg_cond: f64 = {
        let mut sum = 0.0;
        let mut count = 0.0;
        for region in &country.regions {
            let lv = grid
                .region_lv_condition
                .get(&region.id)
                .copied()
                .unwrap_or(1.0);
            let mv = grid
                .region_mv_condition
                .get(&region.id)
                .copied()
                .unwrap_or(1.0);
            sum += lv.min(mv);
            count += 1.0;
        }
        if count > 0.0 {
            sum / count
        } else {
            1.0
        }
    };

    // National tiers (worst case across regions).
    let national_shed = grid
        .load_shed_tiers
        .values()
        .max()
        .cloned()
        .unwrap_or(LoadShedTier::Normal);
    let national_overprod = grid
        .overproduction_tiers
        .values()
        .max()
        .cloned()
        .unwrap_or(OverproductionTier::Normal);

    EnergyGridSnapshot {
        national_supply_mw: national_supply,
        national_demand_mw: national_demand,
        national_nameplate_capacity_mw: national_capacity,
        national_overproduction_tier: format!("{:?}", national_overprod),
        national_load_shed_tier: format!("{:?}", national_shed),
        regions,
        active_power_plants,
        generation_mix,
        average_grid_condition: avg_cond,
        interconnector_flows,
        is_classified: is_foreign,
        spot_market: if is_foreign {
            Vec::new()
        } else {
            grid.spot_market
                .clearing_prices
                .iter()
                .map(|(region_id, &price)| {
                    let dispatched: f64 = grid
                        .spot_market
                        .dispatched_mw
                        .values()
                        .filter(|_| true)
                        .sum();
                    SpotMarketInfo {
                        region_id: region_id.clone(),
                        clearing_price: price,
                        dispatched_plant_count: grid.spot_market.dispatch_order.len(),
                        total_dispatched_mw: dispatched,
                    }
                })
                .collect()
        },
        ppa_summary: if is_foreign {
            Vec::new()
        } else {
            country
                .ppa_registry
                .active_ppas
                .iter()
                .map(|ppa| PpaInfo {
                    ppa_id: ppa.id.clone(),
                    seller_company_id: ppa.seller_company_id.clone(),
                    buyer_company_id: ppa.buyer_company_id.clone(),
                    fixed_price_per_mwh: ppa.fixed_price_per_mwh,
                    contracted_mw: ppa.contracted_mw,
                    remaining_turns: ppa.end_turn.saturating_sub(0),
                })
                .collect()
        },
    }
}

// ============================================================================
// PHASE E.10: EDUCATION & MOBILITY TELEMETRY SNAPSHOT
// ============================================================================

/// Phase E.10: Education and social mobility telemetry snapshot.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct EducationMobilitySnapshot {
    pub education_none_share: f64,
    pub education_basic_share: f64,
    pub education_secondary_share: f64,
    pub education_higher_share: f64,
    pub education_consumed: f64,
    pub education_needed: f64,
    pub education_coverage: f64,
    pub primary_need: f64,
    pub secondary_need: f64,
    pub higher_need: f64,
    pub school_system: String,
    pub has_middle_tier: bool,
    pub primary_years: u32,
    pub middle_years: u32,
    pub high_years: u32,
    pub primary_funding_model: String,
    pub primary_subsidy_rate: f64,
    pub secondary_funding_model: String,
    pub secondary_subsidy_rate: f64,
    pub higher_funding_model: String,
    pub higher_subsidy_rate: f64,
    pub rural_to_urban_count: i64,
    pub serf_to_free_peasant_count: i64,
    pub serf_to_landless_count: i64,
    pub worker_to_bourgeoisie_count: i64,
    pub child_labor_law: String,
    pub child_labor_fte: f64,
    pub loyalty_bond_count: i64,
}

/// Phase E.10: Build an education and mobility snapshot for a country.
pub fn build_education_mobility_snapshot(
    country: &crate::state::Country,
    education_consumption: &std::collections::BTreeMap<String, f64>,
    education_needs: &std::collections::BTreeMap<String, f64>,
    class_transition_result: Option<&crate::economy::labor::class_transitions::ClassTransitionResult>,
    child_labor_fte: &std::collections::BTreeMap<String, f64>,
) -> EducationMobilitySnapshot {
    use crate::politics::laws::SchoolSystem;

    let edu = &country.macro_indicators.demographics.education;
    let secondary_sum: f64 = edu.secondary.values().sum();
    let higher_sum: f64 = edu.higher.values().sum();
    let total = edu.none + edu.basic + secondary_sum + higher_sum;
    let (none_share, basic_share, secondary_share, higher_share) = if total > 0.0 {
        (edu.none / total, edu.basic / total, secondary_sum / total, higher_sum / total)
    } else {
        (1.0, 0.0, 0.0, 0.0)
    };

    let consumed: f64 = education_consumption.values().sum();
    let needed: f64 = education_needs.values().sum();
    let coverage = if needed > 0.0 { (consumed / needed).clamp(0.0, 1.0) } else { 0.0 };

    let per_tier = crate::economy::labor::compute_per_tier_education_needs(country);
    let (primary_need, secondary_need, higher_need) = per_tier.values().fold(
        (0.0, 0.0, 0.0),
        |(p, s, h), (rp, rs, rh)| (p + rp, s + rs, h + rh),
    );

    let school_system = country
        .politics
        .education_law
        .as_ref()
        .map(|l| l.school_system)
        .unwrap_or(SchoolSystem::FourPlusFourPlusFour);
    let (py, my, hy) = school_system.tier_years();

    let financing = country
        .politics
        .education_law
        .as_ref()
        .and_then(|l| l.financing.as_ref());

    let (primary_funding_model, primary_subsidy_rate) = financing
        .map(|f| funding_model_str(&f.primary_funding))
        .unwrap_or(("state_funded".to_string(), 1.0));
    let (secondary_funding_model, secondary_subsidy_rate) = financing
        .map(|f| funding_model_str(&f.secondary_funding))
        .unwrap_or(("state_funded".to_string(), 1.0));
    let (higher_funding_model, higher_subsidy_rate) = financing
        .map(|f| funding_model_str(&f.higher_funding))
        .unwrap_or(("state_funded".to_string(), 1.0));

    let (rural_to_urban, serf_to_free, serf_to_landless, worker_to_bourgeoisie) =
        if let Some(r) = class_transition_result {
            (r.rural_to_urban, r.serf_to_free_peasant, r.serf_to_landless, r.worker_to_bourgeoisie)
        } else {
            (0, 0, 0, 0)
        };

    let child_labor_law_str = country
        .politics
        .child_labor_law
        .as_ref()
        .map(|l| match l {
            crate::politics::laws::ChildLaborLaw::Prohibited => "prohibited",
            crate::politics::laws::ChildLaborLaw::Restricted => "restricted",
            crate::politics::laws::ChildLaborLaw::Unrestricted => "unrestricted",
        })
        .unwrap_or("prohibited")
        .to_string();

    let total_child_labor_fte: f64 = child_labor_fte.values().sum();
    let loyalty_bond_count: i64 = country
        .regions
        .iter()
        .flat_map(|r| {
            r.class_demographics.rural_classes.values().chain(
                r.class_demographics.urban_classes.values(),
            )
        })
        .map(|d| d.loyalty_bond_count)
        .sum();

    EducationMobilitySnapshot {
        education_none_share: none_share,
        education_basic_share: basic_share,
        education_secondary_share: secondary_share,
        education_higher_share: higher_share,
        education_consumed: consumed,
        education_needed: needed,
        education_coverage: coverage,
        primary_need,
        secondary_need,
        higher_need,
        school_system: format!("{:?}", school_system).to_lowercase(),
        has_middle_tier: school_system.has_middle_tier(),
        primary_years: py,
        middle_years: my,
        high_years: hy,
        primary_funding_model,
        primary_subsidy_rate,
        secondary_funding_model,
        secondary_subsidy_rate,
        higher_funding_model,
        higher_subsidy_rate,
        rural_to_urban_count: rural_to_urban,
        serf_to_free_peasant_count: serf_to_free,
        serf_to_landless_count: serf_to_landless,
        worker_to_bourgeoisie_count: worker_to_bourgeoisie,
        child_labor_law: child_labor_law_str,
        child_labor_fte: total_child_labor_fte,
        loyalty_bond_count,
    }
}

fn funding_model_str(f: &crate::politics::laws::EducationTierFunding) -> (String, f64) {
    use crate::politics::laws::EducationTierFunding;
    match f {
        EducationTierFunding::StateFunded => ("state_funded".to_string(), 1.0),
        EducationTierFunding::Subsidized { rate } => ("subsidized".to_string(), *rate),
        EducationTierFunding::FreeMarket => ("free_market".to_string(), 0.0),
    }
}

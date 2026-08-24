//! Phase 24E: Country snapshot aggregator.
//!
//! Produces a flat, UI-ready struct from `GameState`, `GlobalMarket`, and
//! `MarketHistory`. The TUI rendering layer consumes `CountrySnapshot` and
//! never touches raw simulation state directly, keeping the presentation
//! layer decoupled from the engine.

use crate::economy::market::market_history::MarketHistory;
use crate::economy::market::GlobalMarket;
use crate::registries::enums::Commodity;
use crate::registries::enums::Sector;
use crate::state::{Country, GameState};
use crate::state::macro_data::{
    GdpBreakdown, InflationIndices, MoneySupplySnapshot,
    TelemetryHistory,
};
use crate::entities::Company;
use crate::entities::legal_form::LegalForm;
use std::collections::BTreeMap;

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
    pub liquid_reserves: f64,
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
    /// Founded turn.
    pub founded_turn: u32,
    /// Active sanctions enacted by this organization.
    pub sanctions: Vec<SanctionRow>,
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

/// Phase 66: Build a diplomacy snapshot for a country.
pub fn build_diplomacy_snapshot(
    state: &crate::state::GameState,
    country_name: &str,
    diplomacy: &std::collections::HashMap<String, std::collections::HashMap<String, crate::international::DiplomaticRelation>>,
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
            let mut posted_vips: Vec<_> = registry.vips.values()
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
            clauses: treaty.clauses.iter().map(|c| c.as_str().to_string()).collect(),
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
        (Some(country.global_reputation.score), Some(country.geopolitical_doctrine.as_str().to_string()))
    } else {
        (None, None)
    };

    // Phase 68: Build organization rows for this country
    let current_turn = state.calendar.global_turn;
    let mut organizations = Vec::new();
    for org in state.international_organizations.orgs_for_country(country_name) {
        let org_sanctions: Vec<SanctionRow> = state.active_sanctions.sanctions.iter()
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
        organizations.push(InternationalOrgRow {
            id: org.id.clone(),
            name: org.name.clone(),
            integration_level: org.integration_level.as_str().to_string(),
            voting_mechanism: org.voting_mechanism.as_str().to_string(),
            member_states: org.member_states.clone(),
            directive_count: org.directives.len(),
            founded_turn: org.founded_turn,
            sanctions: org_sanctions,
        });
    }

    // Phase 68: Build sanctions against this country
    let sanctions_against: Vec<SanctionRow> = state.active_sanctions.active_sanctions_against(country_name, current_turn)
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
    }
}

/// Phase 66: Build foreign country rows with Fog of War applied.
/// The player country gets full data; all others get filtered.
pub fn build_foreign_country_rows(
    state: &crate::state::GameState,
    player_country: &str,
    diplomacy: &std::collections::HashMap<String, std::collections::HashMap<String, crate::international::DiplomaticRelation>>,
) -> Vec<ForeignCountryRow> {
    let mut rows = Vec::new();
    let mut sorted_names: Vec<&String> = state.countries.keys().filter(|n| *n != player_country).collect();
    sorted_names.sort();

    for name in sorted_names {
        let country = match state.countries.get(name) {
            Some(c) => c,
            None => continue,
        };

        // Get intel level for this country from player's perspective
        let intel = state.foreign_intelligence
            .get(player_country)
            .and_then(|m| m.get(name))
            .cloned()
            .unwrap_or_else(crate::international::fog_of_war::ForeignIntelligence::unknown);

        let fog_result = crate::international::fog_of_war::apply_fog_of_war(
            country.budget.gdp,
            country.order_of_battle.unit_count() as u32,
            country.budget.liquid_reserves,
            &intel,
        );

        // Get relations if known
        let relations = diplomacy
            .get(player_country)
            .and_then(|p| p.get(name))
            .map(|r| r.relations);

        let government_form = if intel.government_known {
            Some(format!("{:?}", country.politics.government_form))
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

/// Government tab data.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct GovernmentSnapshot {
    pub head_of_state_name: String,
    pub head_of_state_role: String,
    pub pm_name: String,
    pub pm_party: String,
    pub pm_ideology: String,
    pub cabinet: Vec<MinisterRow>,
    pub state_of_emergency: Option<EmergencySnapshot>,
    pub political_capital: f64,
    /// Phase 41: Named VIPs moved from ParliamentSnapshot to GovernmentSnapshot.
    pub vips: Vec<VipRow>,
    /// Phase 54: Government form string for monarchy detection in the UI.
    pub government_form: String,
    /// Phase 54: Royal dynasty snapshot (only populated for monarchies).
    pub royal_dynasty: Option<RoyalDynastySnapshot>,
}

/// A minister row for the Government tab.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MinisterRow {
    pub ministry_name: String,
    pub minister_name: String,
    pub party: String,
    pub ideology: String,
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
}

/// Phase 34: A named VIP row for the Parliament tab.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct VipRow {
    pub full_name: String,
    pub party: String,
    pub role: String,
    pub ideology: String,
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
    pub is_splinter: bool,
    pub discipline: f64,
    /// Phase 54: Chairperson VIP ID (if assigned).
    pub chairperson_id: Option<String>,
    /// Phase 54: Chairperson display name.
    pub chairperson_name: String,
}

/// A recent vote row.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct VoteRow {
    pub bill_id: String,
    pub bill_title: String,
    pub votes_for: u32,
    pub votes_against: u32,
    pub passed: bool,
    pub turn: u32,
}

/// A legislative queue row.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct QueueRow {
    pub bill_id: String,
    pub bill_title: String,
    pub stage: String,
    pub initiator: String,
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
        Self { offset, limit: limit.max(1) }
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
    pub name: String,
    pub bank_type: String,
    pub reserves: f64,
    pub deposits: f64,
    pub loans: f64,
    pub securities: f64,
    pub is_dspw: bool,
    pub ldr: f64,
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
    pub nav_per_share: f64,
    pub total_aum: f64,
    pub manager_name: String,
    pub manager_vip_id: String,
    pub manager_trait: String,
    pub shares_outstanding: u64,
    pub ytd_return: f64,
    pub top_holdings: Vec<(String, f64)>,
}

/// Phase 57: Detailed fund view for the fund detail panel.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct FundDetail {
    pub fund_id: String,
    pub name: String,
    pub fund_type: String,
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
    pub top_holdings: Vec<(String, f64)>,
    pub portfolio_holdings: Vec<(String, u64, f64)>,
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
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CapitalGainsTaxRow {
    pub entity_id: String,
    pub realized_gains: f64,
    pub realized_losses: f64,
    pub tax_owed: f64,
    pub carried_forward_losses: f64,
}

/// Phase 57: Capital gains tax summary response.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CapitalGainsTaxSummary {
    pub rows: Vec<CapitalGainsTaxRow>,
    pub total_tax_collected: f64,
    pub annual_tax_history: Vec<f64>,
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
    pub processing: bool,
    pub countries: Vec<String>,
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
    pub investment: f64,
    pub government_spending: f64,
    pub net_exports: f64,
    pub cpi: f64,
    pub ppi: f64,
    pub deltas: TelemetryDeltas,
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
) -> CountrySnapshot {
    let macro_data = &country.macro_indicators;

    // Commodities: build rows for all 140 commodities.
    let commodities: Vec<CommodityRow> = Commodity::all()
        .iter()
        .map(|&c| {
            let name = format!("{:?}", c);
            let vwap = market_history.vwap_per_commodity.get(&c).copied().unwrap_or(0.0);
            let base_price = market_history.global_base_prices.get(&c).copied().unwrap_or(0.0);
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
            let prev_surplus = market_history.prev_net_surplus.get(&c).copied().unwrap_or(0.0);
            let tot_balance_change = if prev_surplus.abs() > 0.01 {
                ((net_surplus - prev_surplus) / prev_surplus.abs()) * 100.0
            } else if net_surplus.abs() > 0.01 {
                100.0 // New activity when there was none before.
            } else {
                0.0
            };
            // Phase 27: Mark commodity as active if any market activity exists.
            let active = vwap > 0.0 || net_surplus.abs() > 0.01 || supply_volume > 0.01 || demand_volume > 0.01;
            CommodityRow { name, vwap, base_price, net_surplus, net_trade, tot_balance_change, active, supply_volume, demand_volume }
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
                if a.upheld { "upheld".to_string() } else { "rejected".to_string() }
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
                    current_reserves: dep.current_reserves,
                    estimated_reserves: dep.estimated_reserves,
                    quality: dep.quality,
                    depletion_pct: if dep.estimated_reserves > 0.0 {
                        (1.0 - dep.current_reserves / dep.estimated_reserves) * 100.0
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

    // Labor market
    let labor = LaborSummary {
        unemployment_rate: macro_data.labor_market.unemployment_rate,
        employed_total: macro_data.labor_market.employed_total,
        unemployed: macro_data.labor_market.unemployed,
        workforce: macro_data.labor_market.employed_total + macro_data.labor_market.unemployed,
        average_wage: macro_data.average_wage,
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

    // Phase 24F: Compute ToT/YoY deltas from telemetry history.
    let population_f64 = country.budget.population as f64;
    let deltas = compute_deltas(
        &macro_data.telemetry_history,
        macro_data,
        corruption_index,
        population_f64,
    );

    // Phase 26: Aggregate companies by sector for the Sector Overview table.
    let sectors = aggregate_sectors(companies, country);

    // Phase 34: Build region rows for the [8] Regions tab.
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
            let gdp_per_capita = if r.population > 0 {
                r.gdp / r.population as f64
            } else {
                0.0
            };
            RegionRow {
                id: r.id.clone(),
                display_name: if r.display_name.is_empty() { r.id.clone() } else { r.display_name.clone() },
                megaregion: megaregion_info.0,
                megaregion_id: megaregion_info.1,
                population: r.population,
                regional_gdp: r.gdp,
                gdp_per_capita,
                has_governance: r.governance.is_some(),
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
        turn: 0,  // filled by caller from GameState calendar
        year: 0,  // filled by caller
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
        government: build_government_snapshot(country),
        parliament: build_parliament_snapshot(country),
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
        region_detail: build_region_detail(country, view),
        megaregion_detail: build_megaregion_detail(country, view),
        advisory_council: build_advisory_council_snapshot(country),
        royal_dynasty: build_royal_dynasty_snapshot(country),
        government_form: format!("{:?}", country.politics.government_form),
    }
}

// ============================================================================
// PHASE 49: PAGINATED VIP / BANK / COMPANY BUILDERS
// ============================================================================

/// Build a paginated, filtered page of VIP rows from the VipRegistry.
fn build_vip_page(country: &Country, companies: &[Company], view: &ViewQuery) -> Vec<VipDossierRow> {
    let registry = match &country.politics.vip_registry {
        Some(r) => r,
        None => return Vec::new(),
    };

    // Phase 54: Build a lookup from CEO VIP ID to company name.
    let ceo_to_company: std::collections::HashMap<&str, &str> = companies
        .iter()
        .filter_map(|c| c.ceo_vip_id.as_ref().map(|id| (id.as_str(), c.name.as_str())))
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
        // Phase 54: Role filter — check if any role's as_str() matches.
        if !role_filter.is_empty() {
            let has_role = v.roles.iter().any(|r| r.as_str() == role_filter.as_str());
            if !has_role {
                return false;
            }
        }
        true
    };
    let mut all_vips: Vec<&crate::politics::vip_registry::Vip> = registry
        .vips
        .values()
        .filter(filter_fn)
        .collect();
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
                v.roles.iter().map(|r| r.as_str().to_string()).collect::<Vec<_>>().join(", ")
            };
            let company_name = v.roles.contains(&crate::politics::vip_registry::VipRoleExtended::Ceo)
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
        // Phase 54: Role filter.
        if !view.vip_filter.role_filter.is_empty() {
            let has_role = v.roles.iter().any(|r| r.as_str() == view.vip_filter.role_filter.as_str());
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
    let mut banks: Vec<&Company> = companies
        .iter()
        .filter(|c| c.bank_type.is_some())
        .collect();
    banks.sort_by(|a, b| a.name.cmp(&b.name));

    banks
        .into_iter()
        .skip(view.bank_page.offset)
        .take(view.bank_page.limit.max(1))
        .filter_map(|c| {
            let bs = c.balance_sheet.as_ref()?;
            let loans: f64 = bs.loans_issued.iter().map(|l| l.outstanding_balance).sum();
            let ldr = if bs.deposits > 0.0 { loans / bs.deposits * 100.0 } else { 0.0 };
            Some(BankRow {
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

/// Count total banks (for scroll indicators).
fn count_banks(companies: &[Company]) -> usize {
    companies.iter().filter(|c| c.bank_type.is_some()).count()
}

/// Build a paginated, filtered page of company rows.
fn build_company_page(country: &Country, companies: &[Company], view: &ViewQuery) -> Vec<CompanyRow> {
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
                .map(|r| if r.display_name.is_empty() { r.id.clone() } else { r.display_name.clone() })
                .unwrap_or_else(|| c.region_id.clone()),
            fulfilled_fte: c.fulfilled_fte,
            average_wage: c.offered_wage_per_fte,
            seasonal_state: if c.furloughed_workers_count > 0.0 { "Furloughed".to_string() } else { "Active".to_string() },
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
fn build_company_detail(country: &Country, companies: &[Company], view: &ViewQuery) -> Option<CompanyDetail> {
    let target_id = view.company_detail_id.as_ref()?;
    let c = companies.iter().find(|c| c.id == *target_id)?;

    // Phase 54: Resolve CEO name and ideology from the VIP registry.
    let (ceo_name, ceo_ideology) = if let Some(ref ceo_id) = c.ceo_vip_id {
        if let Some(ref registry) = country.politics.vip_registry {
            if let Some(ceo_vip) = registry.get(ceo_id) {
                (Some(ceo_vip.full_name.clone()), Some(ceo_vip.ideology.clone()))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    Some(CompanyDetail {
        id: c.id.clone(),
        name: c.name.clone(),
        sector: sector_display_name(c.sector),
        region: country
            .regions
            .iter()
            .find(|r| r.id == c.region_id)
            .map(|r| if r.display_name.is_empty() { r.id.clone() } else { r.display_name.clone() })
            .unwrap_or_else(|| c.region_id.clone()),
        legal_form: legal_form_display(&c.legal_form),
        ceo_vip_id: c.ceo_vip_id.clone(),
        ceo_name,
        ceo_ideology,
        union_id: c.union_id.clone(),
        fulfilled_fte: c.fulfilled_fte,
        fte_demand: c.target_fte_demand,
        average_wage: c.offered_wage_per_fte,
        seasonal_state: if c.furloughed_workers_count > 0.0 { "Furloughed".to_string() } else { "Active".to_string() },
        furloughed_workers_count: c.furloughed_workers_count,
        wage_arrears: c.wage_arrears,
        building_count: c.building_ids.len(),
        available_cash: c.available_cash,
    })
}

/// Build region drill-down detail on-demand for the selected region.
fn build_region_detail(country: &Country, view: &ViewQuery) -> Option<RegionDetail> {
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
    let head_name_for_lookup = region.governance.as_ref().map(|g| g.head.name.clone()).unwrap_or_default();
    let head_vip_id = if !head_name_for_lookup.is_empty() {
        country.politics.vip_registry.as_ref().and_then(|r| {
            r.get_by_name(&head_name_for_lookup).map(|v| v.id.clone())
        })
    } else {
        None
    };

    // Build budget/debt from local governance.
    let (budget_reserves, budget_tax_revenue, budget_property_tax, budget_expenditures, budget_balance, debt_total, debt_to_revenue_ratio, credit_rating, admin_status, head_name, head_type) =
        if let Some(ref gov) = region.governance {
            let rev = gov.budget.tax_revenue + gov.budget.property_tax;
            let balance = gov.budget.budget_balance;
            let ratio = if rev > 0.0 { gov.debt.total_debt / rev } else { 0.0 };
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
            (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, String::new(), "None".to_string(), String::new(), "None".to_string())
        };

    // Active mandates (mandates are national, not per-region, so show all).
    let active_mandates = country
        .politics
        .active_mandates
        .iter()
        .map(|m| MandateSummary {
            description: m.description.clone(),
            required_spending: m.required_spending_per_region,
            central_funding: m.central_funding,
            funding_gap: m.funding_gap,
            status: format!("{:?}", m.council_decision),
        })
        .collect();

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

    // Sector employment: Region doesn't have a labor_market field directly.
    // Use capacity utilization as a proxy for economic activity by type.
    let sector_employment: Vec<(String, f64)> = region
        .capacity_utilization
        .iter()
        .map(|(ct, util)| (format!("{:?}", ct), *util * 100.0))
        .collect();

    // Durable cohorts from class demographics.
    let durable_cohorts = region
        .class_demographics
        .rural_classes
        .values()
        .chain(region.class_demographics.urban_classes.values())
        .flat_map(|demo| demo.household_durables.iter())
        .map(|cohort| DurableCohortSummary {
            commodity: format!("{:?}", cohort.commodity),
            count: cohort.count,
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
        infrastructure_avg_condition,
        sector_employment,
        durable_cohorts,
        thermal_grid: Some(ThermalGridSnapshot {
            pipe_network_km: region.thermal_grid.pipe_network_km,
            pipe_condition: region.thermal_grid.pipe_condition,
            loss_per_km: region.thermal_grid.loss_per_km,
            effective_heat_supply_gj: region.capacity_pool
                .get(&crate::infrastructure::CapacityType::DistrictHeating)
                .copied()
                .unwrap_or(0.0),
            max_connectable_buildings: region.thermal_grid
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
            max_connectable_buildings: region.water_network.max_connectable_buildings(region.development_level),
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
            max_connectable_buildings: region.sewer_network.max_connectable_buildings(region.development_level),
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
    })
}

/// Phase 53: Build megaregion drill-down detail on-demand for the selected megaregion.
fn build_megaregion_detail(country: &Country, view: &ViewQuery) -> Option<MegaregionDetail> {
    let megaregion_id = view.megaregion_drilldown_id.as_ref()?;
    let megaregion = country.megaregions.iter().find(|m| m.id == *megaregion_id)?;

    // Aggregate population and GDP from member regions.
    let member_regions: Vec<&crate::society::geography::Region> = country
        .regions
        .iter()
        .filter(|r| megaregion.regions.contains(&r.id))
        .collect();
    let total_population: i64 = member_regions.iter().map(|r| r.population).sum();
    let total_gdp: f64 = member_regions.iter().map(|r| r.gdp).sum();

    // Extract governance fields if available.
    let (governor_name, governor_appointed, competence_level, budget_reserves, regional_transfers, development_expenditures, coordination_expenditures, budget_balance) =
        if let Some(ref gov) = megaregion.governance {
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
            (String::new(), false, "None".to_string(), 0.0, 0.0, 0.0, 0.0, 0.0)
        };

    Some(MegaregionDetail {
        megaregion_id: megaregion.id.clone(),
        display_name: if megaregion.name.is_empty() { megaregion.id.clone() } else { megaregion.name.clone() },
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
            DynastyMemberRow {
                vip_id: m.vip_id.clone(),
                name: resolve_name(&m.vip_id),
                relation: format!("{:?}", m.relation),
                age: 0,
                succession_order: m.succession_order,
                is_heir_apparent: m.is_heir_apparent,
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
    Some(RoyalDynastySnapshot {
        dynasty_name: dynasty.dynasty_name.clone(),
        current_monarch_id: dynasty.current_monarch_id.clone(),
        current_monarch_name,
        current_regent_id: dynasty.current_regent_id.clone(),
        current_regent_name,
        regency_active: dynasty.regency_active,
        members,
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
            config.ministries.iter().fold((0.0, 0.0, 0.0), |(a, s, c), m| {
                (a + m.allocated_cash, s + m.spent_cash, c + m.ministry_cash)
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
    // Phase 37: Debt holder breakdown
    let mut debt_held_by_banks = 0.0_f64;
    let mut debt_held_by_funds = 0.0_f64;
    for c in companies {
        if c.bank_type.is_some() {
            if let Some(ref bs) = c.balance_sheet {
                total_bank_reserves += bs.reserves_at_central_bank;
                total_bank_deposits += bs.deposits;
                total_bank_loans += bs.loans_issued.iter().map(|l| l.outstanding_balance).sum::<f64>();
                // Phase 37: Bank sovereign securities holdings
                debt_held_by_banks += bs.securities;
            }
            total_consumer_debt += c.consumer_loans.iter().map(|l| l.outstanding_principal).sum::<f64>();
            if c.is_dspw {
                dspw_bank_count += 1;
            }
        }
        // Phase 37: Investment fund bond holdings
        if let Some(ref ledger) = c.fund_ledger {
            debt_held_by_funds += ledger.bond_holdings.iter().map(|h| h.face_value).sum::<f64>();
        }
    }

    // Public debt — Phase 42: Include retail bonds held by citizens in the total.
    let debt_market_total = country.debt_market.total_outstanding_debt;
    let retail_bonds_total: f64 = country.debt_market.retail_bonds.iter()
        .map(|b| b.face_value)
        .sum::<f64>();
    let total_public_debt = debt_market_total + retail_bonds_total;
    let weighted_avg_interest_rate = country.debt_market.weighted_avg_interest_rate;

    // Phase 37: Central bank bond holdings and citizen retail bonds
    let debt_held_by_central_bank = cb.omo_bond_holdings;
    let debt_held_by_citizens = country.debt_market.retail_bonds.iter()
        .map(|b| b.face_value)
        .sum::<f64>();

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
    let capital_gains_revenue = last_tax.map(|t| t.capital_gains_tax_collected).unwrap_or(0.0);
    // Phase 39: Customs and state property revenue
    let customs_revenue = last_tax.map(|t| t.customs_revenue).unwrap_or(0.0);
    let state_property_revenue = last_tax.map(|t| t.state_property_revenue).unwrap_or(0.0);

    // Phase 38: Read tax rates for display.
    let pit_rate = country.tax_rates.income_tax.rate;
    let cit_rate = country.tax_rates.corporate_tax;
    let vat_rate = {
        // Average of all VAT brackets
        let brackets: Vec<f64> = country.tax_rates.vat.values()
            .map(|b| b.rate)
            .collect();
        if brackets.is_empty() { 0.0 } else { brackets.iter().sum::<f64>() / brackets.len() as f64 }
    };
    let wealth_tax_rate = country.tax_rates.wealth_tax.brackets.last()
        .map(|b| b.rate)
        .unwrap_or(0.0);
    let capital_gains_rate = country.tax_rates.capital_gains_tax.brackets.last()
        .map(|b| b.rate)
        .unwrap_or(0.0);

    // Phase 42: FX basket — top 3 foreign currencies by reserve amount.
    let domestic_ccy = &country.macro_indicators.currency;
    let mut fx_entries: Vec<(String, f64)> = cb.fx_reserves
        .iter()
        .filter(|(k, _)| *k != domestic_ccy)
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    fx_entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let fx_basket: Vec<FxBasketEntry> = fx_entries.iter().take(3)
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
        pit_rate,
        cit_rate,
        vat_rate,
        wealth_tax_rate,
        capital_gains_rate,
        total_public_debt,
        debt_service: 0.0,
        weighted_avg_interest_rate,
        debt_held_by_banks,
        debt_held_by_central_bank,
        debt_held_by_funds,
        debt_held_by_citizens,
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
            (category, m.spent_cash)
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
fn build_government_snapshot(country: &Country) -> GovernmentSnapshot {
    let politics = &country.politics;

    // Head of State.
    let head_of_state_name = if !politics.head_of_state.name.is_empty() {
        politics.head_of_state.name.clone()
    } else {
        "(vacant)".to_string()
    };

    // PM from ruling party leader.
    let pm_name = politics
        .active_parties
        .get(&politics.ruling_party)
        .map(|p| {
            if !p.leader.name.is_empty() {
                p.leader.name.clone()
            } else {
                // Phase 34: Fallback to party name instead of "(unnamed)".
                format!("Leader ({})", politics.ruling_party)
            }
        })
        .unwrap_or_else(|| "(vacant)".to_string());
    let pm_ideology = politics
        .active_parties
        .get(&politics.ruling_party)
        .map(|p| p.ideology.clone())
        .unwrap_or_default();

    // Cabinet from ministry_config.
    let cabinet: Vec<MinisterRow> = if let Some(ref config) = politics.ministry_config {
        config
            .ministries
            .iter()
            .map(|m| {
                let ideology = politics
                    .active_parties
                    .get(&m.minister_party)
                    .map(|p| p.ideology.clone())
                    .unwrap_or_default();
                MinisterRow {
                    // Phase 34: Strip "Ministry of " prefix for cleaner UI display.
                    ministry_name: m.name.clone()
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
                    allocated_cash: m.allocated_cash,
                    spent_cash: m.spent_cash,
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

    GovernmentSnapshot {
        head_of_state_name,
        head_of_state_role: format!("{:?}", politics.government_form),
        pm_name,
        pm_party: politics.ruling_party.clone(),
        pm_ideology,
        cabinet,
        state_of_emergency,
        political_capital: politics.political_capital,
        // Phase 41: VIPs moved from Parliament to Government tab.
        vips: build_vip_rows(country),
        // Phase 54: Government form + royal dynasty for monarchy sub-tab.
        government_form: format!("{:?}", politics.government_form),
        royal_dynasty: build_royal_dynasty_snapshot(country),
    }
}

/// Phase 41: Build VIP rows from the parliament struct for the Government tab.
fn build_vip_rows(country: &Country) -> Vec<VipRow> {
    if let Some(ref parl) = country.politics.parliament_struct {
        parl.vips
            .iter()
            .map(|v| VipRow {
                full_name: v.full_name.clone(),
                party: v.party.clone(),
                role: format!("{:?}", v.role),
                ideology: v.ideology.clone(),
                age: v.age,
            })
            .collect()
    } else {
        Vec::new()
    }
}

/// Phase 32: Build the Parliament tab snapshot.
fn build_parliament_snapshot(country: &Country) -> ParliamentSnapshot {
    let politics = &country.politics;

    if let Some(ref parl) = politics.parliament_struct {
        // Chambers.
        let chambers: Vec<ChamberSnapshot> = parl
            .chambers
            .iter()
            .map(|c| ChamberSnapshot {
                name: c.name.clone(),
                total_seats: c.total_seats,
                speaker_name: c.presidium.speaker.full_name.clone(),
                speaker_club: c.presidium.speaker_club.clone(),
                seat_distribution: c
                    .seats
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect(),
            })
            .collect();

        // Clubs.
        let clubs: Vec<ClubRow> = parl
            .clubs
            .iter()
            .map(|c| ClubRow {
                name: c.name.clone(),
                seats: c.seats,
                ideology: c.ideology.clone(),
                is_splinter: c.is_splinter,
                discipline: c.discipline,
                chairperson_id: c.chairperson_id.clone(),
                chairperson_name: c.chairperson_name.clone(),
            })
            .collect();

        // Recent votes from lower chamber.
        let recent_votes: Vec<VoteRow> = parl
            .lower_chamber()
            .map(|c| {
                c.recent_votes
                    .iter()
                    .map(|v| VoteRow {
                        bill_id: v.bill_id.clone(),
                        bill_title: v.bill_title.clone(),
                        votes_for: v.votes_for,
                        votes_against: v.votes_against,
                        passed: v.passed,
                        turn: v.turn,
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Legislative queue from lower chamber.
        let legislative_queue: Vec<QueueRow> = parl
            .lower_chamber()
            .map(|c| {
                c.legislative_queue
                    .iter()
                    .map(|id| {
                        // Try to get bill details from legislative session.
                        let (title, stage, initiator) = politics
                            .legislative_session
                            .as_ref()
                            .and_then(|s| s.active_bills.get(id))
                            .map(|b| {
                                (
                                    b.title.clone(),
                                    format!("{:?}", b.stage),
                                    b.initiator.clone(),
                                )
                            })
                            .unwrap_or_else(|| {
                                (
                                    "(unknown)".to_string(),
                                    "(unknown)".to_string(),
                                    String::new(),
                                )
                            });
                        QueueRow {
                            bill_id: id.clone(),
                            bill_title: title,
                            stage,
                            initiator,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Phase 41: VIPs moved to GovernmentSnapshot.
        // Phase 42: Populate committees from committee_system.
        let committees: Vec<CommitteeRow> = politics
            .committee_system
            .as_ref()
            .map(|cs| {
                cs.committees
                    .values()
                    .map(|c| CommitteeRow {
                        name: c.name.clone(),
                        committee_type: format!("{:?}", c.committee_type),
                        chair: c.chair.clone(),
                        chair_party: c.chair.clone(),
                        member_count: c.members.values().map(|v| *v as usize).sum(),
                        bills_under_review: c.bills_under_review.len(),
                        partisan_bias: c.partisan_bias,
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
        }
    } else {
        // No parliament struct — return empty snapshot.
        ParliamentSnapshot::default()
    }
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
    let sector_pmi: HashMap<Sector, f64> = country.budget.sectors.iter()
        .filter_map(|(sector, share)| {
            share.extra.get("pmi").and_then(|v| v.as_f64()).map(|pmi| (*sector, pmi))
        })
        .collect();

    // Phase 28: Read previous-turn employment and wage for ToT computation.
    let prev_employment: HashMap<Sector, f64> = country.budget.sectors.iter()
        .filter_map(|(sector, share)| {
            share.extra.get("_prev_employment").and_then(|v| v.as_f64()).map(|e| (*sector, e))
        })
        .collect();
    let prev_avg_wage: HashMap<Sector, f64> = country.budget.sectors.iter()
        .filter_map(|(sector, share)| {
            share.extra.get("_prev_avg_wage").and_then(|v| v.as_f64()).map(|w| (*sector, w))
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
                if *prev > 0.0 { Some(((fte_f - *prev) / *prev) * 100.0) } else { None }
            });
            let wage_tot = prev_avg_wage.get(sector).and_then(|prev| {
                if *prev > 0.0 { Some(((avg_wage - *prev) / *prev) * 100.0) } else { None }
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
    rows.sort_by(|a, b| b.pct_gdp_share.partial_cmp(&a.pct_gdp_share).unwrap_or(std::cmp::Ordering::Equal));
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
        unemployment_tot: history.previous_turn().map(|s| md.labor_market.unemployment_rate - s.unemployment_pct),
        unemployment_yoy: history.one_year_ago().map(|s| md.labor_market.unemployment_rate - s.unemployment_pct),
        shadow_gdp_tot: history.tot_pct(g.shadow_gdp, |s| s.shadow_gdp),
        shadow_gdp_yoy: history.yoy_pct(g.shadow_gdp, |s| s.shadow_gdp),
        // Corruption: absolute delta (index points).
        corruption_tot: history.previous_turn().map(|s| corruption_index - s.corruption_index),
        corruption_yoy: history.one_year_ago().map(|s| corruption_index - s.corruption_index),
        population_tot: history.tot_pct(population, |s| s.population as f64),
        population_yoy: history.yoy_pct(population, |s| s.population as f64),
        wage_tot: history.tot_pct(md.average_wage, |s| s.average_wage),
        wage_yoy: history.yoy_pct(md.average_wage, |s| s.average_wage),
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
        let buildings = buildings_by_country.get(name).map(|v| v.as_slice()).unwrap_or(&[]);
        let companies = companies_by_country.get(name).map(|v| v.as_slice()).unwrap_or(&[]);
        let mut snap = build_country_snapshot(country, market_history, market, buildings, companies, view);
        snap.turn = turn;
        snap.year = year;
        countries.insert(name.clone(), snap);
    }

    GlobalSnapshot { turn, year, countries, diplomacy: None, foreign_countries: Vec::new() }
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
    /// War morale (0.0–100.0).
    pub war_morale: f64,
    /// Mental health (0.0–100.0).
    pub mental_health: f64,
    /// Population.
    pub population: i64,
    /// Whether strikes are active.
    pub strikes_active: bool,
    /// Whether desertions are active.
    pub desertions_active: bool,
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
    let pow_camps = Vec::new();

    for (country_name, country) in &state.countries {
        // Active wars/fronts
        for front in &country.military_fronts {
            let battle_count = front.battles.len();
            let last_battle_result = front.battles.last()
                .map(|b| format!("{:?}", b.result))
                .unwrap_or_else(|| "No battles".to_string());

            wars.push(WarRow {
                front_id: front.id.clone(),
                front_name: front.name.clone(),
                involved_countries: front.involved_countries.clone(),
                regions: front.regions.clone(),
                war_exhaustion: front.war_exhaustion.iter()
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
        let regiment_count: usize = oob.armies.iter()
            .flat_map(|a| a.divisions.iter())
            .map(|d| d.regiments.len())
            .sum();
        let unit_count = oob.unit_count();

        // Count units by type
        let mut units_by_type: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        let mut manpower_by_type: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
        for unit in oob.flatten() {
            let type_name = format!("{:?}", unit.unit_type);
            *units_by_type.entry(type_name.clone()).or_insert(0) += 1;
            *manpower_by_type.entry(type_name).or_insert(0) += unit.manpower;
        }

        army_compositions.push(ArmyCompositionRow {
            country: country_name.clone(),
            total_manpower,
            army_count,
            division_count,
            regiment_count,
            unit_count,
            units_by_type: units_by_type.into_iter().collect(),
            manpower_by_type: manpower_by_type.into_iter().collect(),
        });

        // War morale per class — regions is a Vec<Region>
        for region in &country.regions {
            for (class_name, demo) in &region.class_demographics.rural_classes {
                war_morale.push(MoraleRow {
                    country: country_name.clone(),
                    region_id: region.id.clone(),
                    class_name: class_name.clone(),
                    war_morale: demo.war_morale,
                    mental_health: demo.mental_health,
                    population: demo.population,
                    strikes_active: demo.war_morale < 30.0,
                    desertions_active: demo.war_morale < 15.0,
                });
            }
        }

        // POW camp stats — POW camps are managed in the turn context, not on Country.
        // The dashboard exposes zero-row POW data when no camps are active.
        // (POW data would be populated from the turn context's POW registry if present.)
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
        let supply = grid.region_supply_mw.get(&region.id).copied().unwrap_or(0.0);
        let demand = grid.region_demand_mw.get(&region.id).copied().unwrap_or(0.0);
        let max_cap = grid.region_max_capacity_mw.get(&region.id).copied().unwrap_or(0.0);
        let spot = grid.spot_prices.get(&region.id).copied().unwrap_or(0.0);
        let shed = grid.load_shed_tiers.get(&region.id).copied().unwrap_or(LoadShedTier::Normal);
        let overprod = grid.overproduction_tiers.get(&region.id).copied().unwrap_or(OverproductionTier::Normal);
        let lv_cond = grid.region_lv_condition.get(&region.id).copied().unwrap_or(1.0);
        let mv_cond = grid.region_mv_condition.get(&region.id).copied().unwrap_or(1.0);
        let grid_cond = lv_cond.min(mv_cond);

        // Bugfix Sprint: Compute effective supply (after grid capacity clamping)
        // to expose the grid bottleneck that causes Blackout despite surplus.
        let lv_cap = grid.region_lv_capacity.get(&region.id).copied().unwrap_or(0.0);
        let mv_cap = grid.region_mv_capacity.get(&region.id).copied().unwrap_or(0.0);
        let grid_cap = lv_cap.min(mv_cap);
        let effective_supply = supply.min(grid_cap);

        national_supply += supply;
        national_demand += demand;

        regions.push(RegionEnergyInfo {
            region_id: region.id.clone(),
            region_name: if region.display_name.is_empty() { region.id.clone() } else { region.display_name.clone() },
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
            let lv = grid.region_lv_condition.get(&region.id).copied().unwrap_or(1.0);
            let mv = grid.region_mv_condition.get(&region.id).copied().unwrap_or(1.0);
            sum += lv.min(mv);
            count += 1.0;
        }
        if count > 0.0 { sum / count } else { 1.0 }
    };

    // National tiers (worst case across regions).
    let national_shed = grid.load_shed_tiers.values().max().cloned().unwrap_or(LoadShedTier::Normal);
    let national_overprod = grid.overproduction_tiers.values().max().cloned().unwrap_or(OverproductionTier::Normal);

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

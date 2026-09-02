//! Save file serialization: loads and saves the game state from/to JSON files.
//!
//! The engine persists per-country state across several JSON files, each keyed
//! by country name: `budgets.json`, `macro.json`, `tax_rates.json`,
//! `politics.json`, `currencies.json`, `geology.json`, `transport_networks.json`,
//! and `storage.json`. This module deserializes those files and joins the
//! slices into [`Country`] / [`GameState`] values.

use crate::politics::Politics;
use crate::state::{Country, Currency, GameState, MacroData, TaxRates, TradePolicy, Treasury};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::fs;
use std::path::Path;

/// Error type for save-file loading.
#[derive(Debug)]
pub enum SaveError {
    /// The file could not be read from disk.
    Io(std::io::Error),
    /// The file contents could not be parsed as the expected schema.
    Json(serde_json::Error),
    /// A requested country was not present in a save file.
    MissingCountry(String),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::Io(e) => write!(f, "I/O error: {e}"),
            SaveError::Json(e) => write!(f, "JSON error: {e}"),
            SaveError::MissingCountry(name) => write!(f, "country not found in save: {name}"),
        }
    }
}

impl std::error::Error for SaveError {}

impl From<std::io::Error> for SaveError {
    fn from(e: std::io::Error) -> Self {
        SaveError::Io(e)
    }
}

impl From<serde_json::Error> for SaveError {
    fn from(e: serde_json::Error) -> Self {
        SaveError::Json(e)
    }
}

/// Deserializes a save file that is a JSON object keyed by country name.
///
/// # Arguments
/// * `path` - Path to a save file such as `data/budgets.json`.
///
/// # Returns
/// `Ok(HashMap<String, T>)` mapping country name to the typed slice `T`, or a
/// [`SaveError`] on read/parse failure.
pub fn load_named_map<T: DeserializeOwned>(path: &Path) -> Result<HashMap<String, T>, SaveError> {
    let text = fs::read_to_string(path)?;
    let map = serde_json::from_str(&text)?;
    Ok(map)
}

/// Loads the full game state by joining every country present in the saves.
///
/// # Arguments
/// * `data_dir` - Directory containing the standard per-country save files.
///
/// # Returns
/// `Ok(GameState)` whose `countries` map is keyed by name, or a [`SaveError`].
///
/// # Rules
/// * A country must appear in `budgets.json` to be included; its `macro`,
///   `tax_rates`, and `politics` slices are joined when present. A missing
///   macro/tax/politics slice for a budgeted country is treated as a
///   [`SaveError::MissingCountry`].
/// * No migration or fallback logic is applied — the save must match the
///   current English-only schema.
pub fn load_game_state(data_dir: &Path) -> Result<GameState, SaveError> {
    let budgets: HashMap<String, Treasury> = load_named_map(&data_dir.join("budgets.json"))?;
    let macro_map: HashMap<String, MacroData> = load_named_map(&data_dir.join("macro.json"))?;
    let mut taxes: HashMap<String, TaxRates> = load_named_map(&data_dir.join("tax_rates.json"))?;
    let politics_map: HashMap<String, Politics> =
        load_named_map(&data_dir.join("politics.json")).unwrap_or_default();

    let mut state = GameState::new();
    state.currencies =
        load_named_map::<Currency>(&data_dir.join("currencies.json")).unwrap_or_default();

    for (name, budget) in budgets {
        let macro_indicators = macro_map
            .get(&name)
            .cloned()
            .ok_or_else(|| SaveError::MissingCountry(name.clone()))?;
        let tax_rates = taxes
            .remove(&name)
            .ok_or_else(|| SaveError::MissingCountry(name.clone()))?;
        let politics = politics_map.get(&name).cloned().unwrap_or_default();

        state.countries.insert(
            name.clone(),
            Country {
                name,
                budget,
                macro_indicators,
                tax_rates,
                trade_policy: TradePolicy::default(),
                politics,
                regions: Vec::new(),
                megaregions: Vec::new(),
                is_rebellion: false,
                mother_country: None,
                rebellion_type: None,
                rebellion_goals: None,
                economic_policy: crate::state::EconomicPolicy::default(),
                order_of_battle: crate::military::oob::OrderOfBattle::default(),
                military_fronts: Vec::new(),
                military_stockpile: rustc_hash::FxHashMap::default(),
                military_config: crate::military::config::MilitaryCombatConfig::default(),
                pow_camp: crate::military::pows::PowCamp::default(),
                morale_config: crate::military::morale::MoraleConfig::default(),
                guild_config: crate::economy::guild_system::GuildConfig::default(),
                war_economy: crate::military::war_economy::WarEconomyState::default(),
                at_war_with: Vec::new(),
                pending_defense_orders: Vec::new(),
                rationing_system: crate::state::RationingSystem::default(),
                emergency_powers: crate::state::EmergencyPowers::default(),
                emergency_escalation_counter: 0,
                emergency_deescalation_counter: 0,
                ministry_public_service_pool: 0.0,
                intelligence_budget: crate::state::IntelligenceBudget::default(),
                active_lobbying_operations: Vec::new(),
                central_bank: crate::state::CentralBank::default(),
                currency_zone: None,
                interbank_market: crate::state::InterbankMarket::default(),
                bfg_fund: crate::state::BfgFund::default(),
                sobk_scheme: crate::state::SobkScheme::default(),
                bank_resolution: crate::state::BankResolution::default(),
                bank_tax: crate::state::BankTax::default(),
                stock_exchange: crate::securities::StockExchange::default(),
                dividend_queue: Vec::new(), ipo_queue: Vec::new(), bankruptcy_auction_pool: crate::corporate::BankruptcyAuctionPool::default(), demolition_queue: Vec::new(), halt_queue: Vec::new(), furlough_wage_queue: Vec::new(), recruitment_cost_queue: Vec::new(),
                knf: crate::securities::KNF::default(),
                capital_gains_tax: crate::state::capital_gains_tax::CapitalGainsTaxRegistry::default(),
                sovereign_default_turns_remaining: 0,
                foreign_debt: 0.0,
                minimum_wage: None,
                debt_market: crate::economy::debt_market::DebtMarket::default(),
                cultural_institutions: Vec::new(),
                cooperative_federations: Vec::new(),
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
                mining_concessions: crate::economy::production::geology::MiningConcessionRegistry::default(),
                geological_survey_ledger: crate::economy::production::geology::GeologicalSurveyLedger::default(),
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
                accumulated_pit: 0.0,
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
                ppa_registry: crate::energy::types::PpaRegistry::default(),
                turn_config: crate::engine::turn_config::TurnConfig::default(),
                market_clearing_config: crate::economy::market::clearing_config::MarketClearingConfig::default(),
                labor_config: crate::economy::labor::labor_config::LaborConfig::default(),
                geography_config: crate::society::geography_config::GeographyConfig::default(),
                municipal_infrastructure_plan: crate::energy::municipal_infrastructure_ai::MunicipalInfrastructurePlan::default(),
                state_customs_warehouse: rustc_hash::FxHashMap::default(),
                last_smuggling_result: None,
                pending_foreign_transit_fees: Vec::new(),
            },
        );
    }

    // Load geological formations from geology.json.
    let geology_path = data_dir.join("geology.json");
    if geology_path.exists() {
        if let Ok(geology_text) = fs::read_to_string(&geology_path) {
            if let Ok(geology_map) = serde_json::from_str::<
                HashMap<String, Vec<crate::society::geography::GeologicalFormation>>,
            >(&geology_text)
            {
                for (name, formations) in geology_map {
                    if let Some(country) = state.countries.get_mut(&name) {
                        country.geological_formations = formations;
                    }
                }
            }
        }
    }

    // Load transport networks from transport_networks.json.
    let transport_path = data_dir.join("transport_networks.json");
    if transport_path.exists() {
        if let Ok(transport_text) = fs::read_to_string(&transport_path) {
            if let Ok(transport_map) = serde_json::from_str::<
                HashMap<
                    String,
                    crate::economy::logistics::transport_networks::TransportNetworkOverlay,
                >,
            >(&transport_text)
            {
                for (name, networks) in transport_map {
                    if let Some(country) = state.countries.get_mut(&name) {
                        country.transport_networks = networks;
                    }
                }
            }
        }
    }

    // Populate state.calendar from storage.json.
    let storage_path = data_dir.join("storage.json");
    if storage_path.exists() {
        if let Ok(storage_text) = fs::read_to_string(&storage_path) {
            if let Ok(storage_value) = serde_json::from_str::<Value>(&storage_text) {
                let turn = storage_value
                    .get("current_turn")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                let year = storage_value
                    .get("year")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1900) as u32;
                state.calendar.global_turn = turn;
                state.calendar.current_year = year;
                state.calendar.current_month = if turn > 0 {
                    ((turn - 1) % 24) / 2 + 1
                } else {
                    1
                };
                state.calendar.half_month = turn > 0 && (turn - 1) % 2 == 1;
                state
                    .extra
                    .insert("current_turn".to_string(), Value::from(turn));
                state.extra.insert("year".to_string(), Value::from(year));
            }
        }
    }

    // World Generation & Climate Audit (v0.5.3): Ensure the climate-season
    // matrix is populated even for saves created before v0.5.3. If the matrix
    // is empty, fill it with defaults. Existing non-empty matrices are preserved
    // (they may contain custom scenario data).
    if state.climate_config.climate_season_matrix.is_empty() {
        state.climate_config.populate_defaults();
    }

    Ok(state)
}

/// Serializes a JSON object keyed by country name (or any top-level key) to a file.
///
/// # Arguments
/// * `path` - Path to the save file.
/// * `map` - Map to serialize.
///
/// # Returns
/// `Ok(())` on success, or a [`SaveError`] on I/O or JSON failure.
pub fn save_named_map<T: Serialize>(
    path: &Path,
    map: &HashMap<String, T>,
) -> Result<(), SaveError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(map)?;
    fs::write(path, text)?;
    Ok(())
}

/// Saves a `GameState` to the split-file save format.
///
/// # Arguments
/// * `data_dir` - Root directory containing the save files.
/// * `state` - Full game state to persist.
///
/// # Returns
/// `Ok(())` when all core files are written, or a [`SaveError`] on failure.
///
/// # Rules
/// * Writes `budgets.json`, `macro.json`, `tax_rates.json`, `politics.json`,
///   `currencies.json`, `geology.json`, `transport_networks.json`, and
///   `storage.json`.
pub fn save_game_state(data_dir: &Path, state: &GameState) -> Result<(), SaveError> {
    let mut budgets: HashMap<String, Treasury> = HashMap::new();
    let mut macro_map: HashMap<String, MacroData> = HashMap::new();
    let mut tax_rates: HashMap<String, TaxRates> = HashMap::new();
    let mut politics_map: HashMap<String, Politics> = HashMap::new();
    let mut geology: HashMap<String, Vec<crate::society::geography::GeologicalFormation>> =
        HashMap::new();
    let mut transport: HashMap<
        String,
        crate::economy::logistics::transport_networks::TransportNetworkOverlay,
    > = HashMap::new();
    for (name, country) in &state.countries {
        budgets.insert(name.clone(), country.budget.clone());
        macro_map.insert(name.clone(), country.macro_indicators.clone());
        tax_rates.insert(name.clone(), country.tax_rates.clone());
        politics_map.insert(name.clone(), country.politics.clone());
        geology.insert(name.clone(), country.geological_formations.clone());
        transport.insert(name.clone(), country.transport_networks.clone());
    }

    save_named_map(&data_dir.join("budgets.json"), &budgets)?;
    save_named_map(&data_dir.join("macro.json"), &macro_map)?;
    save_named_map(&data_dir.join("tax_rates.json"), &tax_rates)?;
    save_named_map(&data_dir.join("politics.json"), &politics_map)?;
    save_named_map(&data_dir.join("currencies.json"), &state.currencies)?;
    save_named_map(&data_dir.join("geology.json"), &geology)?;
    save_named_map(&data_dir.join("transport_networks.json"), &transport)?;

    let storage = if state.extra.is_empty() {
        serde_json::json!({})
    } else {
        Value::Object(state.extra.clone())
    };
    let storage_text = serde_json::to_string_pretty(&storage)?;
    fs::write(data_dir.join("storage.json"), storage_text)?;

    Ok(())
}

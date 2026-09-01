//! In-memory turn context, replacing all per-turn disk I/O.
//!
//! `InMemoryTurnContext` holds the global market, diplomatic relations, and
//! per-country entities in memory. It is loaded once at game start (or load
//! game) and persisted in Tauri's managed state. `run_turn_in_memory` operates
//! on this context without any disk I/O — disk access is reserved strictly for
//! explicit Save/Load actions via `load_from_disk` / `save_to_disk`.

use super::turn::TurnError;
use crate::economy::market::{GlobalMarket, MarketOrders};
use crate::economy::market_history::MarketHistory;
use crate::entities::{Building, Company, Union};
use crate::international::DiplomaticRelation;
use crate::io::entity_store::{DiskEntityStore, EntityStore};
use crate::registries::enums::Commodity;
use crate::society::housing::{
    CommercialBuilding, CommercialBuildingType, HousingBuilding, HousingType,
};
use crate::state::GameState;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

// ============================================================================
// CONTEXT STRUCTS
// ============================================================================

/// In-memory context for `run_turn_in_memory`, replacing ALL per-turn disk I/O.
/// Persisted in Tauri's managed state alongside `GameState`.
/// Also used by integration tests (loaded from disk at test setup).
#[derive(Debug, Clone)]
pub struct InMemoryTurnContext {
    /// Global market (was loaded from `market.json` each turn).
    pub market: GlobalMarket,
    /// Diplomatic relations matrix (was loaded from `diplomacy.json` each turn).
    pub diplomacy: HashMap<String, HashMap<String, DiplomaticRelation>>,
    /// Per-country entities (were loaded from `entities/<country>/` each turn).
    pub entities: HashMap<String, CountryEntities>,
}

/// All entity collections for a single country.
#[derive(Debug, Clone, Default)]
pub struct CountryEntities {
    /// Companies belonging to this country.
    pub companies: Vec<Company>,
    /// Spatial-registry buildings (factories, infrastructure, etc.).
    pub buildings: Vec<Building>,
    /// Trade unions.
    pub unions: Vec<Union>,
    /// Commercial buildings (retail, offices, hotels, etc.).
    pub commercial_buildings: Vec<CommercialBuilding>,
    /// Housing buildings.
    pub housing_buildings: Vec<HousingBuilding>,
}

impl InMemoryTurnContext {
    /// Load all context from disk. Used at game start, load game, and test setup.
    /// This is the ONLY disk I/O path — called once, not per-turn.
    ///
    /// # Arguments
    /// * `data_dir` - Path to the root save directory.
    /// * `state` - Mutable game state (regions, megaregions, market history, and
    ///   turn/year are loaded into it).
    ///
    /// # Returns
    /// A fully populated `InMemoryTurnContext`, or a `TurnError` on failure.
    pub fn load_from_disk(data_dir: &Path, state: &mut GameState) -> Result<Self, TurnError> {
        let market = load_market(data_dir)?;
        let diplomacy = load_diplomacy(data_dir)?;

        // Load regions, megaregions, and market history into state.
        load_regions_into_state(data_dir, state)?;
        load_megaregions_into_state(data_dir, state)?;
        load_market_history_into_state(data_dir, state);

        // Capture prev_net_surplus at the START of the turn.
        state.market_history.prev_net_surplus = market.net_surplus.clone();

        // Load per-country entities.
        let mut entities = HashMap::new();
        for name in state.countries.keys() {
            let mut companies = load_companies(data_dir, name)?;
            // Backfill empty region_id for banks.
            if let Some(country) = state.countries.get(name) {
                if let Some(capital_region) = country
                    .regions
                    .iter()
                    .find(|r| r.is_capital)
                    .or_else(|| country.regions.first())
                {
                    for company in &mut companies {
                        if company.sector == crate::registries::enums::Sector::Banking
                            && company.region_id.is_empty()
                        {
                            company.region_id = capital_region.id.clone();
                        }
                    }
                }
            }
            let mut buildings = load_buildings(data_dir, name)?;
            // Migrate legacy Polish building names to English.
            for b in &mut buildings {
                if b.name == "forest_district" {
                    b.name = "forest_district".to_string();
                }
            }
            let mut unions = load_unions(data_dir, name)?;
            let mut commercial_buildings = load_commercial_buildings(data_dir, name)?;
            let mut housing_buildings = load_housing_buildings(data_dir, name)?;

            companies.sort_by(|a, b| a.id.cmp(&b.id));
            buildings.sort_by(|a, b| a.id.cmp(&b.id));
            unions.sort_by(|a, b| a.id.cmp(&b.id));
            commercial_buildings.sort_by(|a, b| a.id.cmp(&b.id));
            housing_buildings.sort_by(|a, b| a.id.cmp(&b.id));

            entities.insert(
                name.clone(),
                CountryEntities {
                    companies,
                    buildings,
                    unions,
                    commercial_buildings,
                    housing_buildings,
                },
            );
        }

        Ok(Self {
            market,
            diplomacy,
            entities,
        })
    }

    /// Save all context to disk. Used for explicit Save Game action only.
    ///
    /// # Arguments
    /// * `data_dir` - Path to the root save directory.
    /// * `state` - Game state (for telemetry export and state.extra update).
    /// * `global_orders` - Global market orders (for market.json persistence).
    /// * `trade_result` - Trade balance result (for res_stats in market.json).
    ///
    /// # Returns
    /// `Ok(())` on success, or a `TurnError` on failure.
    pub fn save_to_disk(
        &self,
        data_dir: &Path,
        state: &GameState,
        global_orders: &MarketOrders,
        trade_result: &crate::international::TradeBalanceResult,
    ) -> Result<(), TurnError> {
        // Save per-country entities.
        for (country_name, ents) in &self.entities {
            save_companies(data_dir, country_name, &ents.companies)?;
            save_buildings(data_dir, country_name, &ents.buildings)?;
            save_commercial_buildings(data_dir, country_name, &ents.commercial_buildings)?;
            save_housing_buildings(data_dir, country_name, &ents.housing_buildings)?;
            save_unions(data_dir, country_name, &ents.unions)?;
        }

        // Telemetry CSV export.
        let turn = state.calendar.global_turn;
        let year = state.calendar.current_year;
        for (country_name, country) in &state.countries {
            let _ = crate::io::telemetry_export::append_telemetry_row(
                data_dir,
                country_name,
                country,
                turn,
                year,
            );
        }

        // Persist diplomacy matrix.
        let diplomacy_path = data_dir.join("diplomacy.json");
        crate::io::save_named_map(&diplomacy_path, &self.diplomacy)
            .map_err(|e| TurnError::Io(std::io::Error::other(e.to_string())))?;

        // Persist market history.
        let mh_path = data_dir.join("market_history.json");
        let mh_text = serde_json::to_string_pretty(&state.market_history)
            .map_err(|e| TurnError::Io(std::io::Error::other(e.to_string())))?;
        fs::write(&mh_path, mh_text).map_err(TurnError::Io)?;

        // Persist market.json.
        save_market(data_dir, &self.market, global_orders, trade_result)?;

        Ok(())
    }
}

// ============================================================================
// LOAD FUNCTIONS (private — called only by load_from_disk)
// ============================================================================

/// Serialized shape of a single order in `market.json`.
#[derive(Debug, Deserialize, Serialize)]
struct MarketOrderJson {
    buy: f64,
    sell: f64,
}

/// Serialized trade stats for one country in `market.json`.
#[derive(Debug, Deserialize, Serialize)]
struct MarketTradeStatsJson {
    export: f64,
    import: f64,
    net: f64,
}

/// Serialized shape of `market.json`.
#[derive(Debug, Deserialize, Serialize)]
struct MarketJson {
    #[serde(default)]
    prices: FxHashMap<Commodity, f64>,
    #[serde(default)]
    orders: HashMap<Commodity, MarketOrderJson>,
    #[serde(default)]
    res_stats: HashMap<String, MarketTradeStatsJson>,
}

fn load_market(data_dir: &Path) -> Result<GlobalMarket, TurnError> {
    let path = data_dir.join("market.json");
    if !path.exists() {
        return Ok(default_market());
    }
    let text = fs::read_to_string(&path)?;
    let parsed: MarketJson = serde_json::from_str(&text)?;
    let mut base_prices = default_price_map();
    for (good, price) in parsed.prices {
        base_prices.insert(good, price);
    }
    let mut net_surplus = FxHashMap::default();
    let mut supply_volume = FxHashMap::default();
    let mut demand_volume = FxHashMap::default();
    for (good, order) in parsed.orders {
        base_prices.entry(good).or_insert(100.0);
        net_surplus.insert(good, order.sell - order.buy);
        supply_volume.insert(good, order.sell);
        demand_volume.insert(good, order.buy);
    }
    Ok(GlobalMarket {
        base_prices,
        net_surplus,
        offshore_capital: 0.0,
        apostolic_see_ledger: crate::economy::market::ApostolicSeeLedger::default(),
        supply_volume,
        demand_volume,
        net_trade: FxHashMap::default(),
        b2c_demand_volume: FxHashMap::default(),
        foreign_patent_fee_ledger: 0.0,
    })
}

fn load_diplomacy(
    data_dir: &Path,
) -> Result<HashMap<String, HashMap<String, DiplomaticRelation>>, TurnError> {
    let path = data_dir.join("diplomacy.json");
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let text = fs::read_to_string(&path)?;
    let map = serde_json::from_str(&text)?;
    Ok(map)
}

fn load_regions_into_state(data_dir: &Path, state: &mut GameState) -> Result<(), TurnError> {
    let path = data_dir.join("regions.json");
    if !path.exists() {
        return Ok(());
    }
    let text = fs::read_to_string(&path)?;
    let all_regions: HashMap<String, crate::society::geography::Region> =
        serde_json::from_str(&text)?;

    for country in state.countries.values_mut() {
        country.regions = all_regions
            .values()
            .filter(|r| r.owner_country == country.name)
            .cloned()
            .collect();
        country.regions.sort_by(|a, b| a.id.cmp(&b.id));

        let country_name = country.name.clone();
        for region in &mut country.regions {
            if region.governance.is_none() {
                region.governance = Some(
                    crate::politics::local_government::initialize_regional_governance(
                        &region.id,
                        &country_name,
                    ),
                );
            }
        }

        crate::economy::labor::labor::reconcile_population_bottom_up(country);
    }

    Ok(())
}

fn load_megaregions_into_state(data_dir: &Path, state: &mut GameState) -> Result<(), TurnError> {
    let path = data_dir.join("megaregions.json");
    if !path.exists() {
        return Ok(());
    }
    let text = fs::read_to_string(&path)?;
    let all_megaregions: HashMap<String, crate::society::geography::Megaregion> =
        serde_json::from_str(&text)?;

    for country in state.countries.values_mut() {
        country.megaregions = all_megaregions
            .values()
            .filter(|m| m.country == country.name)
            .cloned()
            .collect();
        country.megaregions.sort_by(|a, b| a.id.cmp(&b.id));
    }

    Ok(())
}

fn load_market_history_into_state(data_dir: &Path, state: &mut GameState) {
    let path = data_dir.join("market_history.json");
    if path.exists() {
        if let Ok(text) = fs::read_to_string(&path) {
            if let Ok(history) = serde_json::from_str::<MarketHistory>(&text) {
                state.market_history = history;
            }
        }
    }

    if state.market_history.global_base_prices.is_empty() {
        let market_path = data_dir.join("market.json");
        if market_path.exists() {
            if let Ok(text) = fs::read_to_string(&market_path) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(prices) = parsed.get("prices").and_then(|v| v.as_object()) {
                        for (key, value) in prices {
                            if let Ok(commodity) =
                                serde_json::from_str::<Commodity>(&format!("\"{}\"", key))
                            {
                                if let Some(price) = value.as_f64() {
                                    state
                                        .market_history
                                        .global_base_prices
                                        .insert(commodity, price);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn load_companies(data_dir: &Path, country: &str) -> Result<Vec<Company>, TurnError> {
    let mut companies = Vec::new();
    let companies_dir = data_dir.join("entities").join(country).join("companies");

    if !companies_dir.exists() {
        panic!(
            "CRITICAL DATA ERROR: Companies directory does not exist for country '{}'. \
             Path: {}. This indicates a data generation or export failure. \
             Fix the data pipeline at the source - do not patch with procedural generation.",
            country,
            companies_dir.display()
        );
    }

    let store = DiskEntityStore::<Company>::new(data_dir);
    let entries = match fs::read_dir(&companies_dir) {
        Ok(e) => e,
        Err(e) => {
            panic!(
                "CRITICAL DATA ERROR: Could not read companies directory for country '{}': {}. \
                 Path: {}. Fix the data pipeline at the source.",
                country,
                e,
                companies_dir.display()
            );
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let sector = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if sector.is_empty() {
            continue;
        }
        let loaded = match store.load_sector(country, &sector, None) {
            Ok(l) => l,
            Err(e) => {
                panic!(
                    "CRITICAL DATA ERROR: Could not load company sector '{}' for country '{}': {}. \
                     Path: {}. Fix the data pipeline at the source.",
                    sector,
                    country,
                    e,
                    path.display()
                );
            }
        };
        companies.extend(loaded);
    }

    if companies.is_empty() {
        panic!(
            "CRITICAL DATA ERROR: No companies loaded for country '{}'. \
             Directory exists but contains no valid JSON sector files. \
             Path: {}. Fix the data pipeline at the source.",
            country,
            companies_dir.display()
        );
    }

    Ok(companies)
}

fn load_commercial_buildings(
    data_dir: &Path,
    country: &str,
) -> Result<Vec<CommercialBuilding>, TurnError> {
    let mut commercial_buildings = Vec::new();
    let commercial_dir = data_dir.join("entities").join(country).join("commercial");

    if !commercial_dir.exists() {
        return Ok(commercial_buildings);
    }

    let store = DiskEntityStore::<CommercialBuilding>::new(data_dir);
    let entries = match fs::read_dir(&commercial_dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "Warning: Could not read commercial buildings directory for {}: {}",
                country, e
            );
            return Ok(commercial_buildings);
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let sector = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if sector.is_empty() {
            continue;
        }
        let loaded = match store.load_sector(country, &sector, None) {
            Ok(l) => l,
            Err(e) => {
                eprintln!(
                    "Warning: Could not load commercial building sector '{}' for country '{}': {}",
                    sector, country, e
                );
                continue;
            }
        };
        commercial_buildings.extend(loaded);
    }

    Ok(commercial_buildings)
}

fn load_housing_buildings(
    data_dir: &Path,
    country: &str,
) -> Result<Vec<HousingBuilding>, TurnError> {
    let mut housing_buildings = Vec::new();
    let housing_dir = data_dir.join("entities").join(country).join("housing");

    if !housing_dir.exists() {
        return Ok(housing_buildings);
    }

    let store = DiskEntityStore::<HousingBuilding>::new(data_dir);
    let entries = match fs::read_dir(&housing_dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "Warning: Could not read housing buildings directory for {}: {}",
                country, e
            );
            return Ok(housing_buildings);
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let sector = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if sector.is_empty() {
            continue;
        }
        let loaded = match store.load_sector(country, &sector, None) {
            Ok(l) => l,
            Err(e) => {
                eprintln!(
                    "Warning: Could not load housing building sector '{}' for country '{}': {}",
                    sector, country, e
                );
                continue;
            }
        };
        housing_buildings.extend(loaded);
    }

    Ok(housing_buildings)
}

fn load_unions(data_dir: &Path, country: &str) -> Result<Vec<Union>, TurnError> {
    let mut unions = Vec::new();
    let unions_dir = data_dir.join("entities").join(country).join("unions");

    if !unions_dir.exists() {
        return Ok(unions);
    }

    let entries = match fs::read_dir(&unions_dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "Warning: Could not read unions directory for {}: {}",
                country, e
            );
            return Ok(unions);
        }
    };

    let store = DiskEntityStore::<Union>::new(data_dir);

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let sector = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if sector.is_empty() {
            continue;
        }
        let loaded = match store.load_sector(country, "unions", Some(&sector)) {
            Ok(l) => l,
            Err(e) => {
                eprintln!(
                    "Warning: Could not load union sector {} for {}: {}",
                    sector, country, e
                );
                continue;
            }
        };
        unions.extend(loaded);
    }
    Ok(unions)
}

fn load_buildings(data_dir: &Path, country: &str) -> Result<Vec<Building>, TurnError> {
    let mut buildings = Vec::new();
    let spatial_dir = data_dir.join("spatial_registry").join(country);
    if !spatial_dir.exists() {
        return Ok(buildings);
    }
    let store = DiskEntityStore::<Building>::new(data_dir);
    let entries = match fs::read_dir(&spatial_dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "Warning: Could not read spatial registry directory for {}: {}",
                country, e
            );
            return Ok(buildings);
        }
    };
    for region_entry in entries {
        let region_entry = match region_entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let region_path = region_entry.path();
        if !region_path.is_dir() {
            continue;
        }
        let region = region_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if region.is_empty() {
            continue;
        }
        let buildings_dir = region_path.join("buildings");
        if !buildings_dir.exists() {
            continue;
        }
        let building_entries = match fs::read_dir(&buildings_dir) {
            Ok(e) => e,
            Err(e) => {
                eprintln!(
                    "Warning: Could not read buildings directory for region {} in {}: {}",
                    region, country, e
                );
                continue;
            }
        };
        for entry in building_entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let sector = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if sector.is_empty() {
                continue;
            }
            let mut sector_buildings = match store.load_sector(country, &sector, Some(&region)) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!(
                        "Warning: Could not load building sector {} for region {} in {}: {}",
                        sector, region, country, e
                    );
                    continue;
                }
            };
            buildings.append(&mut sector_buildings);
        }
    }
    Ok(buildings)
}

// ============================================================================
// SAVE FUNCTIONS (private — called only by save_to_disk)
// ============================================================================

fn save_market(
    data_dir: &Path,
    market: &GlobalMarket,
    orders: &MarketOrders,
    trade_result: &crate::international::TradeBalanceResult,
) -> Result<(), TurnError> {
    let mut prices = default_price_map();
    for (&good, &price) in &market.base_prices {
        prices.insert(good, price);
    }
    let mut orders_map = HashMap::new();
    for (&good, order) in &orders.orders {
        orders_map.insert(
            good,
            MarketOrderJson {
                buy: order.buy,
                sell: order.sell,
            },
        );
    }
    let mut res_stats = HashMap::new();
    for delta in &trade_result.deltas {
        res_stats.insert(
            delta.country_name.clone(),
            MarketTradeStatsJson {
                export: delta.exports,
                import: delta.imports,
                net: delta.trade_balance,
            },
        );
    }
    let market_json = MarketJson {
        prices,
        orders: orders_map,
        res_stats,
    };
    // Persist supply/demand volumes for Market UI continuity.
    let sv_path = data_dir.join("market_volumes.json");
    let volumes_json = serde_json::json!({
        "supply_volume": market.supply_volume.iter().collect::<HashMap<_, _>>(),
        "demand_volume": market.demand_volume.iter().collect::<HashMap<_, _>>(),
    });
    let _ = fs::write(
        sv_path,
        serde_json::to_string_pretty(&volumes_json).unwrap_or_default(),
    );
    let path = data_dir.join("market.json");
    fs::write(path, serde_json::to_string_pretty(&market_json)?)?;
    Ok(())
}

fn save_companies(data_dir: &Path, country: &str, companies: &[Company]) -> Result<(), TurnError> {
    let store = DiskEntityStore::<Company>::new(data_dir);
    let mut by_file_stem: HashMap<String, Vec<Company>> = HashMap::new();
    for company in companies {
        let file_stem = if company.file_stem.is_empty() {
            sector_name(&company.sector)
        } else {
            company.file_stem.clone()
        };
        by_file_stem
            .entry(file_stem)
            .or_default()
            .push(company.clone());
    }
    for (file_stem, list) in by_file_stem {
        store.save_sector(country, &file_stem, None, &list)?;
    }
    Ok(())
}

fn save_unions(data_dir: &Path, country: &str, unions: &[Union]) -> Result<(), TurnError> {
    if unions.is_empty() {
        return Ok(());
    }
    let unions_dir = data_dir.join("entities").join(country).join("unions");
    if !unions_dir.exists() && fs::create_dir_all(&unions_dir).is_err() {
        return Ok(());
    }
    let store = DiskEntityStore::<Union>::new(data_dir);
    let mut by_sector: HashMap<String, Vec<Union>> = HashMap::new();
    for union in unions {
        let sector = sector_name(&union.sector);
        by_sector.entry(sector).or_default().push(union.clone());
    }
    for (sector, list) in by_sector {
        if list.is_empty() {
            continue;
        }
        if store
            .save_sector(country, "unions", Some(&sector), &list)
            .is_err()
        {
            // Ignore errors - unions are optional
        }
    }
    Ok(())
}

fn save_buildings(data_dir: &Path, country: &str, buildings: &[Building]) -> Result<(), TurnError> {
    let store = DiskEntityStore::<Building>::new(data_dir);
    let mut by_key: HashMap<(String, String), Vec<Building>> = HashMap::new();
    for building in buildings {
        let sector = sector_name(&building.sector);
        let region = building.region_id.clone();
        by_key
            .entry((sector, region))
            .or_default()
            .push(building.clone());
    }
    for ((sector, region), list) in by_key {
        store.save_sector(country, &sector, Some(&region), &list)?;
    }
    Ok(())
}

fn save_commercial_buildings(
    data_dir: &Path,
    country: &str,
    commercial_buildings: &[CommercialBuilding],
) -> Result<(), TurnError> {
    let store = DiskEntityStore::<CommercialBuilding>::new(data_dir);

    let mut by_type: std::collections::HashMap<CommercialBuildingType, Vec<CommercialBuilding>> =
        std::collections::HashMap::new();

    for building in commercial_buildings {
        by_type
            .entry(building.building_type)
            .or_default()
            .push(building.clone());
    }

    for (building_type, buildings) in by_type {
        let sector = match building_type {
            CommercialBuildingType::Office => "office",
            CommercialBuildingType::Retail => "retail",
            CommercialBuildingType::MixedUse => "mixed_use",
            CommercialBuildingType::Warehouse => "warehouse",
            CommercialBuildingType::Marketplace => "retail",
            CommercialBuildingType::Wholesaler => "retail",
            CommercialBuildingType::RetailStore => "retail",
            CommercialBuildingType::Supermarket => "retail",
            CommercialBuildingType::DepartmentStore => "retail",
            CommercialBuildingType::ShoppingCenter => "retail",
            CommercialBuildingType::Hotel => "hotel",
            CommercialBuildingType::Resort => "resort",
            CommercialBuildingType::Restaurant => "restaurant",
            CommercialBuildingType::Casino => "casino",
            CommercialBuildingType::GasStation => "gas_station",
        };
        store.save_sector(country, sector, None, &buildings)?;
    }

    Ok(())
}

fn save_housing_buildings(
    data_dir: &Path,
    country: &str,
    housing_buildings: &[HousingBuilding],
) -> Result<(), TurnError> {
    let store = DiskEntityStore::<HousingBuilding>::new(data_dir);

    let mut by_type: HashMap<HousingType, Vec<HousingBuilding>> = HashMap::new();

    for building in housing_buildings {
        by_type
            .entry(building.housing_type)
            .or_default()
            .push(building.clone());
    }

    for (housing_type, buildings) in by_type {
        let sector = match housing_type {
            HousingType::Hut => "hut",
            HousingType::Slum => "slum",
            HousingType::WorkersHousing => "workers_housing",
            HousingType::SkilledHousing => "skilled_housing",
            HousingType::Tenement => "tenement",
            HousingType::CityPalace => "city_palace",
            HousingType::Palace => "palace",
            HousingType::Rectory => "rectory",
            HousingType::Monastery => "monastery",
            HousingType::SocialHousing => "social_housing",
            HousingType::EstateHousing => "estate_housing",
        };
        store.save_sector(country, sector, None, &buildings)?;
    }

    Ok(())
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn default_market() -> GlobalMarket {
    GlobalMarket {
        base_prices: default_price_map(),
        net_surplus: FxHashMap::default(),
        offshore_capital: 0.0,
        apostolic_see_ledger: crate::economy::market::ApostolicSeeLedger::default(),
        supply_volume: FxHashMap::default(),
        demand_volume: FxHashMap::default(),
        net_trade: FxHashMap::default(),
        b2c_demand_volume: FxHashMap::default(),
        foreign_patent_fee_ledger: 0.0,
    }
}

fn default_price_map() -> FxHashMap<Commodity, f64> {
    let mut prices = FxHashMap::default();
    for commodity in Commodity::all() {
        prices.insert(commodity, 100.0);
    }
    prices
}

fn sector_name(sector: &crate::registries::enums::Sector) -> String {
    serde_json::to_value(sector)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| format!("{sector:?}"))
}

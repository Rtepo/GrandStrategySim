//! Procedural world generation for creating a new `Turn 0` state.
//!
//! This module ports the Python `engine/world_generator` pipeline: it creates
//! countries, demographics, economies, currencies, banks, diplomacy, regions
//! and megaregions from a small set of seed parameters and writes the result
//! to the standard split-file save layout.

#![allow(missing_docs)]

mod corporate;

use crate::international::generate_diplomacy;
use crate::io::save_manager::{save_game_state, save_named_map};
use crate::engine::generator::corporate::generate_corporate_entities;
use crate::politics::Politics;
use crate::registries::enums::{Commodity, Sector, WealthBracket};
use crate::registries::Registries;
use crate::society::cultures::{generate_cultural_background, CulturalBackground};
use crate::society::geography::{generate_land_registry, generate_megaregions, generate_regional_topology, LandRegistry, Megaregion, Region};
use crate::state::{Country, Currency, CurrencyPolicy, GameState, MacroData, TaxRates, Treasury};
use crate::state::banking::{BankBalanceSheet, BankType as BankingBankType, Loan, LoanStatus, LoanType, InterestType};
use crate::entities::{Company, LegalForm};
use crate::registries::enums::Sector as EntitySector;
use crate::state::macro_data::{AgeGroups, Demographics, Education, EnergyMix, Gender, LaborMarket, UnemploymentStructure};
use crate::state::tax::{IncomeTax, PublicDebt, VatBracket};
use crate::state::treasury::{BudgetAllocations, ProductionMethodChoice, ScienceState, SectorShare, StockMarket};
use rand::seq::SliceRandom;
use rand::Rng;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::path::Path;

const COUNTRY_NAMES: &[&str] = &[
    "Sarmatia", "Iliria", "Helwecja", "Nordia", "Baktria", "Persja", "Lechia", "Eldoria",
    "Wenedia", "Oksytania", "Galia", "Dacja", "Krasnowia", "Anatolia", "Iberia", "Anglia",
];

const WEALTH_WEIGHTS: &[i32] = &[15, 25, 35, 25];

/// Scenario year options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartYear {
    /// 1900 — Age of Steam and Coal.
    Y1900 = 1900,
    /// 1925 — Factories and Electricity.
    Y1925 = 1925,
    /// 1950 — Golden Age of Industry.
    Y1950 = 1950,
    /// 1975 — Dawn of the Silicon Age.
    Y1975 = 1975,
}

impl StartYear {
    /// GDP-per-capita multiplier applied to the chosen scenario year.
    pub fn year_multiplier(self) -> f64 {
        match self {
            StartYear::Y1900 => 0.8,
            StartYear::Y1925 => 1.2,
            StartYear::Y1950 => 2.5,
            StartYear::Y1975 => 4.5,
        }
    }

    /// Phase 44: Get the year as a u32 for era-aware generation logic.
    pub fn as_year(self) -> u32 {
        self as u32
    }

    /// Technology-count limit based on wealth bracket for this year.
    pub fn tech_limit(self, wealth: WealthBracket) -> usize {
        let map = match self {
            StartYear::Y1900 => [(WealthBracket::VeryHigh, 17), (WealthBracket::High, 10), (WealthBracket::Medium, 4), (WealthBracket::Low, 0)],
            StartYear::Y1925 => [(WealthBracket::VeryHigh, 31), (WealthBracket::High, 24), (WealthBracket::Medium, 10), (WealthBracket::Low, 3)],
            StartYear::Y1950 => [(WealthBracket::VeryHigh, 45), (WealthBracket::High, 38), (WealthBracket::Medium, 22), (WealthBracket::Low, 8)],
            StartYear::Y1975 => [(WealthBracket::VeryHigh, 64), (WealthBracket::High, 55), (WealthBracket::Medium, 38), (WealthBracket::Low, 20)],
        };
        map.iter().find(|(w, _)| *w == wealth).map(|(_, n)| *n).unwrap_or(0)
    }
}

/// Options that control the world generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerateOptions {
    /// Number of countries to create (4–16).
    pub country_count: usize,
    /// Starting historical scenario.
    pub start_year: StartYear,
}

/// Result of the world generator.
#[derive(Debug, Clone)]
pub struct GeneratedWorld {
    /// The generated game state.
    pub state: GameState,
    /// Per-country land registries.
    pub land_registry: HashMap<String, LandRegistry>,
    /// Flat region map.
    pub regions: HashMap<String, Region>,
    /// Megaregion map.
    pub megaregions: HashMap<String, Megaregion>,
    /// Bilateral diplomacy matrix.
    pub diplomacy: HashMap<String, HashMap<String, crate::international::DiplomaticRelation>>,
}

/// Generates a new world and writes the resulting save files to `data_dir`.
///
/// # Arguments
/// * `data_dir` - Root directory for the split save files.
/// * `options` - Generator options.
/// * `_registries` - Static registries; reserved for future tech/building aware generation.
///
/// # Returns
/// `Ok(GeneratedWorld)` on success, or a boxed error on failure.
///
/// # Rules
/// * Overwrites any existing `budgets.json`, `makro.json`, `tax_rates.json`,
///   `waluty.json`, `banks.json`, `storage.json`, `diplomacy.json`,
///   `regions.json`, `megaregions.json` and `land_registry.json` in `data_dir`.
/// * Leaves `entities/` and `spatial_registry/` empty so the lazy loader returns
///   an empty initial corporate sector; the first `run_turn` will seed them.
pub fn generate_world(
    data_dir: &Path,
    options: GenerateOptions,
    _registries: &Registries,
) -> Result<GeneratedWorld, Box<dyn Error>> {
    let mut rng = rand::thread_rng();
    let mut state = GameState::new();

    // Phase 53: Initialize calendar with the selected scenario year so that
    // turn-zero snapshots report the correct year (was defaulting to 0).
    state.calendar.start_year = options.start_year.as_year();
    state.calendar.current_year = options.start_year.as_year();
    state.calendar.global_turn = 0;
    state.calendar.current_month = 1;
    state.calendar.half_month = false;

    let mut land_registry = HashMap::new();
    let mut regions = HashMap::new();
    let mut megaregions = HashMap::new();

    let count = options.country_count.clamp(4, COUNTRY_NAMES.len());
    let mut available: Vec<_> = COUNTRY_NAMES.iter().map(|s| (*s).to_string()).collect();
    available.shuffle(&mut rng);
    let selected: Vec<String> = available.into_iter().take(count).collect();

    for name in &selected {
        let (country, currency, lr, country_regions, bank_companies) = generate_country(name, options.start_year, &mut rng);
        let region_ids: Vec<String> = country_regions.keys().cloned().collect();
        let megaregion_list = generate_megaregions(name, &region_ids);
        for megaregion in megaregion_list {
            megaregions.insert(megaregion.id.clone(), megaregion);
        }

        // Save bank companies to disk immediately so they persist across turns.
        // The corporate generator (called below) does not create banks because
        // Sector::Banking is not in country.budget.sectors (it's not a GDP-
        // producing sector). Without this, the simulation runs with zero banks.
        let company_store = crate::io::entity_store::DiskEntityStore::<Company>::new(data_dir);
        use crate::io::entity_store::EntityStore;
        let banking_sector_name = serde_json::to_value(crate::registries::enums::Sector::Banking)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "Banking".to_string());
        let _ = company_store.save_sector(name, &banking_sector_name, None, &bank_companies);

        state.countries.insert(name.clone(), country);
        state.currencies.insert(currency.prefix.clone(), currency);
        land_registry.insert(name.clone(), lr);
        regions.extend(country_regions);
    }

    let diplomacy = generate_diplomacy(&selected);

    state.extra.insert("current_turn".to_string(), Value::from(0));
    state.extra.insert("year".to_string(), Value::from(options.start_year as u32));

    for country in state.countries.values_mut() {
        generate_corporate_entities(data_dir, country, &regions, _registries, options.start_year as u32, &mut rng)?;
    }

    save_game_state(data_dir, &state)?;
    save_named_map(&data_dir.join("diplomacy.json"), &diplomacy)?;
    save_named_map(&data_dir.join("land_registry.json"), &land_registry)?;
    save_named_map(&data_dir.join("regions.json"), &regions)?;
    save_named_map(&data_dir.join("megaregions.json"), &megaregions)?;

    // Seed market.json with base prices for every commodity and an empty
    // order book so `run_turn` and the report UI have a valid global market.
    let mut prices: BTreeMap<String, f64> = BTreeMap::new();
    for commodity in Commodity::all() {
        prices.insert(commodity.into(), 100.0);
    }
    let market = serde_json::json!({ "prices": prices, "orders": {} });
    std::fs::write(data_dir.join("market.json"), serde_json::to_string_pretty(&market)?)?;

    Ok(GeneratedWorld {
        state,
        land_registry,
        regions,
        megaregions,
        diplomacy,
    })
}

fn generate_country(
    name: &str,
    start_year: StartYear,
    rng: &mut impl Rng,
) -> (Country, Currency, LandRegistry, HashMap<String, Region>, Vec<Company>) {
    let wealth = weighted_wealth(rng);
    let year_mult = start_year.year_multiplier();

    let gdp_pc = match wealth {
        WealthBracket::VeryHigh => rng.gen_range(3.5..5.0) * year_mult,
        WealthBracket::High => rng.gen_range(2.0..3.5) * year_mult,
        WealthBracket::Medium => rng.gen_range(1.0..2.0) * year_mult,
        WealthBracket::Low => rng.gen_range(0.3..1.0) * year_mult,
    };

    let population = rng.gen_range(2_000_000..=50_000_000) as u64;
    let gdp_total = population as f64 * gdp_pc * 1000.0;

    let cultural = generate_cultural_background(name);
    let demographics = build_demographics(&cultural, population, gdp_pc);
    let tech_limit = start_year.tech_limit(wealth);

    let (mut treasury, average_wage, energy_mix) = build_treasury(name, gdp_total, population, gdp_pc, &demographics, tech_limit, start_year, rng);
    let macro_data = build_macro_data(
        name,
        &cultural,
        &demographics,
        gdp_pc,
        gdp_total,
        average_wage,
        energy_mix,
        cultural.activity_rate,
        start_year,
        &mut treasury,
        rng,
    );
    let tax_rates = build_tax_rates(gdp_total, rng);
    let currency = build_currency(name, &treasury);
    let central_bank = build_central_bank(name, &treasury);

    let mut country = Country {
        name: name.to_string(),
        budget: treasury,
        macro_indicators: macro_data,
        tax_rates,
        trade_policy: crate::state::TradePolicy::default(),
        politics: Politics::default(),
        regions: Vec::new(),
        megaregions: Vec::new(),
        is_rebellion: false,
        mother_country: None,
        rebellion_type: None,
        rebellion_goals: None,
        economic_policy: crate::state::EconomicPolicy::default(),
        military_units: Vec::new(),
        military_fronts: Vec::new(),
        military_stockpile: std::collections::HashMap::new(),
        military_config: crate::military::config::MilitaryCombatConfig::default(),
        pending_defense_orders: Vec::new(),
        rationing_system: crate::state::RationingSystem::default(),
        emergency_powers: crate::state::EmergencyPowers::default(),
        emergency_escalation_counter: 0,
        emergency_deescalation_counter: 0,
        ministry_public_service_pool: 0.0,
        intelligence_budget: crate::state::IntelligenceBudget::default(),
        active_lobbying_operations: Vec::new(),
        central_bank,
        currency_zone: None,
        interbank_market: crate::state::InterbankMarket::default(),
        bfg_fund: crate::state::BfgFund::default(),
        sobk_scheme: crate::state::SobkScheme::default(),
        bank_resolution: crate::state::BankResolution::default(),
        bank_tax: crate::state::BankTax::default(),
        stock_exchange: crate::securities::StockExchange::default(),
        dividend_queue: Vec::new(), ipo_queue: Vec::new(), bankruptcy_auction_pool: crate::corporate::BankruptcyAuctionPool::default(), demolition_queue: Vec::new(), halt_queue: Vec::new(),
        knf: crate::securities::KNF::default(),
        sovereign_default_turns_remaining: 0,
        foreign_debt: 0.0,
        minimum_wage: None,
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
        state_forest_state: crate::economy::state_forests::StateForestState::default(),
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
    };
    country.macro_indicators.currency = currency.prefix.clone();

    let mut companies = Vec::new();  // Empty companies for bootstrap
    // Add bank companies
    let bank_companies = build_bank_companies(name, &country.budget, &country.central_bank);
    // Phase 37: Populate debt_market with DSPW primary dealers and enable DSPW.
    let dspw_dealers: Vec<String> = bank_companies.iter()
        .filter(|b| b.is_dspw)
        .map(|b| b.id.clone())
        .collect();
    if !dspw_dealers.is_empty() {
        country.debt_market.dspw_enabled = true;
        country.debt_market.primary_dealers = dspw_dealers;
    }
    companies.extend(bank_companies);
    crate::politics::bootstrap_politics(&mut country, &mut companies, start_year as u32, rng);

    let land_registry = generate_land_registry(name, population as i64, gdp_total);
    let country_regions = generate_regional_topology(name, population as i64, gdp_total, start_year.as_year());

    // Phase 21A: Generate geological formations with finite, depletable deposits.
    let region_ids: Vec<String> = country_regions.keys().cloned().collect();
    country.geological_formations = crate::society::geography::generate_geological_formations(&region_ids, rng);

    // Phase 26: Generate baseline transport network links from regional adjacency.
    // Only DirtRoad or None levels are seeded — advanced infrastructure (Rail,
    // Highways) must be built organically via Phase 22 ConstructionTenders
    // funded by Ministries. No magical infrastructure spawning.
    country.transport_networks = generate_baseline_transport_networks(&country_regions, rng);

    // Phase 28: Assign bank companies to the first region so they participate
    // in the regional labor market. Banks need a region_id for labor clearing.
    if let Some(first_region_id) = country_regions.keys().next() {
        for company in &mut companies {
            if company.sector == EntitySector::Banking && company.region_id.is_empty() {
                company.region_id = first_region_id.clone();
            }
        }
    }

    // Phase 45: Spawn a standing army with ToE equipment reserves.
    country.military_units = spawn_standing_army(
        &country,
        &country_regions,
        start_year.as_year(),
        rng,
    );

    (country, currency, land_registry, country_regions, companies)
}

/// Phase 45: Spawn a standing army for a country based on its population and era.
///
/// # Rules
/// * Army size = max(1000, population * 0.005) — 0.5% of population under arms.
/// * Infantry Division is always present.
/// * Artillery Brigade added if year >= 1880.
/// * Tank Brigade added if year >= 1916.
/// * Air Wing added if year >= 1940.
/// * Naval Fleet added if country has coastline and year >= 1880.
/// * Equipment is seeded at 90% ToE strength (not 100% — represents existing stock).
/// * Manpower is drawn proportionally from rural classes.
fn spawn_standing_army(
    country: &crate::state::Country,
    regions: &HashMap<String, crate::society::geography::Region>,
    start_year: u32,
    rng: &mut impl Rng,
) -> Vec<crate::military::units::MilitaryUnit> {
    use crate::military::units::{MilitaryUnit, UnitType, EquipmentReserve};

    let total_pop: i64 = regions.values().map(|r| r.population).sum();
    let army_size = ((total_pop as f64) * 0.005).max(1000.0) as i64;

    let has_coast = regions.values().any(|r| r.geographic_traits.has_coastline);
    let home_region = regions.keys().next().cloned().unwrap_or_default();

    let mut units = Vec::new();

    // Helper: scale ToE by manpower and seed at 90% strength
    let make_toe = |unit_type: &UnitType, manpower: i64| -> Vec<EquipmentReserve> {
        unit_type.table_of_equipment(start_year)
            .into_iter()
            .map(|mut r| {
                let scale = manpower as f64 / 1000.0;
                r.toe_quantity *= scale;
                r.current_quantity = r.toe_quantity * 0.9;
                r
            })
            .collect()
    };

    // Helper: draw manpower proportionally from rural classes
    let draw_manpower = |regions: &HashMap<String, crate::society::geography::Region>,
                         needed: i64| -> HashMap<crate::society::geography::RuralClass, i64> {
        let mut origin = HashMap::new();
        let total_rural: i64 = regions.values()
            .flat_map(|r| r.class_demographics.rural_classes.values())
            .map(|d| d.population)
            .sum();
        if total_rural <= 0 {
            return origin;
        }
        // Draw proportionally from FreePeasant and LandlessLaborer
        for region in regions.values() {
            for (class_key, demo) in &region.class_demographics.rural_classes {
                let rural_class = match class_key.as_str() {
                    "FreePeasant" => Some(crate::society::geography::RuralClass::FreePeasant),
                    "LandlessLaborer" => Some(crate::society::geography::RuralClass::LandlessLaborer),
                    "Serf" => Some(crate::society::geography::RuralClass::Serf),
                    "Aristocracy" => Some(crate::society::geography::RuralClass::Aristocracy),
                    _ => None,
                };
                if let Some(rc) = rural_class {
                    let share = demo.population as f64 / total_rural as f64;
                    let drawn = (needed as f64 * share) as i64;
                    if drawn > 0 {
                        *origin.entry(rc).or_insert(0) += drawn;
                    }
                }
            }
        }
        origin
    };

    // Infantry Division (always present)
    let infantry_manpower = army_size / 2;
    let mut infantry = MilitaryUnit::new(
        format!("{}-INF-1", country.name),
        UnitType::Infantry,
        infantry_manpower,
        draw_manpower(regions, infantry_manpower),
        home_region.clone(),
    );
    infantry.equipment_reserves = make_toe(&UnitType::Infantry, infantry_manpower);
    units.push(infantry);

    // Artillery Brigade (if year >= 1880)
    if start_year >= 1880 {
        let arty_manpower = (army_size / 10).max(100);
        let mut artillery = MilitaryUnit::new(
            format!("{}-ART-1", country.name),
            UnitType::Artillery,
            arty_manpower,
            draw_manpower(regions, arty_manpower),
            home_region.clone(),
        );
        artillery.equipment_reserves = make_toe(&UnitType::Artillery, arty_manpower);
        units.push(artillery);
    }

    // Tank Brigade (if year >= 1916)
    if start_year >= 1916 {
        let tank_manpower = (army_size / 20).max(100);
        let mut tanks = MilitaryUnit::new(
            format!("{}-TNK-1", country.name),
            UnitType::Tanks,
            tank_manpower,
            draw_manpower(regions, tank_manpower),
            home_region.clone(),
        );
        tanks.equipment_reserves = make_toe(&UnitType::Tanks, tank_manpower);
        units.push(tanks);
    }

    // Air Wing (if year >= 1940)
    if start_year >= 1940 {
        let air_manpower = (army_size / 50).max(50);
        let mut air = MilitaryUnit::new(
            format!("{}-AIR-1", country.name),
            UnitType::AirForce,
            air_manpower,
            draw_manpower(regions, air_manpower),
            home_region.clone(),
        );
        air.equipment_reserves = make_toe(&UnitType::AirForce, air_manpower);
        units.push(air);
    }

    // Naval Fleet (if coastal and year >= 1880)
    if has_coast && start_year >= 1880 {
        let naval_manpower = (army_size / 20).max(100);
        let coastal_region = regions.values()
            .find(|r| r.geographic_traits.has_coastline)
            .map(|r| r.id.clone())
            .unwrap_or(home_region.clone());
        let mut naval = MilitaryUnit::new(
            format!("{}-NAV-1", country.name),
            UnitType::Naval,
            naval_manpower,
            draw_manpower(regions, naval_manpower),
            coastal_region,
        );
        naval.equipment_reserves = make_toe(&UnitType::Naval, naval_manpower);
        units.push(naval);
    }

    units
}

/// Phase 26: Generate baseline transport network links from regional adjacency.
///
/// Creates `DirtRoad` or `None` level links between adjacent regions based on
/// the region graph's `edges`. Only primitive paths are seeded — no rail,
/// highways, or canals. Advanced infrastructure must be built via Phase 22
/// ConstructionTenders funded by Ministries.
fn generate_baseline_transport_networks(
    regions: &HashMap<String, crate::society::geography::Region>,
    rng: &mut impl Rng,
) -> crate::economy::logistics::transport_networks::TransportNetworkOverlay {
    use crate::economy::logistics::transport_networks::{
        NetworkLevel, NetworkLink, TransportNetworkOverlay,
    };
    use crate::society::geography::EdgeType;

    let mut overlay = TransportNetworkOverlay::default();

    for (region_id, region) in regions {
        for edge in &region.edges {
            // Only create links for land borders (not sea lanes or coastlines).
            if !matches!(edge.edge_type, EdgeType::LandBorder) {
                continue;
            }
            let target = &edge.target_node;
            // Skip if target is not in our region set (could be a sea/ocean node).
            if !regions.contains_key(target) {
                continue;
            }
            let key = TransportNetworkOverlay::link_key(region_id, target);
            // Skip if already added (bidirectional edges may duplicate).
            if overlay.links.contains_key(&key) {
                continue;
            }
            // 60% chance of DirtRoad, 40% chance of None (no improved path).
            // This reflects that not all adjacent regions have even a dirt road
            // connecting them at game start.
            let level = if rng.gen::<f64>() < 0.6 {
                NetworkLevel::DirtRoad
            } else {
                NetworkLevel::None
            };
            let link = NetworkLink {
                region_a: region_id.clone(),
                region_b: target.clone(),
                level,
                condition: rng.gen_range(0.5..0.9),
                built_turn: 0,
                congestion: 0.0,
            };
            overlay.links.insert(key, link);
        }
    }

    overlay
}

fn weighted_wealth(rng: &mut impl Rng) -> WealthBracket {
    let total: i32 = WEALTH_WEIGHTS.iter().sum();
    let mut roll = rng.gen_range(0..total);
    for (idx, weight) in WEALTH_WEIGHTS.iter().enumerate() {
        roll -= *weight;
        if roll < 0 {
            return match idx {
                0 => WealthBracket::VeryHigh,
                1 => WealthBracket::High,
                2 => WealthBracket::Medium,
                _ => WealthBracket::Low,
            };
        }
    }
    WealthBracket::Low
}

fn build_demographics(cultural: &CulturalBackground, _population: u64, gdp_pc: f64) -> Demographics {
    let analfabetyzm = (0.4 - (gdp_pc * 0.15)).max(0.01);
    let srednie_total = (gdp_pc * 0.15).min(0.45);
    let wyzsze_total = (gdp_pc * 0.08).min(0.35);
    let podstawowe = (1.0 - wyzsze_total - srednie_total - analfabetyzm).max(0.0);

    let mut srednie = BTreeMap::new();
    srednie.insert("Zawodowe".to_string(), srednie_total * 0.4);
    srednie.insert("Techniczne".to_string(), srednie_total * 0.3);
    srednie.insert("Humanistyczne".to_string(), srednie_total * 0.3);

    let mut wyzsze = BTreeMap::new();
    wyzsze.insert("Techniczne".to_string(), wyzsze_total * 0.4);
    wyzsze.insert("Humanistyczne".to_string(), wyzsze_total * 0.4);
    wyzsze.insert("Medyczne".to_string(), wyzsze_total * 0.2);

    let education = Education {
        brak: analfabetyzm,
        podstawowe,
        srednie,
        wyzsze,
        extra: Map::new(),
    };

    let age = &cultural.age_groups;
    let average_age = 7.5 * age.children + 38.0 * age.working + 75.0 * age.elderly;
    let median_age = median_from_age_groups(age.children, age.working, age.elderly);

    let city_urban = (0.2 + 0.6 / (1.0 + (-2.0 * (gdp_pc - 1.5)).exp())).min(0.95);
    let rural = (1.0 - city_urban).max(0.0);

    Demographics {
        birth_rate: cultural.birth_rate,
        death_rate: cultural.mortality,
        net_migration: 0.0,
        age_groups: AgeGroups {
            children: age.children,
            adults: age.working,
            elderly: age.elderly,
            extra: Map::new(),
        },
        gender: Gender {
            male: 0.5,
            female: 0.5,
            extra: Map::new(),
        },
        ethnic_composition: cultural.ethnic_composition.clone(),
        religious_composition: cultural.religious_composition.clone(),
        education,
        average_age,
        median_age,
        city_urban,
        rural,
        ..Demographics::default()
    }
}

fn median_from_age_groups(children: f64, adults: f64, elderly: f64) -> f64 {
    let total = children + adults + elderly;
    if total == 0.0 {
        return 35.0;
    }
    let p = 0.5 * total;
    if p < children {
        return (p / children) * 15.0;
    }
    if p < children + adults {
        let t = (p - children) / adults;
        return 15.0 + t * 45.0;
    }
    let t = (p - children - adults) / elderly.max(f64::EPSILON);
    60.0 + t * 30.0
}

fn build_treasury(
    _name: &str,
    gdp_total: f64,
    population: u64,
    gdp_pc: f64,
    _demographics: &Demographics,
    tech_limit: usize,
    start_year: StartYear,
    rng: &mut impl Rng,
) -> (Treasury, f64, EnergyMix) {
    let is_petrostate = rng.gen::<f64>() < 0.25;

    // Phase 44: Era-aware sector shares.
    // 1900: Heavy agriculture, light industry, minimal services.
    // 1975: Light agriculture, heavy industry, large services.
    let (agri_range, industry_mult, services_mult) = match start_year {
        StartYear::Y1900 => ((0.20..0.50), 0.7, 0.6),
        StartYear::Y1925 => ((0.10..0.30), 0.9, 0.8),
        StartYear::Y1950 => ((0.05..0.15), 1.1, 1.2),
        StartYear::Y1975 => ((0.02..0.08), 1.3, 1.5),
    };

    let wydobycie = if is_petrostate { rng.gen_range(0.15..0.4) } else { rng.gen_range(0.01..0.05) };
    let roln = if gdp_pc < 1.5 {
        rng.gen_range(agri_range.start..agri_range.end)
    } else {
        rng.gen_range(0.01..0.05)
    };
    let p_ciezki = rng.gen_range(0.05..0.25) * industry_mult;
    let p_lekki = rng.gen_range(0.1..0.3) * industry_mult;
    let u_lokalne = rng.gen_range(0.2..0.4) * services_mult;
    let u_eksportowe = if gdp_pc > 2.0 { rng.gen_range(0.05..0.3) * services_mult } else { rng.gen_range(0.01..0.05) };
    let bud = rng.gen_range(0.05..0.15);
    let energetyka = rng.gen_range(0.05..0.12);
    let u_medyczne = rng.gen_range(0.04..0.10) * services_mult;
    let u_edukacyjne = rng.gen_range(0.03..0.08) * services_mult;

    let sum = wydobycie + roln + p_ciezki + p_lekki + u_lokalne + u_eksportowe + bud + energetyka + u_medyczne + u_edukacyjne;

    let wegiel_mix = rng.gen_range(0.3..0.8);
    let gaz_mix = rng.gen_range(0.1..0.6);
    let oze_mix = rng.gen_range(0.05..0.2);
    let mix_sum = wegiel_mix + gaz_mix + oze_mix;

    let energy_mix = EnergyMix {
        coal: wegiel_mix / mix_sum,
        natural_gas: gaz_mix / mix_sum,
        uranium: 0.0,
        renewables: oze_mix / mix_sum,
        extra: Map::new(),
    };

    let average_wage = gdp_pc * 800.0;

    let mut sectors = HashMap::new();
    sectors.insert(Sector::Mining, sector_share(wydobycie / sum, 0.5, tech_limit));
    sectors.insert(Sector::Agriculture, sector_share(roln / sum, 0.2, tech_limit));
    sectors.insert(Sector::HeavyIndustry, sector_share(p_ciezki / sum, 0.6, tech_limit));
    sectors.insert(Sector::LightIndustry, sector_share(p_lekki / sum, 0.4, tech_limit));
    sectors.insert(Sector::LocalServices, sector_share(u_lokalne / sum, 0.3, tech_limit));
    sectors.insert(Sector::ExportServices, sector_share(u_eksportowe / sum, 0.7, tech_limit));
    sectors.insert(Sector::Construction, sector_share(bud / sum, 0.8, tech_limit));
    sectors.insert(Sector::Energy, sector_share(energetyka / sum, 0.3, tech_limit));
    sectors.insert(Sector::PublicServices, sector_share((u_medyczne + u_edukacyjne) / sum, 0.2, tech_limit));

    let mut allocations = HashMap::new();
    allocations.insert("Industry".to_string(), Value::from(rng.gen_range(0.02..0.15)));
    allocations.insert("Education and Propaganda".to_string(), Value::from(rng.gen_range(0.02..0.1)));
    allocations.insert("Healthcare".to_string(), Value::from(rng.gen_range(0.05..0.15)));
    allocations.insert("Infrastructure and Transport".to_string(), Value::from(rng.gen_range(0.05..0.2)));
    allocations.insert("Social Programs".to_string(), Value::from(rng.gen_range(0.05..0.25)));
    allocations.insert("Agriculture and Rural Development".to_string(), Value::from(rng.gen_range(0.02..0.1)));
    allocations.insert("Armed Forces".to_string(), Value::from(rng.gen_range(0.02..0.15)));
    allocations.insert("Justice".to_string(), Value::from(rng.gen_range(0.01..0.05)));
    allocations.insert("Public Administration".to_string(), Value::from(rng.gen_range(0.01..0.05)));

    let total_alloc: f64 = allocations.values().filter_map(|v| v.as_f64()).sum();
    if total_alloc > 0.0 {
        for v in allocations.values_mut() {
            if let Value::Number(n) = v {
                if let Some(x) = n.as_f64() {
                    *v = Value::from(x / total_alloc);
                }
            }
        }
    }

    let budget = BudgetAllocations {
        industry: allocations["Industry"].as_f64().unwrap_or(0.0),
        education_propaganda: allocations["Education and Propaganda"].as_f64().unwrap_or(0.0),
        healthcare: allocations["Healthcare"].as_f64().unwrap_or(0.0),
        infrastructure_transport: allocations["Infrastructure and Transport"].as_f64().unwrap_or(0.0),
        social_programs: allocations["Social Programs"].as_f64().unwrap_or(0.0),
        agriculture_rural: allocations["Agriculture and Rural Development"].as_f64().unwrap_or(0.0),
        armed_forces: allocations["Armed Forces"].as_f64().unwrap_or(0.0),
        justice: allocations["Justice"].as_f64().unwrap_or(0.0),
        public_administration: allocations["Public Administration"].as_f64().unwrap_or(0.0),
        extra: Map::new(),
    };

    let discovered: Vec<String> = (1..=tech_limit).map(|i| format!("tech_{i:03}")).collect();

    let treasury = Treasury {
        gdp: gdp_total,
        population,
        nominal_budget: gdp_total * rng.gen_range(0.15..0.30),
        liquid_reserves: gdp_total * rng.gen_range(0.02..0.10),
        citizen_savings: gdp_total * rng.gen_range(0.05..0.20),
        private_capital: gdp_total * rng.gen_range(0.10..0.40),
        infrastructure_level: rng.gen_range(10.0..80.0) * gdp_pc,
        energy_infrastructure: rng.gen_range(500.0..1500.0) * gdp_pc,
        stock_market: StockMarket {
            index: 1000.0,
            confidence: rng.gen_range(30.0..80.0),
            last_change: 0.0,
            sector_indices: Value::Object(Map::new()),
            extra: Map::new(),
        },
        allocations: budget,
        black_ops_budget: 0.0,
        sectors,
        outstanding_corporate_debts: HashMap::new(),
        liquidation_expenses: 0.0,
        logistics_revenue: 0.0,
        science: ScienceState {
            innovation_points: 0.0,
            researching: None,
            discovered,
            base_innovativeness: 0.0,
            extra: Map::new(),
        },
        tax_office_ids: Vec::new(),
        tax_history: std::collections::VecDeque::new(),
        last_balance_log: String::new(),
        trade_balance: None,
        max_public_wage_multiplier: 1.2, // Phase 5: Default to prevent crowding out
        extra: Map::new(),
    };

    (treasury, average_wage, energy_mix)
}

fn sector_share(gdp_share: f64, vulnerability: f64, _tech_limit: usize) -> SectorShare {
    SectorShare {
        gdp_share,
        crisis_vulnerability: Some(vulnerability),
        active_method: Some(ProductionMethodChoice {
            automation: "Tradycyjne".to_string(),
            production: "Tradycyjne".to_string(),
            organization: "Tradycyjne".to_string(),
            extra: Map::new(),
        }),
        extra: Map::new(),
    }
}

fn build_macro_data(
    name: &str,
    cultural: &CulturalBackground,
    demographics: &Demographics,
    gdp_pc: f64,
    _gdp_total: f64,
    average_wage: f64,
    energy_mix: EnergyMix,
    activity_rate: f64,
    _start_year: StartYear,
    treasury: &mut Treasury,
    rng: &mut impl Rng,
) -> MacroData {
    let unemployment_rate = rng.gen_range(3.0..15.0);
    let workforce = (treasury.population as f64 * activity_rate / 100.0).max(1.0);
    let employed_total = (workforce * (1.0 - unemployment_rate / 100.0)).max(0.0);
    let unemployed = (workforce - employed_total).max(0.0);

    // Seed sector employment/extra so `update_gdp_shares_from_employment` has
    // a deterministic fallback before the first corporate sector is generated.
    let total_gdp_share: f64 = treasury.sectors.values().map(|s| s.gdp_share).sum();
    if total_gdp_share > 0.0 {
        for share in treasury.sectors.values_mut() {
            let share_emp = (employed_total * (share.gdp_share / total_gdp_share)) as i64;
            share.extra.insert("zatrudnienie".to_string(), Value::from(share_emp));
            share.extra.insert("pmi".to_string(), Value::from(50.0));
        }
    }

    let labor_market = LaborMarket {
        unemployment_rate,
        labor_force_participation: activity_rate,
        employed_total,
        unemployed,
        unemployment_structure: UnemploymentStructure {
            friction: 0.03,
            structural: 0.05,
            cyclical: 0.02,
            extra: Map::new(),
        },
        underemployment: 0.0,
        subsistence_peasants: if gdp_pc < 1.5 { population_f64(demographics) * rng.gen_range(0.05..0.40) } else { population_f64(demographics) * 0.01 },
        ..LaborMarket::default()
    };

    let mut extra = Map::new();
    extra.insert(
        "statystyki_zdrowotne".to_string(),
        serde_json::json!({
            "baza_infrastruktury_medycznej": rng.gen_range(20.0..60.0) * (gdp_pc / 2.0),
            "baza_rehabilitacyjna": rng.gen_range(10.0..40.0) * (gdp_pc / 2.0),
            "jakosc_sluzby_zdrowia": rng.gen_range(30.0..70.0),
            "sila_sanepidu": rng.gen_range(20.0..80.0),
            "wyleczeni_z_kalectwa": 0,
            "wypadki_w_pracy": 0,
            "zgony_w_pracy": 0,
            "nowi_niepelnosprawni": 0,
            "oczekiwana_dlugosc_zycia": 40.0 + (gdp_pc * 10.0),
            "dlugosc_zycia_w_zdrowiu": 35.0 + (gdp_pc * 8.0)
        }),
    );
    extra.insert(
        "statystyki_edukacyjne".to_string(),
        serde_json::json!({
            "baza_infrastruktury_edukacyjnej": rng.gen_range(20.0..70.0) * (gdp_pc / 2.0)
        }),
    );
    extra.insert("minimum_wage".to_string(), Value::from(average_wage * 0.0));

    let mut polityka_extra = Map::new();
    polityka_extra.insert(
        "prawo_wojskowe".to_string(),
        serde_json::json!({
            "obowiazkowa_sluzba": "obowiazkowe_szkolenia",
            "kobiety_w_armii": "jedynie_w_rezerwie",
            "zakres_poboru": "dobrowolna"
        }),
    );
    extra.insert("polityka".to_string(), Value::Object(polityka_extra));

    MacroData {
        inflation: rng.gen_range(1.0..15.0),
        gini: rng.gen_range(0.25..0.55),
        social_unrest: rng.gen_range(5.0..40.0),
        wealth_bracket: WealthBracket::Low,
        productivity: rng.gen_range(0.8..1.5) * gdp_pc,
        currency: "XXX".to_string(),
        energy_mix,
        average_wage,
        culture: name.to_string(),
        cultural_group: cultural.cultural_group.clone(),
        religion: cultural.religion.clone(),
        election_turn: 0,
        labor_market,
        demographics: demographics.clone(),
        health_statistics: crate::state::macro_data::HealthStatistics::default(),
        education_statistics: crate::state::macro_data::EducationStatistics::default(),
        gdp_breakdown: crate::state::macro_data::GdpBreakdown::default(),
        inflation_indices: crate::state::macro_data::InflationIndices::default(),
        money_supply: crate::state::macro_data::MoneySupplySnapshot::default(),
        telemetry_history: crate::state::macro_data::TelemetryHistory::default(),
        extra,
    }
}

fn population_f64(demographics: &Demographics) -> f64 {
    demographics.population_size.max(1.0)
}

fn build_tax_rates(gdp_total: f64, rng: &mut impl Rng) -> TaxRates {
    TaxRates {
        income_tax: IncomeTax {
            rate: rng.gen_range(0.1..0.25),
            structure: "linear".to_string(),
            extra: Map::new(),
        },
        corporate_tax: rng.gen_range(0.05..0.2),
        vat: HashMap::from([
            (
                "services".to_string(),
                VatBracket {
                    rate: 0.15,
                    consumption_share: 0.45,
                    extra: Map::new(),
                },
            ),
            (
                "industry".to_string(),
                VatBracket {
                    rate: 0.23,
                    consumption_share: 0.35,
                    extra: Map::new(),
                },
            ),
            (
                "agriculture".to_string(),
                VatBracket {
                    rate: 0.05,
                    consumption_share: 0.20,
                    extra: Map::new(),
                },
            ),
        ]),
        public_debt: PublicDebt {
            current_debt: gdp_total * rng.gen_range(0.1..0.6),
            interest_rate: rng.gen_range(0.03..0.08),
            extra: Map::new(),
        },
        excise_taxes: std::collections::BTreeMap::new(),
        wealth_tax: crate::state::tax::WealthTax {
            // Phase 39: Baseline wealth tax — 1% on assets > 5M
            brackets: vec![
                crate::state::tax::TaxBracket {
                    threshold: 5_000_000.0,
                    rate: 0.01,
                    extra: Map::new(),
                },
            ],
            asset_types: vec!["liquid_capital".to_string(), "real_estate".to_string()],
            extra: Map::new(),
        },
        capital_gains_tax: crate::state::tax::CapitalGainsTax {
            // Phase 39: Baseline capital gains tax — 19% (Belka tax)
            brackets: vec![
                crate::state::tax::TaxBracket {
                    threshold: 0.0,
                    rate: 0.19,
                    extra: Map::new(),
                },
            ],
            holding_period_modifier: 1.0,
            extra: Map::new(),
        },
        sectoral_preferences: Vec::new(),
        tax_havens: Vec::new(),
        exemption_registry: crate::state::tax::TaxExemptionRegistry::default(),
        tax_routing: crate::state::tax::TaxRouting::default(),
        exit_tax_rate: 0.10, // Phase 5: Default 10% exit tax for capital flight
        extra: Map::new(),
    }
}

fn build_currency(name: &str, treasury: &Treasury) -> Currency {
    let mut rng = rand::thread_rng();
    let prefix = name[..3.min(name.len())].to_uppercase();
    Currency {
        prefix: prefix.clone(),
        exchange_rate: rng.gen_range(0.5..5.0),
        policy: CurrencyPolicy {
            regime: "Fluid".to_string(),
            target: 0.0,
            extra: Map::new(),
        },
        members: vec![name.to_string()],
        qe_volume: 0.0,
        last_message: String::new(),
        extra: Map::new(),
    }
}

fn build_central_bank(name: &str, treasury: &Treasury) -> crate::state::CentralBank {
    let mut rng = rand::thread_rng();
    let prefix = name[..3.min(name.len())].to_uppercase();
    let rezerwy = treasury.gdp * rng.gen_range(0.05..0.20);
    
    // Initialize FX reserves with some foreign currencies
    let mut fx_reserves = std::collections::HashMap::new();
    fx_reserves.insert("USD".to_string(), rezerwy * 0.5);
    fx_reserves.insert("EUR".to_string(), rezerwy * 0.3);
    fx_reserves.insert("GBP".to_string(), rezerwy * 0.2);
    
    // Phase 38: Initialize interest rates anchored to the neutral rate (2%)
    // rather than a random 1-10% range. This ensures the first turn's sovereign
    // bonds are issued at ~2.5% (neutral + credit spread), not a random 4.5%+.
    // The Taylor Rule in update_reference_rate will adjust from here.
    let neutral_rate = 0.02;
    let reference_rate: f64 = neutral_rate + rng.gen_range(-0.005..0.005); // 1.5% - 2.5%
    let lombard_rate = reference_rate + 0.015; // +150 bps
    let deposit_rate = (reference_rate - 0.015).max(0.0_f64); // -150 bps, floor at 0
    
    crate::state::CentralBank {
        id: format!("BC-{}", prefix),
        name: format!("Bank Centralny {}", name),
        independence_model: crate::state::CentralBankIndependence::CentralIndependent,
        mandate: crate::state::MonetaryMandate::Mixed,
        governor_id: format!("GOV-{}-001", prefix),
        governor_appointment_turn: 0,
        governor_term_length: 60, // 5 years (assuming 12 turns/year)
        regional_directors: Vec::new(),
        interest_rates: crate::state::RppInterestRates {
            reference_rate,
            lombard_rate,
            deposit_rate,
            rediscount_rate: reference_rate + 0.01,
            discount_rate: reference_rate - 0.01,
            extra: Map::new(),
        },
        rpp: Some(crate::state::MonetaryPolicyCouncil {
            last_meeting_turn: 0,
            next_meeting_turn: 12, // Monthly meetings
            decision_log: Vec::new(),
            extra: Map::new(),
        }),
        reserve_requirement_ratio: 0.10,
        fx_reserves,
        physical_gold_reserves: 0.0,
        liquidity_injected: 0.0,
        omo_bond_holdings: 0.0,
        omo_target_rate: 0.0,
        omo_last_operation_turn: 0,
        omo_last_operation_amount: 0.0,
        deposit_facility_interest_paid: 0.0,
        lombard_facility_interest_received: 0.0,
        last_message: String::new(),
        // Phase 36: Configurable Taylor Rule targets
        target_inflation: 0.02,    // 2% inflation target
        potential_growth: 0.02,    // 2% potential GDP growth
        neutral_rate: 0.02,        // 2% neutral real rate
        extra: Map::new(),
    }
}

fn build_bank_companies(
    name: &str,
    treasury: &Treasury,
    central_bank: &crate::state::CentralBank,
) -> Vec<Company> {
    let mut rng = rand::thread_rng();
    let prefix = &name[..3.min(name.len())].to_uppercase();

    // Phase 36/37: Generate multiple banks based on population size.
    // Formula: max(1, population / 2M), capped at 5.
    // The first bank is the main state bank (Universal/Commercial).
    // Additional banks are regional banks with smaller balance sheets.
    // Phase 37: Designate up to 3 DSPW primary dealers (was just 1).
    let num_banks = ((treasury.population / 2_000_000) as usize).max(1).min(5);
    let num_dspw = ((num_banks + 1) / 2).min(3); // Half of banks, max 3

    let mut banks = Vec::new();
    let base_wage = (treasury.gdp / treasury.population.max(1) as f64).max(1000.0);

    for i in 0..num_banks {
        let bank_idx = i + 1;
        let is_first = i == 0;
        let is_dspw_bank = i < num_dspw;

        let bank_type = if is_first {
            if treasury.gdp > 100_000_000.0 {
                BankingBankType::Universal
            } else {
                BankingBankType::Commercial
            }
        } else if rng.gen::<f64>() > 0.5 {
            BankingBankType::Commercial
        } else {
            BankingBankType::Investment
        };

        let bank_id = format!("BANK-{}-{:03}", prefix, bank_idx);
        let bank_name = if is_first {
            format!("State Bank of {name}")
        } else {
            // Phase 51: Use cultural surnames for regional bank names.
            let bank_surnames = [
                "Kowalski", "Müller", "Rossi", "Andersen", "Dubois", "Petrović",
                "Schmidt", "Garcia", "Hussein", "Novak", "Fischer", "Marković",
                "Weber", "López", "Khalil", "Sokolov", "Becker", "Fernández",
            ];
            let surname = bank_surnames.choose(&mut rng).copied().unwrap_or("Smith");
            format!("{surname} Bank of {name}")
        };

        // Smaller balance sheets for regional banks
        let size_factor = if is_first { 1.0 } else { 0.3 + rng.gen::<f64>() * 0.4 };
        let total_deposits = match bank_type {
            BankingBankType::Commercial | BankingBankType::Universal => {
                treasury.citizen_savings * 0.5 * size_factor / num_banks as f64
            }
            _ => 0.0,
        };
        let tier_1_capital = treasury.gdp * 0.05 * size_factor / num_banks as f64;
        let reserves = total_deposits * central_bank.reserve_requirement_ratio;

        let balance_sheet = BankBalanceSheet {
            reserves_at_central_bank: reserves,
            loans_issued: Vec::new(),
            interbank_loans_given: std::collections::HashMap::new(),
            securities: 0.0,
            mbs_holdings: Vec::new(),
            real_estate: 0.0,
            deposits: total_deposits,
            cb_lombard_loans: 0.0,
            cb_deposit_facility_balance: 0.0,
            interbank_loans_taken: std::collections::HashMap::new(),
            issued_bonds: 0.0,
            tier_1_capital,
            extra: Map::new(),
        };

        // Phase 28/36: Set FTE demand and wages so banks participate in labor market.
        let bank_fte = if is_first {
            rng.gen_range(100..=300) as f64
        } else {
            rng.gen_range(30..=100) as f64
        };
        let bank_wage = base_wage * 1.2; // Banks pay above-average wages
        let operating_cash = tier_1_capital * 0.1; // 10% of tier_1 for payroll

        let mut company = Company::new(
            bank_id.clone(),
            bank_name,
            EntitySector::Banking,
            LegalForm::JointStockCompany(crate::entities::JointStockData::default()),
            tier_1_capital,
            operating_cash,
            bank_fte as u32,
        );
        company.bank_type = Some(bank_type);
        company.balance_sheet = Some(balance_sheet);
        company.loan_margin = Some(0.02);
        company.target_fte_demand = bank_fte;
        company.physical_fte_demand = bank_fte;
        company.offered_wage_per_fte = bank_wage;
        // Phase 41: Initialize target_wage for banks with max(50.0) fallback.
        company.target_wage = bank_wage.max(50.0);
        // Phase 37: Designate up to num_dspw banks as DSPW primary dealers
        company.is_dspw = is_dspw_bank;

        // Phase 43: Genesis Labor Fix for banks — pre-populate workforce and
        // inject payroll grant. Without this, banks start at 0 FTE and cannot
        // participate in the labor market, causing Banking sector employment
        // to stay at 0 for the first several turns.
        let initial_fte = (bank_fte * 0.6).max(2.0);
        company.fulfilled_fte = initial_fte;
        company.prev_fulfilled_fte = initial_fte;
        let payroll_grant = initial_fte * bank_wage * 3.0;
        company.available_cash += payroll_grant;

        banks.push(company);
    }

    banks
}

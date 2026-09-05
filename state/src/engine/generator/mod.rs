//! Procedural world generation for creating a new `Turn 0` state.
//!
//! This module ports the Python `engine/world_generator` pipeline: it creates
//! countries, demographics, economies, currencies, banks, diplomacy, regions
//! and megaregions from a small set of seed parameters and writes the result
//! to the standard split-file save layout.

#![allow(missing_docs)]

mod corporate;

use crate::engine::generator::corporate::generate_corporate_entities;
use crate::engine::generator::corporate::generate_investment_funds;
use crate::entities::{Company, LegalForm};
use crate::international::generate_diplomacy;
use crate::io::save_manager::{save_game_state, save_named_map};
use crate::politics::Politics;
use crate::registries::enums::Sector as EntitySector;
use crate::registries::enums::{Commodity, Sector, WealthBracket};
use crate::registries::Registries;
use crate::society::cadastre::generate_cadastre;
use crate::society::cultures::{generate_cultural_background, CulturalBackground};
use crate::society::geography::{
    generate_megaregions, generate_regional_topology, Megaregion, Region, RuralClass, UrbanClass,
};
use crate::state::banking::{BankBalanceSheet, BankType as BankingBankType};
use crate::state::macro_data::{
    AgeGroups, Demographics, Education, EnergyMix, Gender, LaborMarket, UnemploymentStructure,
};
use crate::state::tax::{IncomeTax, PublicDebt, VatBracket};
use crate::state::treasury::{
    BudgetAllocations, ProductionMethodChoice, ScienceState, SectorShare, StockMarket,
};
use crate::state::{Country, Currency, CurrencyPolicy, GameState, MacroData, TaxRates, Treasury};
use rand::seq::SliceRandom;
use rand::Rng;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::path::Path;

const COUNTRY_NAMES: &[&str] = &[
    "Sarmatia",
    "Illyria",
    "Helvetia",
    "Nordia",
    "Bactria",
    "Persia",
    "Lechia",
    "Eldoria",
    "Venedia",
    "Occitania",
    "Gallia",
    "Dacia",
    "Krasnovia",
    "Anatolia",
    "Iberia",
    "Anglia",
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
            StartYear::Y1900 => [
                (WealthBracket::VeryHigh, 17),
                (WealthBracket::High, 10),
                (WealthBracket::Medium, 4),
                (WealthBracket::Low, 0),
            ],
            StartYear::Y1925 => [
                (WealthBracket::VeryHigh, 31),
                (WealthBracket::High, 24),
                (WealthBracket::Medium, 10),
                (WealthBracket::Low, 3),
            ],
            StartYear::Y1950 => [
                (WealthBracket::VeryHigh, 45),
                (WealthBracket::High, 38),
                (WealthBracket::Medium, 22),
                (WealthBracket::Low, 8),
            ],
            StartYear::Y1975 => [
                (WealthBracket::VeryHigh, 64),
                (WealthBracket::High, 55),
                (WealthBracket::Medium, 38),
                (WealthBracket::Low, 20),
            ],
        };
        map.iter()
            .find(|(w, _)| *w == wealth)
            .map(|(_, n)| *n)
            .unwrap_or(0)
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
/// * Overwrites any existing `budgets.json`, `macro.json`, `tax_rates.json`,
///   `waluty.json`, `banks.json`, `storage.json`, `diplomacy.json`,
///   `regions.json`, `megaregions.json` and `cadastres.json` in `data_dir`.
/// * Leaves `entities/` and `spatial_registry/` empty so the lazy loader returns
///   an empty initial corporate sector; the first `run_turn` will seed them.
pub fn generate_world(
    data_dir: &Path,
    options: GenerateOptions,
    _registries: &Registries,
) -> Result<GeneratedWorld, Box<dyn Error>> {
    let mut rng = rand::thread_rng();
    let mut state = GameState::new();

    // World Generation & Climate Audit (v0.5.3): Populate the climate-season
    // matrix with biologically sensible multipliers for all 7 climate profiles
    // × 4 seasons. Without this, the matrix is empty and agricultural yields
    // are zeroed out (the Phantom Harvest bug).
    state.climate_config.populate_defaults();

    // Phase 53: Initialize calendar with the selected scenario year so that
    // turn-zero snapshots report the correct year (was defaulting to 0).
    // Emergency Stabilization: Start in September (month 9) so the autumn
    // harvest provides organic food supply from Turn 1, eliminating the need
    // for artificial 12-month food seeding.
    state.calendar.start_year = options.start_year.as_year();
    state.calendar.current_year = options.start_year.as_year();
    state.calendar.global_turn = 0;
    state.calendar.start_month = 9; // September harvest start
    state.calendar.current_month = 9;
    state.calendar.half_month = false;

    let mut regions = HashMap::new();
    let mut megaregions = HashMap::new();

    let count = options.country_count.clamp(4, COUNTRY_NAMES.len());
    let mut available: Vec<_> = COUNTRY_NAMES.iter().map(|s| (*s).to_string()).collect();
    available.shuffle(&mut rng);
    let selected: Vec<String> = available.into_iter().take(count).collect();

    for name in &selected {
        let (mut country, currency, mut country_regions, bank_companies) =
            generate_country(name, options.start_year, &mut rng);
        let region_ids: Vec<String> = country_regions.keys().cloned().collect();
        let mut megaregion_list =
            generate_megaregions(name, &region_ids, country.politics.state_structure);

        // Phase 54: Assign mayor/governor names and register them in the VIP
        // registry. Must run AFTER generate_regional_topology (which happened
        // inside generate_country) so that governance structures exist.
        crate::politics::turn::assign_regional_heads(
            &mut country,
            &mut country_regions,
            &mut megaregion_list,
            &mut rng,
        );

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
        regions.extend(country_regions);
    }

    // Phase 87+: Generate the global Planet with geological veins.
    // Map all world regions to (id, lat, lon) tuples for vein placement.
    // coord_x → lon, coord_y → lat (already in approximate geographic units).
    let planet_regions: Vec<(String, f64, f64)> = regions
        .values()
        .map(|r| (r.id.clone(), r.coord_y, r.coord_x))
        .collect();
    state.planet = crate::society::planet::generate_planet(&planet_regions, &mut rng);

    // Phase 90: Ensure each populated region has diverse base industrial veins.
    // The global generate_veins places too few veins for regional coverage,
    // resulting in Limestone monoculture. This guarantees each populated
    // region has at least one vein for each AbundantIndustrial and Ubiquitous
    // commodity (Iron, HardCoal, BrownCoal, Stone, Sand, Limestone, Peat, Gravel).
    let populated_region_coords: Vec<(String, f64, f64)> = regions
        .values()
        .filter(|r| r.population > 0)
        .map(|r| (r.id.clone(), r.coord_y, r.coord_x))
        .collect();
    state
        .planet
        .ensure_base_industrial_veins_per_region(&populated_region_coords, &mut rng);

    // Phase 89: Auto-discover base industrial veins in populated regions.
    // Base industrial commodities (iron, coal, copper, stone, etc.) are
    // surface-visible deposits that any settled civilization knows about.
    // Rare/precious veins (gold, silver, uranium) remain hidden for exploration.
    let populated_region_ids: std::collections::HashSet<String> = regions
        .values()
        .filter(|r| r.population > 0)
        .map(|r| r.id.clone())
        .collect();
    state
        .planet
        .discover_base_industrial_veins(&populated_region_ids);

    // Phase 88: Reseed region resources from the Planet's geological vein
    // system. This replaces the deprecated reseed_resources_from_formations
    // call inside generate_country. The Planet didn't exist when generate_country
    // ran, so we reseed now with the authoritative vein data.
    // CRITICAL: Resource keys are vein IDs (not commodity strings) so that
    // building.deposit_id matches the resource key for the mine counter.
    crate::society::geography::reseed_resources_from_planet(&mut regions, &state.planet);

    // Phase C3: Derive has_geothermal_potential from Planet vein data.
    // Regions with UltraRare or Rare veins at shallow depth (< 200m) suggest
    // volcanic activity → geothermal potential. This makes the field a derived
    // attribute from planetary geology, not a random or manual flag.
    use crate::society::planet::RarityTier;
    for region in regions.values_mut() {
        region.geographic_traits.has_geothermal_potential = state
            .planet
            .veins_for_region(&region.id)
            .iter()
            .any(|v| {
                (v.rarity_tier == RarityTier::UltraRare || v.rarity_tier == RarityTier::Rare)
                    && v.depth < 200.0
            });
    }

    let diplomacy = generate_diplomacy(&selected);

    // Phase 68: Spawn the World Forum — neutral, all countries as members, Unanimity voting.
    let world_forum =
        crate::international::organizations::InternationalOrganization::new_world_forum(
            &selected, 0,
        );
    state
        .international_organizations
        .organizations
        .push(world_forum);

    state
        .extra
        .insert("current_turn".to_string(), Value::from(0));
    state
        .extra
        .insert("year".to_string(), Value::from(options.start_year as u32));

    for country in state.countries.values_mut() {
        generate_corporate_entities(
            data_dir,
            country,
            &mut regions,
            &state.planet,
            _registries,
            options.start_year as u32,
            &mut rng,
        )?;
    }

    // Bugfix Sprint (5B): Initialize power grids AFTER corporate entities are
    // generated, so LV/MV capacities can be derived from actual connected
    // housing/commercial electricity demand (Rule 15 — no magic numbers).
    use crate::io::entity_store::EntityStore;
    for country in state.countries.values_mut() {
        let housing_store = crate::io::entity_store::DiskEntityStore::<
            crate::society::housing::HousingBuilding,
        >::new(data_dir);
        let commercial_store = crate::io::entity_store::DiskEntityStore::<
            crate::society::housing::CommercialBuilding,
        >::new(data_dir);
        let housing_buildings = housing_store
            .load_sector(&country.name, "housing", None)
            .unwrap_or_default();
        let commercial_buildings = commercial_store
            .load_sector(&country.name, "commercial", None)
            .unwrap_or_default();
        crate::energy::grid::init_power_grid(
            country,
            &housing_buildings,
            &commercial_buildings,
            options.start_year as u32,
            &mut rng,
        );
    }

    // Phase 85: Generate factional domains AFTER cadastre and corporate entities
    // are generated, so parcel ownership and building data are available for
    // faction type assignment (Rule 4 — complete lifecycle from world gen).
    let domain_config = crate::society::factional_domains::FactionalDomainConfig::default();
    for country in state.countries.values_mut() {
        crate::society::factional_domains::generate_factional_domains(
            country,
            &domain_config,
            &mut rng,
        );
    }

    // Phase 57: Generate investment funds for each country.
    for country in state.countries.values_mut() {
        let cultural_group = if country.macro_indicators.cultural_group.is_empty() {
            "slavic".to_string()
        } else {
            country.macro_indicators.cultural_group.clone()
        };
        generate_investment_funds(
            data_dir,
            country,
            &cultural_group,
            options.start_year as u32,
            &mut rng,
        );
    }

    save_game_state(data_dir, &state)?;
    save_named_map(&data_dir.join("diplomacy.json"), &diplomacy)?;
    save_named_map(&data_dir.join("regions.json"), &regions)?;
    save_named_map(&data_dir.join("megaregions.json"), &megaregions)?;

    // Seed market.json with base prices for every commodity and an empty
    // order book so `run_turn` and the report UI have a valid global market.
    let mut prices: BTreeMap<String, f64> = BTreeMap::new();
    for commodity in Commodity::all() {
        prices.insert(commodity.into(), 100.0);
    }

    // Seed the foreign sector balance from aggregate simulated GDP.
    // Represents the rest-of-world economy at half the simulated GDP
    // (conservative: simulated countries are the major economies).
    // This is a one-time genesis allocation that scales with the actual
    // generated world — not a magic number.
    let total_world_gdp: f64 = state.countries.values().map(|c| c.budget.gdp).sum();
    let foreign_sector_balance = total_world_gdp * 0.5;

    let market = serde_json::json!({
        "prices": prices,
        "orders": {},
        "foreign_sector_balance": foreign_sector_balance,
    });
    std::fs::write(
        data_dir.join("market.json"),
        serde_json::to_string_pretty(&market)?,
    )?;

    // Phase 80: Populate state.market_history.global_base_prices with
    // commodity-specific prices from estimated_base_price(). The flat 100.0
    // used previously caused B2B spread deadlock for manufactured goods
    // (unit_cost >> 105.0 buy bid → spread never crosses → no trades).
    for commodity in Commodity::all() {
        state
            .market_history
            .global_base_prices
            .insert(commodity, corporate::estimated_base_price(commodity));
    }

    Ok(GeneratedWorld {
        state,
        regions,
        megaregions,
        diplomacy,
    })
}

fn generate_country(
    name: &str,
    start_year: StartYear,
    rng: &mut impl Rng,
) -> (Country, Currency, HashMap<String, Region>, Vec<Company>) {
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

    let (mut treasury, average_wage, energy_mix) = build_treasury(
        name,
        gdp_total,
        population,
        gdp_pc,
        &demographics,
        tech_limit,
        start_year,
        rng,
    );
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
        central_bank,
        currency_zone: None,
        interbank_market: crate::state::InterbankMarket::default(),
        bfg_fund: crate::state::BfgFund::default(),
        sobk_scheme: crate::state::SobkScheme::default(),
        bank_resolution: crate::state::BankResolution::default(),
        bank_tax: crate::state::BankTax::default(),
        stock_exchange: crate::securities::StockExchange::default(),
        dividend_queue: Vec::new(),
        ipo_queue: Vec::new(),
        bankruptcy_auction_pool: crate::corporate::BankruptcyAuctionPool::default(),
        demolition_queue: Vec::new(),
        halt_queue: Vec::new(),
        cooperative_registry: crate::society::housing::CooperativeRegistry::default(),
        furlough_wage_queue: Vec::new(),
        recruitment_cost_queue: Vec::new(),
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
        building_condition_config:
            crate::infrastructure::building_condition::BuildingConditionConfig::default(),
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
        education_config: crate::economy::config::education_config::EducationConfig::default(),
        disability_config: crate::economy::labor::disability_config::DisabilityConfig::default(),
        pension_law: None,
        pension_liabilities: Vec::new(),
        pension_contribution_history: std::collections::BTreeMap::new(),
        disability_pension_config: None,
        begging_config: None,
        justice_config: crate::economy::justice::justice_system::JusticeConfig::default(),
        infrastructure_config: crate::economy::infrastructure_config::InfrastructureConfig::default(
        ),
        innovation_config: crate::economy::innovation_config::InnovationConfig::default(),
        corporate_tech_config: crate::economy::corporate_config::CorporateTechConfig::default(),
        ip_theft_config: crate::economy::ip_theft::IPTheftConfig::default(),
        fish_stocks: Vec::new(),
        fish_farms: Vec::new(),
        fishing_policies: Vec::new(),
        special_economic_zones: Vec::new(),
        conservation_policies: Vec::new(),
        national_parks: Vec::new(),
        landscape_parks: Vec::new(),
        nature_reserves: Vec::new(),
        buffer_zones: Vec::new(),
        urban_parks: Vec::new(),
        utility_pricing_config: crate::utilities::UtilityPricingConfig::default(),
        utility_config: crate::utilities::UtilityConfig::default(),
        natural_wonders: Vec::new(),
        tourism_destinations: BTreeMap::new(),
        social_programs: Vec::new(),
        weather_state: crate::economy::weather::WeatherState::default(),
        maintenance_config: crate::economy::maintenance::MaintenanceConfig::default(),
        state_forest_state: crate::economy::state_forests::ForestDistrictState::default(),
        religious_authority_state:
            crate::society::religious_authority::ReligiousAuthorityState::default(),
        generative_goods_config:
            crate::economy::generative_goods_config::GenerativeGoodsConfig::default(),
        geological_formations: Vec::new(),
        mining_concessions: crate::economy::production::geology::MiningConcessionRegistry::default(
        ),
        geological_survey_ledger:
            crate::economy::production::geology::GeologicalSurveyLedger::default(),
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
        market_clearing_config:
            crate::economy::market::clearing_config::MarketClearingConfig::default(),
        labor_config: crate::economy::labor::labor_config::LaborConfig::default(),
        geography_config: crate::society::geography_config::GeographyConfig::default(),
        municipal_infrastructure_plan:
            crate::energy::municipal_infrastructure_ai::MunicipalInfrastructurePlan::default(),
        state_customs_warehouse: rustc_hash::FxHashMap::default(),
        last_smuggling_result: None,
        pending_foreign_transit_fees: Vec::new(),
    };
    country.macro_indicators.currency = currency.prefix.clone();

    // Phase 65: Assign StateStructure based on government form.
    let state_structure = assign_state_structure(&country.politics.government_form, rng);
    country.politics.state_structure = state_structure;

    let mut companies = Vec::new(); // Empty companies for bootstrap
                                    // Add bank companies
    let bank_companies = build_bank_companies(name, &mut country.budget, &country.central_bank);
    // Phase 37: Populate debt_market with DSPW primary dealers and enable DSPW.
    let dspw_dealers: Vec<String> = bank_companies
        .iter()
        .filter(|b| b.is_dspw)
        .map(|b| b.id.clone())
        .collect();
    if !dspw_dealers.is_empty() {
        country.debt_market.dspw_enabled = true;
        country.debt_market.primary_dealers = dspw_dealers;
    }
    companies.extend(bank_companies);
    crate::politics::bootstrap_politics(&mut country, &mut companies, start_year as u32, rng);

    let mut country_regions = generate_regional_topology(
        name,
        population as i64,
        gdp_total,
        start_year.as_year(),
        &cultural.demonym,
        &cultural.ethnic_composition,
    );

    // Phase 21A: Generate geological formations with finite, depletable deposits.
    let region_ids: Vec<String> = country_regions.keys().cloned().collect();
    let formations = crate::society::geography::generate_geological_formations(&region_ids, rng);

    // World Generation & Climate Audit (v0.5.3): Re-seed region resources
    // from the geological formations, replacing the homogeneous smear that
    // generate_regional_topology applied via seed_geological_deposits().
    // This enforces geographic sparsity — regions without overlapping
    // formations get NO geological resources (forcing reliance on biomass,
    // hydro, or imports).
    crate::society::geography::reseed_resources_from_formations(
        &mut country_regions,
        &formations,
        rng,
    );

    // Phase 58: Generate topological cadastre with slotmap-backed ParcelChunks.
    let region_list: Vec<Region> = country_regions.values().cloned().collect();
    let cadastre = generate_cadastre(name, &region_list, rng, 0);

    // Populate parcel_ids on each region based on the generated cadastre.
    for region in country_regions.values_mut() {
        region.parcel_ids.clear();
    }
    for (parcel_id, parcel) in cadastre.iter() {
        if let Some(region) = country_regions.get_mut(&parcel.region_id) {
            region.parcel_ids.push(parcel_id);
        }
    }

    // Store the cadastre on the country.
    country.cadastre = cadastre;
    country.geological_formations = formations;

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

    // Phase 70: Generate the Order of Battle natively (no flat list, no shim).
    country.order_of_battle =
        spawn_standing_oob(&country, &country_regions, start_year.as_year(), rng);

    // Phase 74: Seed initial military stockpile with 3 turns of upkeep worth
    // of Ammunition and Rifles so armies don't immediately starve on Turn 1.
    // The stockpile is proportional to the total manpower under arms.
    let total_manpower: f64 = country
        .order_of_battle
        .armies
        .iter()
        .flat_map(|a| a.divisions.iter())
        .map(|d| d.total_manpower() as f64)
        .sum();
    if total_manpower > 0.0 {
        // Ammunition: ~15 units per 1000 soldiers per turn × 3 turns
        let ammo_seed = (total_manpower / 1000.0 * 15.0 * 3.0).max(500.0);
        // Rifles: ~1 rifle per 3 soldiers (not everyone needs a rifle) × 3 turns reserve
        let rifle_seed = (total_manpower / 3.0 * 3.0).max(200.0);
        country
            .military_stockpile
            .insert(crate::registries::enums::Commodity::Ammunition, ammo_seed);
        country
            .military_stockpile
            .insert(crate::registries::enums::Commodity::Rifles, rifle_seed);
    }

    // Phase 77: List JSC companies on the stock exchange during world generation.
    // NOTE: This is now called from generate_corporate_entities after JSC companies
    // are actually created. The call here was operating on bootstrap bank companies
    // only, which are never JSC.

    // Bugfix Sprint (5B): init_power_grid is now called AFTER generate_corporate_entities
    // (in the world gen flow) so that LV/MV capacities can be derived from actual
    // connected housing/commercial electricity demand. See generate_world().

    (country, currency, country_regions, companies)
}

/// Phase 77: List all JointStockCompany companies on the stock exchange.
///
/// For each JSC company:
/// 1. Set an initial listing price based on fixed_capital / shares_issued.
/// 2. Seed an AMM liquidity pool with the free-float shares + IPO cash proceeds.
/// 3. Fund IPO proceeds from wealthy demographics (Aristocracy, Bourgeoisie),
///    capped at 5% of each class's savings per region.
/// 4. Unsold shares (when citizen savings are exhausted) go into the limit
///    order book as ask orders at the listing price — NOT into the AMM.
/// 5. Assign initial share ownership to founders and wealthy demographics.
pub fn list_jsc_companies_on_exchange(
    country: &mut crate::state::Country,
    companies: &mut Vec<Company>,
    country_regions: &HashMap<String, crate::society::geography::Region>,
    rng: &mut impl rand::Rng,
) {
    use crate::entities::LegalForm;
    use crate::securities::exchange::{InstrumentType, LiquidityPool, Order};

    for company in companies.iter_mut() {
        // Only list JointStockCompany firms
        let (shares_issued, free_float_pct) = match company.legal_form {
            LegalForm::JointStockCompany(ref jsd) => (jsd.shares_issued, jsd.free_float),
            _ => continue,
        };
        if shares_issued == 0 {
            continue;
        }

        // Calculate listing price from fixed capital per share
        let listing_price = if shares_issued > 0 {
            (company.fixed_capital / shares_issued as f64).max(1.0)
        } else {
            1.0
        };

        let instrument_id = format!("EQUITY:{}", company.id);
        let free_float_shares = (shares_issued as f64 * free_float_pct).round() as u64;
        let founder_shares = shares_issued - free_float_shares;

        // Assign founder ownership (60% to founding entity)
        company.shares_count = shares_issued;
        company.owners.insert(
            format!("FOUNDER:{}", company.id),
            founder_shares as f64 / shares_issued as f64,
        );
        company.free_float = free_float_pct;

        // Phase 77: Fund IPO from wealthy demographics only.
        // Draw from Aristocracy (rural) and Bourgeoisie (urban) savings,
        // capped at 5% of each class's savings per region.
        let company_region = company.region_id.clone();
        let ipo_target_cash = free_float_shares as f64 * listing_price;

        // Collect wealthy-class savings from the company's region
        let mut wealthy_cash_available = 0.0_f64;
        if let Some(region) = country_regions.get(&company_region) {
            if let Some(aristocracy) = region.class_demographics.rural_classes.get(&RuralClass::Aristocracy) {
                wealthy_cash_available += aristocracy.savings * 0.05;
            }
            if let Some(bourgeoisie) = region.class_demographics.urban_classes.get(&UrbanClass::Bourgeoisie) {
                wealthy_cash_available += bourgeoisie.savings * 0.05;
            }
        }

        let (purchased_shares, unsold_shares, ipo_cash);
        if wealthy_cash_available >= ipo_target_cash {
            // Full IPO — all free-float shares purchased by wealthy classes
            purchased_shares = free_float_shares;
            unsold_shares = 0;
            ipo_cash = ipo_target_cash;
        } else if wealthy_cash_available > 0.0 {
            // Partial IPO — purchase what citizens can afford
            let mut purchased = (wealthy_cash_available / listing_price).floor() as u64;
            purchased = purchased.min(free_float_shares);
            purchased_shares = purchased;
            unsold_shares = free_float_shares - purchased_shares;
            ipo_cash = purchased_shares as f64 * listing_price;
        } else {
            // No wealthy savings — all shares go to order book as asks
            purchased_shares = 0;
            unsold_shares = free_float_shares;
            ipo_cash = 0.0;
        }

        // Debit wealthy-class savings (proportionally from Aristocracy and Bourgeoisie)
        if ipo_cash > 0.0 {
            if let Some(region) = country_regions.get(&company_region) {
                let aristo_savings = region
                    .class_demographics
                    .rural_classes
                    .get(&RuralClass::Aristocracy)
                    .map(|d| d.savings * 0.05)
                    .unwrap_or(0.0);
                let bourg_savings = region
                    .class_demographics
                    .urban_classes
                    .get(&UrbanClass::Bourgeoisie)
                    .map(|d| d.savings * 0.05)
                    .unwrap_or(0.0);
                let total_wealthy = aristo_savings + bourg_savings;
                if total_wealthy > 0.0 {
                    let aristo_share =
                        (ipo_cash * aristo_savings / total_wealthy).min(aristo_savings);
                    let bourg_share = (ipo_cash * bourg_savings / total_wealthy).min(bourg_savings);
                    // Debit from country-level citizen_savings as a proxy
                    // (region-level savings are aggregated into citizen_savings)
                    country.budget.citizen_savings =
                        (country.budget.citizen_savings - aristo_share - bourg_share).max(0.0);
                }
            }
        }

        // Credit IPO proceeds to the company's brokerage account
        if ipo_cash > 0.0 {
            if let Some(ref mut ba) = company.brokerage_account {
                ba.cash += ipo_cash;
            } else {
                company.liquid_capital += ipo_cash;
            }
        }

        // Seed AMM liquidity pool with purchased shares + cash (valid constant-product pool)
        if purchased_shares > 0 && ipo_cash > 0.0 {
            let pool = LiquidityPool {
                shares: purchased_shares,
                cash: ipo_cash,
                providers: {
                    let mut p = std::collections::BTreeMap::new();
                    p.insert("WEALTHY_CLASS_AGGREGATE".to_string(), 1.0);
                    p
                },
                pool_fee: 0.001,
                treasury_bonds: Vec::new(),
                total_value: ipo_cash,
            };
            country
                .stock_exchange
                .liquidity_pools
                .insert(instrument_id.clone(), pool);
        }

        // Place unsold shares as limit ask orders in the order book
        if unsold_shares > 0 {
            let order = Order::Sell {
                order_id: format!("IPO-ASK-{}-{}", company.id, rng.gen_range(0..100000)),
                investor_id: format!("TREASURY:{}", company.id),
                instrument_id: instrument_id.clone(),
                instrument_type: InstrumentType::Equity,
                quantity: unsold_shares,
                limit_price: listing_price,
                expiry_turn: u32::MAX, // No expiry — sits until bought
            };
            // Insert into order book
            let ob = country
                .stock_exchange
                .order_book
                .entry(instrument_id.clone())
                .or_default();
            // Add to asks at listing price
            if let Some(pos) = ob
                .asks
                .iter()
                .position(|(p, _)| (*p - listing_price).abs() < 0.001)
            {
                ob.asks[pos].1.push(order);
            } else {
                ob.asks.push((listing_price, vec![order]));
                ob.asks
                    .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            }
            if ob.best_ask <= 0.0 || listing_price < ob.best_ask {
                ob.best_ask = listing_price;
            }
        }
    }
}

/// Phase 70: Spawn a standing Order of Battle for a country based on its
/// population, GDP, and era.
///
/// This function constructs the OOB hierarchy natively:
/// `OrderOfBattle → Army → Division → Regiment → MilitaryUnit`.
///
/// There is no `rebuild_oob()` shim and no flat-list-to-hierarchy conversion.
///
/// # Rules
/// * Army size = max(1000, population * 0.005) — 0.5% of population under arms.
/// * OOB structure is generated via `generate_asymmetric_oob` (rich/poor scaling).
/// * Era-appropriate unit types are applied post-generation:
///   - Artillery added if year >= 1880.
///   - Tanks added if year >= 1916.
///   - Air Force added if year >= 1940.
///   - Naval units added if country has coastline and year >= 1880.
/// * Equipment is seeded at 90% ToE strength (not 100% — represents existing stock).
/// * Manpower is drawn proportionally from rural classes.
fn spawn_standing_oob(
    country: &crate::state::Country,
    regions: &HashMap<String, crate::society::geography::Region>,
    start_year: u32,
    rng: &mut impl Rng,
) -> crate::military::oob::OrderOfBattle {
    use crate::military::oob::{generate_asymmetric_oob, Army, Division, OrderOfBattle, Regiment};
    use crate::military::units::{EquipmentReserve, MilitaryUnit, UnitType};

    let total_pop: i64 = regions.values().map(|r| r.population).sum();
    let total_gdp: f64 = regions.values().map(|r| r.gdp).sum();
    let has_coast = regions.values().any(|r| r.geographic_traits.has_coastline);

    // Phase 76: Derive GDP per capita and average wage for OOB scaling.
    let gdp_per_capita = if total_pop > 0 {
        total_gdp / total_pop as f64
    } else {
        0.0
    };
    let average_wage = country.macro_indicators.average_wage.max(1.0);

    // Collect home regions for army basing.
    let home_regions: Vec<String> = regions.keys().take(8).cloned().collect();
    if home_regions.is_empty() {
        return OrderOfBattle::default();
    }

    // Generate the base OOB structure natively (asymmetric rich/poor).
    let mut oob = generate_asymmetric_oob(
        &country.name,
        total_gdp,
        gdp_per_capita,
        average_wage,
        total_pop,
        home_regions.clone(),
        rng,
    );

    // Helper: scale ToE by manpower and seed at 90% strength
    let make_toe = |unit_type: &UnitType, manpower: i64| -> Vec<EquipmentReserve> {
        unit_type
            .table_of_equipment(start_year)
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
                         needed: i64|
     -> rustc_hash::FxHashMap<crate::society::geography::RuralClass, i64> {
        let mut origin = rustc_hash::FxHashMap::default();
        let total_rural: i64 = regions
            .values()
            .flat_map(|r| r.class_demographics.rural_classes.values())
            .map(|d| d.population)
            .sum();
        if total_rural <= 0 {
            return origin;
        }
        for region in regions.values() {
            for (class_key, demo) in &region.class_demographics.rural_classes {
                let rural_class = Some(*class_key);
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

    let army_size = ((total_pop as f64) * 0.005).max(1000.0) as i64;

    // Apply era-appropriate equipment and unit type adjustments to all units.
    for army in &mut oob.armies {
        for division in &mut army.divisions {
            for regiment in &mut division.regiments {
                for unit in &mut regiment.units {
                    // Apply era-appropriate equipment
                    unit.equipment_reserves = make_toe(&unit.unit_type, unit.manpower);
                    // Apply proportional manpower origin
                    unit.manpower_origin = draw_manpower(regions, unit.manpower);
                }
            }
        }
    }

    // Phase 76: Support Army is conditional on country size.
    // Only countries with population > 5M and sufficient GDP per capita
    // can afford specialized artillery/tank/air/naval arms.
    let support_army_threshold_pop = 5_000_000_i64;
    // Phase 76: Support Army affordability — based on absolute GDP per capita
    // thresholds (not wage-relative, since average_wage = gdp_pc × 800 in the
    // generator, making wage-relative thresholds always fail).
    // gdp_per_capita > 500: post-industrial economy can support specialist arms.
    let can_afford_specialists = gdp_per_capita > 500.0;

    if total_pop > support_army_threshold_pop && can_afford_specialists {
        let mut support_army = Army::new(
            format!("ARMY-{}-SPT", country.name),
            format!("{} Support Command", country.name),
            home_regions[0].clone(),
        );

        let mut support_division = Division::new(
            format!("DIV-{}-SPT-001", country.name),
            "Support Division".to_string(),
            home_regions[0].clone(),
        );

        let mut support_regiment = Regiment::new(
            format!("REG-{}-SPT-001", country.name),
            "Specialist Regiment".to_string(),
            home_regions[0].clone(),
        );

        // Artillery Brigade (if year >= 1880)
        if start_year >= 1880 {
            let arty_manpower = (army_size / 10).max(50);
            let mut artillery = MilitaryUnit::new(
                format!("{}-ART-1", country.name),
                UnitType::Artillery,
                arty_manpower,
                draw_manpower(regions, arty_manpower),
                home_regions[0].clone(),
            );
            artillery.equipment_reserves = make_toe(&UnitType::Artillery, arty_manpower);
            support_regiment.add_unit(artillery);
        }

        // Tank Brigade (if year >= 1916 and country can afford armored units)
        if start_year >= 1916 && gdp_per_capita > 1000.0 {
            let tank_manpower = (army_size / 20).max(50);
            let mut tanks = MilitaryUnit::new(
                format!("{}-TNK-1", country.name),
                UnitType::Tanks,
                tank_manpower,
                draw_manpower(regions, tank_manpower),
                home_regions[0].clone(),
            );
            tanks.equipment_reserves = make_toe(&UnitType::Tanks, tank_manpower);
            support_regiment.add_unit(tanks);
        }

        // Air Wing (if year >= 1940 and country can afford air force)
        if start_year >= 1940 && gdp_per_capita > 1500.0 {
            let air_manpower = (army_size / 50).max(30);
            let mut air = MilitaryUnit::new(
                format!("{}-AIR-1", country.name),
                UnitType::AirForce,
                air_manpower,
                draw_manpower(regions, air_manpower),
                home_regions[0].clone(),
            );
            air.equipment_reserves = make_toe(&UnitType::AirForce, air_manpower);
            support_regiment.add_unit(air);
        }

        // Naval Fleet (if coastal, year >= 1880, and country can afford navy)
        if has_coast && start_year >= 1880 && gdp_per_capita > 800.0 {
            let naval_manpower = (army_size / 20).max(50);
            let coastal_region = regions
                .values()
                .find(|r| r.geographic_traits.has_coastline)
                .map(|r| r.id.clone())
                .unwrap_or_else(|| home_regions[0].clone());
            let mut naval = MilitaryUnit::new(
                format!("{}-NAV-1", country.name),
                UnitType::Naval,
                naval_manpower,
                draw_manpower(regions, naval_manpower),
                coastal_region,
            );
            naval.equipment_reserves = make_toe(&UnitType::Naval, naval_manpower);
            support_regiment.add_unit(naval);
        }

        // Only add the support army if it has units
        if !support_regiment.units.is_empty() {
            support_division.add_regiment(support_regiment);
            support_army.add_division(support_division);
            oob.add_army(support_army);
        }
    }

    oob
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

fn build_demographics(
    cultural: &CulturalBackground,
    _population: u64,
    gdp_pc: f64,
) -> Demographics {
    let illiteracy_rate = (0.4 - (gdp_pc * 0.15)).max(0.01);
    let secondary_edu_total = (gdp_pc * 0.15).min(0.45);
    let higher_edu_total = (gdp_pc * 0.08).min(0.35);
    let basic_edu_total = (1.0 - higher_edu_total - secondary_edu_total - illiteracy_rate).max(0.0);

    let mut secondary_map = BTreeMap::new();
    secondary_map.insert("Vocational".to_string(), secondary_edu_total * 0.4);
    secondary_map.insert("Technical".to_string(), secondary_edu_total * 0.3);
    secondary_map.insert("Humanities".to_string(), secondary_edu_total * 0.3);

    let mut higher_map = BTreeMap::new();
    higher_map.insert("Technical".to_string(), higher_edu_total * 0.4);
    higher_map.insert("Humanities".to_string(), higher_edu_total * 0.4);
    higher_map.insert("Medical".to_string(), higher_edu_total * 0.2);

    let education = Education {
        none: illiteracy_rate,
        basic: basic_edu_total,
        secondary: secondary_map,
        higher: higher_map,
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

    let mining_share = if is_petrostate {
        rng.gen_range(0.15..0.4)
    } else {
        rng.gen_range(0.01..0.05)
    };
    let agriculture_share = if gdp_pc < 1.5 {
        rng.gen_range(agri_range.start..agri_range.end)
    } else {
        rng.gen_range(0.01..0.05)
    };
    let heavy_industry_share = rng.gen_range(0.05..0.25) * industry_mult;
    let light_industry_share = rng.gen_range(0.1..0.3) * industry_mult;
    let local_services_share = rng.gen_range(0.2..0.4) * services_mult;
    let export_services_share = if gdp_pc > 2.0 {
        rng.gen_range(0.05..0.3) * services_mult
    } else {
        rng.gen_range(0.01..0.05)
    };
    let construction_share = rng.gen_range(0.05..0.15);
    let energy_share = rng.gen_range(0.05..0.12);
    let healthcare_services_share = rng.gen_range(0.04..0.10) * services_mult;
    let education_services_share = rng.gen_range(0.03..0.08) * services_mult;

    let sum = mining_share
        + agriculture_share
        + heavy_industry_share
        + light_industry_share
        + local_services_share
        + export_services_share
        + construction_share
        + energy_share
        + healthcare_services_share
        + education_services_share;

    let coal_mix_raw = rng.gen_range(0.3..0.8);
    let gas_mix_raw = rng.gen_range(0.1..0.6);
    let renewables_mix_raw = rng.gen_range(0.05..0.2);
    let mix_sum = coal_mix_raw + gas_mix_raw + renewables_mix_raw;

    let energy_mix = EnergyMix {
        coal: coal_mix_raw / mix_sum,
        natural_gas: gas_mix_raw / mix_sum,
        uranium: 0.0,
        renewables: renewables_mix_raw / mix_sum,
        extra: Map::new(),
    };

    let average_wage = gdp_pc * 800.0;

    let mut sectors = HashMap::new();
    sectors.insert(
        Sector::Mining,
        sector_share(mining_share / sum, 0.5, tech_limit),
    );
    sectors.insert(
        Sector::Agriculture,
        sector_share(agriculture_share / sum, 0.2, tech_limit),
    );
    sectors.insert(
        Sector::HeavyIndustry,
        sector_share(heavy_industry_share / sum, 0.6, tech_limit),
    );
    sectors.insert(
        Sector::LightIndustry,
        sector_share(light_industry_share / sum, 0.4, tech_limit),
    );
    sectors.insert(
        Sector::LocalServices,
        sector_share(local_services_share / sum, 0.3, tech_limit),
    );
    sectors.insert(
        Sector::ExportServices,
        sector_share(export_services_share / sum, 0.7, tech_limit),
    );
    sectors.insert(
        Sector::Construction,
        sector_share(construction_share / sum, 0.8, tech_limit),
    );
    sectors.insert(
        Sector::Energy,
        sector_share(energy_share / sum, 0.3, tech_limit),
    );
    sectors.insert(
        Sector::PublicServices,
        sector_share(
            (healthcare_services_share + education_services_share) / sum,
            0.2,
            tech_limit,
        ),
    );

    let mut allocations = HashMap::new();
    allocations.insert(
        "Industry".to_string(),
        Value::from(rng.gen_range(0.02..0.15)),
    );
    allocations.insert(
        "Education and Propaganda".to_string(),
        Value::from(rng.gen_range(0.02..0.1)),
    );
    allocations.insert(
        "Healthcare".to_string(),
        Value::from(rng.gen_range(0.05..0.15)),
    );
    allocations.insert(
        "Infrastructure and Transport".to_string(),
        Value::from(rng.gen_range(0.05..0.2)),
    );
    allocations.insert(
        "Social Programs".to_string(),
        Value::from(rng.gen_range(0.05..0.25)),
    );
    allocations.insert(
        "Agriculture and Rural Development".to_string(),
        Value::from(rng.gen_range(0.02..0.1)),
    );
    allocations.insert(
        "Armed Forces".to_string(),
        Value::from(rng.gen_range(0.02..0.15)),
    );
    allocations.insert(
        "Justice".to_string(),
        Value::from(rng.gen_range(0.01..0.05)),
    );
    allocations.insert(
        "Public Administration".to_string(),
        Value::from(rng.gen_range(0.01..0.05)),
    );

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
        education_propaganda: allocations["Education and Propaganda"]
            .as_f64()
            .unwrap_or(0.0),
        healthcare: allocations["Healthcare"].as_f64().unwrap_or(0.0),
        infrastructure_transport: allocations["Infrastructure and Transport"]
            .as_f64()
            .unwrap_or(0.0),
        social_programs: allocations["Social Programs"].as_f64().unwrap_or(0.0),
        agriculture_rural: allocations["Agriculture and Rural Development"]
            .as_f64()
            .unwrap_or(0.0),
        armed_forces: allocations["Armed Forces"].as_f64().unwrap_or(0.0),
        justice: allocations["Justice"].as_f64().unwrap_or(0.0),
        public_administration: allocations["Public Administration"].as_f64().unwrap_or(0.0),
        extra: Map::new(),
    };

    // Initialize discovered technologies from the actual tech tree (Phase E.1).
    // Previously this generated fake IDs like "tech_001" through "tech_017",
    // which don't exist in the tree (real IDs are branch-specific like
    // "thermo_001", "steam_001", "chem_001"). Now we select real tech IDs
    // whose `year <= start_year` and whose prerequisites are also satisfied.
    let tech_tree = crate::registries::tech_tree_data::default_tech_tree();
    let discovered: Vec<String> = {
        let mut sorted_techs: Vec<(&String, &crate::registries::tech_tree::TechNode)> =
            tech_tree.iter().collect();
        sorted_techs.sort_by_key(|(id, node)| (node.year, (*id).clone()));
        let mut discovered_set: std::collections::HashSet<String> = std::collections::HashSet::new();
        // Iteratively add techs whose prerequisites are met (earliest year first).
        // Multiple passes handle chained prerequisites.
        loop {
            let mut added_any = false;
            for (tech_id, node) in &sorted_techs {
                if discovered_set.contains(*tech_id) {
                    continue;
                }
                if node.year > start_year as u32 {
                    continue;
                }
                let prereqs_met = node
                    .prerequisites
                    .iter()
                    .all(|p| discovered_set.contains(p));
                if prereqs_met {
                    discovered_set.insert(tech_id.to_string());
                    added_any = true;
                }
            }
            if !added_any {
                break;
            }
        }
        discovered_set.into_iter().collect()
    };

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
            innovation_pool: HashMap::new(),
            research_output: 0.0,
            researching: None,
            discovered,
            extra: Map::new(),
        },
        tax_office_ids: Vec::new(),
        tax_history: std::collections::VecDeque::new(),
        last_balance_log: String::new(),
        trade_balance: None,
        max_public_wage_multiplier: 1.2, // Phase 5: Default to prevent crowding out
        equalization_fund: 0.0,          // Phase D.9: Equalization fund starts empty
        external_financing_injected: 0.0, // Phase M0-Audit
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
            ..Default::default()
        }),
        extra: Map::new(),
    }
}

/// Phase 92: Labor intensity ratio — maps a sector's GDP share to its
/// employment share for a given era.
///
/// Values >1.0 mean the sector is labor-intensive (employs more workers per
/// unit of GDP than the average). Values <1.0 mean the sector is capital-
/// intensive (employs fewer workers per unit of GDP).
///
/// In 1900, agriculture was highly labor-intensive (2.5×) because pre-
/// mechanization farming required many workers per unit of output. Heavy
/// industry was capital-intensive (0.6×) because factories used expensive
/// machinery that replaced labor. By 1975, agricultural mechanization reduced
/// its labor intensity to ~1.0×, while services became slightly less labor-
/// intensive due to automation.
fn labor_intensity_ratio(sector: Sector, start_year: StartYear) -> f64 {
    match start_year {
        StartYear::Y1900 => match sector {
            Sector::Agriculture => 2.5,
            Sector::Mining => 0.8,
            Sector::HeavyIndustry => 0.6,
            Sector::LightIndustry => 1.0,
            Sector::LocalServices => 1.2,
            Sector::ExportServices => 1.0,
            Sector::Construction => 1.5,
            Sector::Energy => 0.5,
            Sector::PublicServices => 1.3,
            Sector::MedicalServices => 1.4,
            Sector::EducationalServices => 1.5,
            Sector::TransportLogistics => 1.1,
            Sector::Hospitality => 1.3,
            Sector::MediaAndEntertainment => 1.2,
            Sector::MaintenanceWorkshops => 1.3,
            Sector::ArmamentsIndustry => 0.8,
            _ => 1.0,
        },
        StartYear::Y1925 => match sector {
            Sector::Agriculture => 2.0,
            Sector::Mining => 0.8,
            Sector::HeavyIndustry => 0.7,
            Sector::LightIndustry => 1.0,
            Sector::LocalServices => 1.1,
            Sector::ExportServices => 1.0,
            Sector::Construction => 1.4,
            Sector::Energy => 0.6,
            Sector::PublicServices => 1.2,
            Sector::MedicalServices => 1.3,
            Sector::EducationalServices => 1.4,
            Sector::TransportLogistics => 1.1,
            Sector::Hospitality => 1.2,
            Sector::MediaAndEntertainment => 1.1,
            Sector::MaintenanceWorkshops => 1.2,
            Sector::ArmamentsIndustry => 0.8,
            _ => 1.0,
        },
        StartYear::Y1950 => match sector {
            Sector::Agriculture => 1.5,
            Sector::Mining => 0.8,
            Sector::HeavyIndustry => 0.8,
            Sector::LightIndustry => 1.0,
            Sector::LocalServices => 1.0,
            Sector::ExportServices => 1.0,
            Sector::Construction => 1.3,
            Sector::Energy => 0.6,
            Sector::PublicServices => 1.1,
            Sector::MedicalServices => 1.2,
            Sector::EducationalServices => 1.3,
            Sector::TransportLogistics => 1.0,
            Sector::Hospitality => 1.1,
            Sector::MediaAndEntertainment => 1.0,
            Sector::MaintenanceWorkshops => 1.1,
            Sector::ArmamentsIndustry => 0.9,
            _ => 1.0,
        },
        StartYear::Y1975 => match sector {
            Sector::Agriculture => 1.0,
            Sector::Mining => 0.9,
            Sector::HeavyIndustry => 0.9,
            Sector::LightIndustry => 1.0,
            Sector::LocalServices => 0.9,
            Sector::ExportServices => 1.0,
            Sector::Construction => 1.2,
            Sector::Energy => 0.7,
            Sector::PublicServices => 1.1,
            Sector::MedicalServices => 1.1,
            Sector::EducationalServices => 1.2,
            Sector::TransportLogistics => 1.0,
            Sector::Hospitality => 1.1,
            Sector::MediaAndEntertainment => 1.0,
            Sector::MaintenanceWorkshops => 1.1,
            Sector::ArmamentsIndustry => 0.9,
            _ => 1.0,
        },
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
    start_year: StartYear,
    treasury: &mut Treasury,
    rng: &mut impl Rng,
) -> MacroData {
    let unemployment_rate = rng.gen_range(3.0..15.0);
    let workforce = (treasury.population as f64 * activity_rate / 100.0).max(1.0);
    let employed_total = (workforce * (1.0 - unemployment_rate / 100.0)).max(0.0);
    let unemployed = (workforce - employed_total).max(0.0);

    // Phase 92: Labor-intensity-weighted employment distribution.
    // GDP share ≠ employment share. In 1900, agriculture had ~40-60% of
    // employment but only ~15-30% of GDP (low labor productivity). Heavy
    // industry had ~10-15% of employment but ~20-30% of GDP (high capital
    // intensity). Using GDP share directly understates agricultural employment
    // and overstates industrial employment.
    //
    // The labor_intensity_ratio maps GDP share → employment share. Values >1.0
    // mean the sector employs more workers per unit of GDP than average (labor-
    // intensive). Values <1.0 mean the sector is capital-intensive.
    let total_weighted: f64 = treasury
        .sectors
        .iter()
        .map(|(sector, s)| s.gdp_share * labor_intensity_ratio(*sector, start_year))
        .sum();
    if total_weighted > 0.0 {
        for (sector, share) in treasury.sectors.iter_mut() {
            let weight = share.gdp_share * labor_intensity_ratio(*sector, start_year);
            let share_emp = (employed_total * (weight / total_weighted)) as i64;
            share
                .extra
                .insert("employment".to_string(), Value::from(share_emp));
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
        subsistence_peasants: if gdp_pc < 1.5 {
            population_f64(demographics) * rng.gen_range(0.05..0.40)
        } else {
            population_f64(demographics) * 0.01
        },
        ..LaborMarket::default()
    };

    let mut extra = Map::new();
    // health_statistics and education_statistics are now typed fields on MacroData,
    // so they must NOT be duplicated in extra (Rule 12 — duplicate field error).
    // Only legacy/untyped keys remain in extra.
    extra.insert("minimum_wage".to_string(), Value::from(average_wage * 0.0));

    let mut policy_extra = Map::new();
    policy_extra.insert(
        "military_law".to_string(),
        serde_json::json!({
            "mandatory_service": "mandatory_training",
            "women_in_army": "reserve_only",
            "conscription_scope": "voluntary"
        }),
    );
    extra.insert("policy".to_string(), Value::Object(policy_extra));

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
        demonym: cultural.demonym.clone(),
        cultural_group: cultural.cultural_group.clone(),
        religion: cultural.religion.clone(),
        election_turn: 0,
        labor_market,
        demographics: demographics.clone(),
        health_statistics: crate::state::macro_data::HealthStatistics {
            service_quality: rng.gen_range(30.0..70.0),
            average_lifespan: 40.0 + (gdp_pc * 10.0),
            mortality_rate: 0.0,
            hospital_coverage: rng.gen_range(20.0..60.0) * (gdp_pc / 2.0),
        },
        education_statistics: crate::state::macro_data::EducationStatistics {
            // Phase C.4: Derive from actual demographics, not magic envelopes.
            // literacy_rate = 1 - none (share with any formal education).
            // higher_education_rate = sum of higher specializations.
            literacy_rate: (1.0 - demographics.education.none).clamp(0.0, 1.0),
            higher_education_rate: demographics.education.higher_share().clamp(0.0, 1.0),
            // infrastructure_base: derived from education capacity (physical),
            // not a random magic number. Use the education share as a proxy
            // for institutional development (Rule 2: no magic constants).
            infrastructure_base: ((1.0 - demographics.education.none) * 100.0).clamp(0.0, 100.0),
        },
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

/// Phase 65: Assign a StateStructure based on the government form.
///
/// Autocratic forms (Absolute Monarchy, One-Party State, Military Dictatorship,
/// Theocracy) default to Totalitarian. Democratic forms get Unitary or Federation
/// based on a random draw. AutonomousRepublic is not assigned at country level
/// during generation — it is a per-region designation.
fn assign_state_structure(
    government_form: &crate::politics::system::GovernmentForm,
    rng: &mut impl Rng,
) -> crate::politics::state_structure::StateStructure {
    use crate::politics::state_structure::StateStructure;
    use crate::politics::system::GovernmentForm;

    match government_form {
        GovernmentForm::AbsoluteMonarchy
        | GovernmentForm::OnePartyState
        | GovernmentForm::MilitaryDictatorship
        | GovernmentForm::Theocracy => StateStructure::Totalitarian,
        GovernmentForm::ConstitutionalMonarchy | GovernmentForm::DirectorialDemocracy => {
            // Federations are more likely for these forms
            if rng.gen::<f64>() < 0.6 {
                StateStructure::Federation
            } else {
                StateStructure::Unitary
            }
        }
        _ => {
            // Parliamentary/Presidential republics: mostly unitary, sometimes federation
            if rng.gen::<f64>() < 0.3 {
                StateStructure::Federation
            } else {
                StateStructure::Unitary
            }
        }
    }
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
            brackets: vec![crate::state::tax::TaxBracket {
                threshold: 5_000_000.0,
                rate: 0.01,
                extra: Map::new(),
            }],
            asset_types: vec!["liquid_capital".to_string(), "real_estate".to_string()],
            extra: Map::new(),
        },
        capital_gains_tax: crate::state::tax::CapitalGainsTax {
            // Phase 39: Baseline capital gains tax — 19% (Belka tax)
            brackets: vec![crate::state::tax::TaxBracket {
                threshold: 0.0,
                rate: 0.19,
                extra: Map::new(),
            }],
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

fn build_currency(name: &str, _treasury: &Treasury) -> Currency {
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
    let fx_reserve_value = treasury.gdp * rng.gen_range(0.05..0.20);

    // Initialize FX reserves with some foreign currencies
    let mut fx_reserves = std::collections::HashMap::new();
    fx_reserves.insert("USD".to_string(), fx_reserve_value * 0.5);
    fx_reserves.insert("EUR".to_string(), fx_reserve_value * 0.3);
    fx_reserves.insert("GBP".to_string(), fx_reserve_value * 0.2);

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
        name: format!("Central Bank of {}", name),
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
        target_inflation: 0.02, // 2% inflation target
        potential_growth: 0.02, // 2% potential GDP growth
        neutral_rate: 0.02,     // 2% neutral real rate
        extra: Map::new(),
    }
}

fn build_bank_companies(
    name: &str,
    treasury: &mut Treasury,
    central_bank: &crate::state::CentralBank,
) -> Vec<Company> {
    let mut rng = rand::thread_rng();
    let prefix = &name[..3.min(name.len())].to_uppercase();

    // Phase 91: Generate multiple banks based on GDP, not population.
    // Each bank serves an economy of ~500K average-wage-years of GDP.
    // This is dynamic, inflation-proof, and scales with development level.
    // A high-GDP small-population country gets enough banks; a low-GDP
    // large-population country doesn't get undercapitalized banks.
    // Cap at 8 to avoid excessive fragmentation in huge economies.
    let avg_wage = (treasury.gdp / treasury.population.max(1) as f64).max(1000.0);
    let gdp_per_bank_threshold = avg_wage * 500_000.0;
    let num_banks = ((treasury.gdp / gdp_per_bank_threshold).round() as usize)
        .max(1)
        .min(8);
    let num_dspw = num_banks.div_ceil(2).min(3); // Half of banks, max 3

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
        } else {
            // Phase 90: Weighted distribution — 40% Commercial, 30% Cooperative,
            // 20% Investment, 10% Universal. Cooperative banks handle
            // agricultural and small-business working capital loans.
            let roll = rng.gen::<f64>();
            if roll < 0.40 {
                BankingBankType::Commercial
            } else if roll < 0.70 {
                BankingBankType::Cooperative
            } else if roll < 0.90 {
                BankingBankType::Investment
            } else {
                BankingBankType::Universal
            }
        };

        let bank_id = format!("BANK-{}-{:03}", prefix, bank_idx);
        let bank_name = if is_first {
            format!("State Bank of {name}")
        } else if bank_type == BankingBankType::Cooperative {
            // Phase 90: Cooperative banks get a distinct naming pattern.
            format!("Cooperative Bank of {name} {bank_idx}")
        } else {
            // Phase 51: Use cultural surnames for regional bank names.
            let bank_surnames = [
                "Kowalski",
                "Müller",
                "Rossi",
                "Andersen",
                "Dubois",
                "Petrović",
                "Schmidt",
                "Garcia",
                "Hussein",
                "Novak",
                "Fischer",
                "Marković",
                "Weber",
                "López",
                "Khalil",
                "Sokolov",
                "Becker",
                "Fernández",
            ];
            let surname = bank_surnames.choose(&mut rng).copied().unwrap_or("Smith");
            format!("{surname} Bank of {name}")
        };

        // Smaller balance sheets for regional banks
        let size_factor = if is_first {
            1.0
        } else {
            0.3 + rng.gen::<f64>() * 0.4
        };
        let total_deposits = match bank_type {
            BankingBankType::Commercial
            | BankingBankType::Universal
            | BankingBankType::Cooperative => {
                treasury.citizen_savings * 0.5 * size_factor / num_banks as f64
            }
            _ => 0.0,
        };
        // Phase 88: Scale reserves to at least 2% of GDP (scaled by bank size)
        // to ensure healthy initial liquidity for Working Capital Loans.
        let reserves = (total_deposits * central_bank.reserve_requirement_ratio)
            .max(treasury.gdp * 0.02 * size_factor / num_banks as f64);

        // Phase 94: Derive tier_1_capital strictly from the balance-sheet
        // identity A = L + E. At genesis (no loans, no securities):
        //   assets = reserves
        //   liabilities = total_deposits
        //   tier_1_capital = reserves - total_deposits
        // This may be negative (reserves << deposits since reserve_ratio ~10%).
        // If so, inject treasury equity to reach the regulatory minimum.
        // Double-entry: treasury.liquid_reserves -= injection (state pays),
        //   bank.reserves_at_central_bank += injection (cash received),
        //   bank.tier_1_capital += injection (equity increases).
        // This preserves A = L + E at every step.
        const TARGET_TIER_1_RATIO: f64 = 0.12; // 1.5x the 8% KNF minimum
        let avg_wage = treasury.gdp / (treasury.population as f64).max(1.0) * 800.0;
        let workforce = treasury.population as f64 * 0.65; // ~65% activity rate
        let estimated_total_loan_exposure = workforce * avg_wage * 4.0 * 0.5;
        let estimated_loan_exposure =
            estimated_total_loan_exposure * size_factor / num_banks as f64;
        let estimated_total_assets = total_deposits + reserves + estimated_loan_exposure;
        let min_tier_1 = estimated_total_assets * TARGET_TIER_1_RATIO;

        // Step 1: Derive equity from A - L (may be negative).
        let mut tier_1_capital = reserves - total_deposits;
        let mut reserves = reserves;

        // Step 2: If below regulatory minimum, inject from treasury.
        // Cap at 30% of treasury liquid_reserves per bank to prevent
        // state bankruptcy. The safety net in issue_working_capital_loans
        // (corporate.rs) provides a secondary top-up after loans are issued.
        if tier_1_capital < min_tier_1 {
            let needed = min_tier_1 - tier_1_capital;
            let available = treasury.liquid_reserves * 0.30 / num_banks as f64;
            let injection = needed.min(available);
            if injection > 0.0 {
                tier_1_capital += injection;
                reserves += injection;
                treasury.liquid_reserves -= injection;
            }
        }

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

        // Phase 77/80: Scale bank FTE by deposit volume — banks managing billions
        // need thousands of clerks, not 100. Each 500 units of deposits (relative
        // to average_wage) require ~1 clerk.
        // Phase 80 FIX: Previous minimum of 50 for non-first banks was far too
        // small. A nation of 10M should have banks with 500+ clerks, not 50.
        // Scale minimum by population: max(50, population/100_000) for non-first,
        // max(500, population/20_000) for the first (state) bank.
        let deposits_per_clerk = base_wage * 500.0;
        let pop = treasury.population.max(1) as u32;
        let bank_fte = if is_first {
            ((total_deposits / deposits_per_clerk).round() as u32)
                .max(pop / 20_000)
                .max(500)
                .min(5000)
        } else {
            ((total_deposits / deposits_per_clerk).round() as u32)
                .max(pop / 100_000)
                .max(50)
                .min(2000)
        };
        let bank_wage = base_wage * 1.2; // Banks pay above-average wages
        let operating_cash = (tier_1_capital * 0.1).max(0.0); // 10% of tier_1 for payroll

        let mut company = Company::new(
            bank_id.clone(),
            bank_name,
            EntitySector::Banking,
            LegalForm::JointStockCompany(crate::entities::JointStockData::default()),
            tier_1_capital.max(0.0),
            operating_cash,
            bank_fte,
        );
        // R3.1: Cooperative banks (SKOK) use a lower margin (0.01) for member
        // lending, while commercial banks use 0.02. This is a structural
        // pricing difference, not a magic number — it reflects the cooperative
        // business model where profits are reinvested as lower member rates.
        let is_cooperative_bank = bank_type == BankingBankType::Cooperative;
        company.bank_type = Some(bank_type);
        company.balance_sheet = Some(balance_sheet);
        company.loan_margin = Some(if is_cooperative_bank { 0.01 } else { 0.02 });
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
        // Phase 80: Use 80% of target (up from 60%) — established banks start
        // well-staffed. Minimum 2 to avoid zero-FTE edge cases.
        let initial_fte = ((bank_fte as f64 * 0.8).round() as u32).max(2);
        company.fulfilled_fte = initial_fte;
        company.prev_fulfilled_fte = initial_fte;
        let payroll_grant = initial_fte as f64 * bank_wage * 3.0;
        company.available_cash += payroll_grant;

        banks.push(company);
    }

    banks
}

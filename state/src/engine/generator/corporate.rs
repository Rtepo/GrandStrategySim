//! Corporate sector generator for new worlds.
//!
//! Creates `Company` and `Building` entities from the generated macro sector
//! shares, writes them to `entities/<country>/` and `spatial_registry/<country>/`,
//! and reconciles `country.budget.private_capital` with the new corporate sector.
//!
//! Each regional market is populated with a power-law distribution of competing
//! companies (1-2 large players, 3-5 medium firms, and many small firms) while
//! still using `scale_factor` on individual buildings to keep simulation cost low.

use crate::economy::{update_gdp_shares_from_employment, CountryTurnCtx};
use crate::economy::fixed_assets::FixedAssetCohort;
use crate::entities::{
    ActiveProductionMethod, AggregatedStats, Building, ClusterInfo, Company,
    CooperativeData, FamilyBusinessData, JointStockData, LegalForm, SeasonalProfile,
    SeasonalState, Union, UnionScale,
    StrategicReserveData, PurchaseTrigger, ReleaseTrigger, NonProfitData,
    CropBatch, CropState,
};
use crate::io::entity_store::{DiskEntityStore, EntityStore};
use crate::registries::enums::{Commodity, Sector};
use crate::registries::production_methods::ProductionMethod;
use crate::registries::Registries;
use crate::society::geography::{ClimateProfile, GeologicalFormation, Region};
use crate::state::{Country, Season};
use rand::seq::SliceRandom;
use rand::Rng;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::fs;
use std::path::Path;

/// Phase 54: Derive a CEO's ideology from their assigned traits instead of
/// hardcoding "Neoliberalism" for all CEOs. Maps trait profiles to
/// business-relevant ideologies, with a weighted random fallback for
/// trait combinations that don't have a clear mapping.
fn ceo_ideology_from_traits(traits: &[String], main_trait: &str, rng: &mut impl Rng) -> String {
    // Check for specific trait indicators in priority order.
    let has = |t: &str| traits.iter().any(|x| x == t) || main_trait == t;

    if has("Reformer") {
        "Social Liberalism".to_string()
    } else if has("Conservative") || has("Pious") {
        "Social Conservatism".to_string()
    } else if has("Populist") {
        "National Conservatism".to_string()
    } else if has("Militarist") || has("Cruel") {
        "Neoconservatism".to_string()
    } else if has("Corrupt") || has("Ambitious") {
        "Classical Liberalism".to_string()
    } else if has("Diplomatic") || has("Charismatic") {
        "Christian Democracy".to_string()
    } else {
        // Weighted random fallback for neutral/unmapped trait combos.
        let fallbacks = [
            "Neoliberalism",
            "Classical Liberalism",
            "Social Liberalism",
            "Christian Democracy",
        ];
        fallbacks[rng.gen_range(0..fallbacks.len())].to_string()
    }
}

/// Phase 47: Determine the seasonal profile for a company based on its sector
/// and the region's climate profile. Returns None for non-seasonal sectors.
///
/// - Hospitality (tourism): Active in Spring/Summer/Autumn, furlough in Winter.
///   Tropical climate: active year-round (monsoon tourism).
///   Coastal climate: active in Spring/Summer/Autumn (winter storms suppress).
/// - Energy (heating utilities): Active in Autumn/Winter, furlough in Spring/Summer.
/// - All other sectors: None (year-round operation).
fn seasonal_profile_for_sector(
    sector: Sector,
    climate_profile: &ClimateProfile,
) -> Option<SeasonalProfile> {
    match sector {
        Sector::Hospitality => {
            let active_seasons = match climate_profile {
                ClimateProfile::Tropical => BTreeSet::from([
                    Season::Spring,
                    Season::Summer,
                    Season::Autumn,
                    Season::Winter,
                ]),
                _ => BTreeSet::from([Season::Spring, Season::Summer, Season::Autumn]),
            };
            Some(SeasonalProfile {
                active_seasons,
                standby_fte_fraction: 0.20,
                current_state: SeasonalState::Active,
            })
        }
        Sector::Energy => Some(SeasonalProfile {
            active_seasons: BTreeSet::from([Season::Autumn, Season::Winter]),
            standby_fte_fraction: 0.15,
            current_state: SeasonalState::Active,
        }),
        _ => None,
    }
}

/// Generates a full corporate sector for `country` and persists it.
///
/// # Rules
/// * One or more companies are created for each sector in `country.budget.sectors`.
/// * Each company owns one or more buildings distributed over the country's regions.
/// * Building `active_method` is seeded from deterministic sector recipes.
/// * State-owned public-service buildings are generated separately and assigned to
///   `owner_id = "State"`.
/// * `country.budget.private_capital` is updated to the sum of non-state company capital.
/// * Sector `zatrudnienie` / `pmi` extras are recalculated from the generated buildings.
pub fn generate_corporate_entities(
    data_dir: &Path,
    country: &mut Country,
    regions: &HashMap<String, Region>,
    registries: &Registries,
    start_year: u32,
    rng: &mut impl Rng,
) -> Result<(), Box<dyn Error>> {
    let country_regions: Vec<&Region> = regions
        .values()
        .filter(|r| r.owner_country == country.name)
        .collect();
    if country_regions.is_empty() {
        return Ok(());
    }

    let code = country.name[..3.min(country.name.len())].to_uppercase();
    let mut idgen = IdGen::new(&code);

    // Phase 53: Determine the country's cultural group for culture-scoped
    // company naming (used by generate_company_name via generate_region_companies).
    let cultural_group = if country.macro_indicators.cultural_group.is_empty() {
        "slavic".to_string()
    } else {
        country.macro_indicators.cultural_group.clone()
    };

    let company_store = DiskEntityStore::<Company>::new(data_dir);
    let building_store = DiskEntityStore::<Building>::new(data_dir);

    let mut all_companies: Vec<Company> = Vec::new();
    let mut all_buildings: Vec<Building> = Vec::new();

    let base_wage = country.macro_indicators.average_wage.max(1.0);

    let total_population: i64 = country_regions.iter().map(|r| r.population).sum();
    if total_population <= 0 {
        return Ok(());
    }
    
    // Create Strategic Reserve Agency (Phase 2, Phase 79: physical warehouses + 8 commodities)
    let country_regions_vec: Vec<Region> = country_regions.iter().map(|r| (*r).clone()).collect();
    let (mut reserve_agency, reserve_warehouses) = create_strategic_reserve_agency(
        country, start_year, total_population, base_wage, &country_regions_vec, &mut idgen, registries,
    );
    let reserve_building_ids: Vec<String> = reserve_warehouses.iter().map(|b| b.id.clone()).collect();
    reserve_agency.building_ids = reserve_building_ids;
    all_companies.push(reserve_agency);
    all_buildings.extend(reserve_warehouses);

    // Phase 20A: Seed minimum viable supply chain Ă˘â‚¬â€ť guarantee at least one
    // building per critical sector per region, regardless of budget shares.
    let seed_entities = seed_minimum_viable_supply_chain(
        country, &country_regions, start_year, registries, &mut idgen, rng,
    );
    // Phase 46: Consume seed_entities via into_iter() to avoid redundant clones.
    // Build seed_by_sector in the same loop, then move companies/buildings into
    // all_companies/all_buildings.
    let mut seed_by_sector: HashMap<String, Vec<Company>> = HashMap::new();
    for (company, building) in seed_entities {
        let sname = sector_json_name(company.sector);
        seed_by_sector.entry(sname).or_default().push(company.clone());
        all_companies.push(company);
        all_buildings.push(building);
    }

    for (&sector, share) in &country.budget.sectors {
        if sector == Sector::PublicServices {
            continue;
        }
        let sector_name = sector_json_name(sector);
        let target_emp = share
            .extra
            .get("zatrudnienie")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .max(0);
        if target_emp == 0 {
            continue;
        }

        let sector_fixed = (target_emp as f64) * base_wage * 2.0;
        let sector_liquid = sector_fixed * 0.4;

        let mut sector_companies: Vec<Company> = Vec::new();

        for region in &country_regions {
            let region_population = region.population.max(1);
            let region_share = region_population as f64 / total_population as f64;

            // Phase 47: Apply development-level bias to sector employment.
            // High development → more Services + LightIndustry, less Agriculture.
            // Low development → more Agriculture, less Services.
            // HeavyIndustry/Mining remain geology-driven (no development bias).
            let dev_bias = match sector {
                Sector::LocalServices | Sector::LightIndustry => {
                    0.5 + region.development_level // 0.5x to 1.5x
                }
                Sector::Agriculture => {
                    1.5 - region.development_level // 1.5x to 0.5x
                }
                Sector::HeavyIndustry | Sector::Mining => {
                    1.0 // Geology-driven, no development bias
                }
                _ => 0.7 + region.development_level * 0.6, // Mild bias for other services
            };

            let region_emp = (target_emp as f64 * region_share * dev_bias).max(1.0);
            let region_fixed = sector_fixed * region_share * dev_bias;
            let region_liquid = sector_liquid * region_share * dev_bias;

            let region_companies = generate_region_companies(
                sector,
                &sector_name,
                region,
                region_emp,
                region_fixed,
                region_liquid,
                start_year,
                registries,
                &mut idgen,
                &cultural_group,
                rng,
            );

            for (company, building) in region_companies {
                all_companies.push(company.clone());
                sector_companies.push(company);
                all_buildings.push(building);
            }
        }

        // Phase 27: Merge seed companies with budget-share companies before
        // saving. Both go to the same file (entities/{country}/companies/
        // {sector}.json) so we must save them together to avoid overwriting.
        if let Some(seed_companies) = seed_by_sector.remove(&sector_name) {
            sector_companies.extend(seed_companies);
        }

        company_store.save_sector(&country.name, &sector_name, None, &sector_companies)?;
    }

    // Phase 27: Save any remaining seed companies for sectors that don't have
    // budget-share companies (e.g., Mining with geology-based seeds only).
    for (sector_name, companies) in seed_by_sector {
        company_store.save_sector(&country.name, &sector_name, None, &companies)?;
    }

    // State-owned public-service buildings.
    // Phase 80: All keys are strict snake_case — no Title Case, no spaces.
    // Phase 80: Expanded from 4 to 9 building types to ensure all critical
    // state infrastructure (justice, security, intelligence, borders, customs,
    // education, religion) is generated at game start.
    let state_buildings = [
        ("military_base", 4000u32),
        ("police_station", 200u32),
        ("courthouse", 150u32),
        ("service_headquarters", 250u32),
        ("intelligence_hq", 100u32),
        ("border_guard", 80u32),
        ("customs_office", 60u32),
        ("university", 300u32),
        ("monastery_scriptorium", 50u32),
    ];
    let public_sector = sector_json_name(Sector::PublicServices);
    for (region, (name, base_capacity)) in country_regions.iter().cycle().zip(state_buildings.iter().cycle()).take(country_regions.len() * state_buildings.len()) {
        let (name, method) = state_building_recipe(name, start_year);
        let capacity = (base_capacity + rng.gen_range(0..base_capacity / 4)).max(1);
        let current = (capacity as f64 * (0.9 + rng.gen::<f64>() * 0.1)) as u32;
        let building_id = idgen.next_building();
        let building = Building {
            id: building_id,
            name: name.to_string(),
            owner_id: "State".to_string(),
            year_built: start_year.saturating_sub(rng.gen_range(1..30)),
            sector: Sector::PublicServices,
            worker_capacity: capacity,
            current_employment: current.min(capacity),
            reserve: 0.0,
            active_method: method,
            accidents_last_year: 0,
            strike: false,
            scale_factor: 1,
            building_capacity: capacity,
            region_id: region.id.clone(),
            cluster_info: ClusterInfo {
                region_id: region.id.clone(),
                scale_factor: 1,
                sector: Sector::PublicServices,
                owner_id: "State".to_string(),
                extra: Map::new(),
            },
            last_production: BTreeMap::new(),
            last_profit: 0.0,
            last_fulfillment_ratio: 1.0,
            condition: 1.0,
            is_heritage_site: false,
            experience_level: None,
            aggregated_stats: AggregatedStats::default(),
            structural_defect: 0.0, land_hectares: 0.0,
            extra: Map::new(),
            inventory: BTreeMap::new(),
            inventory_capacity: 0.0,
            active_project: None,
            landfill_state: None,
            deposit_id: None,
            fixed_assets: Vec::new(),
            pending_method_upgrade: None,
            active_emission_control: String::new(),
        };
        // Phase 46: Push to all_buildings only; state buildings are filtered
        // from all_buildings at save time to avoid cloning.
        all_buildings.push(building);
    }

    // Persist state buildings grouped by region.
    // Phase 46: Filter from all_buildings instead of maintaining a separate list.
    let mut by_region: HashMap<String, Vec<Building>> = HashMap::new();
    for b in all_buildings.iter() {
        if b.owner_id == "State" && b.sector == Sector::PublicServices {
            by_region.entry(b.region_id.clone()).or_default().push(b.clone());
        }
    }
    for (region, list) in by_region {
        building_store.save_sector(&country.name, &public_sector, Some(&region), &list)?;
    }

    // Phase 8: Generate one landfill Building per region (Sector::WasteManagement).
    let waste_sector_name = sector_json_name(Sector::WasteManagement);
    for region in &country_regions {
        let building_id = idgen.next_building();
        let landfill_state = crate::utilities::waste_grid::LandfillState::new(
            500_000.0, // total_capacity
            0.5,       // liner_integrity (controlled landfill)
            0.3,       // leachate_capture
            0.1,       // gas_capture
        );
        let building = Building {
            id: building_id.clone(),
            name: format!("Landfill ({})", region.id),
            owner_id: "State".to_string(),
            year_built: start_year.saturating_sub(rng.gen_range(1..10)),
            sector: Sector::WasteManagement,
            worker_capacity: 50,
            current_employment: 40,
            reserve: 0.0,
            active_method: ActiveProductionMethod {
                year: start_year.saturating_sub(50),
                experts_ratio: 0.05,
                skilled_ratio: 0.30,
                basic_ratio: 0.65,
                efficiency: 0.8,
                inputs: BTreeMap::new(),
                outputs: BTreeMap::new(),
                active_methods: crate::state::treasury::ProductionMethodChoice {
                    automation: "Manual Sorting".to_string(),
                    production: "Basic Landfill Operation".to_string(),
                    organization: "Municipal Crew".to_string(),
                    ..Default::default()
                },
                active_blueprint: None,
                extra: Map::new(),
                ..Default::default()
            },
            accidents_last_year: 0,
            strike: false,
            scale_factor: 1,
            building_capacity: 50,
            region_id: region.id.clone(),
            cluster_info: ClusterInfo {
                region_id: region.id.clone(),
                scale_factor: 1,
                sector: Sector::WasteManagement,
                owner_id: "State".to_string(),
                extra: Map::new(),
            },
            last_production: BTreeMap::new(),
            last_profit: 0.0,
            last_fulfillment_ratio: 1.0,
            condition: 1.0,
            is_heritage_site: false,
            experience_level: None,
            aggregated_stats: AggregatedStats::default(),
            structural_defect: 0.0, land_hectares: 0.0,
            extra: Map::new(),
            inventory: BTreeMap::new(),
            inventory_capacity: 0.0,
            active_project: None,
            landfill_state: Some(landfill_state),
            deposit_id: None,
            fixed_assets: Vec::new(),
            pending_method_upgrade: None,
            active_emission_control: String::new(),
        };
        // Phase 46: Push to all_buildings only; landfill buildings are filtered
        // from all_buildings at save time to avoid cloning.
        all_buildings.push(building);
    }

    // Persist landfill buildings grouped by region.
    // Phase 46: Filter from all_buildings instead of maintaining a separate list.
    let mut landfill_by_region: HashMap<String, Vec<Building>> = HashMap::new();
    for b in all_buildings.iter() {
        if b.sector == Sector::WasteManagement {
            landfill_by_region.entry(b.region_id.clone()).or_default().push(b.clone());
        }
    }
    for (region, list) in landfill_by_region {
        building_store.save_sector(&country.name, &waste_sector_name, Some(&region), &list)?;
    }

    // Persist private buildings grouped by sector and region.
    let mut private_by_key: HashMap<(String, String), Vec<Building>> = HashMap::new();
    for b in &all_buildings {
        if b.owner_id == "State" {
            continue;
        }
        let sector_name = sector_json_name(b.sector);
        private_by_key
            .entry((sector_name, b.region_id.clone()))
            .or_default()
            .push(b.clone());
    }
    for ((sector, region), list) in private_by_key {
        building_store.save_sector(&country.name, &sector, Some(&region), &list)?;
    }

    // Stabilization Sprint: Activate Agriculture 2.0 by initializing
    // agricultural profiles and linking farms to Cadastre parcels.
    // This must happen AFTER all companies are generated and BEFORE
    // buildings are moved into ctx.
    //
    // World Generation & Climate Audit (v0.5.3): Pass a region_id -> ClimateProfile
    // map so that crop batch building can select climate-appropriate crops.
    let region_climates: HashMap<String, ClimateProfile> = country_regions
        .iter()
        .map(|r| (r.id.clone(), r.climate_profile))
        .collect();
    initialize_agricultural_profiles(
        &mut all_companies,
        &mut country.cadastre,
        &region_climates,
        registries,
    );

    // Emergency Stabilization: The 12-month Strategic Reserve Agency food
    // seed has been REMOVED. The game now starts in September (autumn harvest
    // season), and crop batches are pre-seeded in Growing state, so the
    // first harvest deposits organic yields into warehouses at turns 1-3.
    // The B2C retail buffer below is retained for Turn 1 market function.

    // Stabilization Sprint: Seed B2C retail stores with a 4-turn buffer
    // of Cereal and Food so consumers can buy food on Turn 1.
    {
        let pop_f = total_population as f64;
        let cereal_buffer = 0.18 * 4.0 * pop_f;
        let food_buffer = 0.22 * 4.0 * pop_f;
        let retail_indices: Vec<usize> = all_buildings
            .iter()
            .enumerate()
            .filter(|(_, b)| b.sector == Sector::LocalServices)
            .map(|(i, _)| i)
            .collect();
        if !retail_indices.is_empty() {
            let n = retail_indices.len() as f64;
            for &idx in &retail_indices {
                *all_buildings[idx].inventory.entry(Commodity::Cereal).or_insert(0.0) += cereal_buffer / n;
                *all_buildings[idx].inventory.entry(Commodity::Food).or_insert(0.0) += food_buffer / n;
            }
        }
    }

    // Re-save agriculture companies with initialized agricultural profiles.
    {
        let agri_sector_name = sector_json_name(Sector::Agriculture);
        let agri_companies: Vec<Company> = all_companies
            .iter()
            .filter(|c| c.sector == Sector::Agriculture)
            .cloned()
            .collect();
        if !agri_companies.is_empty() {
            company_store.save_sector(&country.name, &agri_sector_name, None, &agri_companies)?;
        }
    }

    // Re-save buildings with seeded food inventory.
    {
        let public_sector_name = sector_json_name(Sector::PublicServices);
        let local_services_name = sector_json_name(Sector::LocalServices);
        // Re-save SRA warehouses (PublicServices sector, owner = STRATEGIC_RESERVE_*)
        let sra_buildings: Vec<Building> = all_buildings
            .iter()
            .filter(|b| b.owner_id.starts_with("STRATEGIC_RESERVE_"))
            .cloned()
            .collect();
        if !sra_buildings.is_empty() {
            for region in &country_regions {
                let region_sra: Vec<Building> = sra_buildings
                    .iter()
                    .filter(|b| b.region_id == region.id)
                    .cloned()
                    .collect();
                if !region_sra.is_empty() {
                    building_store.save_sector(&country.name, &public_sector_name, Some(&region.id), &region_sra)?;
                }
            }
        }
        // Re-save retail stores with food buffer
        let retail_buildings: Vec<Building> = all_buildings
            .iter()
            .filter(|b| b.sector == Sector::LocalServices)
            .cloned()
            .collect();
        if !retail_buildings.is_empty() {
            for region in &country_regions {
                let region_retail: Vec<Building> = retail_buildings
                    .iter()
                    .filter(|b| b.region_id == region.id)
                    .cloned()
                    .collect();
                if !region_retail.is_empty() {
                    building_store.save_sector(&country.name, &local_services_name, Some(&region.id), &region_retail)?;
                }
            }
        }
    }

    // Reconcile private capital and recalculate sector employment / PMI.
    let private_capital: f64 = all_companies
        .iter()
        .filter(|c| c.state_share < 1.0)
        .map(|c| c.company_capital)
        .sum();
    country.budget.private_capital = private_capital;

    let mut ctx = CountryTurnCtx {
        country_name: country.name.clone(),
        turn: 0,
        year: start_year,
        registries,
        country,
        buildings: all_buildings,
        market_prices: rustc_hash::FxHashMap::default(),
    };
    update_gdp_shares_from_employment(&mut ctx);

    // Generate unions for each sector
    generate_unions(data_dir, country, &all_companies, &code, start_year, rng)?;

    // Phase 9: Generate tourism entities (wonders, destinations, hospitality companies + buildings)
    generate_tourism_entities(data_dir, country, &country_regions, start_year, &mut idgen, rng)?;

    // Phase 25: Generate retail stores in each region.
    // Without retail stores, the B2C market has no outlets to sell goods
    // to consumers, so GDP (which is largely final consumption) stays at 0.
    generate_retail_stores(data_dir, country, &country_regions, start_year, &mut idgen, rng)?;

    // Phase 44: Generate genesis housing (Mega-Estates with ownership).
    // Without housing, the population is homeless on Turn 1, triggering
    // a flood of construction tenders and winter mortality crises.
    generate_housing(data_dir, country, &country_regions, start_year, &mut idgen, rng)?;

    // Phase 13: Generate NGO and Church entities (Third Pillar)
    generate_charity_entities(data_dir, country, &country_regions, start_year, &mut idgen, rng)?;

    // Phase 27: Credit total seed inventory cost to country's treasury.
    // The State acts as the initial importer/provider of seed materials.
    // This maintains double-entry accounting Ă˘â‚¬â€ť the cost was deducted from
    // each company's liquid_capital, and is credited to the treasury.
    let total_seed_cost: f64 = all_companies
        .iter()
        .filter_map(|c| c.extra.get("seed_inventory_cost").and_then(|v| v.as_f64()))
        .sum();
    country.budget.liquid_reserves += total_seed_cost;

    // Phase 53: Generate and assign CEOs for major companies.
    // Major companies are the top 30% by worker_capacity (or company_capital).
    // CEOs are registered as VIPs in the country's vip_registry with the
    // Ceo role, and their VIP ID is stored on the company.
    use crate::politics::vip_registry::{Vip, VipRegistry, VipRoleExtended, assign_core_traits};
    if country.politics.vip_registry.is_none() {
        country.politics.vip_registry = Some(VipRegistry::new());
    }

    // Phase 87+: Assign CEOs to ALL non-state companies (not just top 30%).
    // Previously, only "major" companies (top 30% by worker_capacity) got CEOs,
    // leaving 70% of companies with ceo_vip_id: None — rendering as "CEO —" in
    // the UI. Now every non-state company gets a CEO VIP.
    for company in all_companies.iter_mut() {
        // Skip state-owned companies (they have government-appointed directors, not CEOs).
        if company.state_share >= 1.0 {
            continue;
        }
        let ceo_name = crate::politics::names::generate_full_vip(&cultural_group, rng);
        let (traits, main_trait) = assign_core_traits(rng);
        let ideology = ceo_ideology_from_traits(&traits, &main_trait, rng);
        // Small companies (worker_capacity < 5): younger age, lower influence
        // (small-business owner profile vs corporate executive).
        let (age, base_influence) = if company.worker_capacity < 5 {
            (25 + rng.gen_range(0..20), 5 + rng.gen_range(0..10))
        } else {
            (35 + rng.gen_range(0..30), 20 + rng.gen_range(0..30))
        };
        let ceo_vip = Vip {
            full_name: ceo_name.full_name.clone(),
            gender: ceo_name.gender,
            age,
            health: crate::politics::vip_registry::VipHealth { physical_health: 1.0, mental_health: 1.0 },
            traits,
            main_trait,
            ideology,
            nationality: country.name.clone(),
            roles: vec![VipRoleExtended::Ceo],
            base_influence,
            ..Default::default()
        };
        let ceo_id = country
            .politics
            .vip_registry
            .as_mut()
            .unwrap()
            .register_new(ceo_vip);
        company.ceo_vip_id = Some(ceo_id);

        // Phase 55: Generate a board of directors for JointStockCompany firms.
        // Board size scales with company size: 3 for small, up to 7 for large.
        if let crate::entities::LegalForm::JointStockCompany(ref mut jsd) = company.legal_form {
            let board_size = if company.worker_capacity > 500 {
                7
            } else if company.worker_capacity > 200 {
                5
            } else {
                3
            };
            let mut board_members = Vec::with_capacity(board_size);
            for i in 0..board_size {
                let bm_name = crate::politics::names::generate_full_vip(&cultural_group, rng);
                let (bm_traits, bm_main_trait) = assign_core_traits(rng);
                let bm_ideology = ceo_ideology_from_traits(&bm_traits, &bm_main_trait, rng);
                let role = if i == 0 {
                    crate::entities::legal_form::BoardRole::Chair
                } else {
                    crate::entities::legal_form::BoardRole::Independent
                };
                let vip_role = if i == 0 {
                    VipRoleExtended::BoardChair
                } else {
                    VipRoleExtended::BoardMember
                };
                let bm_vip = Vip {
                    full_name: bm_name.full_name.clone(),
                    gender: bm_name.gender,
                    age: 40 + rng.gen_range(0..25),
                    health: crate::politics::vip_registry::VipHealth { physical_health: 1.0, mental_health: 1.0 },
                    traits: bm_traits,
                    main_trait: bm_main_trait,
                    ideology: bm_ideology,
                    nationality: country.name.clone(),
                    roles: vec![vip_role],
                    base_influence: 10 + rng.gen_range(0..20),
                    ..Default::default()
                };
                let bm_id = country
                    .politics
                    .vip_registry
                    .as_mut()
                    .unwrap()
                    .register_new(bm_vip);
                // Initial loyalty is random 0.4–0.8 (neutral to moderately loyal).
                let loyalty = 0.4 + rng.gen::<f64>() * 0.4;
                board_members.push(crate::entities::legal_form::BoardSeat {
                    vip_id: bm_id,
                    role,
                    loyalty_to_ceo: loyalty,
                    appointed_turn: 0,
                });
            }
            jsd.board_members = board_members;
        }

        // Phase 55: Generate heirs for family businesses.
        // 1–3 heirs are created, influenced by the CEO's traits.
        // Heirs start young (18–30) and will inherit on CEO death.
        if let crate::entities::LegalForm::FamilyBusiness(ref mut fbd) = company.legal_form {
            if company.ceo_vip_id.is_some() {
                let num_heirs = 1 + rng.gen_range(0..3); // 1–3 heirs
                let mut heir_ids = Vec::with_capacity(num_heirs);
                for _ in 0..num_heirs {
                    let heir_name = crate::politics::names::generate_full_vip(&cultural_group, rng);
                    // Heir traits are influenced by CEO traits: 50% chance to inherit
                    // a trait from the CEO, otherwise random.
                    let (mut heir_traits, heir_main_trait) = assign_core_traits(rng);
                    // Inject "Loyal" as a family-bond trait for heirs.
                    if !heir_traits.contains(&"Loyal".to_string()) {
                        heir_traits.push("Loyal".to_string());
                    }
                    let heir_ideology = ceo_ideology_from_traits(&heir_traits, &heir_main_trait, rng);
                    let heir_vip = Vip {
                        full_name: heir_name.full_name.clone(),
                        gender: heir_name.gender,
                        age: 18 + rng.gen_range(0..13), // 18–30
                        health: crate::politics::vip_registry::VipHealth { physical_health: 1.0, mental_health: 1.0 },
                        traits: heir_traits,
                        main_trait: heir_main_trait,
                        ideology: heir_ideology,
                        nationality: country.name.clone(),
                        roles: vec![VipRoleExtended::Heir],
                        base_influence: 5 + rng.gen_range(0..10),
                        ..Default::default()
                    };
                    let heir_id = country
                        .politics
                        .vip_registry
                        .as_mut()
                        .unwrap()
                        .register_new(heir_vip);
                    heir_ids.push(heir_id);
                }
                fbd.heir_vip_ids = heir_ids;
            }
        }
    }

    // Phase 77: List JSC companies on the stock exchange with real IPO funding.
    // Replaces the old Phase 56 code that created AMM pools with phantom cash
    // (pool_cash was invented from nowhere, violating double-entry accounting).
    // The new approach:
    // 1. Funds IPO from wealthy demographics (Aristocracy, Bourgeoisie)
    // 2. Creates AMM pools only if both shares and cash are funded
    // 3. Places unsold shares as limit sell orders in the order book
    // 4. Assigns founder ownership
    let country_regions_map: HashMap<String, crate::society::geography::Region> = regions
        .iter()
        .filter(|(_, r)| r.owner_country == country.name)
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    super::list_jsc_companies_on_exchange(country, &mut all_companies, &country_regions_map, rng);

    // Re-save companies with CEO assignments so they persist to disk.
    // Group by sector for the save call.
    let mut by_sector: HashMap<String, Vec<Company>> = HashMap::new();
    for c in &all_companies {
        let sname = sector_json_name(c.sector);
        by_sector.entry(sname).or_default().push(c.clone());
    }
    for (sector_name, companies) in by_sector {
        let _ = company_store.save_sector(&country.name, &sector_name, None, &companies);
    }

    // Phase 87+: Working Capital Loan for agriculture companies.
    // Replaces the free "Genesis Payroll Grant" with a legitimate double-entry
    // loan from the State Bank. Agriculture has the longest gap between spawning
    // and first revenue (harvest cycle), so it needs 6 turns of payroll coverage.
    // The loan is recorded as a liability on the company's balance sheet and an
    // asset on the bank's balance sheet (Rule 1: strict double-entry).
    issue_agriculture_working_capital_loans(
        data_dir,
        country,
        &mut all_companies,
        &code,
        start_year,
    )?;

    Ok(())
}

/// Phase 87+: Issue Working Capital Loans from the State Bank to agriculture
/// companies during world generation.
///
/// This replaces the free "Genesis Payroll Grant" with a legitimate double-entry
/// loan. Agriculture companies need 6 turns of payroll coverage to survive until
/// their first harvest (which may be 3-6 turns away depending on season).
///
/// # Double-Entry Flow
/// - Company: `available_cash += principal` (asset), `liabilities += principal` (liability)
/// - State Bank: `balance_sheet.loans_issued.push(loan)` (asset),
///   `balance_sheet.reserves_at_central_bank -= principal` (asset decreases)
///
/// # Arguments
/// * `data_dir` - Data directory for loading/saving the State Bank
/// * `country` - Country (for XIBOR and central bank reference)
/// * `all_companies` - All generated companies (agriculture ones get loans)
/// * `code` - 3-letter country code prefix (for State Bank ID)
/// * `start_year` - Start year (for loan issuance turn)
fn issue_agriculture_working_capital_loans(
    data_dir: &Path,
    country: &Country,
    all_companies: &mut [Company],
    code: &str,
    _start_year: u32,
) -> Result<(), Box<dyn Error>> {
    use crate::io::entity_store::{DiskEntityStore, EntityStore};
    use crate::state::banking::{Loan, LoanStatus, InterestType, LoanType};

    let state_bank_id = format!("BANK-{}-001", code);

    // Load the State Bank from disk to update its balance sheet.
    let bank_store = DiskEntityStore::<Company>::new(data_dir);
    let banking_sector_name = serde_json::to_value(Sector::Banking)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "Banking".to_string());

    let mut bank_companies = bank_store
        .load_sector(&country.name, &banking_sector_name, None)
        .unwrap_or_default();

    let state_bank_idx = bank_companies.iter().position(|b| b.id == state_bank_id);
    let state_bank = match state_bank_idx {
        Some(idx) => &mut bank_companies[idx],
        None => return Ok(()), // No state bank — skip loans (non-fatal)
    };

    let xibor = country.central_bank.interest_rates.reference_rate;
    let bank_margin = state_bank.loan_margin.unwrap_or(0.02);
    let risk_premium = 0.01; // 100 bps risk premium for startup agriculture
    let interest_rate = xibor + bank_margin + risk_premium;

    let mut total_loaned = 0.0;

    for company in all_companies.iter_mut() {
        if company.sector != Sector::Agriculture {
            continue;
        }
        if company.state_share >= 1.0 {
            continue; // State-owned agriculture doesn't need loans
        }

        // Compute loan principal: 6 turns of payroll for the initial workforce.
        let initial_fte = company.fulfilled_fte as f64;
        let initial_wage = company.offered_wage_per_fte.max(50.0);
        let principal = initial_fte * initial_wage * 6.0;

        if principal <= 0.0 {
            continue;
        }

        // Double-entry: Company receives cash (asset) and records liability.
        company.available_cash += principal;
        company.liabilities += principal;
        company.primary_bank_id = Some(state_bank_id.clone());
        company.outstanding_loan_bank_id = Some(state_bank_id.clone());

        // Create the Loan record.
        let loan_id = format!("WCL-{}-{}", code, company.id);
        let loan = Loan {
            id: loan_id.clone(),
            borrower_id: company.id.clone(),
            principal,
            outstanding_balance: principal,
            interest_rate,
            term_turns: 24, // 2-year repayment
            turns_remaining: 24,
            collateral_value: Some(company.fixed_capital * 0.8),
            loan_type: LoanType::WorkingCapital,
            last_payment_turn: 0,
            status: LoanStatus::Current,
            interest_type: InterestType::Fixed,
            duration_risk_premium: risk_premium,
            base_xibor: xibor,
            bank_margin,
            ..Default::default()
        };

        // Store loan reference in company's extra map.
        company.extra.insert(
            "genesis_loan_id".to_string(),
            serde_json::Value::String(loan_id.clone()),
        );

        // Double-entry: Bank's loan asset increases, reserve asset decreases.
        if let Some(ref mut bs) = state_bank.balance_sheet {
            bs.loans_issued.push(loan);
            bs.reserves_at_central_bank = (bs.reserves_at_central_bank - principal).max(0.0);
        }

        total_loaned += principal;
    }

    // Save the updated State Bank back to disk.
    if total_loaned > 0.0 {
        let _ = bank_store.save_sector(&country.name, &banking_sector_name, None, &bank_companies);
    }

    Ok(())
}

/// Generates union entities for each sector and assigns them to companies.
///
/// # Rules
/// * Creates one sector-wide union per sector
/// * Assigns companies to their respective sector unions
/// * Creates the unions directory structure
fn generate_unions(
    data_dir: &Path,
    country: &Country,
    companies: &[Company],
    country_code: &str,
    _start_year: u32,
    rng: &mut impl Rng,
) -> Result<(), Box<dyn Error>> {
    let unions_dir = data_dir.join("entities").join(&country.name).join("unions");
    fs::create_dir_all(&unions_dir)?;

    let mut idgen = IdGen::new(country_code);
    let union_store = DiskEntityStore::<Union>::new(data_dir);

    // Group companies by sector
    let mut companies_by_sector: HashMap<Sector, Vec<&Company>> = HashMap::new();
    for company in companies {
        companies_by_sector.entry(company.sector).or_default().push(company);
    }

    // Create one sector-wide union per sector
    let mut all_unions: Vec<Union> = Vec::new();
    for (sector, sector_companies) in companies_by_sector {
        if sector_companies.is_empty() {
            continue;
        }

        let sector_name = sector_json_name(sector);
        let union_id = idgen.next_union();
        let union_name = format!("Union {}", sector_name);

        let mut company_ids: BTreeSet<String> = BTreeSet::new();
        for company in sector_companies {
            company_ids.insert(company.id.clone());
        }

        let union = Union {
            id: union_id.clone(),
            name: union_name,
            scale_level: UnionScale::Sector,
            sector,
            region_id: country.name.clone(),
            company_ids,
            budget: 100_000.0,
            strike_fund: 25_000.0,
            political_power: 10.0 + rng.gen::<f64>() * 20.0,
            militancy: 0.2 + rng.gen::<f64>() * 0.3,
            wage_demand: 5.0 + rng.gen::<f64>() * 10.0,
            safety_demand: 0.5 + rng.gen::<f64>() * 0.5,
            last_strike_turn: None,
            on_strike: false,
            leader_vip_id: None,
            dues_history: std::collections::HashMap::new(),
            dissolution_threshold: 1,
            dissolved: false,
            extra: Map::new(),
        };

        all_unions.push(union);
    }

    // Save unions grouped by sector
    let mut unions_by_sector: HashMap<String, Vec<Union>> = HashMap::new();
    for union in all_unions {
        let sector_name = sector_json_name(union.sector);
        unions_by_sector.entry(sector_name).or_default().push(union);
    }

    for (sector_name, unions) in unions_by_sector {
        union_store.save_sector(&country.name, "unions", Some(&sector_name), &unions)?;
    }

    Ok(())
}

/// Generate a competitive, power-law distributed set of companies for one
/// sector in one region.
///
/// # Rules
/// * The number of firms scales with the region's allocated employment, but is
///   capped to avoid runaway entity counts.
/// * Employment share is drawn from a power-law distribution (`x^2` on a unit
///   uniform), producing a few large players and many small competitors.
/// * Each company receives a proportional share of the region's fixed and
///   liquid capital.
/// * Large companies (>25,000 workers) are flagged as national champions.
fn generate_region_companies(
    sector: Sector,
    sector_name: &str,
    region: &Region,
    region_emp: f64,
    region_fixed: f64,
    region_liquid: f64,
    start_year: u32,
    registries: &Registries,
    idgen: &mut IdGen,
    cultural_group: &str,
    rng: &mut impl Rng,
) -> Vec<(Company, Building)> {
    // Phase 44: Strict Zero Agriculture — if arable land is zero, spawn NO
    // agricultural companies. The region must rely on imported food.
    if sector == Sector::Agriculture && region.arable_land_max <= 0 {
        return Vec::new();
    }

    // Determine how many firms can realistically operate in this market.
    // Small regions still get at least three competitors; huge markets cap at 20.
    // Phase 44: For Agriculture, scale company count by arable land.
    let company_count = if sector == Sector::Agriculture {
        // Agriculture: scale by arable land. More arable land = more farms.
        let arable_scale = (region.arable_land_max as f64 / 10_000.0).max(1.0);
        (region_emp / 1500.0 * arable_scale).round().max(3.0).min(20.0) as usize
    } else {
        (region_emp / 1500.0).round().max(3.0).min(20.0) as usize
    };

    // Phase 44: Compute diversified methods for this sector (one call per region).
    let diversified_methods = {
        let raw_methods = diversified_registry_methods(sector, start_year, registries);
        era_filtered_methods(raw_methods, start_year)
    };

    // Draw power-law weights and normalize them to employment shares.
    let mut weights: Vec<f64> = (0..company_count)
        .map(|_| rng.gen::<f64>().powf(2.0))
        .collect();
    let total_weight: f64 = weights.iter().sum();
    if total_weight <= 0.0 {
        weights = vec![1.0; company_count];
    }
    let total_weight = weights.iter().sum::<f64>().max(f64::EPSILON);

    let mut shares: Vec<f64> = weights
        .iter()
        .map(|w| w / total_weight)
        .collect();

    // Sort descending by share so the largest firms get the top ranks.
    shares.sort_by(|a, b| b.partial_cmp(a).unwrap());

    // First pass: compute rounded actual employment capacities and total.
    let mut plans: Vec<(u32, u32, f64)> = Vec::new();
    for &share in &shares {
        let target = (region_emp * share).max(1.0);
        let actual = target.round() as u32;
        let (scale_factor, base_capacity) = split_capacity(actual);
        plans.push((scale_factor, base_capacity, target));
    }
    let total_actual: u32 = plans.iter().map(|p| p.0 * p.1).sum();

    // Second pass: build companies and buildings, allocating capital proportionally.
    let mut result = Vec::new();
    for (rank, (scale_factor, base_capacity, _target)) in plans.into_iter().enumerate() {
        let rank = rank + 1;
        let actual_capacity = base_capacity * scale_factor;

        let capital_share = if total_actual > 0 {
            actual_capacity as f64 / total_actual as f64
        } else {
            1.0 / company_count as f64
        };
        let company_fixed = region_fixed * capital_share;
        let company_liquid = region_liquid * capital_share;
        let company_capital = company_fixed + company_liquid;

        let is_national_champion = actual_capacity > 25_000;
        // Phase 77: Increase JSC proportion — rank 1-5 and national champions
        // are JSC (was 1-2 only, national champions were Consortium).
        // This ensures ~25-50% of companies are publicly traded.
        let (_label, legal_form, shares_count) = match (is_national_champion, rank) {
            (true, _) => (
                "National Champion",
                LegalForm::JointStockCompany(JointStockData {
                    shares_issued: 1_000_000,
                    // Phase 77: National champions have 40% free float.
                    free_float: 0.40,
                    dividend_per_share: 0.0,
                    board_independence: 0.5,
                    board_members: Vec::new(),
                }),
                1_000_000,
            ),
            (false, 1..=5) => (
                "Corporation",
                LegalForm::JointStockCompany(JointStockData {
                    shares_issued: 1_000_000,
                    // Phase 77: Top JSC firms are listed at generation with 20-40% free float.
                    free_float: if rank <= 2 { 0.40 } else { 0.20 },
                    dividend_per_share: 0.0,
                    board_independence: 0.5,
                    board_members: Vec::new(),
                }),
                1_000_000,
            ),
            (false, 6..=10) => {
                // Phase 77: 50/50 split between Cooperative and FamilyBusiness
                if rng.gen::<f64>() > 0.5 {
                    (
                        "Cooperative",
                        LegalForm::Cooperative(CooperativeData {
                            member_count: actual_capacity,
                            patronage_pool: 0.0,
                            federation_id: None,
                        }),
                        0,
                    )
                } else {
                    (
                        "Enterprise",
                        LegalForm::FamilyBusiness(FamilyBusinessData {
                            dynasty_id: None,
                            successor_generation: 0,
                            family_retained_share: 1.0,
                            heir_vip_ids: Vec::new(),
                            succession_crisis: false,
                        }),
                        0,
                    )
                }
            }
            _ => (
                "Enterprise",
                LegalForm::FamilyBusiness(FamilyBusinessData {
                    dynasty_id: None,
                    successor_generation: 0,
                    family_retained_share: 1.0,
                    heir_vip_ids: Vec::new(),
                    succession_crisis: false,
                }),
                0,
            ),
        };

        let company_id = idgen.next_company();
        // Phase 37: Use English-only company name generator with cultural surname prefix.
        // Phase 53: Pass cultural_group for culture-scoped surnames.
        let company_name = generate_company_name(sector, &legal_form, &region.id, rank, cultural_group, rng);
        let share_price = if shares_count > 0 {
            company_capital / shares_count as f64
        } else {
            0.0
        };
        // Phase 61.2: Derive is_listed and free_float from legal_form before it's moved.
        let is_listed = legal_form.is_listed();
        let free_float = legal_form.free_float();

        let mut company = Company {
            id: company_id.clone(),
            file_stem: sector_name.to_string(),
            name: company_name,
            sector,
            region_id: region.id.clone(),
            legal_form,
            state_share: 0.0,
            fixed_capital: company_fixed,
            liquid_capital: company_liquid,
            available_cash: 0.0,
            debit_cash: 0.0,
            credit_cash: 0.0,
            unfilled_bid_prices: std::collections::HashMap::new(),
            liabilities: 0.0,
            company_capital,
            shares_count,
            share_price,
            shareholders: BTreeMap::new(),
            price_history: Vec::new(),
            financial_history: Vec::new(),
            safety_level: 0.5,
            union_id: None,
            building_ids: Vec::new(),
            scale_factor,
            worker_capacity: actual_capacity,
            is_national_champion,
            is_listed,
            owners: BTreeMap::new(),
            free_float,
            aggregated_stats: crate::entities::AggregatedStats::default(),
            bank_type: None,
            balance_sheet: None,
            loan_margin: None,
            brokerage_account: None,
            primary_bank_id: None, outstanding_loan_bank_id: None,
            fund_type: None,
            fund_ledger: None,
            temporary_disruption_modifier: 0.0,
            target_fte_demand: actual_capacity,
            offered_wage_per_fte: (company_liquid * 0.6 / (actual_capacity as f64).max(1.0)).max(1.0),
            prev_offered_wage_per_fte: (company_liquid * 0.6 / (actual_capacity as f64).max(1.0)).max(1.0).max(50.0),
            wage_arrears: 0.0,
            severance_arrears: 0.0,
            furlough_turns_accumulated: 0,
            productivity_penalty: 0.0,
            target_wage: (company_liquid * 0.6 / (actual_capacity as f64).max(1.0)).max(50.0),
            is_striking: false,
            fulfilled_fte: 0,
            prev_fulfilled_fte: 0,
            physical_fte_demand: actual_capacity,
            is_in_receivership: false,
            agricultural_profile: None,
            rd_budget: 0.0,
            patents: Vec::new(),
            licensed_methods: Vec::new(),
            information_quality: None,
            shadow_employment: None,
            pending_expansion: None,
            blueprints: Vec::new(),
            licensed_blueprints: Vec::new(),
            reputation_score: 50.0, donation_history: Vec::new(), is_dspw: false, consumer_loans: Vec::new(),
            annual_profit_accumulator: 0.0,
            seasonal_profile: seasonal_profile_for_sector(sector, &region.climate_profile),
            furloughed_workers_count: 0.0,
            ceo_vip_id: None,
            eps: 0.0, pe_ratio: 0.0, dividend_yield: 0.0, open_price: 0.0, close_price: 0.0,
            action_ledger: crate::entities::ActionLedger::default(),
            extra: serde_json::Map::new(),
        };

        // Phase 42: Genesis Labor Fix — pre-populate workforce and inject payroll grant.
        let initial_wage = (company_liquid * 0.6 / (actual_capacity as f64).max(1.0)).max(50.0);
        let initial_fte = (actual_capacity as f64 * 0.6).round().max(2.0); // Phase 43: min 2.0 FTE floor
        company.fulfilled_fte = initial_fte as u32;
        company.prev_fulfilled_fte = initial_fte as u32;
        // Genesis Payroll Grant: 3 turns of wages for the initial workforce.
        let payroll_grant = initial_fte * initial_wage * 3.0;
        company.available_cash = company_liquid + payroll_grant;

        let (building_name, method) = if diversified_methods.is_empty() {
            // Fallback to best_registry_method if no diversified methods available.
            best_registry_method(sector, start_year, registries)
        } else {
            // Phase 44: Use diversified method selection (round-robin by rank).
            select_diversified_method(&diversified_methods, rank - 1)
        };
        let current_employment = (initial_fte / scale_factor as f64) as u32;
        let building_id = idgen.next_building();

        // Phase 20C: Seed fixed-asset cohort and one turn of inventory
        let fixed_assets = seed_fixed_assets(sector, start_year, rng);
        let (inventory, seed_cost) = seed_inventory(&method, base_capacity, sector);
        let inventory_capacity = (base_capacity as f64 * 10.0).max(100.0);

        // Phase 27: Deduct seed inventory cost from company's liquid capital
        // to maintain double-entry accounting. The cost is credited to the
        // country's treasury in generate_corporate_entities.
        let deductible = seed_cost.min(company.liquid_capital * 0.5);
        company.liquid_capital -= deductible;
        company.available_cash -= deductible;
        company.extra.insert("seed_inventory_cost".to_string(), Value::from(deductible));

        let building = Building {
            id: building_id.clone(),
            name: building_name,
            owner_id: company_id.clone(),
            year_built: start_year.saturating_sub(rng.gen_range(1..30)),
            sector,
            worker_capacity: base_capacity,
            current_employment: current_employment.min(base_capacity),
            reserve: company_fixed * 0.05,
            active_method: method,
            accidents_last_year: 0,
            strike: false,
            scale_factor,
            building_capacity: base_capacity,
            region_id: region.id.clone(),
            cluster_info: ClusterInfo {
                region_id: region.id.clone(),
                scale_factor,
                sector,
                owner_id: company_id.clone(),
                extra: Map::new(),
            },
            last_production: BTreeMap::new(),
            last_profit: 0.0,
            last_fulfillment_ratio: 1.0,
            condition: 1.0,
            is_heritage_site: false,
            experience_level: None,
            aggregated_stats: AggregatedStats::default(),
            structural_defect: 0.0, land_hectares: 0.0,
            extra: Map::new(),
            inventory,
            inventory_capacity,
            active_project: None,
            landfill_state: None,
            deposit_id: None,
            fixed_assets,
            pending_method_upgrade: None,
            active_emission_control: String::new(),
        };

        company.building_ids.push(building_id);
        company.aggregated_stats.total_employment = current_employment * scale_factor;
        result.push((company, building));
    }

    result
}

/// Split a desired employment capacity into a base building capacity and a
/// `scale_factor` so the building-level simulation stays cheap.
///
/// # Rules
/// * Small companies (<100 workers) keep a single building (`scale_factor = 1`).
/// * Larger companies use `scale_factor = actual / 100` and a base of ~100 workers.
fn split_capacity(actual_capacity: u32) -> (u32, u32) {
    if actual_capacity < 100 {
        (1, actual_capacity.max(1))
    } else {
        let scale_factor = (actual_capacity / 100).max(1);
        let base_capacity = actual_capacity / scale_factor;
        (scale_factor, base_capacity)
    }
}

struct IdGen {
    prefix: String,
    company_counter: usize,
    building_counter: usize,
    union_counter: usize,
}

impl IdGen {
    fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            company_counter: 0,
            building_counter: 0,
            union_counter: 0,
        }
    }

    fn next_company(&mut self) -> String {
        self.company_counter += 1;
        format!("KRS-{}-{:04}", self.prefix, self.company_counter)
    }

    fn next_building(&mut self) -> String {
        self.building_counter += 1;
        format!("B-{}-{:04}", self.prefix, self.building_counter)
    }

    fn next_union(&mut self) -> String {
        self.union_counter += 1;
        format!("UNION-{}-{:04}", self.prefix, self.union_counter)
    }
}

fn sector_json_name(sector: Sector) -> String {
    serde_json::to_value(sector)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| format!("{sector:?}"))
}

fn sector_display(sector: Sector) -> String {
    match sector {
        Sector::Agriculture => "Agriculture",
        Sector::Mining => "Mining",
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
        Sector::TransportLogistics => "Transport and Logistics",
        Sector::PublicAdministration => "Public Administration",
        Sector::Banking => "Banking",
        Sector::MediaAndEntertainment => "Media and Entertainment",
        Sector::WasteManagement => "Waste Management",
        Sector::Hospitality => "Hospitality",
        Sector::NGO => "NGO",
        Sector::Religion => "Religion",
        Sector::MaintenanceWorkshops => "Maintenance Workshops",
        Sector::Government => "Government",
    }
    .to_string()
}

/// Phase 37: English-only sector suffix for company names.
/// Returns a descriptive English business descriptor per sector.
/// Phase 51: Multiple variants per sector for name variety.
fn sector_suffix(sector: Sector, rng: &mut impl rand::Rng) -> &'static str {
    match sector {
        Sector::Agriculture => ["Agricultural Trust", "Farming Co", "Agro Holdings", "Rural Estates"].choose(rng).unwrap(),
        Sector::Mining => ["Mining Corp", "Extractive Ltd", "Mineral Holdings", "Quarry Group"].choose(rng).unwrap(),
        Sector::HeavyIndustry => ["Steel Works", "Heavy Industries", "Iron & Steel", "Industrial Corp"].choose(rng).unwrap(),
        Sector::LightIndustry => ["Manufacturing Co", "Industrial Works", "Production Ltd", "Goods Manufacturing"].choose(rng).unwrap(),
        Sector::ArmamentsIndustry => ["Defense Industries", "Armaments Corp", "Military Industries", "Ordnance Works"].choose(rng).unwrap(),
        Sector::LocalServices => ["Services Ltd", "Local Services", "Community Services", "Civic Holdings"].choose(rng).unwrap(),
        Sector::ExportServices => ["Trading Co", "Export Holdings", "International Trade", "Commerce Ltd"].choose(rng).unwrap(),
        Sector::Construction => ["Construction Group", "Building Corp", "Infrastructure Ltd", "Construction Works"].choose(rng).unwrap(),
        Sector::Energy => ["Energy Holdings", "Power Corp", "Utility Group", "Energy Works"].choose(rng).unwrap(),
        Sector::PublicServices => ["Public Utilities", "Civic Services", "Municipal Holdings", "Public Corp"].choose(rng).unwrap(),
        Sector::MedicalServices => ["Healthcare Group", "Medical Holdings", "Health Services", "Clinic Group"].choose(rng).unwrap(),
        Sector::EducationalServices => ["Education Trust", "Academic Holdings", "Learning Group", "Education Corp"].choose(rng).unwrap(),
        Sector::TransportLogistics => ["Logistics Inc", "Transport Holdings", "Freight Corp", "Shipping Group"].choose(rng).unwrap(),
        Sector::PublicAdministration => ["Administration", "State Bureau", "Public Office", "Civic Administration"].choose(rng).unwrap(),
        Sector::Banking => ["Banking Group", "Financial Holdings", "Capital Trust", "Finance Corp"].choose(rng).unwrap(),
        Sector::MediaAndEntertainment => ["Media Holdings", "Broadcast Group", "Entertainment Corp", "Media Trust"].choose(rng).unwrap(),
        Sector::WasteManagement => ["Waste Management Ltd", "Sanitation Corp", "Environmental Services", "Waste Holdings"].choose(rng).unwrap(),
        Sector::Hospitality => ["Hospitality Group", "Hotel Holdings", "Tourism Corp", "Leisure Group"].choose(rng).unwrap(),
        Sector::NGO => ["Foundation", "Charitable Trust", "Civic Foundation", "Social Initiative"].choose(rng).unwrap(),
        Sector::Religion => ["Religious Trust", "Ecclesiastical Holdings", "Diocesan Trust", "Religious Foundation"].choose(rng).unwrap(),
        Sector::MaintenanceWorkshops => ["Maintenance Services", "Repair Works", "Technical Services", "Workshop Ltd"].choose(rng).unwrap(),
        Sector::Government => ["State Agency", "Government Bureau", "State Holdings", "Public Authority"].choose(rng).unwrap(),
    }
}

/// Phase 37: English-only legal form suffix.
fn legal_form_suffix(legal_form: &crate::entities::LegalForm) -> &'static str {
    use crate::entities::LegalForm;
    match legal_form {
        LegalForm::JointStockCompany(_) => "Inc",
        LegalForm::StateMonopoly(_) => "State Corp",
        LegalForm::FamilyBusiness(_) => "Ltd",
        LegalForm::Cooperative(_) => "Cooperative",
        LegalForm::Latifundium(_) => "Estates",
        LegalForm::Consortium(_) => "Group",
        LegalForm::MunicipalCompany(_) => "Municipal Corp",
        LegalForm::HousingCommunity(_) => "Housing Assn",
        LegalForm::HousingCooperative(_) => "Housing Co-op",
        LegalForm::StrategicReserveAgency(_) => "Reserve Agency",
        LegalForm::LogisticsCompany(_) => "Logistics Inc",
        LegalForm::NonProfit(_) => "Foundation",
        LegalForm::MutualAidCircle(_) => "Mutual Aid",
        LegalForm::Guild(_) => "Guild",
    }
}

/// Phase 37: Generate a realistic company name using a cultural surname prefix
/// and an English-only sector suffix + legal form suffix.
/// Phase 51: Expanded surname pool with 80+ names from all cultural groups.
/// Phase 53: Use culture-scoped surnames from `name_pool_for_culture` so
/// company names match the country's cultural group (no more cross-cultural
/// names like Arabic surnames in a Germanic country).
/// Example: "Kowalski Steel Works Inc" instead of "Seed HeavyIndustry (R1) #1".
fn generate_company_name(
    sector: Sector,
    legal_form: &crate::entities::LegalForm,
    _region_id: &str,
    rank: usize,
    cultural_group: &str,
    rng: &mut impl rand::Rng,
) -> String {
    // Phase 53: Draw surnames from the culture-specific pool.
    let cg = if cultural_group.is_empty() { "slavic" } else { cultural_group };
    let pool = crate::politics::names::name_pool_for_culture(cg);
    let surname = pool.surnames.choose(rng).copied().unwrap_or("Smith");

    let suffix = sector_suffix(sector, rng);
    let legal = legal_form_suffix(legal_form);

    // For uniqueness, include the region ID and rank as a subtle suffix.
    if rank > 1 {
        format!("{surname} {suffix} {legal} #{rank}")
    } else {
        format!("{surname} {suffix} {legal}")
    }
}

/// Phase 79: Look up a production method by name from a sector's method registry.
///
/// Searches the `BuildingMethods` for the given sector key and returns an
/// `ActiveProductionMethod` matching the given method name, if found and
/// available at `current_year`.
fn find_storage_method_by_name(
    registries: &Registries,
    sector_key: &str,
    method_name: &str,
    current_year: u32,
) -> Option<ActiveProductionMethod> {
    let methods = registries.production_methods.get(sector_key)?;
    let pm = methods.production.get(method_name)?;
    if pm.year > current_year {
        return None;
    }
    Some(ActiveProductionMethod {
        year: pm.year,
        experts_ratio: pm.experts_ratio,
        skilled_ratio: pm.skilled_ratio,
        basic_ratio: pm.basic_ratio,
        efficiency: pm.efficiency,
        inputs: pm.inputs.iter().map(|(&k, &v)| (k, v)).collect(),
        outputs: pm.outputs.iter().map(|(&k, &v)| (k, v)).collect(),
        active_methods: crate::state::treasury::ProductionMethodChoice {
            automation: String::new(),
            production: method_name.to_string(),
            organization: String::new(),
            ..Default::default()
        },
        active_blueprint: None,
        thermal_efficiency: pm.thermal_efficiency,
        storage_efficiency: pm.storage_efficiency,
        emission_factor: pm.emission_factor,
        biohazard_factor: pm.biohazard_factor,
        output_water_quality: pm.output_water_quality,
        discharge_quality: pm.discharge_quality,
        extra: Default::default(),
    })
}

fn method_from_ratios(
    experts: f64,
    skilled: f64,
    basic: f64,
    inputs: BTreeMap<Commodity, f64>,
    outputs: BTreeMap<Commodity, f64>,
    year: u32,
) -> ActiveProductionMethod {
    ActiveProductionMethod {
        year,
        experts_ratio: experts,
        skilled_ratio: skilled,
        basic_ratio: basic,
        efficiency: 1.0,
        inputs,
        outputs,
        active_methods: Default::default(),
        active_blueprint: None,
        extra: Map::new(),
        ..Default::default()
    }
}

/// Phase 20: Select the best available production method from the registry
/// for a sector at the given start year.
///
/// # Rules
/// * Looks up the sector's BuildingMethods from the registry.
/// * Iterates all Production-slot methods.
/// * Returns the method with the highest year that is <= start_year
///   and whose required_tech is None or whose tech year <= start_year.
/// * Falls back to the earliest method if none match.
/// * Converts the registry's ProductionMethod to an ActiveProductionMethod.
/// Phase 26: Find a HeavyIndustry production method that outputs IndustrialMachinery.
///
/// Returns `Some((building_name, method))` if a suitable method exists for the
/// given start year, or `None` if no machinery-producing method is available.
fn best_machinery_method(
    start_year: u32,
    registries: &Registries,
) -> Option<(String, ActiveProductionMethod)> {
    let sector_key = sector_json_name(Sector::HeavyIndustry);
    let building_name = default_building_name(Sector::HeavyIndustry);

    let building_methods = registries.production_methods.get(&sector_key)?;
    let best = building_methods.production.values()
        .filter(|pm| pm.year <= start_year)
        .filter(|pm| {
            match &pm.required_tech {
                None => true,
                Some(tech_id) => {
                    registries.tech_tree.get(tech_id)
                        .map(|node| node.year <= start_year)
                        .unwrap_or(false)
                }
            }
        })
        .filter(|pm| pm.outputs.iter().any(|(c, _)| *c == Commodity::IndustrialMachinery))
        .max_by_key(|pm| pm.year)?;

    let method = method_from_ratios(
        best.experts_ratio,
        best.skilled_ratio,
        best.basic_ratio,
        best.inputs.iter().map(|(k, v)| (*k, *v)).collect(),
        best.outputs.iter().map(|(k, v)| (*k, *v)).collect(),
        best.year,
    );
    Some((building_name, method))
}

/// Phase 27: Find a HeavyIndustry method that produces IndustrialMachinery
/// WITHOUT requiring ElectronicComponents or other advanced inputs.
///
/// This breaks the chicken-and-egg deadlock where machinery production
/// requires ElectronicComponents, but ElectronicComponents production
/// requires IndustrialMachinery.
fn best_simple_machinery_method(
    start_year: u32,
    registries: &Registries,
) -> Option<(String, ActiveProductionMethod)> {
    let sector_key = sector_json_name(Sector::HeavyIndustry);
    let building_name = default_building_name(Sector::HeavyIndustry);

    let building_methods = registries.production_methods.get(&sector_key)?;
    let best = building_methods.production.values()
        .filter(|pm| pm.year <= start_year)
        .filter(|pm| {
            match &pm.required_tech {
                None => true,
                Some(tech_id) => {
                    registries.tech_tree.get(tech_id)
                        .map(|node| node.year <= start_year)
                        .unwrap_or(false)
                }
            }
        })
        .filter(|pm| pm.outputs.iter().any(|(c, _)| *c == Commodity::IndustrialMachinery))
        // Key filter: exclude methods that need ElectronicComponents or Semiconductors.
        .filter(|pm| {
            !pm.inputs.iter().any(|(c, _)| {
                *c == Commodity::ElectronicComponents
                    || *c == Commodity::Semiconductors
                    || *c == Commodity::Software
            })
        })
        .max_by_key(|pm| pm.year)?;

    let method = method_from_ratios(
        best.experts_ratio,
        best.skilled_ratio,
        best.basic_ratio,
        best.inputs.iter().map(|(k, v)| (*k, *v)).collect(),
        best.outputs.iter().map(|(k, v)| (*k, *v)).collect(),
        best.year,
    );
    Some((building_name, method))
}

/// Phase 27: Find a HeavyIndustry method that produces Steel WITHOUT
/// requiring ElectronicComponents.
///
/// The 1975 "Mini-Mill Production" method needs ElectronicComponents, but
/// "Basic Oxygen Furnace" (1950) only needs Iron, Coal, Energy. This ensures
/// Steel production can happen even when ElectronicComponents aren't available.
fn best_simple_steel_method(
    start_year: u32,
    registries: &Registries,
) -> Option<(String, ActiveProductionMethod)> {
    let sector_key = sector_json_name(Sector::HeavyIndustry);
    let building_name = default_building_name(Sector::HeavyIndustry);

    let building_methods = registries.production_methods.get(&sector_key)?;
    let best = building_methods.production.values()
        .filter(|pm| pm.year <= start_year)
        .filter(|pm| {
            match &pm.required_tech {
                None => true,
                Some(tech_id) => {
                    registries.tech_tree.get(tech_id)
                        .map(|node| node.year <= start_year)
                        .unwrap_or(false)
                }
            }
        })
        .filter(|pm| pm.outputs.iter().any(|(c, _)| *c == Commodity::Steel))
        // Key filter: exclude methods that need ElectronicComponents.
        .filter(|pm| {
            !pm.inputs.iter().any(|(c, _)| {
                *c == Commodity::ElectronicComponents
                    || *c == Commodity::Semiconductors
            })
        })
        .max_by_key(|pm| pm.year)?;

    let method = method_from_ratios(
        best.experts_ratio,
        best.skilled_ratio,
        best.basic_ratio,
        best.inputs.iter().map(|(k, v)| (*k, *v)).collect(),
        best.outputs.iter().map(|(k, v)| (*k, *v)).collect(),
        best.year,
    );
    Some((building_name, method))
}

/// Phase 27: Find a HeavyIndustry method that produces MechanicalComponents.
/// These methods use IndustrialMachinery as an input, which drives fixed-asset
/// purchase bids and thus Investment (I) in GDP.
fn best_mechanical_components_method(
    start_year: u32,
    registries: &Registries,
) -> Option<(String, ActiveProductionMethod)> {
    let sector_key = sector_json_name(Sector::HeavyIndustry);
    let building_name = default_building_name(Sector::HeavyIndustry);

    let building_methods = registries.production_methods.get(&sector_key)?;
    let best = building_methods.production.values()
        .filter(|pm| pm.year <= start_year)
        .filter(|pm| {
            match &pm.required_tech {
                None => true,
                Some(tech_id) => {
                    registries.tech_tree.get(tech_id)
                        .map(|node| node.year <= start_year)
                        .unwrap_or(false)
                }
            }
        })
        .filter(|pm| pm.outputs.iter().any(|(c, _)| *c == Commodity::MechanicalComponents))
        // Exclude methods that need ElectronicComponents (may not exist yet).
        .filter(|pm| {
            !pm.inputs.iter().any(|(c, _)| {
                *c == Commodity::ElectronicComponents
                    || *c == Commodity::Semiconductors
                    || *c == Commodity::Software
            })
        })
        .max_by_key(|pm| pm.year)?;

    let method = method_from_ratios(
        best.experts_ratio,
        best.skilled_ratio,
        best.basic_ratio,
        best.inputs.iter().map(|(k, v)| (*k, *v)).collect(),
        best.outputs.iter().map(|(k, v)| (*k, *v)).collect(),
        best.year,
    );
    Some((building_name, method))
}

fn best_registry_method(
    sector: Sector,
    start_year: u32,
    registries: &Registries,
) -> (String, ActiveProductionMethod) {
    let sector_key = sector_json_name(sector);
    let building_name = default_building_name(sector);

    match registries.production_methods.get(&sector_key) {
        Some(building_methods) => {
            let best = building_methods.production.values()
                .filter(|pm| pm.year <= start_year)
                .filter(|pm| {
                    match &pm.required_tech {
                        None => true,
                        Some(tech_id) => {
                            registries.tech_tree.get(tech_id)
                                .map(|node| node.year <= start_year)
                                .unwrap_or(false)
                        }
                    }
                })
                .max_by_key(|pm| pm.year)
                .or_else(|| building_methods.production.values().min_by_key(|pm| pm.year));

            match best {
                Some(pm) => {
                    let method = method_from_ratios(
                        pm.experts_ratio,
                        pm.skilled_ratio,
                        pm.basic_ratio,
                        pm.inputs.iter().map(|(c, q)| (*c, *q)).collect(),
                        pm.outputs.iter().map(|(c, q)| (*c, *q)).collect(),
                        pm.year,
                    );
                    (building_name, method)
                }
                None => (building_name, method_from_ratios(0.10, 0.40, 0.50, BTreeMap::new(), BTreeMap::new(), start_year)),
            }
        }
        None => (building_name, method_from_ratios(0.10, 0.40, 0.50, BTreeMap::new(), BTreeMap::new(), start_year)),
    }
}

/// Phase 44: Diversified production method selector.
///
/// Instead of picking the single highest-year method for an entire sector
/// (which causes monoculture — e.g., ALL Agriculture companies produce only
/// Cereal), this function returns a *list* of era-appropriate methods for the
/// sector, so different companies can be assigned different methods.
///
/// Returns a Vec of (building_name, method) pairs, one per eligible method.
fn diversified_registry_methods(
    sector: Sector,
    start_year: u32,
    registries: &Registries,
) -> Vec<(String, ActiveProductionMethod)> {
    let sector_key = sector_json_name(sector);
    let building_name = default_building_name(sector);

    match registries.production_methods.get(&sector_key) {
        Some(building_methods) => {
            // Collect all era-eligible methods.
            let mut eligible: Vec<&ProductionMethod> = building_methods.production.values()
                .filter(|pm| pm.year <= start_year)
                .filter(|pm| {
                    match &pm.required_tech {
                        None => true,
                        Some(tech_id) => {
                            registries.tech_tree.get(tech_id)
                                .map(|node| node.year <= start_year)
                                .unwrap_or(false)
                        }
                    }
                })
                .collect();

            // Sort by year (ascending) for deterministic ordering.
            eligible.sort_by_key(|pm| pm.year);

            if eligible.is_empty() {
                // Fallback: use the earliest method if no era-eligible one exists.
                let earliest = building_methods.production.values().min_by_key(|pm| pm.year);
                match earliest {
                    Some(pm) => {
                        let method = method_from_ratios(
                            pm.experts_ratio,
                            pm.skilled_ratio,
                            pm.basic_ratio,
                            pm.inputs.iter().map(|(c, q)| (*c, *q)).collect(),
                            pm.outputs.iter().map(|(c, q)| (*c, *q)).collect(),
                            pm.year,
                        );
                        vec![(building_name, method)]
                    }
                    None => vec![(building_name, method_from_ratios(0.10, 0.40, 0.50, BTreeMap::new(), BTreeMap::new(), start_year))],
                }
            } else {
                eligible.iter().map(|pm| {
                    let method = method_from_ratios(
                        pm.experts_ratio,
                        pm.skilled_ratio,
                        pm.basic_ratio,
                        pm.inputs.iter().map(|(c, q)| (*c, *q)).collect(),
                        pm.outputs.iter().map(|(c, q)| (*c, *q)).collect(),
                        pm.year,
                    );
                    (building_name.clone(), method)
                }).collect()
            }
        }
        None => vec![(building_name, method_from_ratios(0.10, 0.40, 0.50, BTreeMap::new(), BTreeMap::new(), start_year))],
    }
}

/// Phase 44: Select a diversified method for a specific company rank within a sector.
///
/// Given the list of eligible methods and the company's rank (0-based), pick
/// a method using round-robin distribution so different companies get different
/// outputs. This breaks the monoculture caused by `best_registry_method`.
fn select_diversified_method(
    methods: &[(String, ActiveProductionMethod)],
    rank: usize,
) -> (String, ActiveProductionMethod) {
    if methods.is_empty() {
        return (default_building_name(Sector::Agriculture), method_from_ratios(0.10, 0.40, 0.50, BTreeMap::new(), BTreeMap::new(), 1900));
    }
    let idx = rank % methods.len();
    methods[idx].clone()
}

/// Phase 44: Check if a commodity is era-appropriate.
///
/// Returns false for commodities that should not be produced in early eras
/// (e.g., Electronics, Plastics, Semiconductors before 1950).
fn is_era_appropriate_commodity(commodity: Commodity, start_year: u32) -> bool {
    match commodity {
        // Electronics and advanced tech: post-1950
        Commodity::ElectronicComponents => start_year >= 1950,
        Commodity::Semiconductors | Commodity::Software => start_year >= 1975,
        // Plastics: post-1950
        Commodity::Plastics => start_year >= 1950,
        // Televisions: post-1950
        Commodity::Televisions => start_year >= 1950,
        // Advanced chemicals (Haber-Bosch): post-1925
        Commodity::Fertilizers => start_year >= 1925,
        // Most other commodities are era-appropriate
        _ => true,
    }
}

/// Phase 44: Filter production methods to only those whose outputs are era-appropriate.
fn era_filtered_methods(
    methods: Vec<(String, ActiveProductionMethod)>,
    start_year: u32,
) -> Vec<(String, ActiveProductionMethod)> {
    methods.into_iter()
        .filter(|(_, m)| {
            m.outputs.keys().all(|c| is_era_appropriate_commodity(*c, start_year))
        })
        .collect()
}

/// Phase 20: Default building name per sector (English, no localization).
fn default_building_name(sector: Sector) -> String {
    match sector {
        Sector::Mining => "Mine".to_string(),
        Sector::Agriculture => "Farm".to_string(),
        Sector::HeavyIndustry => "Heavy Industry Plant".to_string(),
        Sector::LightIndustry => "Factory".to_string(),
        Sector::ArmamentsIndustry => "Armaments Factory".to_string(),
        Sector::Construction => "Construction Company".to_string(),
        Sector::Energy => "Power Plant".to_string(),
        Sector::TransportLogistics => "Transport Depot".to_string(),
        Sector::MediaAndEntertainment => "Media Studio".to_string(),
        Sector::MedicalServices => "Medical Facility".to_string(),
        Sector::EducationalServices => "School".to_string(),
        Sector::PublicServices => "Public Office".to_string(),
        Sector::MaintenanceWorkshops => "Maintenance Workshop".to_string(),
        Sector::LocalServices => "Local Services".to_string(),
        Sector::ExportServices => "Export Company".to_string(),
        Sector::Hospitality => "Hospitality Venue".to_string(),
        Sector::Banking => "Bank Branch".to_string(),
        Sector::PublicAdministration => "Administrative Office".to_string(),
        Sector::WasteManagement => "Waste Facility".to_string(),
        Sector::NGO => "NGO Office".to_string(),
        Sector::Religion => "Religious Institution".to_string(),
        Sector::Government => "Parliament".to_string(),
    }
}

/// Phase 20A: Seed minimum viable supply chain.
///
/// For each region, create at least one building for every critical sector,
/// regardless of budget employment share. This guarantees that every
/// fundamental commodity has at least one producer at world birth.
fn seed_minimum_viable_supply_chain(
    country: &Country,
    country_regions: &[&Region],
    start_year: u32,
    registries: &Registries,
    idgen: &mut IdGen,
    rng: &mut impl Rng,
) -> Vec<(Company, Building)> {
    let mut result = Vec::new();
    // Bugfix Sprint: Use real average_wage with .max(1.0) floor (Rule 2/15).
    let base_wage = country.macro_indicators.average_wage.max(1.0);

    let critical_sectors = [
        Sector::Mining,
        Sector::Energy,
        Sector::Agriculture,
        Sector::HeavyIndustry,
        Sector::LightIndustry,
        Sector::Construction,
        Sector::MaintenanceWorkshops,
        Sector::TransportLogistics,
        Sector::MedicalServices,
        Sector::EducationalServices,
        Sector::PublicServices,
        Sector::ArmamentsIndustry,
        Sector::MediaAndEntertainment,
        Sector::LocalServices,
        Sector::ExportServices,
        Sector::Hospitality,
    ];

    for region in country_regions {
        let region_pop = region.population.max(1000) as f64;

        for &sector in &critical_sectors {
            if sector == Sector::Mining {
                // Phase 27: Geology-respecting mining generation.
                // Instead of one mining company with the highest-year method,
                // spawn one small mining company per mineral deposit found in
                // this region's geological formations. Each mine gets the
                // method that produces the deposit's commodity.
                let mining_entities = seed_geology_based_mines(
                    country,
                    region,
                    start_year,
                    registries,
                    idgen,
                    rng,
                );
                result.extend(mining_entities);
                // Phase 44: Spawn processing plants for mined commodities.
                // Mines produce raw ores; processing plants transform them
                // into usable industrial goods (Steel, Copper, Aluminum, etc.).
                let processing_entities = seed_processing_plants_for_region(
                    country,
                    region,
                    start_year,
                    registries,
                    idgen,
                    rng,
                );
                result.extend(processing_entities);
                continue;
            }

            let min_workers = min_workers_for_sector(sector, region_pop);
            let (company, building) = if sector == Sector::Energy {
                // Phase 27: Ensure era-appropriate fallback energy methods.
                // Not all energy plants should use the highest-year method
                // (which may require ElectronicComponents/NaturalGas that
                // don't exist yet). Mix advanced and fallback methods.
                // Bugfix Sprint: pass real average_wage (with .max(1.0) floor)
                // instead of the hardcoded 500.0 fallback.
                create_seed_energy_company(
                    region,
                    min_workers,
                    start_year,
                    base_wage,
                    registries,
                    idgen,
                    rng,
                )
            } else {
                create_seed_company(
                    sector,
                    region,
                    min_workers,
                    start_year,
                    registries,
                    idgen,
                    rng,
                )
            };
            result.push((company, building));
        }
    }

    result
}

/// Phase 27: Spawn geology-based mining companies for a region.
///
/// Queries the country's `geological_formations` for deposits overlapping
/// this region. For each deposit commodity, finds the best available mining
/// method that outputs that commodity and spawns a small mining company.
/// If no deposits exist for a region, spawns one fallback coal mine.
fn seed_geology_based_mines(
    country: &Country,
    region: &Region,
    start_year: u32,
    registries: &Registries,
    idgen: &mut IdGen,
    rng: &mut impl Rng,
) -> Vec<(Company, Building)> {
    let mut result = Vec::new();

    // Phase 43: Collect ALL deposits overlapping this region (not just one per
    // commodity). The previous code used a BTreeMap<Commodity, String> which
    // deduplicated by commodity via or_insert_with, so only the FIRST deposit
    // for each commodity got a mining company. Now we keep all deposits and
    // create one mining company per deposit, capped at 5 per region.
    let mut all_deposits: Vec<(Commodity, String)> = Vec::new();
    for formation in &country.geological_formations {
        if !formation.overlapping_regions.contains(&region.id) {
            continue;
        }
        for (key, deposit) in &formation.resource_deposits {
            all_deposits.push((deposit.commodity, format!("{}/{}", formation.id, key)));
        }
    }

    if all_deposits.is_empty() {
        // No deposits in this region — spawn one fallback coal mine so the
        // sector isn't completely absent. This mine will have low output.
        let (company, mut building) = create_seed_company_with_method_name(
            Sector::Mining,
            region,
            150,
            start_year,
            registries,
            idgen,
            rng,
            "Manual Mining",
        );
        // Try to link to any HardCoal deposit in the country (not region-specific).
        // If none, the building operates without a deposit link (lower output).
        building.deposit_id = find_any_deposit_for_commodity(
            &country.geological_formations,
            &Commodity::HardCoal,
        );
        result.push((company, building));
        return result;
    }

    // Phase 43: Cap at 5 mining companies per region to avoid entity explosion.
    let max_mines = 5;
    for (commodity, deposit_id) in all_deposits.iter().take(max_mines) {
        let method_name = mining_method_name_for_commodity(*commodity);
        let min_workers = 150u32; // Small mines — keep entity count manageable.

        let (company, mut building) = create_seed_company_with_method_name(
            Sector::Mining,
            region,
            min_workers,
            start_year,
            registries,
            idgen,
            rng,
            method_name,
        );
        building.deposit_id = Some(deposit_id.clone());
        result.push((company, building));
    }

    result
}

/// Phase 44: Spawn processing plants for mined commodities in a region.
///
/// For each mined commodity produced in this region, find a HeavyIndustry
/// processing method that consumes it as an input and produces a refined
/// industrial good. Spawn one processing plant per matching method.
///
/// # Rules
/// * Only spawns plants for commodities actually mined in this region.
/// * Uses era-appropriate methods (filtered by start_year).
/// * Caps at 3 processing plants per region to avoid entity explosion.
/// * Gracefully skips mined commodities with no processing methods.
fn seed_processing_plants_for_region(
    country: &Country,
    region: &Region,
    start_year: u32,
    registries: &Registries,
    idgen: &mut IdGen,
    rng: &mut impl Rng,
) -> Vec<(Company, Building)> {
    let mut result = Vec::new();

    // Collect mined commodities for this region.
    let mut mined_commodities: std::collections::BTreeSet<Commodity> = std::collections::BTreeSet::new();
    for formation in &country.geological_formations {
        if !formation.overlapping_regions.contains(&region.id) {
            continue;
        }
        for deposit in formation.resource_deposits.values() {
            mined_commodities.insert(deposit.commodity);
        }
    }

    if mined_commodities.is_empty() {
        return result;
    }

    // Map mined commodities to processing method names (HeavyIndustry sector).
    let processing_methods: Vec<(Commodity, &'static str)> = vec![
        (Commodity::Iron, "Iron Smelting"),
        (Commodity::Copper, "Copper Smelting"),
        (Commodity::Bauxite, "Aluminum Smelting"),
        (Commodity::Tin, "Tin Smelting"),
        (Commodity::Zinc, "Zinc Smelting"),
        (Commodity::Lead, "Lead Smelting"),
        (Commodity::HardCoal, "Coke Production"),
        (Commodity::Oil, "Oil Refining"),
        (Commodity::NaturalGas, "Gas Processing"),
        (Commodity::Limestone, "Cement Production"),
        (Commodity::Clay, "Brick Production"),
        (Commodity::Sulfur, "Sulfuric Acid Production"),
        (Commodity::Stone, "Stone Cutting"),
        (Commodity::Sand, "Glass Production"),
    ];

    let sector_key = sector_json_name(Sector::HeavyIndustry);
    let building_methods = match registries.production_methods.get(&sector_key) {
        Some(bm) => bm,
        None => return result,
    };

    let mut spawned = 0;
    let max_plants = 3;

    for (mined_commodity, method_name) in &processing_methods {
        if spawned >= max_plants {
            break;
        }
        if !mined_commodities.contains(mined_commodity) {
            continue;
        }

        // Find the processing method by name, filtered by era.
        let pm = building_methods.production.iter()
            .find(|(name, _)| name.to_lowercase() == method_name.to_lowercase())
            .map(|(_, pm)| pm)
            .filter(|pm| pm.year <= start_year)
            .filter(|pm| {
                match &pm.required_tech {
                    None => true,
                    Some(tech_id) => {
                        registries.tech_tree.get(tech_id)
                            .map(|node| node.year <= start_year)
                            .unwrap_or(false)
                    }
                }
            });

        if pm.is_none() {
            continue;
        }

        let pm = pm.unwrap();
        let min_workers = 200u32;

        let method = method_from_ratios(
            pm.experts_ratio,
            pm.skilled_ratio,
            pm.basic_ratio,
            pm.inputs.iter().map(|(k, v)| (*k, *v)).collect(),
            pm.outputs.iter().map(|(k, v)| (*k, *v)).collect(),
            pm.year,
        );

        let (company, building) = create_seed_company_with_explicit_method(
            Sector::HeavyIndustry,
            region,
            min_workers,
            start_year,
            registries,
            idgen,
            rng,
            &default_building_name(Sector::HeavyIndustry),
            &method,
        );
        result.push((company, building));
        spawned += 1;
    }

    result
}

/// Phase 27: Map a commodity to the best mining method name that produces it.
fn mining_method_name_for_commodity(commodity: Commodity) -> &'static str {
    match commodity {
        Commodity::HardCoal => "Manual Mining", // Fallback; era-appropriate selected by year filter
        Commodity::Iron => "Iron Ore Mining",
        Commodity::Copper => "Copper Ore Mining",
        Commodity::Oil => "Oil Drilling",
        Commodity::NaturalGas => "Natural Gas Extraction",
        Commodity::Tin => "Tin Ore Mining",
        Commodity::Bauxite => "Bauxite Mining",
        Commodity::Sand => "Sand And Gravel Quarry",
        Commodity::Gravel => "Sand And Gravel Quarry",
        Commodity::Stone => "Stone Quarrying",
        Commodity::Clay => "Clay Mining",
        Commodity::Limestone => "Limestone Quarrying",
        Commodity::Sulfur => "Sulfur Mining",
        Commodity::Salt => "Salt Mining",
        Commodity::Zinc => "Zinc Ore Mining",
        Commodity::Lead => "Lead Ore Mining",
        _ => "Manual Mining", // Generic fallback
    }
}

/// Phase 27: Find any deposit in the country for a given commodity (not region-specific).
fn find_any_deposit_for_commodity(
    formations: &[GeologicalFormation],
    commodity: &Commodity,
) -> Option<String> {
    for formation in formations {
        for (key, deposit) in &formation.resource_deposits {
            if deposit.commodity == *commodity {
                return Some(format!("{}/{}", formation.id, key));
            }
        }
    }
    None
}

/// Phase 27: Create a seed company with a specific named production method.
///
/// Falls back to `best_registry_method` if the named method is not found or
/// its tech requirements aren't met for the given start year.
fn create_seed_company_with_method_name(
    sector: Sector,
    region: &Region,
    target_workers: u32,
    start_year: u32,
    registries: &Registries,
    idgen: &mut IdGen,
    rng: &mut impl Rng,
    method_name: &str,
) -> (Company, Building) {
    // Try to find the named method; fall back to best_registry_method.
    let (building_name, method) = find_method_by_name(sector, start_year, registries, method_name)
        .unwrap_or_else(|| best_registry_method(sector, start_year, registries));

    create_seed_company_with_explicit_method(
        sector,
        region,
        target_workers,
        start_year,
        registries,
        idgen,
        rng,
        &building_name,
        &method,
    )
}

/// Phase 27: Find a production method by name in the registry, filtered by year/tech.
/// The method name must match the key in the `production` HashMap exactly
/// (case-insensitive). Falls back to a "contains" match if exact match fails.
fn find_method_by_name(
    sector: Sector,
    start_year: u32,
    registries: &Registries,
    method_name: &str,
) -> Option<(String, ActiveProductionMethod)> {
    let sector_key = sector_json_name(sector);
    let building_methods = registries.production_methods.get(&sector_key)?;

    // Try exact key match first (case-insensitive).
    let pm = building_methods.production.iter()
        .find(|(name, _)| name.to_lowercase() == method_name.to_lowercase())
        .map(|(_, pm)| pm)
        // Fallback: contains match.
        .or_else(|| {
            building_methods.production.iter()
                .find(|(name, _)| name.to_lowercase().contains(&method_name.to_lowercase()))
                .map(|(_, pm)| pm)
        })
        .filter(|pm| pm.year <= start_year)
        .filter(|pm| {
            match &pm.required_tech {
                None => true,
                Some(tech_id) => {
                    registries.tech_tree.get(tech_id)
                        .map(|node| node.year <= start_year)
                        .unwrap_or(false)
                }
            }
        })?;

    let method = method_from_ratios(
        pm.experts_ratio,
        pm.skilled_ratio,
        pm.basic_ratio,
        pm.inputs.iter().map(|(k, v)| (*k, *v)).collect(),
        pm.outputs.iter().map(|(k, v)| (*k, *v)).collect(),
        pm.year,
    );
    Some((default_building_name(sector), method))
}

/// Phase 27: Create a seed company with an explicit method (not looked up by sector).
fn create_seed_company_with_explicit_method(
    sector: Sector,
    region: &Region,
    target_workers: u32,
    start_year: u32,
    _registries: &Registries,
    idgen: &mut IdGen,
    rng: &mut impl Rng,
    building_name: &str,
    method: &ActiveProductionMethod,
) -> (Company, Building) {
    let sector_name = sector_json_name(sector);
    let (scale_factor, base_capacity) = split_capacity(target_workers.max(1));
    let actual_capacity = base_capacity * scale_factor;

    let company_id = idgen.next_company();
    let company_name = format!("Seed {} ({}) #{}", sector_display(sector), region.id, idgen.company_counter);
    let company_capital = (actual_capacity as f64) * 1000.0;
    let company_fixed = company_capital * 0.6;
    let company_liquid = company_capital * 0.4;

    let mut company = Company {
        id: company_id.clone(),
        file_stem: sector_name,
        name: company_name,
        sector,
        region_id: region.id.clone(),
        legal_form: LegalForm::FamilyBusiness(FamilyBusinessData {
            dynasty_id: None,
            successor_generation: 0,
            family_retained_share: 1.0,
            heir_vip_ids: Vec::new(),
            succession_crisis: false,
        }),
        state_share: 0.0,
        fixed_capital: company_fixed,
        liquid_capital: company_liquid,
        available_cash: company_liquid,
        debit_cash: 0.0,
        credit_cash: 0.0,
        unfilled_bid_prices: std::collections::HashMap::new(),
        liabilities: 0.0,
        company_capital,
        shares_count: 0,
        share_price: 0.0,
        shareholders: BTreeMap::new(),
        price_history: Vec::new(),
        financial_history: Vec::new(),
        safety_level: 0.5,
        union_id: None,
        building_ids: Vec::new(),
        scale_factor,
        worker_capacity: actual_capacity,
        is_national_champion: false,
        is_listed: false,
        owners: BTreeMap::new(),
        free_float: 0.0,
        aggregated_stats: AggregatedStats::default(),
        bank_type: None,
        balance_sheet: None,
        loan_margin: None,
        brokerage_account: None,
        primary_bank_id: None, outstanding_loan_bank_id: None,
        fund_type: None,
        fund_ledger: None,
        temporary_disruption_modifier: 0.0,
        target_fte_demand: actual_capacity,
        offered_wage_per_fte: (company_liquid * 0.6 / (actual_capacity as f64).max(1.0)).max(1.0),
        prev_offered_wage_per_fte: (company_liquid * 0.6 / (actual_capacity as f64).max(1.0)).max(1.0).max(50.0),
        wage_arrears: 0.0,
        severance_arrears: 0.0,
        furlough_turns_accumulated: 0,
        productivity_penalty: 0.0,
        target_wage: (company_liquid * 0.6 / (actual_capacity as f64).max(1.0)).max(50.0),
        is_striking: false,
        fulfilled_fte: 0,
        prev_fulfilled_fte: 0,
        physical_fte_demand: actual_capacity,
        is_in_receivership: false,
        agricultural_profile: None,
        rd_budget: 0.0,
        patents: Vec::new(),
        licensed_methods: Vec::new(),
        information_quality: None,
        shadow_employment: None,
        pending_expansion: None,
        blueprints: Vec::new(),
        licensed_blueprints: Vec::new(),
        reputation_score: 50.0, donation_history: Vec::new(), is_dspw: false, consumer_loans: Vec::new(),
        annual_profit_accumulator: 0.0,
        seasonal_profile: None,
        furloughed_workers_count: 0.0,
        ceo_vip_id: None,
        eps: 0.0, pe_ratio: 0.0, dividend_yield: 0.0, open_price: 0.0, close_price: 0.0,
        action_ledger: crate::entities::ActionLedger::default(),
        extra: serde_json::Map::new(),
    };

    // Phase 42: Genesis Labor Fix — pre-populate workforce and inject payroll grant.
    let initial_wage = (company_liquid * 0.6 / (actual_capacity as f64).max(1.0)).max(50.0);
    let initial_fte = (actual_capacity as f64 * 0.6).round().max(2.0); // Phase 43: min 2.0 FTE floor
    company.fulfilled_fte = initial_fte as u32;
    company.prev_fulfilled_fte = initial_fte as u32;
    let payroll_grant = initial_fte * initial_wage * 3.0;
    company.available_cash += payroll_grant;

    let current_employment = (initial_fte / scale_factor as f64) as u32;
    let building_id = idgen.next_building();

    let fixed_assets = seed_fixed_assets(sector, start_year, rng);
    let (inventory, seed_cost) = seed_inventory(method, base_capacity, sector);
    let inventory_capacity = (base_capacity as f64 * 10.0).max(100.0);

    // Phase 27: Deduct seed inventory cost from company's liquid capital.
    let deductible = seed_cost.min(company.liquid_capital * 0.5);
    company.liquid_capital -= deductible;
    company.available_cash -= deductible;
    company.extra.insert("seed_inventory_cost".to_string(), Value::from(deductible));

    let building = Building {
        id: building_id.clone(),
        name: building_name.to_string(),
        owner_id: company_id.clone(),
        year_built: start_year.saturating_sub(rng.gen_range(1..30)),
        sector,
        worker_capacity: base_capacity,
        current_employment: current_employment.min(base_capacity),
        reserve: company_fixed * 0.05,
        active_method: method.clone(),
        accidents_last_year: 0,
        strike: false,
        scale_factor,
        building_capacity: base_capacity,
        region_id: region.id.clone(),
        cluster_info: ClusterInfo {
            region_id: region.id.clone(),
            scale_factor,
            sector,
            owner_id: company_id.clone(),
            extra: Map::new(),
        },
        last_production: BTreeMap::new(),
        last_profit: 0.0,
        last_fulfillment_ratio: 1.0,
        condition: 1.0,
        is_heritage_site: false,
        experience_level: None,
        aggregated_stats: AggregatedStats::default(),
        structural_defect: 0.0, land_hectares: 0.0,
        extra: Map::new(),
        inventory,
        inventory_capacity,
        active_project: None,
        landfill_state: None,
        deposit_id: None,
        fixed_assets,
        pending_method_upgrade: None,
        active_emission_control: String::new(),
    };

    company.building_ids.push(building_id);
    company.aggregated_stats.total_employment = current_employment * scale_factor;
    (company, building)
}

/// Phase 27: Create a seed energy company with era-appropriate fallback methods.
///
/// 50% of energy plants use the highest-year method (advanced).
/// 50% use a fallback method that doesn't require advanced inputs
/// (ElectronicComponents, NaturalGas) Ă˘â‚¬â€ť ensuring energy production
/// can happen even when advanced supply chains aren't established.
fn create_seed_energy_company(
    region: &Region,
    target_workers: u32,
    start_year: u32,
    average_wage: f64,
    registries: &Registries,
    idgen: &mut IdGen,
    rng: &mut impl Rng,
) -> (Company, Building) {
    // Phase 81: Use specialized power plant generation.
    create_specialized_power_plant(
        region,
        target_workers,
        start_year,
        average_wage,
        registries,
        idgen,
        rng,
    )
}

/// AI & Stability Audit (Pillar 1B): Check if a region has a non-zero
/// geological resource deposit for the given commodity key.
///
/// Returns `true` if `region.resources` contains the key with
/// `geological_reserves > 0`.
fn has_geological_resource(region: &Region, resource_key: &str) -> bool {
    // Phase 87+: Check both the legacy region.resources map and the new
    // Planet vein system. The vein system is the authoritative source going
    // forward, but region.resources remains as a compatibility layer.
    if let Some(Value::Object(map)) = region.resources.get(resource_key) {
        if let Some(Value::Number(n)) = map.get("geological_reserves") {
            if let Some(reserves) = n.as_f64() {
                return reserves > 0.0;
            }
        }
    }
    false
}

/// Phase 87+: Check if a region has a geological resource via the Planet vein system.
/// This is the authoritative source for geological resources going forward.
#[allow(dead_code)]
fn has_geological_resource_vein(
    planet: &crate::society::planet::Planet,
    region_id: &str,
    commodity: crate::registries::enums::Commodity,
) -> bool {
    planet.has_geological_resource(region_id, commodity)
}

/// AI & Stability Audit (Pillar 1B): Check if a region has forest tracts
/// suitable for biomass feedstock. Uses the LandUseInventory Forests category
/// area — Phase 87+: lowered threshold from 1000 to 500 hectares to allow
/// more regions to support biomass power plants.
fn has_forest_tract(region: &Region) -> bool {
    if let Some(forest_data) = region.land_use_inventory.get_category(
        crate::society::geography::LandCategory::Forests,
    ) {
        return forest_data.area_hectares > 500.0;
    }
    false
}

/// Phase 81: Create a specialized power plant with `PowerPlantMetadata`.
///
/// Determines the plant type based on geographic constraints and era,
/// selects the best available production method for that plant type,
/// and stores `PowerPlantMetadata` in the building's `extra` map.
fn create_specialized_power_plant(
    region: &Region,
    target_workers: u32,
    start_year: u32,
    average_wage: f64,
    registries: &Registries,
    idgen: &mut IdGen,
    rng: &mut impl Rng,
) -> (Company, Building) {
    use crate::energy::generation::{
        available_plant_types, nameplate_per_plant, plant_count,
        target_regional_capacity_mw, workers_per_plant,
    };
    use crate::energy::types::{CoolingType, PowerPlantMetadata, PowerPlantType};

    // Determine geographic constraints.
    let has_coast = region.geographic_traits.has_coastline;
    let has_river = region.geographic_traits.has_navigable_river;
    let has_water = has_coast || has_river;

    // AI & Stability Audit (Pillar 1B): Query actual geological and geographic
    // data instead of hardcoding all flags to false. This was the root cause of
    // the "Biomass Clones" bug — every region got BiomassFired plants because
    // all other plant types required resource flags that were always false.
    //
    // World Generation & Climate Audit (v0.5.3): Updated resource keys to match
    // the Commodity enum's serde serialization (hard_coal, brown_coal, etc.)
    // instead of the old Polish-era keys (coal, lignite).
    let has_coal_deposit = has_geological_resource(region, "hard_coal")
        || has_geological_resource(region, "brown_coal");
    let has_uranium = has_geological_resource(region, "uranium");
    let has_geothermal = region.geographic_traits.has_geothermal_potential;
    let has_forest = has_forest_tract(region);
    let has_livestock = has_geological_resource(region, "peat"); // Peat is organic fuel

    // Get available plant types.
    let plant_types = available_plant_types(
        start_year,
        has_coal_deposit,
        has_water,
        has_forest,
        has_livestock,
        has_uranium,
        has_geothermal,
    );

    // Pick a plant type (weighted random selection).
    let total_weight: f64 = plant_types.iter().map(|(_, w)| *w).sum();
    let mut roll = rng.gen_range(0.0..total_weight.max(0.001));
    let mut selected_type = PowerPlantType::BiomassFired; // Fallback.
    for (pt, w) in &plant_types {
        roll -= w;
        if roll <= 0.0 {
            selected_type = *pt;
            break;
        }
    }

    // Determine cooling type for thermal plants.
    let cooling_type = if selected_type.is_thermal() {
        if !has_water {
            CoolingType::AirCooled
        } else if start_year >= 1950 && rng.gen::<f64>() < 0.3 {
            CoolingType::ClosedLoop
        } else {
            CoolingType::OnceThrough
        }
    } else {
        CoolingType::OnceThrough // Irrelevant for non-thermal.
    };

    // Calculate nameplate capacity.
    // Bugfix Sprint: Use the real average_wage passed from the caller (with
    // .max(1.0) floor applied at the call site), not the hardcoded 500.0.
    let nameplate = nameplate_per_plant(start_year);
    let target_mw = target_regional_capacity_mw(
        region.population as f64,
        region.development_level,
        average_wage,
        start_year,
    );
    let plant_count = plant_count(target_mw, start_year);

    // Select the best available production method for this plant type.
    let sector_key = selected_type.registry_key();
    let method = registries.production_methods.get(sector_key)
        .and_then(|methods| {
            methods.production.values()
                .filter(|pm| pm.year <= start_year)
                .filter(|pm| {
                    match &pm.required_tech {
                        None => true,
                        Some(tech_id) => {
                            registries.tech_tree.get(tech_id)
                                .map(|node| node.year <= start_year)
                                .unwrap_or(false)
                        }
                    }
                })
                .max_by_key(|pm| pm.year)
        });

    let (company, mut building) = match method {
        Some(pm) => {
            let method = method_from_ratios(
                pm.experts_ratio,
                pm.skilled_ratio,
                pm.basic_ratio,
                pm.inputs.iter().map(|(k, v)| (*k, *v)).collect(),
                pm.outputs.iter().map(|(k, v)| (*k, *v)).collect(),
                pm.year,
            );
            let building_name = match selected_type {
                PowerPlantType::CoalFired => "Coal-Fired Power Plant",
                PowerPlantType::LigniteFired => "Lignite Power Plant",
                PowerPlantType::OilGas => "Oil/Gas Power Plant",
                PowerPlantType::Nuclear => "Nuclear Power Plant",
                PowerPlantType::Solar => "Solar Power Plant",
                PowerPlantType::Wind => "Wind Farm",
                PowerPlantType::Hydro => "Hydroelectric Plant",
                PowerPlantType::PumpedStorage => "Pumped Storage Plant",
                PowerPlantType::BatteryStorage => "Battery Storage Facility",
                PowerPlantType::Geothermal => "Geothermal Plant",
                PowerPlantType::BiomassFired => "Biomass Power Plant",
                PowerPlantType::BiogasPlant => "Biogas Plant",
            };
            create_seed_company_with_explicit_method(
                Sector::Energy,
                region,
                // Bugfix Sprint: Scale workers by plant_count so larger regions
                // get proportionally more workers (Rule 15).
                (target_workers.max(workers_per_plant(start_year))) * (plant_count as u32).max(1),
                start_year,
                registries,
                idgen,
                rng,
                building_name,
                &method,
            )
        }
        None => {
            // Fallback to default energy company creation.
            create_seed_company(
                Sector::Energy,
                region,
                target_workers,
                start_year,
                registries,
                idgen,
                rng,
            )
        }
    };

    // Store PowerPlantMetadata in the building's extra map.
    // Bugfix Sprint: Scale nameplate by plant_count so larger regions get
    // proportionally more capacity (Rule 15 — Universal Physical Scaling).
    let total_nameplate = nameplate * plant_count as f64;
    let metadata = PowerPlantMetadata {
        plant_type: selected_type,
        cooling_type,
        has_cooling_upgrade: cooling_type == CoolingType::ClosedLoop,
        fuel_source_deposit_id: None,
        water_source_region: if has_water { Some(region.id.clone()) } else { None },
        nameplate_capacity_mw: total_nameplate,
        capacity_factor: 0.5,
    };
    building.extra.insert(
        PowerPlantMetadata::EXTRA_KEY.to_string(),
        metadata.to_json(),
    );

    (company, building)
}

/// Phase 20: Minimum workers for a seed building in a sector, scaled by region population.
fn min_workers_for_sector(sector: Sector, region_pop: f64) -> u32 {
    let base: u32 = match sector {
        Sector::Mining => 200,
        Sector::Energy => 150,
        Sector::Agriculture => 300,
        Sector::HeavyIndustry => 250,
        Sector::LightIndustry => 200,
        Sector::Construction => 100,
        Sector::MaintenanceWorkshops => 50,
        Sector::TransportLogistics => 80,
        Sector::MedicalServices => 60,
        Sector::EducationalServices => 50,
        Sector::PublicServices => 40,
        Sector::ArmamentsIndustry => 100,
        Sector::MediaAndEntertainment => 40,
        Sector::LocalServices => 80,
        Sector::ExportServices => 50,
        Sector::Hospitality => 50,
        _ => 50,
    };
    ((base as f64) * (region_pop / 100_000.0).max(0.5).min(5.0)) as u32
}

/// Phase 20A: Create a single seed company + building for a critical sector.
fn create_seed_company(
    sector: Sector,
    region: &Region,
    target_workers: u32,
    start_year: u32,
    registries: &Registries,
    idgen: &mut IdGen,
    rng: &mut impl Rng,
) -> (Company, Building) {
    let sector_name = sector_json_name(sector);
    let (scale_factor, base_capacity) = split_capacity(target_workers.max(1));
    let actual_capacity = base_capacity * scale_factor;

    let company_id = idgen.next_company();
    let company_name = format!("Seed {} ({}) #1", sector_display(sector), region.id);
    let company_capital = (actual_capacity as f64) * 1000.0;
    let company_fixed = company_capital * 0.6;
    let company_liquid = company_capital * 0.4;

    let mut company = Company {
        id: company_id.clone(),
        file_stem: sector_name,
        name: company_name,
        sector,
        region_id: region.id.clone(),
        legal_form: LegalForm::FamilyBusiness(FamilyBusinessData {
            dynasty_id: None,
            successor_generation: 0,
            family_retained_share: 1.0,
            heir_vip_ids: Vec::new(),
            succession_crisis: false,
        }),
        state_share: 0.0,
        fixed_capital: company_fixed,
        liquid_capital: company_liquid,
        available_cash: company_liquid,
        debit_cash: 0.0,
        credit_cash: 0.0,
        unfilled_bid_prices: std::collections::HashMap::new(),
        liabilities: 0.0,
        company_capital,
        shares_count: 0,
        share_price: 0.0,
        shareholders: BTreeMap::new(),
        price_history: Vec::new(),
        financial_history: Vec::new(),
        safety_level: 0.5,
        union_id: None,
        building_ids: Vec::new(),
        scale_factor,
        worker_capacity: actual_capacity,
        is_national_champion: false,
        is_listed: false,
        owners: BTreeMap::new(),
        free_float: 0.0,
        aggregated_stats: AggregatedStats::default(),
        bank_type: None,
        balance_sheet: None,
        loan_margin: None,
        brokerage_account: None,
        primary_bank_id: None, outstanding_loan_bank_id: None,
        fund_type: None,
        fund_ledger: None,
        temporary_disruption_modifier: 0.0,
        target_fte_demand: actual_capacity,
        offered_wage_per_fte: (company_liquid * 0.6 / (actual_capacity as f64).max(1.0)).max(1.0),
        prev_offered_wage_per_fte: (company_liquid * 0.6 / (actual_capacity as f64).max(1.0)).max(1.0).max(50.0),
        wage_arrears: 0.0,
        severance_arrears: 0.0,
        furlough_turns_accumulated: 0,
        productivity_penalty: 0.0,
        target_wage: (company_liquid * 0.6 / (actual_capacity as f64).max(1.0)).max(50.0),
        is_striking: false,
        fulfilled_fte: 0,
        prev_fulfilled_fte: 0,
        physical_fte_demand: actual_capacity,
        is_in_receivership: false,
        agricultural_profile: None,
        rd_budget: 0.0,
        patents: Vec::new(),
        licensed_methods: Vec::new(),
        information_quality: None,
        shadow_employment: None,
        pending_expansion: None,
        blueprints: Vec::new(),
        licensed_blueprints: Vec::new(),
        reputation_score: 50.0, donation_history: Vec::new(), is_dspw: false, consumer_loans: Vec::new(),
        annual_profit_accumulator: 0.0,
        seasonal_profile: None,
        furloughed_workers_count: 0.0,
        ceo_vip_id: None,
        eps: 0.0, pe_ratio: 0.0, dividend_yield: 0.0, open_price: 0.0, close_price: 0.0,
        action_ledger: crate::entities::ActionLedger::default(),
        extra: serde_json::Map::new(),
    };

    let (building_name, method) = if sector == Sector::HeavyIndustry {
        // Phase 27: Distribute HeavyIndustry across three product types:
        //   1. IndustrialMachinery producers (sells fixed assets Ă˘â€ â€™ enables I)
        //   2. MechanicalComponents producers (buys IndustrialMachinery Ă˘â€ â€™ drives I)
        //   3. Steel producers (basic input for the above)
        // Without category 2, nobody buys IndustrialMachinery and Investment (I) = 0.
        let roll: f64 = rng.gen();
        if roll < 0.33 {
            // IndustrialMachinery producer Ă˘â‚¬â€ť prefer simple methods without
            // ElectronicComponents.
            best_simple_machinery_method(start_year, registries)
                .or_else(|| best_machinery_method(start_year, registries))
                .unwrap_or_else(|| best_registry_method(sector, start_year, registries))
        } else if roll < 0.66 {
            // MechanicalComponents producer Ă˘â‚¬â€ť uses IndustrialMachinery as input,
            // driving fixed-asset purchase bids (Investment in GDP).
            best_mechanical_components_method(start_year, registries)
                .unwrap_or_else(|| best_registry_method(sector, start_year, registries))
        } else {
            // Steel producer Ă˘â‚¬â€ť prefer methods without ElectronicComponents.
            best_simple_steel_method(start_year, registries)
                .unwrap_or_else(|| best_registry_method(sector, start_year, registries))
        }
    } else {
        best_registry_method(sector, start_year, registries)
    };
    // Phase 42: Genesis Labor Fix — pre-populate workforce and inject payroll grant.
    let initial_wage = (company_liquid * 0.6 / (actual_capacity as f64).max(1.0)).max(50.0);
    let initial_fte = (actual_capacity as f64 * 0.6).round().max(2.0); // Phase 43: min 2.0 FTE floor
    company.fulfilled_fte = initial_fte as u32;
    company.prev_fulfilled_fte = initial_fte as u32;
    let payroll_grant = initial_fte * initial_wage * 3.0;
    company.available_cash += payroll_grant;
    let current_employment = (initial_fte / scale_factor as f64) as u32;
    let building_id = idgen.next_building();

    let fixed_assets = seed_fixed_assets(sector, start_year, rng);
    let (inventory, seed_cost) = seed_inventory(&method, base_capacity, sector);
    let inventory_capacity = (base_capacity as f64 * 10.0).max(100.0);

    // Phase 27: Deduct seed inventory cost from company's liquid capital.
    let deductible = seed_cost.min(company.liquid_capital * 0.5);
    company.liquid_capital -= deductible;
    company.available_cash -= deductible;
    company.extra.insert("seed_inventory_cost".to_string(), Value::from(deductible));

    let building = Building {
        id: building_id.clone(),
        name: building_name,
        owner_id: company_id.clone(),
        year_built: start_year.saturating_sub(rng.gen_range(1..30)),
        sector,
        worker_capacity: base_capacity,
        current_employment: current_employment.min(base_capacity),
        reserve: company_fixed * 0.05,
        active_method: method,
        accidents_last_year: 0,
        strike: false,
        scale_factor,
        building_capacity: base_capacity,
        region_id: region.id.clone(),
        cluster_info: ClusterInfo {
            region_id: region.id.clone(),
            scale_factor,
            sector,
            owner_id: company_id.clone(),
            extra: Map::new(),
        },
        last_production: BTreeMap::new(),
        last_profit: 0.0,
        last_fulfillment_ratio: 1.0,
        condition: 1.0,
        is_heritage_site: false,
        experience_level: None,
        aggregated_stats: AggregatedStats::default(),
        structural_defect: 0.0, land_hectares: 0.0,
        extra: Map::new(),
        inventory,
        inventory_capacity,
        active_project: None,
        landfill_state: None,
        deposit_id: None,
        fixed_assets,
        pending_method_upgrade: None,
        active_emission_control: String::new(),
    };

    company.building_ids.push(building_id);
    company.aggregated_stats.total_employment = current_employment * scale_factor;
    (company, building)
}

/// Phase 20C: Seed a fixed-asset cohort so factories have machinery on turn 1.
///
/// # Rules
/// * Machinery-type sectors get the machinery they produce (self-supply).
/// * Non-machinery sectors get IndustrialMachinery as a generic capital good.
/// * Cohorts are legacy (no blueprint), quality 0.8, condition 0.7-1.0.
fn seed_fixed_assets(
    sector: Sector,
    start_year: u32,
    rng: &mut impl Rng,
) -> Vec<FixedAssetCohort> {
    // Phase 45: Era-aware fixed asset seeding.
    // Pre-tractor agriculture uses DraftAnimals, not AgriculturalMachinery.
    // Pre-truck logistics uses DraftAnimals; rail era uses Trains; truck era uses Trucks.
    let machinery_commodity = match sector {
        Sector::HeavyIndustry => Commodity::IndustrialMachinery,
        Sector::Construction => Commodity::ConstructionMachinery,
        Sector::Agriculture => {
            if start_year < 1920 {
                Commodity::DraftAnimals
            } else {
                Commodity::AgriculturalMachinery
            }
        }
        Sector::PublicServices | Sector::PublicAdministration | Sector::Banking => Commodity::OfficeMachinery,
        Sector::TransportLogistics | Sector::ExportServices => {
            if start_year < 1900 {
                Commodity::DraftAnimals
            } else if start_year < 1930 {
                Commodity::Trains
            } else {
                Commodity::Trucks
            }
        }
        _ => Commodity::IndustrialMachinery,
    };

    let count = match sector {
        Sector::HeavyIndustry | Sector::Mining | Sector::Energy => 8.0,
        Sector::Construction | Sector::Agriculture => 5.0,
        Sector::LightIndustry | Sector::ArmamentsIndustry => 5.0,
        Sector::TransportLogistics => 4.0,
        _ => 3.0,
    };

    vec![FixedAssetCohort {
        blueprint_id: "legacy_seed".to_string(),
        commodity: machinery_commodity,
        count,
        condition: 0.7 + rng.gen::<f64>() * 0.3,
        quality: 0.8,
        durability: 240.0,
        base_tech: "legacy".to_string(),
        base_tech_year: start_year.saturating_sub(20),
        acquired_turn: 0,
    }]
}

/// Phase 20C: Seed one production cycle of inputs into a building's inventory.
///
/// This prevents first-turn production starvation Ă˘â‚¬â€ť buildings can produce
/// immediately without waiting for B2B market clearing.
/// Phase 27: Seed initial inventory and compute the cost.
///
/// Returns `(inventory, total_cost)` where `total_cost` is the value of the
/// seeded goods at estimated base prices. This cost must be deducted from the
/// company's `liquid_capital` and credited to `country.budget.liquid_reserves`
/// to maintain double-entry accounting.
/// Emergency Stabilization: Number of turns of input inventory to seed at world
/// generation. Reduced from 5.0 to 2.0 (1 month) because the September harvest
/// start provides organic food supply and B2B trade establishes within 2 turns.
/// Oversupplying raw materials at start distorts early market prices.
const SEED_INVENTORY_TURNS: f64 = 2.0;
const AGRICULTURE_SEED_INVENTORY_TURNS: f64 = 4.0;

fn seed_inventory(
    method: &ActiveProductionMethod,
    building_capacity: u32,
    sector: Sector,
) -> (BTreeMap<Commodity, f64>, f64) {
    let production_scale = building_capacity as f64 / 1000.0;
    let mut inventory = BTreeMap::new();
    let mut total_cost = 0.0;

    // Phase 87+: Agriculture gets 4 turns of seed inventory (vs 2 for other
    // sectors) because the harvest cycle is 3-6 turns away. This is a physical
    // commodity buffer, not a cash flow — the seed cost is already deducted
    // from liquid_capital and credited to the treasury (double-entry).
    let seed_turns = match sector {
        Sector::Agriculture => AGRICULTURE_SEED_INVENTORY_TURNS,
        _ => SEED_INVENTORY_TURNS,
    };

    // Stabilization Sprint: Seed seed_turns turns of inputs (was 1x).
    // Without this buffer, companies exhaust inventory by Turn 2 and cannot
    // produce, causing the Tabula Rasa crash.
    for (&commodity, &qty_per_1k) in &method.inputs {
        if commodity.is_fixed_asset() {
            continue;
        }
        // Stabilization Sprint: Skip local utility commodities (Energy, Heat,
        // Water, WasteUtility) -- these are delivered by physical grids, not
        // stored in building inventory.
        if commodity.is_local_utility() {
            continue;
        }
        let seed_qty = qty_per_1k * production_scale * seed_turns;
        if seed_qty > 0.0 {
            let unit_cost = estimated_base_price(commodity);
            total_cost += seed_qty * unit_cost;
            inventory.insert(commodity, seed_qty);
        }
    }

    // Stabilization Sprint: Seed 1 turn of output inventory for ALL sectors
    // (was transport-only). Companies start with existing stock to sell on
    // Turn 1, breaking the "no VWAP -> no asks -> no trades -> no VWAP" deadlock.
    // Also skip local utility commodities -- they are grid-managed, not tradable.
    for (&commodity, &qty_per_1k) in &method.outputs {
        if commodity.is_fixed_asset() {
            continue;
        }
        if commodity.is_local_utility() {
            continue;
        }
        let seed_qty = qty_per_1k * production_scale;
        if seed_qty > 0.0 {
            let unit_cost = estimated_base_price(commodity);
            total_cost += seed_qty * unit_cost;
            *inventory.entry(commodity).or_insert(0.0) += seed_qty;
        }
    }

    // Also seed a small amount of Food for worker subsistence
    let food_qty = production_scale * 5.0;
    let food_cost = food_qty * estimated_base_price(Commodity::Food);
    total_cost += food_cost;
    inventory.entry(Commodity::Food).or_insert(food_qty);

    (inventory, total_cost)
}

/// Phase 27: Estimated base price for a commodity, used for seed inventory
/// cost calculation. Returns a rough unit price Ă˘â‚¬â€ť not the actual market price.
pub fn estimated_base_price(commodity: Commodity) -> f64 {
    match commodity {
        Commodity::Food => 50.0,
        Commodity::Fuels => 80.0,
        Commodity::Energy => 100.0,
        Commodity::Water => 10.0,
        Commodity::HardCoal => 60.0,
        Commodity::Iron => 120.0,
        Commodity::Copper => 200.0,
        Commodity::Oil => 90.0,
        Commodity::NaturalGas => 70.0,
        Commodity::Steel => 300.0,
        Commodity::MechanicalComponents => 250.0,
        Commodity::ElectronicComponents => 400.0,
        Commodity::IndustrialMachinery => 1000.0,
        Commodity::ConstructionMachinery => 800.0,
        Commodity::AgriculturalMachinery => 600.0,
        Commodity::Chemicals => 150.0,
        Commodity::Tin => 180.0,
        Commodity::Bauxite => 100.0,
        Commodity::Sand | Commodity::Gravel | Commodity::Stone => 20.0,
        Commodity::Clay | Commodity::Limestone => 25.0,
        Commodity::Sulfur | Commodity::Salt => 40.0,
        Commodity::Zinc | Commodity::Lead => 150.0,
        Commodity::Heat => 30.0,
        Commodity::FreightCapacity => 50.0,
        _ => 100.0, // Generic fallback
    }
}

// ============================================================================
// STABILIZATION SPRINT: AGRICULTURE 2.0 ACTIVATION
// ============================================================================

/// Stabilization Sprint: Soil fertility index from soil class string.
/// Mirrors the mapping in geography.rs (lines 2314-2320).
fn soil_fertility_index(soil_class: &str) -> f64 {
    match soil_class {
        "Class_I" => 1.0,
        "Class_II" => 0.9,
        "Class_III" => 0.75,
        "Class_IV" => 0.6,
        "Class_V" => 0.4,
        "Class_VI" => 0.2,
        _ => 0.5, // Unknown soil class — moderate fallback
    }
}

/// Stabilization Sprint: Initialize agricultural profiles for all agriculture
/// companies, linking them to physical parcels from the Cadastre.
///
/// This function activates the dormant Agriculture 2.0 system by:
/// 1. Assigning Cadastre parcels to each agriculture company based on region.
/// 2. Computing arable_land_hectares from parcel sizes weighted by soil fertility.
/// 3. Creating CropBatch entries with monoculture crop designation.
/// 4. Setting company.agricultural_profile = Some(...).
///
/// # Rules
/// * Only Sector::Agriculture companies are processed.
/// * Parcels with zoning Agricultural or Unplanned and owner_type State are
///   eligible for assignment.
/// * Each parcel is assigned to exactly one company (no double-assignment).
/// * Crop batches start in CropState::Idle (will transition via the turn loop).
/// * Plantation crops (cotton, orchard, tobacco, cattle) use plantation_hectares.
fn initialize_agricultural_profiles(
    companies: &mut [Company],
    cadastre: &mut crate::society::cadastre::Cadastre,
    region_climates: &HashMap<String, ClimateProfile>,
    registries: &Registries,
) {
    use crate::entities::AgriculturalProfile;
    use crate::society::cadastre::{ParcelOwnerType, ZoningDesignation, parcel_id_to_index};

    // Collect parcel indices by region for assignment.
    // Only Agricultural or Unplanned parcels currently owned by State are eligible.
    let mut available_parcels_by_region: HashMap<String, Vec<crate::society::cadastre::ParcelId>> = HashMap::new();
    for (parcel_id, parcel) in cadastre.iter() {
        let eligible = matches!(parcel.zoning, ZoningDesignation::Agricultural | ZoningDesignation::Unplanned)
            && matches!(parcel.owner_type, ParcelOwnerType::State);
        if eligible {
            available_parcels_by_region
                .entry(parcel.region_id.clone())
                .or_default()
                .push(parcel_id);
        }
    }

    for company in companies.iter_mut() {
        if company.sector != Sector::Agriculture {
            continue;
        }

        let region_id = &company.region_id;
        let available = match available_parcels_by_region.get_mut(region_id) {
            Some(parcels) if !parcels.is_empty() => parcels,
            _ => continue, // No parcels available for this region
        };

        // Assign parcels to this company. Assign up to a reasonable farm size
        // based on worker capacity (5 hectares per worker for manual farming).
        let target_hectares = (company.worker_capacity as f64) * 5.0;
        let mut assigned_parcel_indices: Vec<u32> = Vec::new();
        let mut total_arable_hectares = 0.0;
        let mut total_plantation_hectares = 0.0;

        while total_arable_hectares + total_plantation_hectares < target_hectares {
            let parcel_id = match available.pop() {
                Some(id) => id,
                None => break, // No more parcels available
            };

            // Update the parcel ownership on the cadastre.
            if let Some(parcel) = cadastre.get_mut(parcel_id) {
                parcel.owner_id = company.id.clone();
                parcel.owner_type = ParcelOwnerType::Corporate;
                parcel.usufruct_holder = Some(company.id.clone());

                let fertility = soil_fertility_index(&parcel.soil_class);
                let effective_hectares = parcel.size_hectares * fertility;

                // Class IV-VI soil is better suited for pasture/plantation
                // (lower fertility, suitable for livestock or perennials).
                if fertility < 0.6 {
                    total_plantation_hectares += effective_hectares;
                } else {
                    total_arable_hectares += effective_hectares;
                }
                assigned_parcel_indices.push(parcel_id_to_index(parcel_id));
            }
        }

        if assigned_parcel_indices.is_empty() {
            continue;
        }

        // Create monoculture crop batches based on the region's climate.
        // World Generation & Climate Audit (v0.5.3): Now passes the actual
        // region climate profile to select climate-appropriate crops.
        let climate_profile = region_climates
            .get(region_id)
            .copied()
            .unwrap_or(ClimateProfile::Temperate);
        let batches = build_crop_batches(
            total_arable_hectares,
            total_plantation_hectares,
            climate_profile,
            registries,
        );

        company.agricultural_profile = Some(AgriculturalProfile {
            arable_land_hectares: total_arable_hectares,
            plantation_hectares: total_plantation_hectares,
            batches,
            owned_parcel_ids: assigned_parcel_indices,
        });
    }
}

/// Stabilization Sprint: Build crop batches with monoculture designation.
///
/// Allocates arable land across cereal, vegetable, and fodder crops using
/// the CROP_*_RATIO constants. Plantation land is assigned to cattle/orchard.
///
/// Emergency Stabilization: Crop batches start in CropState::Growing with
/// active_hectares set to planned_hectares. This is because the game now
/// starts in September (autumn harvest season) — the crops are already in
/// the field and ready to harvest at turns 1-3. Without this pre-seeding,
/// the first harvest wouldn't occur until the following year.
///
/// World Generation & Climate Audit (v0.5.3):
/// * Now accepts `climate_profile` to select climate-appropriate crops.
/// * Pre-injects `accumulated_yield` for pre-seeded crops so the Turn 1
///   harvest actually produces physical commodities. The accumulated yield
///   represents the full growing-season biomass that has accumulated by
///   September (the game start month).
fn build_crop_batches(
    arable_hectares: f64,
    plantation_hectares: f64,
    climate_profile: ClimateProfile,
    registries: &Registries,
) -> Vec<CropBatch> {
    use ClimateProfile as CP;

    // Select crop sets based on climate compatibility.
    // Each tuple is (crop_id, land_type, planted_turn_for_pre_seed).
    // planted_turn represents when the crop was sown in the PREVIOUS year
    // so it's ready for harvest at the September start.
    let arable_crop_sets: &[(&str, u32)] = match climate_profile {
        CP::Tropical | CP::Coastal => &[
            ("rice", 13),
            ("soybeans", 13),
            ("potatoes", 13),
        ],
        CP::SubTropical => &[
            ("rice", 13),
            ("corn", 13),
            ("soybeans", 13),
        ],
        CP::Temperate | CP::Continental => &[
            ("wheat", 13),
            ("corn", 13),
            ("potatoes", 13),
            ("soybeans", 13),
        ],
        CP::Mountainous => &[
            ("potatoes", 13),
            ("alfalfa", 11),
        ],
        CP::Desert => &[
            ("soybeans", 13),
        ],
        CP::Arctic => &[],
    };

    let plantation_crop_sets: &[(&str, u32)] = match climate_profile {
        CP::Tropical | CP::Coastal => &[
            ("sugarcane", 1),
            ("coffee", 1),
            ("cattle", 1),
            ("orchard", 1),
        ],
        CP::SubTropical => &[
            ("sugarcane", 1),
            ("citrus", 1),
            ("olives", 1),
            ("cattle", 1),
        ],
        CP::Temperate | CP::Continental | CP::Mountainous => &[
            ("cattle", 1),
            ("orchard", 1),
            ("tobacco", 1),
        ],
        CP::Desert => &[
            ("cattle", 1),
        ],
        CP::Arctic => &[],
    };

    let mut batches = Vec::new();

    // Distribute arable land across compatible arable crops.
    if !arable_crop_sets.is_empty() && arable_hectares > 0.0 {
        let per_crop_hectares = arable_hectares / arable_crop_sets.len() as f64;
        for &(crop_id, planted_turn) in arable_crop_sets {
            if per_crop_hectares <= 0.0 {
                continue;
            }
            if let Some(crop_def) = registries.crops.get(crop_id) {
                // Pre-calculate accumulated_yield: the full growing-season
                // biomass that has accumulated by September (game start).
                // Formula: active_hectares * sum(tons_per_hectare for all yields)
                let total_yield_per_hectare: f64 = crop_def.yields.values().sum();
                let pre_accumulated = per_crop_hectares * total_yield_per_hectare;

                batches.push(CropBatch {
                    crop_id: crop_id.to_string(),
                    planned_hectares: per_crop_hectares,
                    active_hectares: per_crop_hectares,
                    state: CropState::Growing,
                    planted_turn,
                    accumulated_yield: pre_accumulated,
                    rot_accumulator: 0.0,
                });
            }
        }
    }

    // Distribute plantation land across compatible plantation crops.
    if !plantation_crop_sets.is_empty() && plantation_hectares > 0.0 {
        let per_crop_hectares = plantation_hectares / plantation_crop_sets.len() as f64;
        for &(crop_id, planted_turn) in plantation_crop_sets {
            if per_crop_hectares <= 0.0 {
                continue;
            }
            if let Some(crop_def) = registries.crops.get(crop_id) {
                // Plantation crops are perennial — pre-accumulate yield
                // representing the current season's growth.
                let total_yield_per_hectare: f64 = crop_def.yields.values().sum();
                let pre_accumulated = per_crop_hectares * total_yield_per_hectare;

                batches.push(CropBatch {
                    crop_id: crop_id.to_string(),
                    planned_hectares: per_crop_hectares,
                    active_hectares: per_crop_hectares,
                    state: CropState::Growing,
                    planted_turn,
                    accumulated_yield: pre_accumulated,
                    rot_accumulator: 0.0,
                });
            }
        }
    }

    batches
}

/// Creates a Strategic Reserve Agency for the country (Phase 2, Phase 79).
///
/// Phase 79 changes:
/// * Commodity keys use English snake_case (parseable by `Commodity::from_str()`).
/// * Energy removed — the SRA stockpiles Fuels, not Energy. Energy storage is
///   handled by dedicated `PumpedStoragePlant` / `BatteryBank` buildings.
/// * Mandate expanded to 8 commodities: Food, HardCoal, BrownCoal, Peat, Oil,
///   Ammunition, Steel, Pharmaceuticals.
/// * Triggers are ratio-based relative to moving-average VWAP (no magic numbers).
/// * Physical warehouse buildings are generated and assigned to the SRA.
/// * Capacity and budget scale to `total_population` and `average_wage`.
///
/// # Arguments
/// * `country` - The country to create the agency for
/// * `start_year` - The starting year for the simulation
/// * `total_population` - Total population across all regions
/// * `average_wage` - Current average wage for macro-scaling
/// * `regions` - Country regions for distributed warehouse placement
/// * `idgen` - ID generator for building IDs
///
/// # Returns
/// * A tuple of (Company with LegalForm::StrategicReserveAgency, Vec<Building> warehouses)
fn create_strategic_reserve_agency(
    country: &Country,
    start_year: u32,
    total_population: i64,
    average_wage: f64,
    regions: &[Region],
    idgen: &mut IdGen,
    registries: &Registries,
) -> (Company, Vec<Building>) {
    let agency_id = format!("STRATEGIC_RESERVE_{}", country.name);

    // Phase 79: 8-commodity mandate with ratio-based triggers.
    // Buy when price < 0.75 * moving_avg_vwap (glut), sell when price > 1.5 * moving_avg_vwap (shock).
    // surplus_threshold and deficit_threshold are scaled to total_population.
    let pop_f = total_population.max(1) as f64;
    let surplus_scale = pop_f * 0.01;  // 1% of population as surplus/deficit threshold
    let deficit_scale = pop_f * 0.005; // 0.5% of population as deficit threshold

    // (commodity_key, budget_fraction, surplus_threshold_factor, deficit_threshold_factor)
    let commodity_config: &[(&str, f64, f64, f64)] = &[
        ("food",            0.15, 1.0,  1.0),  // basic survival
        ("hard_coal",       0.12, 0.5,  0.5),  // primary industrial fuel
        ("brown_coal",      0.08, 0.5,  0.5),  // lower-tier fuel
        ("peat",            0.05, 0.3,  0.3),  // lowest-tier fuel
        ("oil",             0.15, 0.3,  0.3),  // strategic military/industrial fuel
        ("ammunition",      0.15, 0.2,  0.2),  // defense
        ("steel",           0.15, 0.3,  0.3),  // industry
        ("pharmaceuticals", 0.15, 0.2,  0.2),  // healthcare
    ];

    let mut purchase_triggers = BTreeMap::new();
    let mut release_triggers = BTreeMap::new();

    for &(key, budget_frac, surp_factor, def_factor) in commodity_config {
        purchase_triggers.insert(
            key.to_string(),
            PurchaseTrigger {
                buy_threshold_ratio: 0.75,  // Buy when price < 75% of moving avg VWAP
                surplus_threshold: surplus_scale * surp_factor,
                budget_fraction: budget_frac,
            },
        );
        release_triggers.insert(
            key.to_string(),
            ReleaseTrigger {
                sell_threshold_ratio: 1.5,  // Release when price > 150% of moving avg VWAP
                deficit_threshold: deficit_scale * def_factor,
                release_fraction: 0.5,
            },
        );
    }

    // Phase 79: Physical warehouse generation.
    // One warehouse per region, capacity scaled to population and wage.
    // Total capacity = num_regions * (pop_per_region * wage * 0.1)
    let num_regions = regions.len().max(1);
    let pop_per_region = pop_f / num_regions as f64;
    let capacity_per_warehouse = (pop_per_region * average_wage * 0.1).max(1000.0);
    let total_warehouse_capacity = capacity_per_warehouse * num_regions as f64;

    // Distribute total capacity across 8 commodities (percentages sum to 1.0).
    let capacity_shares: &[(&str, f64)] = &[
        ("food",            0.25),
        ("hard_coal",       0.15),
        ("brown_coal",      0.10),
        ("peat",            0.05),
        ("oil",             0.15),
        ("ammunition",      0.10),
        ("steel",           0.10),
        ("pharmaceuticals", 0.10),
    ];
    let mut max_capacity = BTreeMap::new();
    for &(key, share) in capacity_shares {
        max_capacity.insert(key.to_string(), total_warehouse_capacity * share);
    }

    // Generate warehouse buildings — one per region.
    let mut warehouses = Vec::new();
    for region in regions {
        let building_id = idgen.next_building();
        let staff = ((capacity_per_warehouse / 10000.0).clamp(50.0, 500.0)) as u32;
        let building = Building {
            id: building_id,
            name: "Strategic Reserve Warehouse".to_string(),
            owner_id: agency_id.clone(),
            year_built: start_year.saturating_sub(5),
            sector: Sector::PublicServices,
            worker_capacity: staff,
            current_employment: (staff as f64 * 0.9) as u32,
            reserve: 0.0,
            active_method: ActiveProductionMethod::default(),
            accidents_last_year: 0,
            strike: false,
            scale_factor: 1,
            building_capacity: staff,
            region_id: region.id.clone(),
            cluster_info: ClusterInfo {
                region_id: region.id.clone(),
                scale_factor: 1,
                sector: Sector::PublicServices,
                owner_id: agency_id.clone(),
                extra: Map::new(),
            },
            last_production: BTreeMap::new(),
            last_profit: 0.0,
            last_fulfillment_ratio: 1.0,
            condition: 1.0,
            is_heritage_site: false,
            experience_level: None,
            aggregated_stats: AggregatedStats::default(),
            structural_defect: 0.0,
            land_hectares: 0.0,
            extra: Map::new(),
            inventory: BTreeMap::new(),
            inventory_capacity: capacity_per_warehouse,
            active_project: None,
            landfill_state: None,
            deposit_id: None,
            fixed_assets: Vec::new(),
            pending_method_upgrade: None,
            active_emission_control: String::new(),
        };
        warehouses.push(building);
    }

    // Phase 79: 2.7 — Optionally generate energy storage buildings for grid stabilization.
    // Pumped Storage Plant (available 1907) and Battery Bank Storage (available 1990).
    // These are production buildings that buffer grid energy with round-trip losses,
    // replacing the broken approach of hoarding Energy in warehouses.
    if start_year >= 1907 && !regions.is_empty() {
        let storage_region = &regions[0];
        let storage_id = idgen.next_building();
        let storage_staff = 80u32;
        // Look up the "Pumped Storage Plant" method from the energy sector registry.
        let storage_method = find_storage_method_by_name(registries, "energy", "Pumped Storage Plant", start_year)
            .unwrap_or_else(|| method_from_ratios(
                0.15, 0.30, 0.55,
                BTreeMap::from([
                    (Commodity::Energy, 100.0),
                    (Commodity::Water, 20.0),
                    (Commodity::MechanicalComponents, 5.0),
                ]),
                BTreeMap::from([(Commodity::Energy, 72.0)]),
                1907,
            ));
        warehouses.push(Building {
            id: storage_id,
            name: "Pumped Storage Plant".to_string(),
            owner_id: agency_id.clone(),
            year_built: start_year.saturating_sub(5),
            sector: Sector::PublicServices,
            worker_capacity: storage_staff,
            current_employment: (storage_staff as f64 * 0.9) as u32,
            reserve: 0.0,
            active_method: storage_method,
            accidents_last_year: 0,
            strike: false,
            scale_factor: 1,
            building_capacity: storage_staff,
            region_id: storage_region.id.clone(),
            cluster_info: ClusterInfo {
                region_id: storage_region.id.clone(),
                scale_factor: 1,
                sector: Sector::PublicServices,
                owner_id: agency_id.clone(),
                extra: Map::new(),
            },
            last_production: BTreeMap::new(),
            last_profit: 0.0,
            last_fulfillment_ratio: 1.0,
            condition: 1.0,
            is_heritage_site: false,
            experience_level: None,
            aggregated_stats: AggregatedStats::default(),
            structural_defect: 0.0,
            land_hectares: 0.0,
            extra: Map::new(),
            inventory: BTreeMap::new(),
            inventory_capacity: 0.0, // Storage buildings don't store commodities
            active_project: None,
            landfill_state: None,
            deposit_id: None,
            fixed_assets: Vec::new(),
            pending_method_upgrade: None,
            active_emission_control: String::new(),
        });
    }

    let budget_alloc = country.budget.nominal_budget * 0.05;

    let company = Company {
        id: agency_id,
        file_stem: "strategic_reserve".to_string(),
        name: "Strategic Reserve Agency".to_string(),
        sector: Sector::PublicServices,
        region_id: country.name.clone(),
        legal_form: LegalForm::StrategicReserveAgency(StrategicReserveData {
            commodity_reserves: BTreeMap::new(),
            purchase_triggers,
            release_triggers,
            budget_allocation: budget_alloc,
            max_capacity,
        }),
        state_share: 1.0,
        fixed_capital: 0.0,
        liquid_capital: budget_alloc,
        available_cash: 0.0,
        debit_cash: 0.0,
        credit_cash: 0.0,
        unfilled_bid_prices: std::collections::HashMap::new(),
        liabilities: 0.0,
        company_capital: budget_alloc,
        shares_count: 0,
        share_price: 0.0,
        shareholders: BTreeMap::new(),
        price_history: Vec::new(),
        financial_history: Vec::new(),
        safety_level: 0.5,
        union_id: None,
        building_ids: Vec::new(), // Will be set by caller
        scale_factor: 1,
        worker_capacity: 0,
        is_national_champion: false,
        is_listed: false,
        owners: BTreeMap::new(),
        free_float: 0.0,
        aggregated_stats: crate::entities::AggregatedStats::default(),
        bank_type: None,
        balance_sheet: None,
        loan_margin: None,
        brokerage_account: None,
        primary_bank_id: None, outstanding_loan_bank_id: None,
        fund_type: None,
        fund_ledger: None,
        temporary_disruption_modifier: 0.0,
        target_fte_demand: 0,
        offered_wage_per_fte: 0.0,
        prev_offered_wage_per_fte: 0.0,
        wage_arrears: 0.0,
        severance_arrears: 0.0,
        furlough_turns_accumulated: 0,
        productivity_penalty: 0.0,
        target_wage: 0.0,
        is_striking: false,
        fulfilled_fte: 0,
        prev_fulfilled_fte: 0,
        physical_fte_demand: 0,
        is_in_receivership: false,
        agricultural_profile: None,
        rd_budget: 0.0,
        patents: Vec::new(),
        licensed_methods: Vec::new(),
        information_quality: None,
        shadow_employment: None,
        pending_expansion: None,
        blueprints: Vec::new(),
        licensed_blueprints: Vec::new(),
        reputation_score: 50.0, donation_history: Vec::new(), is_dspw: false, consumer_loans: Vec::new(),
        annual_profit_accumulator: 0.0,
        seasonal_profile: None,
        furloughed_workers_count: 0.0,
        ceo_vip_id: None,
        eps: 0.0, pe_ratio: 0.0, dividend_yield: 0.0, open_price: 0.0, close_price: 0.0,
        action_ledger: crate::entities::ActionLedger::default(),
        extra: Map::new(),
    };

    (company, warehouses)
}

fn state_building_recipe(name: &str, start_year: u32) -> (String, ActiveProductionMethod) {
    let year = start_year.saturating_sub(1);
    match name {
        "military_base" => (
            name.to_string(),
            method_from_ratios(
                0.15,
                0.35,
                0.50,
                BTreeMap::from([
                    (Commodity::Rifles, 5.0),
                    (Commodity::Ammunition, 10.0),
                    (Commodity::Fuels, 15.0),
                    (Commodity::Food, 20.0),
                    (Commodity::Clothing, 5.0),
                ]),
                BTreeMap::new(),
                year,
            ),
        ),
        "police_station" => (
            name.to_string(),
            method_from_ratios(
                0.10,
                0.60,
                0.30,
                BTreeMap::from([
                    (Commodity::Rifles, 1.0),
                    (Commodity::Cars, 5.0),
                    (Commodity::AdministrativeServices, 2.0),
                    (Commodity::Paper, 2.0),
                ]),
                BTreeMap::from([(Commodity::SecurityCapacity, 20.0)]),
                year,
            ),
        ),
        "courthouse" => (
            name.to_string(),
            method_from_ratios(
                0.40,
                0.40,
                0.20,
                BTreeMap::from([
                    (Commodity::Paper, 5.0),
                    (Commodity::AdministrativeServices, 5.0),
                ]),
                BTreeMap::from([(Commodity::JusticeCapacity, 18.0)]),
                year,
            ),
        ),
        "service_headquarters" => (
            name.to_string(),
            method_from_ratios(
                0.30,
                0.50,
                0.20,
                BTreeMap::from([
                    (Commodity::ElectronicComponents, 10.0),
                    (Commodity::Rifles, 2.0),
                    (Commodity::Cars, 3.0),
                    (Commodity::AdministrativeServices, 5.0),
                ]),
                BTreeMap::new(),
                year,
            ),
        ),
        "intelligence_hq" => (
            name.to_string(),
            method_from_ratios(
                0.30,
                0.50,
                0.20,
                BTreeMap::from([
                    (Commodity::ElectronicComponents, 10.0),
                    (Commodity::Rifles, 2.0),
                    (Commodity::Cars, 3.0),
                    (Commodity::AdministrativeServices, 5.0),
                ]),
                BTreeMap::from([(Commodity::IntelligenceCapacity, 8.0)]),
                year,
            ),
        ),
        "border_guard" => (
            name.to_string(),
            method_from_ratios(
                0.05,
                0.25,
                0.70,
                BTreeMap::from([
                    (Commodity::Food, 10.0),
                    (Commodity::Rifles, 1.0),
                ]),
                BTreeMap::from([(Commodity::BorderEnforcementCapacity, 10.0)]),
                year,
            ),
        ),
        "customs_office" => (
            name.to_string(),
            method_from_ratios(
                0.10,
                0.30,
                0.60,
                BTreeMap::from([
                    (Commodity::Food, 8.0),
                    (Commodity::Paper, 2.0),
                ]),
                BTreeMap::from([(Commodity::CustomsCapacity, 10.0)]),
                year,
            ),
        ),
        "university" => (
            name.to_string(),
            method_from_ratios(
                0.40,
                0.40,
                0.20,
                BTreeMap::from([
                    (Commodity::Paper, 20.0),
                    (Commodity::Chemicals, 10.0),
                ]),
                BTreeMap::from([(Commodity::InnovationPoints, 5.0)]),
                year,
            ),
        ),
        "monastery_scriptorium" => (
            name.to_string(),
            method_from_ratios(
                0.20,
                0.50,
                0.30,
                BTreeMap::from([
                    (Commodity::Paper, 5.0),
                ]),
                BTreeMap::from([(Commodity::ReligiousTexts, 5.0)]),
                year,
            ),
        ),
        _ => (name.to_string(), method_from_ratios(0.10, 0.40, 0.50, BTreeMap::new(), BTreeMap::new(), year)),
    }
}

/// Phase 25: Generate retail stores in each region.
///
/// Creates one retail store per region owned by a LocalServices company.
/// Each store is seeded with initial inventory of consumer goods so that
/// B2C market clearing can happen on Turn 1. Without retail stores, GDP
/// (which is largely final consumption) stays at 0 forever.
fn generate_retail_stores(
    data_dir: &Path,
    country: &Country,
    country_regions: &[&Region],
    start_year: u32,
    idgen: &mut IdGen,
    _rng: &mut impl Rng,
) -> Result<(), Box<dyn Error>> {
    use crate::society::housing::{
        CommercialBuilding, InventoryBatch, RetailProfile,
        UtilityConnections, StorageType,
    };
    use crate::registries::enums::Commodity;

    if country_regions.is_empty() {
        return Ok(());
    }

    let mut retail_buildings: Vec<CommercialBuilding> = Vec::new();
    let mut retail_companies: Vec<Company> = Vec::new();

    // Era-appropriate consumption method defaults
    let default_lighting = if start_year >= 2000 {
        "LED Lighting".to_string()
    } else if start_year >= 1940 {
        "Fluorescent Tubes".to_string()
    } else if start_year >= 1900 {
        "Incandescent Bulbs".to_string()
    } else if start_year >= 1890 {
        "Gas Mantle".to_string()
    } else if start_year >= 1860 {
        "Kerosene Lamps".to_string()
    } else {
        "None".to_string()
    };

    let default_heating = if start_year >= 1980 {
        "Heat Pump".to_string()
    } else if start_year >= 1930 {
        "District Heating".to_string()
    } else if start_year >= 1920 {
        "Electric Radiator".to_string()
    } else if start_year >= 1900 {
        "Oil Heater".to_string()
    } else if start_year >= 1850 {
        "Coal Stove".to_string()
    } else {
        "None".to_string()
    };

    let default_power_generation = if start_year >= 2010 {
        "Rooftop PV + Battery".to_string()
    } else if start_year >= 2000 {
        "Rooftop PV".to_string()
    } else {
        "None".to_string()
    };

    for region in country_regions {
        let region_pop = region.population.max(1000) as f64;

        // Phase 47: Select retail format based on development, wealth, era, capital.
        let wealth_per_capita = {
            let total_savings: f64 = region
                .class_demographics
                .rural_classes
                .values()
                .chain(region.class_demographics.urban_classes.values())
                .map(|d| d.savings)
                .sum::<f64>();
            let total_pop: f64 = region
                .class_demographics
                .rural_classes
                .values()
                .chain(region.class_demographics.urban_classes.values())
                .map(|d| d.population as f64)
                .sum::<f64>()
                .max(1.0);
            total_savings / total_pop
        };
        let format_spec = crate::economy::trade::retail_registry::select_retail_format(
            region.development_level,
            start_year,
            region.is_capital,
            wealth_per_capita,
        );

        // Create a retail company for this region
        let company_id = idgen.next_company();
        let building_id = idgen.next_building();

        let company_name = format!("Retail Co {} ({})", company_id, region.id);
        let company_fixed = region_pop * 10.0 * format_spec.upkeep_mult;
        let company_liquid = region_pop * 5.0 * format_spec.upkeep_mult;
        let base_capacity = (region_pop / 1000.0 * format_spec.capacity_mult).max(10.0) as u32;

        let mut company = Company {
            id: company_id.clone(),
            file_stem: "local_services".to_string(),
            name: company_name,
            sector: Sector::LocalServices,
            region_id: region.id.clone(),
            legal_form: crate::entities::legal_form::LegalForm::FamilyBusiness(
                crate::entities::legal_form::FamilyBusinessData::default(),
            ),
            state_share: 0.0,
            fixed_capital: company_fixed,
            liquid_capital: company_liquid,
            available_cash: 0.0, // Phase 42: Set below with payroll grant
            debit_cash: 0.0,
            credit_cash: 0.0,
            unfilled_bid_prices: std::collections::HashMap::new(),
            liabilities: 0.0,
            company_capital: company_fixed + company_liquid,
            shares_count: 1000,
            share_price: (company_fixed + company_liquid) / 1000.0,
            shareholders: BTreeMap::new(),
            price_history: Vec::new(),
            financial_history: Vec::new(),
            safety_level: 0.5,
            union_id: None,
            building_ids: vec![building_id.clone()],
            scale_factor: 1,
            worker_capacity: base_capacity,
            is_national_champion: false,
            is_listed: false,
            owners: BTreeMap::new(),
            free_float: 0.0,
            aggregated_stats: crate::entities::AggregatedStats::default(),
            bank_type: None,
            balance_sheet: None,
            loan_margin: None,
            brokerage_account: None,
            primary_bank_id: None,
            outstanding_loan_bank_id: None,
            fund_type: None,
            fund_ledger: None,
            temporary_disruption_modifier: 0.0,
            target_fte_demand: base_capacity,
            offered_wage_per_fte: (company_liquid * 0.6 / (base_capacity as f64).max(1.0)).max(1.0),
            prev_offered_wage_per_fte: (company_liquid * 0.6 / (base_capacity as f64).max(1.0)).max(1.0).max(50.0),
            wage_arrears: 0.0,
            severance_arrears: 0.0,
            furlough_turns_accumulated: 0,
            productivity_penalty: 0.0,
            target_wage: (company_liquid * 0.6 / (base_capacity as f64).max(1.0)).max(50.0),
            is_striking: false,
            fulfilled_fte: 0,
            prev_fulfilled_fte: 0,
            physical_fte_demand: base_capacity,
            is_in_receivership: false,
            agricultural_profile: None,
            rd_budget: 0.0,
            patents: Vec::new(),
            licensed_methods: Vec::new(),
            information_quality: None,
            shadow_employment: None,
            pending_expansion: None,
            blueprints: Vec::new(),
            licensed_blueprints: Vec::new(),
            reputation_score: 50.0, donation_history: Vec::new(), is_dspw: false, consumer_loans: Vec::new(),
            annual_profit_accumulator: 0.0,
            seasonal_profile: None,
            furloughed_workers_count: 0.0,
            ceo_vip_id: None,
            eps: 0.0, pe_ratio: 0.0, dividend_yield: 0.0, open_price: 0.0, close_price: 0.0,
            action_ledger: crate::entities::ActionLedger::default(),
            extra: serde_json::Map::new(),
        };

        // Phase 42: Genesis Labor Fix — pre-populate workforce and inject payroll grant.
        let initial_wage = (company_liquid * 0.6 / (base_capacity as f64).max(1.0)).max(50.0);
        let initial_fte = (base_capacity as f64 * 0.6).round().max(2.0); // Phase 43: min 2.0 FTE floor
        company.fulfilled_fte = initial_fte as u32;
        company.prev_fulfilled_fte = initial_fte as u32;
        let payroll_grant = initial_fte * initial_wage * 3.0;
        company.available_cash = company_liquid + payroll_grant;

        // Seed initial inventory for the retail store
        let production_scale = base_capacity as f64 / 1000.0;
        let mut current_inventory: BTreeMap<String, Vec<InventoryBatch>> = BTreeMap::new();

        let seed_goods = [
            (Commodity::Cereal, 30.0 * production_scale.max(1.0)),
            (Commodity::Vegetable, 20.0 * production_scale.max(1.0)),
            (Commodity::Meat, 20.0 * production_scale.max(1.0)),
            (Commodity::Fruit, 8.0 * production_scale.max(1.0)),
            (Commodity::Clothing, 10.0 * production_scale.max(1.0)),
            (Commodity::Furniture, 5.0 * production_scale.max(1.0)),
            (Commodity::Food, 20.0 * production_scale.max(1.0)),
        ];

        for (commodity, qty) in &seed_goods {
            let key: String = (*commodity).into();
            current_inventory.insert(key, vec![InventoryBatch {
                quantity: *qty,
                storage_turn: 0,
                owner_id: company_id.clone(),
                accumulated_fees: 0.0,
                warehouse_id: building_id.clone(),
                fire_sale_discount: 0.0,
                acquisition_cost_per_unit: 100.0, // Base price
            }]);
        }

        let retail_profile = RetailProfile {
            profiles: format_spec.allowed_profiles.iter().cloned().collect(),
            base_attractiveness: format_spec.attractiveness,
            upgrades: std::collections::BTreeSet::new(),
            effective_attractiveness: format_spec.attractiveness,
            markup_ratio: format_spec.markup,
            landlord_building_id: None,
            leased_sqm: 0.0,
            units_sold_last_turn: std::collections::BTreeMap::new(),
            unmet_demand_last_turn: std::collections::BTreeMap::new(),
            market_share_last_turn: std::collections::BTreeMap::new(),
            first_active_turn: 0,
        };

        let retail_capacity = (region_pop * 0.01 * format_spec.capacity_mult).max(100.0);
        let storage_capacity = retail_capacity * 10.0;

        let commercial_building = CommercialBuilding {
            id: building_id.clone(),
            building_type: format_spec.building_type,
            micro_region_id: region.id.clone(),
            owner_id: company_id.clone(),
            office_capacity: 0.0,
            retail_capacity,
            tenants: Vec::new(),
            rent_per_sqm: 0.0,
            utility_connections: UtilityConnections::default(),
            storage_capacity,
            current_inventory,
            storage_type: StorageType::GeneralWarehouse,
            utilization_rate: 0.0,
            retail_profile: Some(retail_profile),
            shopping_center_profile: None,
            wholesale_profile: None,
            retail_leases: Vec::new(),
            fixed_assets: Vec::new(),
            active_lighting: default_lighting.clone(),
            active_heating: default_heating.clone(),
            active_power_generation: default_power_generation.clone(),
            active_water_supply: String::new(),
            active_sanitation: String::new(),
            active_waste_disposal: String::new(),
            pending_upgrade: None,
        };

        retail_buildings.push(commercial_building);
        retail_companies.push(company);
    }

    // Save retail companies
    let company_store = DiskEntityStore::<Company>::new(data_dir);
    company_store.save_sector(
        &country.name,
        "local_services",
        None,
        &retail_companies,
    )?;

    // Save retail commercial buildings
    let commercial_store =
        DiskEntityStore::<CommercialBuilding>::new(data_dir);
    commercial_store.save_sector(
        &country.name,
        "retail_store",
        None,
        &retail_buildings,
    )?;

    Ok(())
}

/// Phase 9: Generate tourism entities Ă˘â‚¬â€ť natural wonders, tourism destinations,
/// hospitality companies, and hotel/resort/restaurant/casino commercial buildings.
///
/// # Rules
/// * ~30% of regions get a natural wonder.
///* Each region with a wonder gets a tourism destination.
/// * 1-3 hospitality companies per destination, each owning 1+ commercial buildings.
/// * Commercial buildings are saved to `entities/<country>/commercial/`.
fn generate_tourism_entities(
    data_dir: &Path,
    country: &mut Country,
    country_regions: &[&Region],
    start_year: u32,
    idgen: &mut IdGen,
    rng: &mut impl Rng,
) -> Result<(), Box<dyn Error>> {
    use crate::society::housing::{CommercialBuilding, CommercialBuildingType, UtilityConnections};
    use crate::society::tourism::{NaturalWonder, TourismDestination, WonderType};

    if country_regions.is_empty() {
        return Ok(());
    }

    let wonder_types = [
        WonderType::Waterfall,
        WonderType::Geyser,
        WonderType::Beach,
        WonderType::MountainPeak,
        WonderType::Canyon,
        WonderType::Cave,
        WonderType::VolcanicCrater,
        WonderType::HotSpring,
        WonderType::AncientForest,
        WonderType::GeologicalFormation,
    ];

    let wonder_names = [
        "Crystal Falls",
        "Old Faithful",
        "Golden Bay",
        "Eagle Peak",
        "Deep Gorge",
        "Whispering Caves",
        "Fire Crater",
        "Silver Springs",
        "Ancient Grove",
        "Stone Arches",
    ];

    let mut wonders = Vec::new();
    let mut destinations = BTreeMap::new();
    let mut hospitality_companies: Vec<Company> = Vec::new();
    let mut tourism_buildings: Vec<CommercialBuilding> = Vec::new();

    // Era-appropriate consumption method defaults
    let default_lighting = if start_year >= 2000 {
        "LED Lighting".to_string()
    } else if start_year >= 1940 {
        "Fluorescent Tubes".to_string()
    } else if start_year >= 1900 {
        "Incandescent Bulbs".to_string()
    } else if start_year >= 1890 {
        "Gas Mantle".to_string()
    } else if start_year >= 1860 {
        "Kerosene Lamps".to_string()
    } else {
        "None".to_string()
    };

    let default_heating = if start_year >= 1980 {
        "Heat Pump".to_string()
    } else if start_year >= 1930 {
        "District Heating".to_string()
    } else if start_year >= 1920 {
        "Electric Radiator".to_string()
    } else if start_year >= 1900 {
        "Oil Heater".to_string()
    } else if start_year >= 1850 {
        "Coal Stove".to_string()
    } else {
        "None".to_string()
    };

    let default_power_generation = if start_year >= 2010 {
        "Rooftop PV + Battery".to_string()
    } else if start_year >= 2000 {
        "Rooftop PV".to_string()
    } else {
        "None".to_string()
    };

    for region in country_regions {
        // ~30% chance of getting a natural wonder
        if rng.gen::<f64>() > 0.3 {
            continue;
        }

        let wonder_idx = rng.gen_range(0..wonder_types.len());
        let wonder_type = wonder_types[wonder_idx];
        let wonder_name = format!("{} ({})", wonder_names[wonder_idx], region.id);

        let wonder = NaturalWonder {
            id: format!("WONDER-{}-{}", country.name[..3.min(country.name.len())].to_uppercase(), idgen.company_counter),
            name: wonder_name.clone(),
            wonder_type,
            region_id: region.id.clone(),
            health: 0.9,
            recreation_value: rng.gen_range(0.5..0.9),
            visitor_capacity: rng.gen_range(1000.0..5000.0),
            current_visitors: 0.0,
            pollution_sensitivity: rng.gen_range(0.2..0.6),
            restoration_cost: 5000.0,
        };

        let wonder_id = wonder.id.clone();
        wonders.push(wonder);

        // Create tourism destination for this region
        let dest_id = format!("DEST-{}-{}", country.name[..3.min(country.name.len())].to_uppercase(), region.id);
        let dest = TourismDestination {
            id: dest_id.clone(),
            region_id: region.id.clone(),
            name: format!("Tourism Region {}", region.id),
            natural_wonders: vec![wonder_id],
            forest_area: rng.gen_range(5000.0..50000.0),
            infrastructure_quality: rng.gen_range(0.5..0.8),
            accommodation_capacity: 0.0, // Will be derived from physical buildings
            visitor_satisfaction: 0.8,
            marketing_budget: 0.0,
        };
        destinations.insert(region.id.clone(), dest);

        // Create 1-3 hospitality companies for this destination
        let company_count = rng.gen_range(1..=3);
        let base_wage = country.macro_indicators.average_wage.max(1.0);

        for i in 0..company_count {
            let company_id = idgen.next_company();
            let building_types = [
                CommercialBuildingType::Hotel,
                CommercialBuildingType::Resort,
                CommercialBuildingType::Restaurant,
                CommercialBuildingType::Casino,
            ];
            let btype = building_types[rng.gen_range(0..building_types.len())];
            let building_id = idgen.next_building();

            let (office_cap, retail_cap) = match btype {
                CommercialBuildingType::Hotel => (rng.gen_range(50.0..200.0), 0.0),
                CommercialBuildingType::Resort => (rng.gen_range(80.0..300.0), rng.gen_range(20.0..80.0)),
                CommercialBuildingType::Restaurant => (0.0, rng.gen_range(30.0..100.0)),
                CommercialBuildingType::Casino => (0.0, rng.gen_range(50.0..200.0)),
                _ => (50.0, 0.0),
            };

            // Pick a micro-region from this region
            let micro_region_id = region
                .micro_regions
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| region.id.clone());

            let commercial_building = CommercialBuilding {
                id: building_id.clone(),
                building_type: btype,
                micro_region_id: micro_region_id.clone(),
                owner_id: company_id.clone(),
                office_capacity: office_cap,
                retail_capacity: retail_cap,
                tenants: Vec::new(),
                rent_per_sqm: 0.0,
                utility_connections: UtilityConnections::default(),
                storage_capacity: 0.0,
                current_inventory: BTreeMap::new(),
                storage_type: crate::society::housing::StorageType::GeneralWarehouse,
                utilization_rate: 0.0,
                retail_profile: None,
                shopping_center_profile: None,
                wholesale_profile: None,
                retail_leases: Vec::new(),
                fixed_assets: Vec::new(),
                active_lighting: default_lighting.clone(),
                active_heating: default_heating.clone(),
                active_power_generation: default_power_generation.clone(),
                active_water_supply: String::new(),
                active_sanitation: String::new(),
                active_waste_disposal: String::new(),
                pending_upgrade: None,
            };
            tourism_buildings.push(commercial_building);

            let company_name = format!("Hospitality {} {} #{}", sector_display(Sector::Hospitality), region.id, i + 1);
            let company_capital = base_wage * 500.0;

            let mut company = Company {
                id: company_id.clone(),
                file_stem: sector_json_name(Sector::Hospitality),
                name: company_name,
                sector: Sector::Hospitality,
                region_id: region.id.clone(),
                legal_form: LegalForm::FamilyBusiness(FamilyBusinessData {
                    dynasty_id: None,
                    successor_generation: 0,
                    family_retained_share: 1.0,
                    heir_vip_ids: Vec::new(),
                    succession_crisis: false,
                }),
                state_share: 0.0,
                fixed_capital: company_capital * 0.6,
                liquid_capital: company_capital * 0.4,
                available_cash: company_capital * 0.2,
                debit_cash: 0.0,
                credit_cash: 0.0,
            unfilled_bid_prices: std::collections::HashMap::new(),
                liabilities: 0.0,
                company_capital,
                shares_count: 0,
                share_price: 0.0,
                shareholders: BTreeMap::new(),
                price_history: Vec::new(),
                financial_history: Vec::new(),
                safety_level: 0.5,
                union_id: None,
                building_ids: vec![building_id],
                scale_factor: 1,
                worker_capacity: rng.gen_range(20..100),
                is_national_champion: false,
                is_listed: false,
                owners: BTreeMap::new(),
                free_float: 0.0,
                aggregated_stats: crate::entities::AggregatedStats::default(),
                bank_type: None,
                balance_sheet: None,
                loan_margin: None,
                brokerage_account: None,
                primary_bank_id: None, outstanding_loan_bank_id: None,
                fund_type: None,
                fund_ledger: None,
                temporary_disruption_modifier: 0.0,
                target_fte_demand: 50,
                offered_wage_per_fte: base_wage * 0.5,
                prev_offered_wage_per_fte: (base_wage * 0.5).max(50.0),
                wage_arrears: 0.0,
                severance_arrears: 0.0,
                furlough_turns_accumulated: 0,
                productivity_penalty: 0.0,
                target_wage: (base_wage * 0.5).max(50.0),
                is_striking: false,
                fulfilled_fte: 0,
                prev_fulfilled_fte: 0,
                physical_fte_demand: 50,
                is_in_receivership: false,
                agricultural_profile: None,
                rd_budget: 0.0,
                patents: Vec::new(),
                licensed_methods: Vec::new(),
                information_quality: None,
                shadow_employment: None,
                pending_expansion: None,
                blueprints: Vec::new(),
                licensed_blueprints: Vec::new(),
                reputation_score: 50.0, donation_history: Vec::new(), is_dspw: false, consumer_loans: Vec::new(),
                annual_profit_accumulator: 0.0,
                seasonal_profile: seasonal_profile_for_sector(
                    Sector::Hospitality,
                    &region.climate_profile,
                ),
                furloughed_workers_count: 0.0,
                ceo_vip_id: None,
                eps: 0.0, pe_ratio: 0.0, dividend_yield: 0.0, open_price: 0.0, close_price: 0.0,
                action_ledger: crate::entities::ActionLedger::default(),
                extra: serde_json::Map::new(),
            };
            // Phase 42: Genesis Labor Fix
            let hosp_wage = (base_wage * 0.5).max(50.0);
            let hosp_fte = 30.0; // 60% of 50.0 target
            company.fulfilled_fte = hosp_fte as u32;
            company.prev_fulfilled_fte = hosp_fte as u32;
            company.available_cash += hosp_fte * hosp_wage * 3.0;
            hospitality_companies.push(company);
        }
    }

    // Save hospitality companies
    let company_store = DiskEntityStore::<Company>::new(data_dir);
    let hospitality_sector_name = sector_json_name(Sector::Hospitality);
    company_store.save_sector(&country.name, &hospitality_sector_name, None, &hospitality_companies)?;

    // Save tourism commercial buildings grouped by type
    let commercial_store = DiskEntityStore::<CommercialBuilding>::new(data_dir);
    let mut by_type: HashMap<CommercialBuildingType, Vec<CommercialBuilding>> = HashMap::new();
    for b in &tourism_buildings {
        by_type.entry(b.building_type).or_default().push(b.clone());
    }
    for (btype, list) in &by_type {
        let type_name = format!("{:?}", btype).to_lowercase();
        commercial_store.save_sector(&country.name, &type_name, None, list)?;
    }

    // Store wonders and destinations on country
    country.natural_wonders = wonders;
    country.tourism_destinations = destinations;

    Ok(())
}

/// Phase 44: Generate genesis housing (Mega-Estates with ownership).
///
/// Creates 10-20 HousingBuilding entities per region with large capacities
/// (10,000-50,000+ slots each) to shelter the generated population.
/// Housing types and ownership are era-appropriate.
///
/// # Rules
/// * Maximum 10-20 buildings per region (Mega-Estate consolidation for CPU).
/// * Each building has 10,000-50,000+ slots.
///* occupied_slots set to ~80-90% of total_capacity.
/// * Owner assigned by housing type:
///   - Palace/FolwarkHousing/SocialHousing → "STATE:<country_id>"
///   - Hut → "CLASS:Aristocracy:<region_id>"
///   - Tenement/CityPalace/Familok/Beamciok → "CLASS:Bourgeoisie:<region_id>"
/// * Housing types distributed by era and class demographics.
/// * Saved to `entities/<country>/housing/`.
fn generate_housing(
    data_dir: &Path,
    country: &Country,
    country_regions: &[&Region],
    start_year: u32,
    idgen: &mut IdGen,
    rng: &mut impl Rng,
) -> Result<(), Box<dyn Error>> {
    use crate::society::housing::{HousingBuilding, HousingType, HousingSlots, UtilityConnections};
    use crate::society::geography::RuralClass;

    if country_regions.is_empty() {
        return Ok(());
    }

    let housing_store = DiskEntityStore::<HousingBuilding>::new(data_dir);
    let mut all_housing: Vec<HousingBuilding> = Vec::new();

    // Era-appropriate consumption method defaults
    let default_lighting = if start_year >= 2000 {
        "LED Lighting".to_string()
    } else if start_year >= 1940 {
        "Fluorescent Tubes".to_string()
    } else if start_year >= 1900 {
        "Incandescent Bulbs".to_string()
    } else if start_year >= 1890 {
        "Gas Mantle".to_string()
    } else if start_year >= 1860 {
        "Kerosene Lamps".to_string()
    } else {
        "None".to_string()
    };

    let default_heating = if start_year >= 1980 {
        "Heat Pump".to_string()
    } else if start_year >= 1930 {
        "District Heating".to_string()
    } else if start_year >= 1920 {
        "Electric Radiator".to_string()
    } else if start_year >= 1900 {
        "Oil Heater".to_string()
    } else if start_year >= 1850 {
        "Coal Stove".to_string()
    } else {
        "None".to_string()
    };

    let default_power_generation = if start_year >= 2010 {
        "Rooftop PV + Battery".to_string()
    } else if start_year >= 2000 {
        "Rooftop PV".to_string()
    } else {
        "None".to_string()
    };

    for region in country_regions {
        let region_pop = region.population.max(1000) as f64;

        // Phase 44: Mega-Estate consolidation — 10-20 buildings per region.
        let num_buildings = if region_pop > 5_000_000.0 {
            20
        } else if region_pop > 1_000_000.0 {
            15
        } else {
            10
        };

        // Total capacity needed: ~100% of population (households ≈ population/4).
        // But we use slots as household units, so target ~region_pop slots.
        let total_capacity_needed = region_pop as u32;
        let capacity_per_building = (total_capacity_needed / num_buildings as u32).max(10_000);

        // Phase 44: Era-aware housing type distribution.
        // Returns (housing_type, owner_id, target_class, living_standard, rent_per_slot).
        let housing_configs: Vec<(HousingType, String, Option<RuralClass>, f64, f64)> = {
            let state_owner = format!("STATE:{}", country.name);
            let aristocracy_owner = format!("CLASS:Aristocracy:{}", region.id);
            let bourgeoisie_owner = format!("CLASS:Bourgeoisie:{}", region.id);

            let rural_pop = region.class_demographics.rural_classes.values()
                .map(|d| d.population).sum::<i64>();
            let urban_pop = region.class_demographics.urban_classes.values()
                .map(|d| d.population).sum::<i64>();
            let total_pop = (rural_pop + urban_pop).max(1) as f64;
            let rural_share = rural_pop as f64 / total_pop;
            let _urban_share = 1.0 - rural_share;

            let rural_buildings = (num_buildings as f64 * rural_share).round() as usize;
            let urban_buildings = num_buildings - rural_buildings;

            let mut configs = Vec::new();

            // Rural housing
            for i in 0..rural_buildings {
                let (ht, owner, target, ls, rent) = if start_year <= 1925 {
                    // 1900/1925 rural: Huts for peasants, FolwarkHousing for serfs, Palace for aristocracy
                    match i % 10 {
                        0 => (HousingType::Palace, state_owner.clone(), Some(RuralClass::Aristocracy), 0.90, 50.0),
                        1..=3 => (HousingType::FolwarkHousing, state_owner.clone(), Some(RuralClass::Serf), 0.40, 5.0),
                        _ => (HousingType::Hut, aristocracy_owner.clone(), Some(RuralClass::FreePeasant), 0.35, 10.0),
                    }
                } else if start_year <= 1950 {
                    // 1950 rural: Familok for workers, Huts modernized
                    match i % 5 {
                        0 => (HousingType::Palace, state_owner.clone(), Some(RuralClass::Aristocracy), 0.90, 50.0),
                        _ => (HousingType::Familok, bourgeoisie_owner.clone(), None, 0.50, 15.0),
                    }
                } else {
                    // 1975 rural: Modernized housing
                    match i % 5 {
                        0 => (HousingType::SocialHousing, state_owner.clone(), None, 0.70, 20.0),
                        _ => (HousingType::Familok, bourgeoisie_owner.clone(), None, 0.55, 15.0),
                    }
                };
                configs.push((ht, owner, target, ls, rent));
            }

            // Urban housing
            for i in 0..urban_buildings {
                let (ht, owner, target, ls, rent) = if start_year <= 1925 {
                    // 1900/1925 urban: Tenements for workers, CityPalace for bourgeoisie
                    match i % 8 {
                        0 => (HousingType::CityPalace, bourgeoisie_owner.clone(), None, 1.00, 100.0),
                        _ => (HousingType::Tenement, bourgeoisie_owner.clone(), None, 0.55, 25.0),
                    }
                } else if start_year <= 1950 {
                    // 1950 urban: Tenements + some SocialHousing
                    match i % 5 {
                        0..=1 => (HousingType::SocialHousing, state_owner.clone(), None, 0.65, 20.0),
                        _ => (HousingType::Tenement, bourgeoisie_owner.clone(), None, 0.60, 25.0),
                    }
                } else {
                    // 1975 urban: SocialHousing + Beamciok
                    match i % 4 {
                        0..=1 => (HousingType::SocialHousing, state_owner.clone(), None, 0.70, 20.0),
                        _ => (HousingType::Beamciok, bourgeoisie_owner.clone(), None, 0.65, 30.0),
                    }
                };
                configs.push((ht, owner, target, ls, rent));
            }

            configs
        };

        for (ht, owner, target_class, living_standard, rent_per_slot) in housing_configs.iter() {
            let building_id = idgen.next_building();
            let total_capacity = capacity_per_building;
            // 80-90% occupied
            let occupied = (total_capacity as f64 * rng.gen_range(0.80..0.90)).round() as u32;

            let housing = HousingBuilding {
                id: building_id,
                housing_type: *ht,
                micro_region_id: region.id.clone(),
                owner: owner.clone(),
                primary_slots: HousingSlots {
                    total_capacity,
                    occupied_slots: occupied,
                    target_class: *target_class,
                    rent_per_slot: *rent_per_slot,
                },
                sublet_slots: None,
                living_standard: *living_standard,
                construction_cost: (total_capacity as f64 * 1000.0),
                maintenance_cost: (total_capacity as f64 * 10.0),
                condition: rng.gen_range(0.7..0.95),
                utility_connections: UtilityConnections {
                    surface_water_capacity: total_capacity as f64 * 50.0,
                    groundwater_capacity: total_capacity as f64 * 20.0,
                    sewage_treatment_capacity: total_capacity as f64 * 30.0,
                    district_heating_capacity: if start_year >= 1925 { total_capacity as f64 * 5.0 } else { 0.0 },
                    electricity_capacity: if start_year >= 1925 { total_capacity as f64 * 10.0 } else { 0.0 },
                    water_quality_received: 0.0,
                },
                active_lighting: default_lighting.clone(),
                active_heating: default_heating.clone(),
                active_power_generation: default_power_generation.clone(),
                active_water_supply: String::new(),
                active_sanitation: String::new(),
                active_waste_disposal: String::new(),
                pending_upgrade: None,
                commercial_slots: None,
            };
            all_housing.push(housing);
        }
    }

    // Save housing buildings
    housing_store.save_sector(&country.name, "housing", None, &all_housing)?;

    Ok(())
}

/// Phase 13: Generate NGO and Church entities for the Third Pillar.
///
/// Creates one secular NGO per region and one religious charity per dominant religion.
/// These are standard `Company` entities with `Sector::NGO` or `Sector::Religion` and
/// `LegalForm::NonProfit`. They participate in the labor market and charity mechanics.
///
/// # Rules
/// * One NGO per region (secular, serves all demographics).
/// * One Church per region per dominant religion (religious, serves co-religionists).
/// * Capital is minimal (micro-scale, funded by donations at runtime).
/// * Worker capacity is small (5-10 FTE for NGOs, 3-8 for churches).
/// * Entities are saved to `entities/<country>/ngo.json` and `religion.json`.
fn generate_charity_entities(
    data_dir: &Path,
    country: &mut Country,
    country_regions: &[&Region],
    start_year: u32,
    idgen: &mut IdGen,
    rng: &mut impl Rng,
) -> Result<(), Box<dyn Error>> {
    if country_regions.is_empty() {
        return Ok(());
    }

    let religion = &country.macro_indicators.religion.clone();
    let mut ngo_companies: Vec<Company> = Vec::new();
    let mut church_companies: Vec<Company> = Vec::new();
    let mut cultural_buildings: Vec<crate::infrastructure::cultural::CulturalBuilding> = Vec::new();

    for region in country_regions {
        // One secular NGO per region.
        let ngo_id = idgen.next_company();
        let ngo_name = format!("Charitable Foundation {}", region.id);
        // Phase 80: Scale NGO capacity by region population. A region with
        // 500K people should have ~50 staff, not 5-10. Formula:
        // max(10, population/50_000).min(500).
        let region_pop = region.population.max(1) as u32;
        let ngo_capacity = (region_pop / 50_000).max(10).min(500);
        let ngo = create_charity_company(
            ngo_id.clone(),
            ngo_name,
            Sector::NGO,
            region.id.clone(),
            NonProfitData {
                religion: String::new(),
                is_religious: false,
            },
            ngo_capacity,
            start_year,
        );
        ngo_companies.push(ngo);

        // One religious charity per region (if country has a dominant religion).
        if !religion.is_empty() {
            let church_id = idgen.next_company();
            let church_name = format!("Parish {} ({})", region.id, religion);
            let church_capacity = rng.gen_range(3..=8);
            let church = create_charity_company(
                church_id.clone(),
                church_name,
                Sector::Religion,
                region.id.clone(),
                NonProfitData {
                    religion: religion.clone(),
                    is_religious: true,
                },
                church_capacity,
                start_year,
            );
            church_companies.push(church);

            // Phase 28: Create a Temple (church building) for this parish.
            let temple = crate::infrastructure::cultural::CulturalBuilding {
                id: idgen.next_building(),
                building_type: crate::infrastructure::cultural::CulturalBuildingType::Temple,
                region_id: region.id.clone(),
                capacity: church_capacity as f64 * 10.0, // Temple serves more than just clergy
                available_cash: 0.0, // Funded organically via donations
                donations_collected_this_turn: 0.0,
                relief_distributed_this_turn: 0.0,
                year_built: start_year,
                condition: 1.0,
                is_heritage_site: false,
                owned_company_shares: BTreeMap::new(),
                owned_latifundium: None,
                production_method: None, // Temples don't produce goods
                owner_company_id: Some(church_id),
            };
            cultural_buildings.push(temple);

            // Phase 28: Create a Monastery for some regions (rural preference).
            let avg_gdp_pc = country_regions.iter().map(|r| r.gdp_pc).sum::<f64>() / country_regions.len() as f64;
            let is_rural = region.gdp_pc < avg_gdp_pc;
            let monastery_chance = if is_rural { 0.7 } else { 0.3 };
            if rng.gen::<f64>() < monastery_chance {
                let monastery_company_id = idgen.next_company();
                let monastery_name = format!("Monastery {} ({})", region.id, religion);
                let monastery_capacity = rng.gen_range(5..=15);
                let monastery = create_charity_company(
                    monastery_company_id.clone(),
                    monastery_name,
                    Sector::Religion,
                    region.id.clone(),
                    NonProfitData {
                        religion: religion.clone(),
                        is_religious: true,
                    },
                    monastery_capacity,
                    start_year,
                );
                church_companies.push(monastery);

                // Assign a production method from the registry
                let prod_method = if is_rural && rng.gen::<f64>() < 0.4 {
                    "monastery_wine_production"
                } else if rng.gen::<f64>() < 0.3 {
                    "monastery_scriptorium"
                } else if rng.gen::<f64>() < 0.5 {
                    "monastery_herbal_garden"
                } else {
                    "monastery_workshop"
                };

                // Optionally assign latifundium to rural monasteries
                let latifundium = if is_rural && rng.gen::<f64>() < 0.3 {
                    Some(crate::entities::legal_form::LatifundiumData {
                        serf_households: rng.gen_range(5..=20),
                        serf_population: 0, // Computed below
                        serf_labor_cost_multiplier: 0.1,
                        dynasty_id: None,
                        region_id: region.id.clone(),
                        total_hectares: rng.gen_range(100..=1000),
                        ..Default::default()
                    })
                } else {
                    None
                };
                // Fix serf_population from households
                let latifundium = latifundium.map(|mut lat| {
                    lat.serf_population = lat.serf_households * 5; // ~5 per household
                    lat
                });

                let monastery_building = crate::infrastructure::cultural::CulturalBuilding {
                    id: idgen.next_building(),
                    building_type: crate::infrastructure::cultural::CulturalBuildingType::Monastery,
                    region_id: region.id.clone(),
                    capacity: monastery_capacity as f64 * 5.0,
                    available_cash: 0.0, // Funded organically via donations + production
                    donations_collected_this_turn: 0.0,
                    relief_distributed_this_turn: 0.0,
                    year_built: start_year,
                    condition: 1.0,
                    is_heritage_site: false,
                    owned_company_shares: BTreeMap::new(),
                    owned_latifundium: latifundium,
                    production_method: Some(prod_method.to_string()),
                    owner_company_id: Some(monastery_company_id),
                };
                cultural_buildings.push(monastery_building);
            }
        }
    }

    // Save NGO companies.
    let company_store = DiskEntityStore::<Company>::new(data_dir);
    if !ngo_companies.is_empty() {
        let sector_name = sector_json_name(Sector::NGO);
        company_store.save_sector(&country.name, &sector_name, None, &ngo_companies)?;
    }

    // Save Church/Monastery companies.
    if !church_companies.is_empty() {
        let sector_name = sector_json_name(Sector::Religion);
        company_store.save_sector(&country.name, &sector_name, None, &church_companies)?;
    }

    // Phase 28: Store cultural buildings on the country.
    country.cultural_institutions = cultural_buildings;

    Ok(())
}

/// Helper: create a charity Company with NonProfit legal form.
fn create_charity_company(
    id: String,
    name: String,
    sector: Sector,
    region_id: String,
    non_profit_data: NonProfitData,
    worker_capacity: u32,
    _start_year: u32,
) -> Company {
    // Phase 28: Set a subsistence wage offer. The labor market clamps hiring
    // by available_cash / offered_wage_per_fte, so if no donations have flowed,
    // the charity hires 0 workers. When donations arrive (via collect_cultural_donations
    // Ă˘â€ â€™ building.available_cash Ă˘â€ â€™ company.available_cash), the charity can hire.
    let subsistence_wage = 500.0; // Minimal wage, will be clamped by available cash

    Company {
        id: id.clone(),
        file_stem: sector_json_name(sector),
        name,
        sector,
        region_id,
        legal_form: LegalForm::NonProfit(non_profit_data),
        state_share: 0.0,
        fixed_capital: 1000.0, // Minimal office space
        liquid_capital: 0.0,
        available_cash: 0.0, // Funded organically by donations at runtime
        debit_cash: 0.0,
        credit_cash: 0.0,
        unfilled_bid_prices: std::collections::HashMap::new(),
        liabilities: 0.0,
        company_capital: 1000.0,
        shares_count: 0, // Non-profits don't issue shares
        share_price: 0.0,
        shareholders: BTreeMap::new(),
        price_history: Vec::new(),
        financial_history: Vec::new(),
        safety_level: 1.0, // Charities don't have industrial accidents
        union_id: None,
        building_ids: Vec::new(),
        scale_factor: 1,
        worker_capacity,
        is_national_champion: false,
        is_listed: false,
        owners: BTreeMap::new(),
        free_float: 0.0,
        aggregated_stats: AggregatedStats::default(),
        bank_type: None,
        balance_sheet: None,
        loan_margin: None,
        brokerage_account: Some(crate::securities::BrokerageAccount {
            cash: 0.0, // Phase 33: Empty account exists so labor market doesn't clamp to 0.
            ..Default::default()
        }), // Funded organically by donations (Phase 28 rule: NO seed grants)
        primary_bank_id: None, outstanding_loan_bank_id: None,
        fund_type: None,
        fund_ledger: None,
        temporary_disruption_modifier: 0.0,
        target_fte_demand: worker_capacity,
        offered_wage_per_fte: subsistence_wage,
        prev_offered_wage_per_fte: subsistence_wage.max(50.0),
        wage_arrears: 0.0,
        severance_arrears: 0.0,
        furlough_turns_accumulated: 0,
        productivity_penalty: 0.0,
        target_wage: subsistence_wage.max(50.0),
        is_striking: false,
        // Phase 80: Pre-populate 70% of workforce so NGOs can operate from turn 1.
        // Previously 0, which meant NGOs couldn't participate in the labor market
        // until donations arrived organically — leaving them dead for many turns.
        fulfilled_fte: ((worker_capacity as f64 * 0.7).round() as u32).max(1),
        prev_fulfilled_fte: ((worker_capacity as f64 * 0.7).round() as u32).max(1),
        physical_fte_demand: worker_capacity,
        is_in_receivership: false,
        agricultural_profile: None,
        rd_budget: 0.0,
        patents: Vec::new(),
        licensed_methods: Vec::new(),
        information_quality: None,
        shadow_employment: None,
        pending_expansion: None,
        blueprints: Vec::new(),
        licensed_blueprints: Vec::new(),
        reputation_score: 50.0, donation_history: Vec::new(), is_dspw: false, consumer_loans: Vec::new(),
        annual_profit_accumulator: 0.0,
        seasonal_profile: None,
        furloughed_workers_count: 0.0,
        ceo_vip_id: None,
        eps: 0.0, pe_ratio: 0.0, dividend_yield: 0.0, open_price: 0.0, close_price: 0.0,
        action_ledger: crate::entities::ActionLedger::default(),
        extra: serde_json::Map::new(),
    }
}

// ============================================================================
// PHASE 57: INVESTMENT FUND GENERATION
// ============================================================================

/// Phase 57: Generate investment funds for a country at world creation.
///
/// # Rules
/// * Generate 2–5 investment funds per country based on GDP size.
/// * Assign `FundType` with weighted probabilities (FIO 40%, Mutual 25%, ETF 15%, FIZ 12%, Hedge 8%).
/// * Create `FundLedger` with initial NAV = 1.0.
/// * Create `BrokerageAccount` with seed capital from treasury (1M per fund).
/// * Assign fund manager VIP with traits appropriate to fund type.
/// * Register fund manager in VIP registry.
pub fn generate_investment_funds(
    data_dir: &std::path::Path,
    country: &mut Country,
    cultural_group: &str,
    _start_year: u32,
    rng: &mut impl Rng,
) {
    use crate::politics::vip_registry::{Vip, VipRoleExtended, assign_core_traits};
    use crate::securities::{FundType, FundLedger, InvestmentMandate, BrokerageAccount};
    use crate::entities::AggregatedStats;
    use crate::io::entity_store::{DiskEntityStore, EntityStore};

    // Determine fund count based on GDP (2–5 funds).
    let total_gdp: f64 = country.regions.iter().map(|r| r.gdp_pc * r.population as f64).sum();
    let fund_count = if total_gdp > 1e12 {
        5
    } else if total_gdp > 5e11 {
        4
    } else if total_gdp > 1e11 {
        3
    } else {
        2
    };

    // Ensure VIP registry exists.
    if country.politics.vip_registry.is_none() {
        country.politics.vip_registry = Some(crate::politics::vip_registry::VipRegistry::new());
    }

    // Pick a region for the fund HQ (first region).
    let hq_region = country.regions.first().map(|r| r.id.clone()).unwrap_or_default();

    // Weighted fund type distribution: FIO 40%, Mutual 25%, ETF 15%, FIZ 12%, Hedge 8%.
    let type_weights: Vec<(FundType, f64)> = vec![
        (FundType::OpenEndInvestmentFund, 0.40),
        (FundType::MutualFund, 0.25),
        (FundType::ExchangeTradedFund, 0.15),
        (FundType::ClosedEndInvestmentFund, 0.12),
        (FundType::HedgeFund, 0.08),
    ];

    let mut fund_companies: Vec<Company> = Vec::new();

    for i in 0..fund_count {
        // Pick fund type via weighted random.
        let roll: f64 = rng.gen();
        let mut cumulative = 0.0;
        let mut chosen_type = FundType::OpenEndInvestmentFund;
        for (ft, w) in &type_weights {
            cumulative += w;
            if roll < cumulative {
                chosen_type = ft.clone();
                break;
            }
        }

        // Generate fund manager VIP with traits appropriate to fund type.
        let (preferred_traits, fund_name_suffix) = match chosen_type {
            FundType::HedgeFund => (
                vec!["Ambitious".to_string(), "Corrupt".to_string()],
                "Hedge Fund",
            ),
            FundType::MutualFund | FundType::ExchangeTradedFund => (
                vec!["Conservative".to_string(), "Diplomatic".to_string()],
                if matches!(chosen_type, FundType::ExchangeTradedFund) { "ETF" } else { "Mutual Fund" },
            ),
            FundType::ClosedEndInvestmentFund => (
                vec!["Ambitious".to_string()],
                "Closed-End Fund",
            ),
            FundType::OpenEndInvestmentFund => (
                vec!["Conservative".to_string()],
                "Open-End Fund",
            ),
        };

        let manager_name = crate::politics::names::generate_full_vip(cultural_group, rng);
        let (mut traits, main_trait) = assign_core_traits(rng);

        // Inject a preferred trait if not already present (50% chance).
        if let Some(pref) = preferred_traits.first() {
            if rng.gen::<f64>() < 0.5 && !traits.iter().any(|t| t == pref) {
                traits.push(pref.clone());
            }
        }

        let ideology = ceo_ideology_from_traits(&traits, &main_trait, rng);
        let manager_vip = Vip {
            full_name: manager_name.full_name.clone(),
            gender: manager_name.gender,
            age: 35 + rng.gen_range(0..30),
            health: crate::politics::vip_registry::VipHealth { physical_health: 1.0, mental_health: 1.0 },
            traits: traits.clone(),
            main_trait: main_trait.clone(),
            ideology,
            nationality: country.name.clone(),
            roles: vec![VipRoleExtended::Ceo], // Fund managers are CEOs of the fund company
            base_influence: 15 + rng.gen_range(0..25),
            ..Default::default()
        };
        let manager_id = country
            .politics
            .vip_registry
            .as_mut()
            .unwrap()
            .register_new(manager_vip);

        // Create fund company.
        let fund_id = format!("FUND-{}-{}", country.name, i + 1);
        let fund_name = format!("{} {}", manager_name.surname, fund_name_suffix);

        // Seed capital: 1M from treasury.
        let seed_capital = 1_000_000.0;
        if country.budget.liquid_reserves >= seed_capital {
            country.budget.liquid_reserves -= seed_capital;
        }

        // Create fund ledger.
        let ledger = FundLedger {
            nav_per_share: 1.0,
            shares_outstanding: (seed_capital / 1.0) as u64, // 1M shares at NAV 1.0
            management_fee: 0.02,   // 2% management fee
            performance_fee: 0.20,  // 20% performance fee (for hedge funds)
            leverage_ratio: if matches!(chosen_type, FundType::HedgeFund) { 2.0 } else { 0.0 },
            investment_mandate: InvestmentMandate::default(),
            liquidity_provision: BTreeMap::new(),
            unit_holders: {
                let mut m = BTreeMap::new();
                m.insert("TREASURY".to_string(), (seed_capital / 1.0) as u64);
                m
            },
            bond_holdings: Vec::new(),
            fund_manager_vip_id: Some(manager_id.clone()),
        };

        let fund_company = Company {
            id: fund_id.clone(),
            file_stem: "banking".to_string(),
            name: fund_name,
            sector: Sector::Banking,
            region_id: hq_region.clone(),
            legal_form: LegalForm::JointStockCompany(JointStockData {
                shares_issued: (seed_capital / 1.0) as u64,
                free_float: 0.0, // Funds are not publicly listed
                dividend_per_share: 0.0,
                board_independence: 0.5,
                board_members: Vec::new(),
            }),
            state_share: 0.0,
            fixed_capital: 100_000.0, // Office space
            liquid_capital: 0.0,
            available_cash: 0.0,
            debit_cash: 0.0,
            credit_cash: 0.0,
            unfilled_bid_prices: std::collections::HashMap::new(),
            liabilities: 0.0,
            company_capital: seed_capital,
            shares_count: (seed_capital / 1.0) as u64,
            share_price: 1.0, // NAV per share
            shareholders: BTreeMap::new(),
            price_history: Vec::new(),
            financial_history: Vec::new(),
            safety_level: 1.0,
            union_id: None,
            building_ids: Vec::new(),
            scale_factor: 1,
            worker_capacity: 10, // Small staff
            is_national_champion: false,
            is_listed: false,
            owners: BTreeMap::new(),
            free_float: 0.0,
            aggregated_stats: AggregatedStats::default(),
            bank_type: None,
            balance_sheet: None,
            loan_margin: None,
            brokerage_account: Some(BrokerageAccount {
                cash: seed_capital,
                ..Default::default()
            }),
            primary_bank_id: None,
            outstanding_loan_bank_id: None,
            fund_type: Some(chosen_type),
            fund_ledger: Some(ledger),
            temporary_disruption_modifier: 0.0,
            target_fte_demand: 10,
            offered_wage_per_fte: 5000.0,
            prev_offered_wage_per_fte: 5000.0,
            wage_arrears: 0.0,
            severance_arrears: 0.0,
            furlough_turns_accumulated: 0,
            productivity_penalty: 0.0,
            target_wage: 5000.0,
            is_striking: false,
            fulfilled_fte: 0,
            prev_fulfilled_fte: 0,
            physical_fte_demand: 10,
            is_in_receivership: false,
            agricultural_profile: None,
            rd_budget: 0.0,
            patents: Vec::new(),
            licensed_methods: Vec::new(),
            information_quality: None,
            shadow_employment: None,
            pending_expansion: None,
            blueprints: Vec::new(),
            licensed_blueprints: Vec::new(),
            reputation_score: 60.0,
            donation_history: Vec::new(),
            is_dspw: false,
            consumer_loans: Vec::new(),
            annual_profit_accumulator: 0.0,
            seasonal_profile: None,
            furloughed_workers_count: 0.0,
            ceo_vip_id: Some(manager_id),
            eps: 0.0,
            pe_ratio: 0.0,
            dividend_yield: 0.0,
            open_price: 1.0,
            close_price: 1.0,
            action_ledger: crate::entities::ActionLedger::default(),
            extra: serde_json::Map::new(),
        };

        fund_companies.push(fund_company);
    }

    // Phase 61.1: Save fund companies to disk so they persist across game load.
    // Funds are stored in the "banking" sector file alongside bank companies.
    if !fund_companies.is_empty() {
        let company_store = DiskEntityStore::<Company>::new(data_dir);
        let sector_name = sector_json_name(Sector::Banking);
        // Load existing banking companies, append funds, and save back.
        let mut existing = company_store
            .load_sector(&country.name, &sector_name, None)
            .unwrap_or_default();
        existing.extend(fund_companies);
        let _ = company_store.save_sector(&country.name, &sector_name, None, &existing);
    }
}

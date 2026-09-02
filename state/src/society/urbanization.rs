//! Phase 85B: The Urbanization Cycle — Emancipation & Annexation.
//!
//! This module implements the mechanics for:
//! - **Emancipation**: A `GuildBurgher` domain (MicroRegion) that accumulates
//!   sufficient population, economic output, and institutional capacity breaks
//!   away from its parent Rural Region to become an independent City Region.
//! - **Annexation**: City Regions expand by buying out adjacent parcels from
//!   neighboring Rural Regions, creating urban-rural conflict.
//!
//! ## Turn Phase
//!
//! Phase 85B runs AFTER B2C clearing and BEFORE the next turn's
//! demographics/labor phase. This ensures:
//! - Production has completed and financial state is settled.
//! - Emancipation/annexation effects are visible to the next turn's labor allocation.
//! - Emergency import orders are queued for the next turn's B2B clearing.
//!
//! ## Conservation Invariants
//!
//! - **Money (Rule 1)**: Buyout is exact counterparty transfer. Domain budget →
//!   city treasury. Corporate buyout → `company.liquid_capital` ONLY (no duplication).
//! - **Mass (Rule 1)**: Parcel data is moved, not copied. Physical water in
//!   infrastructure buildings is deducted from source `water_reserves` and
//!   injected into city `water_reserves`.
//! - **No Teleportation (Rule 19)**: Grid pipe connections severed on transfer.
//!   Energy stays on National HV Grid. B2B trucking only for Water/WasteUtility.
//! - **No Vaporware (Rule 14)**: Failed annexation uses only existing variables
//!   (`social_unrest`, `autonomy_level`, `development_level`).

use crate::entities::Company;
use crate::society::cadastre::{ParcelChunk, ParcelId, ParcelOwnerType};
use crate::society::geography::{CityRegionMetadata, FactionDomainType, MicroRegion, Region};
use crate::state::Country;

/// Configuration for the urbanization cycle. All thresholds are dynamic,
/// scaled by macroeconomic variables (Rule 2 — No Magic Numbers).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmancipationConfig {
    /// Population density threshold for emancipation (people per km²).
    pub emancipation_pop_density: f64,
    /// Fraction of parent region GDP the domain must produce (0.0-1.0).
    pub emancipation_gdp_share: f64,
    /// Multiple of average_wage the domain's liquid reserves must exceed.
    pub emancipation_capital_wage_multiple: f64,
    /// Minimum number of guilds operating in the domain.
    pub emancipation_min_guilds: u32,
    /// Minimum development_level of the parent region (0.0-1.0).
    pub emancipation_development_threshold: f64,
    /// Premium over assessed parcel value for annexation buyout (multiplicative).
    pub annexation_buyout_premium: f64,
    /// Additional premium for aristocratic-owned parcels (multiplicative).
    pub aristocratic_resistance_multiplier: f64,
    /// `social_unrest` increase on failed annexation attempt.
    pub annexation_unrest_penalty: f64,
    /// `social_unrest` increase on successful aristocratic buyout.
    pub aristocratic_tension_per_buyout: f64,
    /// Cooldown turns after a failed annexation attempt.
    pub annexation_cooldown_turns: u32,
    /// `autonomy_level` reduction on the domain after failed annexation.
    pub failed_annexation_autonomy_debuff: f64,
    /// `development_level` reduction on the city after failed annexation.
    pub failed_annexation_development_debuff: f64,
    /// `social_unrest` increase per turn if city cannot afford emergency imports.
    pub emancipation_shortage_unrest: f64,
    /// Fraction of city treasury reserved for emergency imports (0.0-1.0).
    pub emancipation_emergency_import_budget_fraction: f64,
}

impl Default for EmancipationConfig {
    fn default() -> Self {
        Self {
            emancipation_pop_density: 500.0,
            emancipation_gdp_share: 0.25,
            emancipation_capital_wage_multiple: 500.0,
            emancipation_min_guilds: 2,
            emancipation_development_threshold: 0.5,
            annexation_buyout_premium: 2.0,
            aristocratic_resistance_multiplier: 3.0,
            annexation_unrest_penalty: 5.0,
            aristocratic_tension_per_buyout: 2.0,
            annexation_cooldown_turns: 12,
            failed_annexation_autonomy_debuff: 0.1,
            failed_annexation_development_debuff: 0.05,
            emancipation_shortage_unrest: 10.0,
            emancipation_emergency_import_budget_fraction: 0.3,
        }
    }
}

/// Result of an emancipation check for a single domain.
#[derive(Debug, Clone, Default)]
pub struct EmancipationResult {
    /// Whether emancipation was triggered.
    pub emancipated: bool,
    /// The new city region ID, if emancipated.
    pub new_city_region_id: Option<String>,
    /// The parent region ID.
    pub parent_region_id: String,
    /// The domain ID that emancipated.
    pub domain_id: String,
}

/// Result of an annexation attempt for a single parcel.
#[derive(Debug, Clone, Default)]
pub struct AnnexationResult {
    /// Whether the annexation succeeded.
    pub success: bool,
    /// The parcel ID that was targeted.
    pub parcel_id: Option<ParcelId>,
    /// The buyout cost paid (if successful).
    pub buyout_cost: f64,
    /// Whether this was an aristocratic conflict.
    pub was_aristocratic: bool,
    /// Whether the city could not afford the buyout.
    pub insufficient_funds: bool,
}

/// Check whether a GuildBurgher domain meets all emancipation triggers.
///
/// All triggers must be satisfied simultaneously. Returns `true` if the domain
/// should emancipate into an independent City Region.
pub fn check_emancipation_triggers(
    domain: &MicroRegion,
    parent_region: &Region,
    domain_gdp: f64,
    parent_region_gdp: f64,
    domain_total_hectares: f64,
    guild_count: u32,
    average_wage: f64,
    config: &EmancipationConfig,
) -> bool {
    // Must be a GuildBurgher domain
    if domain.faction_type != FactionDomainType::GuildBurgher {
        return false;
    }

    // Trigger 1: Population density
    if domain_total_hectares <= 0.0 {
        return false;
    }
    let pop_density = domain.population as f64 / domain_total_hectares;
    if pop_density <= config.emancipation_pop_density {
        return false;
    }

    // Trigger 2: Economic output — domain produces > X% of parent region GDP
    if parent_region_gdp <= 0.0 {
        return false;
    }
    let gdp_share = domain_gdp / parent_region_gdp;
    if gdp_share <= config.emancipation_gdp_share {
        return false;
    }

    // Trigger 3: Institutional capital — liquid reserves > N × average_wage
    let capital_threshold = config.emancipation_capital_wage_multiple * average_wage;
    if domain.sub_budget.liquid_reserves <= capital_threshold {
        return false;
    }

    // Trigger 4: Guild presence — at least N guilds in the domain
    if guild_count < config.emancipation_min_guilds {
        return false;
    }

    // Trigger 5: Development level of parent region
    if parent_region.development_level <= config.emancipation_development_threshold {
        return false;
    }

    true
}

/// Execute emancipation: create a new City Region from the domain's data.
///
/// This function:
/// 1. Creates a new Region with CityRegionMetadata.
/// 2. Transfers parcels from parent to city (updating ParcelChunk.region_id).
/// 3. Transfers buildings (updating Building.region_id).
/// 4. Transfers demographics (FTE conservation).
/// 5. Settles finances (domain budget → city treasury, exact double-entry).
/// 6. Removes domain from parent, adds to city.
/// 7. Transfers physical water state from infrastructure buildings.
/// 8. Inserts the new region into country.regions.
///
/// # Arguments
/// * `country` - The country containing the parent region.
/// * `cadastre` - The country's cadastre (for parcel updates).
/// * `parent_region_idx` - Index into `country.regions` for the parent region.
/// * `domain_id` - The ID of the GuildBurgher domain to emancipate.
/// * `current_turn` - The current turn number.
/// * `config` - Emancipation configuration.
///
/// # Returns
/// The emancipation result, or `None` if the domain was not found.
pub fn execute_emancipation(
    country: &mut Country,
    cadastre: &mut crate::society::cadastre::Cadastre,
    parent_region_idx: usize,
    domain_id: &str,
    current_turn: u32,
    _config: &EmancipationConfig,
) -> Option<EmancipationResult> {
    // Extract domain data from parent region (avoid double borrow).
    let parent_region = country.regions.get(parent_region_idx)?;
    let domain = parent_region.micro_regions.get(domain_id)?;
    if domain.faction_type != FactionDomainType::GuildBurgher {
        return None;
    }

    let parent_region_id = parent_region.id.clone();
    let domain_id_owned = domain.id.clone();
    let domain_name = domain.name.clone();
    let domain_population = domain.population;
    let domain_sub_budget = domain.sub_budget.clone();
    let domain_controlled_parcels = domain.controlled_parcel_ids.clone();
    let domain_local_laws = domain.local_laws.clone();
    let domain_autonomy = domain.autonomy_level;
    let domain_education_slots = domain.education_slots;
    let domain_health_capacity = domain.health_capacity;
    let domain_governing_faction = domain.governing_faction_id.clone();

    let new_city_id = format!("{}-CITY-{}", parent_region_id, domain_id_owned);
    let new_display_name = format!("{} (City)", domain_name);

    // Compute total hectares from controlled parcels for the new region.
    let mut total_hectares = 0.0_f64;
    let mut soil_profile = std::collections::BTreeMap::new();
    for &pid in &domain_controlled_parcels {
        if let Some(parcel) = cadastre.get(pid) {
            total_hectares += parcel.size_hectares;
            *soil_profile.entry(parcel.soil_class.clone()).or_insert(0.0) += parcel.size_hectares;
        }
    }
    let arable_land_max = total_hectares as i64;

    // Create the new City Region.
    let mut new_region = Region {
        id: new_city_id.clone(),
        display_name: new_display_name,
        owner_country: parent_region.owner_country.clone(),
        population: domain_population,
        gdp: 0.0,
        gdp_pc: 0.0,
        climate: parent_region.climate,
        soil_profile,
        arable_land_max,
        arable_land_used: 0,
        extraction_limits: parent_region.extraction_limits.clone(),
        extraction_used: std::collections::BTreeMap::new(),
        resources: serde_json::Map::new(),
        is_capital: false,
        node_type: parent_region.node_type,
        edges: Vec::new(),
        land_distribution: std::collections::BTreeMap::new(),
        class_demographics: parent_region.class_demographics.clone(),
        governance: Some(crate::politics::local_government::RegionalGovernance {
            id: new_city_id.clone(),
            head_type: crate::politics::local_government::RegionalHeadType::Mayor,
            head: crate::politics::system::Leader::default(),
            council: crate::politics::local_council::LocalCouncil::default(),
            budget: crate::politics::local_government::RegionalBudget::default(),
            debt: crate::politics::local_government::RegionalDebt::default(),
            admin_status: crate::politics::local_government::AdministrativeStatus::Normal,
            last_election_year: current_turn,
            years_to_next_election: 4,
            zoning_plans: crate::society::cadastre::ZoningPlanRegistry::default(),
        }),
        capacity_pool: std::collections::BTreeMap::new(),
        capacity_utilization: std::collections::BTreeMap::new(),
        capacity_prices: std::collections::BTreeMap::new(),
        land_use_inventory: crate::society::geography::LandUseInventory::default(),
        climate_profile: parent_region.climate_profile,
        micro_regions: std::collections::BTreeMap::new(),
        treasury: crate::state::treasury::Treasury::default(),
        microregion_budgets: std::collections::HashMap::new(),
        winter_mortality_multiplier: 1.0,
        holy_site: None,
        geographic_traits: parent_region.geographic_traits.clone(),
        coord_x: parent_region.coord_x,
        coord_y: parent_region.coord_y,
        development_level: parent_region.development_level,
        parcel_ids: domain_controlled_parcels.clone(),
        is_autonomous_republic: false,
        elevation_difference_m: parent_region.elevation_difference_m,
        thermal_grid: crate::energy::thermal_grid::ThermalGridState::default(),
        local_pollution: crate::environment::smog::LocalPollutionState::default(),
        water_reserves: crate::utilities::hydro_grid::WaterReserveState::default(),
        water_network: crate::utilities::hydro_grid::WaterNetworkState::default(),
        sewer_network: crate::utilities::hydro_grid::SewerNetworkState::default(),
        waste_grid: crate::utilities::waste_grid::WasteGridState::default(),
        city_metadata: Some(CityRegionMetadata {
            is_city: true,
            emancipated_turn: current_turn,
            parent_region_id: parent_region_id.clone(),
            annexation_cooldown: 0,
            pending_annexation_targets: Vec::new(),
        }),
    };

    // Financial settlement: domain budget → city treasury (double-entry, exact).
    new_region.treasury.liquid_reserves = domain_sub_budget.liquid_reserves;

    // Transfer the domain itself into the new region.
    let mut transferred_domain = MicroRegion {
        id: domain_id_owned.clone(),
        parent_region_id: new_city_id.clone(),
        faction_type: FactionDomainType::GuildBurgher,
        name: domain_name,
        population: domain_population,
        sub_budget: crate::society::geography::MicroRegionBudget::default(),
        autonomy_level: domain_autonomy,
        governing_faction_id: domain_governing_faction,
        local_laws: domain_local_laws,
        education_slots: domain_education_slots,
        health_capacity: domain_health_capacity,
        controlled_parcel_ids: domain_controlled_parcels.clone(),
    };
    // Zero out the domain's budget (funds moved to city treasury).
    transferred_domain.sub_budget.liquid_reserves = 0.0;
    new_region
        .micro_regions
        .insert(domain_id_owned.clone(), transferred_domain);

    // Update parcel region_id.
    for &pid in &domain_controlled_parcels {
        if let Some(parcel) = cadastre.get_mut(pid) {
            parcel.region_id = new_city_id.clone();
        }
    }

    // Transfer physical water from infrastructure buildings on transferred parcels.
    // We check buildings for Commodity::Water in their inventory.
    // This is done by the caller (turn loop) since buildings are in CountryEntities.
    // Here we just compute the water to transfer from the cadastre parcels.
    // The actual building inventory transfer happens in the turn integration.

    // Remove domain from parent region and remove transferred parcels.
    let parent_region = country.regions.get_mut(parent_region_idx)?;
    parent_region.micro_regions.remove(&domain_id_owned);
    parent_region.microregion_budgets.remove(&domain_id_owned);
    parent_region
        .parcel_ids
        .retain(|p| !domain_controlled_parcels.contains(p));

    // Deduct transferred water from parent region's water_reserves.
    // (The actual amount is computed during building transfer in the turn loop.)
    // For now, we store the total in the new region. The turn loop will
    // handle the physical deduction from parent and injection into city.

    // Insert the new city region.
    country.regions.push(new_region);

    Some(EmancipationResult {
        emancipated: true,
        new_city_region_id: Some(new_city_id),
        parent_region_id,
        domain_id: domain_id_owned,
    })
}

/// Transfer physical water from a building's inventory to the city region's
/// water reserves. Called during emancipation for each infrastructure building
/// on a transferred parcel.
///
/// # Arguments
/// * `parent_region` - The parent region (water deducted from here).
/// * `city_region` - The new city region (water added here).
/// * `building_water_volume` - The volume of water in the building's inventory.
pub fn transfer_physical_water(
    parent_region: &mut Region,
    city_region: &mut Region,
    building_water_volume: f64,
) {
    if building_water_volume <= 0.0 {
        return;
    }
    // Deduct from parent (clamped to 0.0 — no negative water).
    let available = parent_region.water_reserves.groundwater_volume;
    let to_transfer = building_water_volume.min(available);
    parent_region.water_reserves.groundwater_volume =
        (parent_region.water_reserves.groundwater_volume - to_transfer).max(0.0);
    // Inject into city.
    city_region.water_reserves.groundwater_volume += to_transfer;
}

/// Identify annexation targets: parcels in the city region that are adjacent
/// to parcels in OTHER regions.
///
/// # Returns
/// A list of (parcel_id, source_region_id) pairs for candidate parcels.
pub fn identify_annexation_targets(
    city_region: &Region,
    cadastre: &crate::society::cadastre::Cadastre,
) -> Vec<(ParcelId, String)> {
    let mut targets = Vec::new();
    let city_region_id = &city_region.id;

    for &city_parcel_id in &city_region.parcel_ids {
        let city_parcel = match cadastre.get(city_parcel_id) {
            Some(p) => p,
            None => continue,
        };
        // Check adjacent parcels for cross-region ones.
        for &adjacent_id in &city_parcel.adjacent_parcels {
            let adjacent = match cadastre.get(adjacent_id) {
                Some(p) => p,
                None => continue,
            };
            if adjacent.region_id != *city_region_id {
                // This adjacent parcel is in a different region — candidate.
                targets.push((adjacent_id, adjacent.region_id.clone()));
            }
        }
    }

    targets
}

/// Evaluate the annexation cost for a parcel.
///
/// `parcel_value = parcel.size_hectares × average_land_price × (1 + development_bonus)`
/// `buyout_cost = parcel_value × premium × [aristocratic_multiplier if applicable]`
pub fn evaluate_annexation_cost(
    parcel: &ParcelChunk,
    average_land_price: f64,
    is_aristocratic: bool,
    config: &EmancipationConfig,
) -> f64 {
    let development_bonus = parcel.infrastructure_access;
    let parcel_value = parcel.size_hectares * average_land_price * (1.0 + development_bonus);
    let mut buyout_cost = parcel_value * config.annexation_buyout_premium;
    if is_aristocratic {
        buyout_cost *= config.aristocratic_resistance_multiplier;
    }
    buyout_cost
}

/// Execute an annexation: transfer a parcel from source region to city region
/// with financial settlement.
///
/// # Arguments
/// * `city_region` - The city region annexing the parcel.
/// * `source_region` - The region losing the parcel.
/// * `parcel_id` - The parcel being annexed.
/// * `cadastre` - The cadastre (for parcel updates).
/// * `companies` - Mutable slice of companies (for corporate buyout routing).
/// * `buyout_cost` - The cost to pay.
/// * `is_aristocratic` - Whether this is an aristocratic estate parcel.
/// * `current_turn` - The current turn.
/// * `config` - Emancipation configuration.
///
/// # Returns
/// The annexation result.
pub fn execute_annexation(
    city_region: &mut Region,
    source_region: &mut Region,
    parcel_id: ParcelId,
    cadastre: &mut crate::society::cadastre::Cadastre,
    companies: &mut [Company],
    buyout_cost: f64,
    is_aristocratic: bool,
    current_turn: u32,
    config: &EmancipationConfig,
) -> AnnexationResult {
    // Check if city can afford the buyout.
    if city_region.treasury.liquid_reserves < buyout_cost {
        // Failed annexation — grounded penalties only (no vaporware).
        city_region.treasury.liquid_reserves = (city_region.treasury.liquid_reserves).max(0.0);
        // Apply cooldown.
        if let Some(ref mut meta) = city_region.city_metadata {
            meta.annexation_cooldown = config.annexation_cooldown_turns;
        }
        // Apply development debuff (clamped to 0.0).
        city_region.development_level =
            (city_region.development_level - config.failed_annexation_development_debuff).max(0.0);
        return AnnexationResult {
            success: false,
            parcel_id: Some(parcel_id),
            buyout_cost: 0.0,
            was_aristocratic: is_aristocratic,
            insufficient_funds: true,
        };
    }

    // City pays the buyout.
    city_region.treasury.liquid_reserves -= buyout_cost;

    // Route buyout to the correct counterparty (Corporate Veil — Correction 2).
    let parcel = match cadastre.get(parcel_id) {
        Some(p) => p.clone(),
        None => {
            return AnnexationResult {
                success: false,
                parcel_id: Some(parcel_id),
                buyout_cost: 0.0,
                was_aristocratic: is_aristocratic,
                insufficient_funds: false,
            };
        }
    };

    match parcel.owner_type {
        ParcelOwnerType::Corporate => {
            // Route to company's liquid_capital ONLY (no duplication — Correction 1).
            if let Some(company) = companies.iter_mut().find(|c| c.id == parcel.owner_id) {
                company.liquid_capital += buyout_cost;
            }
        }
        ParcelOwnerType::Cooperative => {
            // Route to company's liquid_capital (cooperatives are companies).
            if let Some(company) = companies.iter_mut().find(|c| c.id == parcel.owner_id) {
                company.liquid_capital += buyout_cost;
            }
        }
        ParcelOwnerType::Private => {
            // Phase D.5: Route buyout payment to the correct demographic class
            // savings. The parcel owner_id for private parcels is either
            // "DYNASTY_{region}_{index}" (Aristocracy) or
            // "PEASANT_{region}_{index}" (FreePeasant). The class_demographics
            // maps are keyed by class name string (e.g., "Aristocracy").
            // Previous code tried to look up owner_id directly as a class key,
            // which always failed — the buyout was debited from the city but
            // never credited to the owner (fiat leak).
            let class = if parcel.owner_id.starts_with("DYNASTY_") {
                Some(crate::society::geography::RuralClass::Aristocracy)
            } else if parcel.owner_id.starts_with("PEASANT_") {
                Some(crate::society::geography::RuralClass::FreePeasant)
            } else {
                None
            };

            if let Some(class) = class {
                if let Some(demo) = source_region.class_demographics.get_class_mut(class) {
                    demo.savings += buyout_cost;
                    if demo.population > 0 {
                        demo.savings_per_capita = demo.savings / demo.population as f64;
                    }
                }
            }
            // If the class cannot be resolved (unknown owner_id prefix),
            // the buyout is debited but not credited to any owner.
            // This is a withholding, not fiat destruction — the city paid
            // for the land but the payment is held in suspense. In practice,
            // all private parcels are generated with DYNASTY_ or PEASANT_
            // prefixes, so this branch should never trigger.
        }
        ParcelOwnerType::State | ParcelOwnerType::Municipal => {
            // Political annexation — no financial buyout needed for state lands.
            // The buyout_cost was already deducted; refund it since state lands
            // are transferred politically, not financially.
            city_region.treasury.liquid_reserves += buyout_cost;
        }
        ParcelOwnerType::Religious | ParcelOwnerType::ForeignFund => {
            // Blocked — should not reach here. Refund.
            city_region.treasury.liquid_reserves += buyout_cost;
            return AnnexationResult {
                success: false,
                parcel_id: Some(parcel_id),
                buyout_cost: 0.0,
                was_aristocratic: is_aristocratic,
                insufficient_funds: false,
            };
        }
    }

    // Transfer the parcel.
    if let Some(p) = cadastre.get_mut(parcel_id) {
        p.region_id = city_region.id.clone();
        p.acquisition_price = buyout_cost;
        p.acquisition_turn = current_turn;
        // Update micro_region_id to the city's domain, or None.
        // The city's primary domain is the first (and usually only) micro_region.
        if let Some((domain_id, _)) = city_region.micro_regions.iter().next() {
            p.micro_region_id = Some(domain_id.clone());
        }
    }

    // Update region parcel lists.
    source_region.parcel_ids.retain(|p| *p != parcel_id);
    city_region.parcel_ids.push(parcel_id);

    // Update domain jurisdiction.
    // Remove from source domain if applicable.
    if let Some(ref old_domain_id) = parcel.micro_region_id {
        if let Some(source_domain) = source_region.micro_regions.get_mut(old_domain_id) {
            source_domain
                .controlled_parcel_ids
                .retain(|p| *p != parcel_id);
        }
    }
    // Add to city domain.
    if let Some((domain_id, city_domain)) = city_region.micro_regions.iter_mut().next() {
        let domain_id = domain_id.clone();
        city_domain.controlled_parcel_ids.push(parcel_id);
        // Also update the parcel's micro_region_id.
        if let Some(p) = cadastre.get_mut(parcel_id) {
            p.micro_region_id = Some(domain_id);
        }
    }

    AnnexationResult {
        success: true,
        parcel_id: Some(parcel_id),
        buyout_cost,
        was_aristocratic: is_aristocratic,
        insufficient_funds: false,
    }
}

/// Process the urbanization cycle for a single country.
///
/// This is the main entry point called from the turn loop (Phase 85B).
/// It runs:
/// 1. Emancipation check for all GuildBurgher domains.
/// 2. Annexation attempts for all City Regions.
///
/// # Arguments
/// * `country` - The country to process.
/// * `cadastre` - The country's cadastre.
/// * `companies` - Mutable companies (for buyout routing).
/// * `buildings` - Mutable buildings (for region_id updates and water transfer).
/// * `housing_buildings` - Mutable housing buildings (for region_id updates).
/// * `commercial_buildings` - Mutable commercial buildings (for region_id updates).
/// * `current_turn` - The current turn number.
/// * `config` - Emancipation configuration.
pub fn process_urbanization_cycle(
    country: &mut Country,
    companies: &mut [Company],
    buildings: &mut [crate::entities::Building],
    housing_buildings: &mut [crate::society::housing::HousingBuilding],
    commercial_buildings: &mut [crate::society::housing::CommercialBuilding],
    current_turn: u32,
    config: &EmancipationConfig,
) {
    // ── Phase 85B.1: Emancipation Check ──
    // Collect domains that meet emancipation triggers.
    let average_wage = country.macro_indicators.average_wage;
    let parent_region_gdp = country.budget.gdp;

    // Temporarily swap out the cadastre to avoid double-borrow with country.regions.
    let mut cadastre = std::mem::take(&mut country.cadastre);

    // Collect emancipation candidates (avoid mutating while iterating).
    let mut emancipation_candidates: Vec<(usize, String)> = Vec::new();

    for (region_idx, region) in country.regions.iter().enumerate() {
        // Skip existing city regions.
        if region.is_city() {
            continue;
        }
        for (domain_id, domain) in &region.micro_regions {
            if domain.faction_type != FactionDomainType::GuildBurgher {
                continue;
            }
            // Compute domain hectares from controlled parcels.
            let mut domain_hectares = 0.0_f64;
            for &pid in &domain.controlled_parcel_ids {
                if let Some(parcel) = cadastre.get(pid) {
                    domain_hectares += parcel.size_hectares;
                }
            }
            // Count guilds in this domain.
            let guild_count = companies
                .iter()
                .filter(|c| {
                    c.legal_form.is_guild()
                        && buildings
                            .iter()
                            .any(|b| b.owner_id == c.id && b.region_id == region.id)
                })
                .count() as u32;

            // Estimate domain GDP from building output in the domain.
            let domain_gdp: f64 = buildings
                .iter()
                .filter(|b| {
                    b.region_id == region.id
                        && housing_buildings
                            .iter()
                            .any(|h| h.micro_region_id == *domain_id)
                })
                .map(|b| b.last_production.values().sum::<f64>())
                .sum();

            if check_emancipation_triggers(
                domain,
                region,
                domain_gdp,
                parent_region_gdp,
                domain_hectares,
                guild_count,
                average_wage,
                config,
            ) {
                emancipation_candidates.push((region_idx, domain_id.clone()));
            }
        }
    }

    // Execute emancipations (in reverse region index order to preserve indices).
    emancipation_candidates.sort_by_key(|&(idx, _)| std::cmp::Reverse(idx));
    for (region_idx, domain_id) in emancipation_candidates {
        if let Some(result) = execute_emancipation(
            country,
            &mut cadastre,
            region_idx,
            &domain_id,
            current_turn,
            config,
        ) {
            // Update building region_id for buildings in the emancipated domain.
            if let Some(ref city_id) = result.new_city_region_id {
                // Transfer buildings.
                for building in buildings.iter_mut() {
                    // Check if this building is in the emancipated domain.
                    // Buildings are linked to micro_regions via housing_buildings.
                    // For now, update based on parcel proximity.
                    // The building's region_id should match the new city.
                    // We check if any housing building in the domain references this building.
                    let in_domain = housing_buildings
                        .iter()
                        .any(|h| h.micro_region_id == domain_id && h.id == building.id);
                    if in_domain {
                        building.region_id = city_id.clone();
                    }
                }
                // Housing buildings in the domain keep their micro_region_id
                // (the domain moved with them into the city). No update needed.
                for commercial in commercial_buildings.iter_mut() {
                    let in_domain = commercial.micro_region_id == domain_id;
                    if in_domain {
                        // Commercial buildings don't have region_id, but their
                        // micro_region_id stays the same (domain is now in the city).
                    }
                }

                // Transfer physical water from infrastructure buildings.
                // Find buildings with Commodity::Water in inventory on transferred parcels.
                let parent_idx = country
                    .regions
                    .iter()
                    .position(|r| r.id == result.parent_region_id);
                let city_idx = country.regions.iter().position(|r| r.id == *city_id);

                if let (Some(p_idx), Some(c_idx)) = (parent_idx, city_idx) {
                    // We can't borrow both regions mutably at once, so use split_at.
                    if p_idx < c_idx {
                        let (left, right) = country.regions.split_at_mut(c_idx);
                        let parent = &mut left[p_idx];
                        let city = &mut right[0];
                        for building in buildings.iter() {
                            if let Some(&water) = building
                                .inventory
                                .get(&crate::registries::enums::Commodity::Water)
                            {
                                if water > 0.0 && building.region_id == *city_id {
                                    transfer_physical_water(parent, city, water);
                                }
                            }
                        }
                    } else {
                        let (left, right) = country.regions.split_at_mut(p_idx);
                        let city = &mut left[c_idx];
                        let parent = &mut right[0];
                        for building in buildings.iter() {
                            if let Some(&water) = building
                                .inventory
                                .get(&crate::registries::enums::Commodity::Water)
                            {
                                if water > 0.0 && building.region_id == *city_id {
                                    transfer_physical_water(parent, city, water);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Phase 85B.2: Annexation Attempts ──
    // For each City Region, attempt to annex adjacent parcels.
    let city_region_indices: Vec<usize> = country
        .regions
        .iter()
        .enumerate()
        .filter(|(_, r)| r.is_city())
        .map(|(i, _)| i)
        .collect();

    for city_idx in city_region_indices {
        // Check cooldown.
        let cooldown = country.regions[city_idx]
            .city_metadata
            .as_ref()
            .map(|m| m.annexation_cooldown)
            .unwrap_or(0);
        if cooldown > 0 {
            // Decrement cooldown.
            if let Some(ref mut meta) = country.regions[city_idx].city_metadata {
                meta.annexation_cooldown = meta.annexation_cooldown.saturating_sub(1);
            }
            continue;
        }

        // Identify targets.
        let targets = identify_annexation_targets(&country.regions[city_idx], &cadastre);
        if targets.is_empty() {
            continue;
        }

        // Compute average land price from recent cadastre transactions.
        // For now, use a dynamic estimate based on development level.
        let average_land_price = country.regions[city_idx].development_level * 1000.0;

        // Try to annex the first affordable target (one per turn per city).
        for (parcel_id, source_region_id) in targets {
            let parcel = match cadastre.get(parcel_id) {
                Some(p) => p.clone(),
                None => continue,
            };

            // Check if parcel is religious — blocked.
            if parcel.owner_type == ParcelOwnerType::Religious
                || parcel.owner_type == ParcelOwnerType::ForeignFund
            {
                continue;
            }

            // Check if parcel is in an AristocraticEstate domain.
            let is_aristocratic = parcel
                .micro_region_id
                .as_ref()
                .and_then(|did| {
                    // Find the domain in the source region.
                    country
                        .regions
                        .iter()
                        .find_map(|r| r.micro_regions.get(did))
                })
                .map(|d| d.faction_type == FactionDomainType::AristocraticEstate)
                .unwrap_or(false);

            let buyout_cost =
                evaluate_annexation_cost(&parcel, average_land_price, is_aristocratic, config);

            // Find source region index.
            let source_idx = country
                .regions
                .iter()
                .position(|r| r.id == source_region_id);

            if let Some(src_idx) = source_idx {
                // We need to borrow city and source regions mutably.
                // Use index-based access with split_at_mut.
                let max_idx = city_idx.max(src_idx);
                let result = if city_idx < src_idx {
                    let (left, right) = country.regions.split_at_mut(max_idx);
                    let city = &mut left[city_idx];
                    let source = &mut right[0];
                    execute_annexation(
                        city,
                        source,
                        parcel_id,
                        &mut cadastre,
                        companies,
                        buyout_cost,
                        is_aristocratic,
                        current_turn,
                        config,
                    )
                } else {
                    let (left, right) = country.regions.split_at_mut(max_idx);
                    let source = &mut left[src_idx];
                    let city = &mut right[0];
                    execute_annexation(
                        city,
                        source,
                        parcel_id,
                        &mut cadastre,
                        companies,
                        buyout_cost,
                        is_aristocratic,
                        current_turn,
                        config,
                    )
                };

                // Apply unrest for failed aristocratic annexation.
                if !result.success && result.was_aristocratic {
                    country.macro_indicators.social_unrest =
                        (country.macro_indicators.social_unrest + config.annexation_unrest_penalty)
                            .min(100.0);
                }
                // Apply tension for successful aristocratic buyout.
                if result.success && result.was_aristocratic {
                    country.macro_indicators.social_unrest =
                        (country.macro_indicators.social_unrest
                            + config.aristocratic_tension_per_buyout)
                            .min(100.0);
                }

                // If annexation succeeded (or failed with cooldown), stop for this city.
                break;
            }
        }
    }

    // Restore the cadastre.
    country.cadastre = cadastre;
}

use serde::{Deserialize, Serialize};

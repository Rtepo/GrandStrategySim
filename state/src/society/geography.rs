#![allow(missing_docs)]

use rand::Rng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use crate::state::treasury::Treasury;
use crate::registries::enums::Commodity;
use crate::politics::local_government::{
    initialize_regional_governance, AdministrativeStatus, RegionalHeadType,
};

/// Node type for graph-based geography system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    #[default]
    /// Land region with population and economy
    LandRegion,
    /// Inland sea node (e.g., Baltic Sea)
    SeaNode,
    /// Open ocean node (e.g., Atlantic)
    OceanNode,
}

/// Edge type for graph connections between nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeType {
    /// Land-land adjacency
    LandBorder,
    /// Land-sea interface (coastline)
    Coastline,
    /// Sea-sea connection
    SeaLane,
    /// River crossing
    River,
}

/// Climate profile for seasonal modifiers (Phase 6.1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClimateProfile {
    #[default]
    #[serde(rename = "umiarkowany")]
    Temperate,  // Four distinct seasons, moderate extremes

    #[serde(rename = "górski")]
    Mountainous,  // Harsh winters, mild summers, high energy demand

    #[serde(rename = "morski")]
    Coastal,  // Mild winters, tourism boost in summer

    #[serde(rename = "kontynentalny")]
    Continental,  // Extreme temperature swings, harsh winters

    #[serde(rename = "tropikalny")]
    Tropical,  // Hot year-round, monsoon season

    #[serde(rename = "pustynny")]
    Desert,  // Extreme heat, cold nights, water scarcity

    #[serde(rename = "arktyczny")]
    Arctic,  // Permafrost, extreme cold, limited activity
}

/// Phase 47: Pick a varied climate profile for non-capital regions.
/// Weighted toward temperate/continental (most common globally),
/// with smaller chances for mountainous, coastal, tropical, desert, arctic.
fn pick_climate_profile(rng: &mut impl rand::Rng) -> ClimateProfile {
    let roll: f64 = rng.gen();
    if roll < 0.30 {
        ClimateProfile::Temperate
    } else if roll < 0.55 {
        ClimateProfile::Continental
    } else if roll < 0.70 {
        ClimateProfile::Mountainous
    } else if roll < 0.82 {
        ClimateProfile::Coastal
    } else if roll < 0.92 {
        ClimateProfile::Tropical
    } else if roll < 0.97 {
        ClimateProfile::Desert
    } else {
        ClimateProfile::Arctic
    }
}

/// Distinguishes between rural and urban demographic classes
/// Required to prevent key collisions in labor ledgers (Phase 6.2)
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum DemographyType {
    #[serde(rename = "wiejski")]
    Rural,
    #[serde(rename = "miejski")]
    Urban,
}

/// Structured edge between graph nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Edge {
    /// Target node ID
    #[serde(rename = "cel_węzła")]
    pub target_node: String,
    /// Type of edge connection
    #[serde(rename = "typ_krawędzi")]
    pub edge_type: EdgeType,
    /// Distance for pathfinding cost (kilometers)
    #[serde(rename = "odległość")]
    pub distance: f64,
    /// Whether ships can traverse this edge
    #[serde(rename = "nawigowalny")]
    pub is_navigable: bool,
    /// Phase 30: Territorial owner of this edge (for SeaLane edges).
    /// None = international waters. Some("country") = territorial waters
    /// of that country, subject to blockades and maritime transit tariffs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub territorial_owner: Option<String>,
}

/// Geological formation type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormationType {
    /// Mountain range (e.g., Carpathians)
    MountainRange,
    /// Sedimentary basin (e.g., Silesian Coal Basin)
    SedimentaryBasin,
    /// Rift valley (e.g., East African Rift)
    RiftValley,
    /// Volcanic arc (e.g., Andes)
    VolcanicArc,
    /// Continental shelf (offshore oil/gas)
    ContinentalShelf,
}

/// Resource deposit within a geological formation.
/// Phase 21A: Refactored to use `Commodity` enum natively — no Polish strings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceDeposit {
    /// Tradeable commodity this deposit yields (native enum, no string mapping).
    #[serde(rename = "commodity")]
    pub commodity: Commodity,
    /// Estimated total reserves (tons/barrels) — original quantity at discovery.
    #[serde(rename = "rezerwy_szacunkowe")]
    pub estimated_reserves: f64,
    /// Current remaining reserves (depletes as the deposit is mined).
    #[serde(rename = "rezerwy_aktualne", default)]
    pub current_reserves: f64,
    /// Extraction cost per unit.
    #[serde(rename = "koszt_wydobycia")]
    pub extraction_cost: f64,
    /// Base quality 0-1, affects processing efficiency.
    #[serde(rename = "jakość")]
    pub quality: f64,
    /// Effective quality 0-1 (decays as the deposit is depleted).
    #[serde(rename = "jakość_aktualna", default)]
    pub current_quality: f64,
    /// Depth below surface in meters (gates tech access).
    #[serde(rename = "głębokość", default)]
    pub depth: f64,
    /// Whether this deposit has been discovered (fog-of-war).
    #[serde(rename = "odkryte", default)]
    pub discovered: bool,
}

/// Geological formation spanning multiple regions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeologicalFormation {
    /// Unique formation ID
    #[serde(rename = "id_formacji")]
    pub id: String,
    /// Formation name (e.g., "Carpathian Mountains")
    #[serde(rename = "nazwa")]
    pub name: String,
    /// Type of geological formation
    #[serde(rename = "typ_formacji")]
    pub formation_type: FormationType,
    /// Resource deposits in this formation
    #[serde(rename = "złoża_zasobów")]
    pub resource_deposits: BTreeMap<String, ResourceDeposit>,
    /// Region IDs intersecting this formation
    #[serde(rename = "nakładające_się_regiony")]
    pub overlapping_regions: Vec<String>,
    /// Total area in square kilometers
    #[serde(rename = "całkowita_powierzchnia")]
    pub total_area: f64,
}

/// Climate type assigned to a country or region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Climate {
    #[default]
    #[serde(rename = "Urodzajny")]
    Fertile,
    #[serde(rename = "Pustynny")]
    Desert,
    #[serde(rename = "Górzysty")]
    Mountainous,
    #[serde(rename = "Zrównoważony")]
    Balanced,
}

impl Climate {
    fn random(rng: &mut impl Rng) -> Self {
        match rng.gen_range(0..4) {
            0 => Climate::Fertile,
            1 => Climate::Desert,
            2 => Climate::Mountainous,
            _ => Climate::Balanced,
        }
    }

    fn soil_profile(&self, rng: &mut impl Rng) -> (BTreeMap<String, f64>, f64) {
        let profile = match self {
            Climate::Fertile => BTreeMap::from([
                ("Class_I".to_string(), 0.2),
                ("Class_II".to_string(), 0.3),
                ("Class_III".to_string(), 0.3),
                ("Class_IV".to_string(), 0.15),
                ("Class_V".to_string(), 0.05),
                ("Class_VI".to_string(), 0.0),
            ]),
            Climate::Desert => BTreeMap::from([
                ("Class_I".to_string(), 0.01),
                ("Class_II".to_string(), 0.04),
                ("Class_III".to_string(), 0.1),
                ("Class_IV".to_string(), 0.2),
                ("Class_V".to_string(), 0.3),
                ("Class_VI".to_string(), 0.35),
            ]),
            Climate::Mountainous => BTreeMap::from([
                ("Class_I".to_string(), 0.05),
                ("Class_II".to_string(), 0.1),
                ("Class_III".to_string(), 0.25),
                ("Class_IV".to_string(), 0.3),
                ("Class_V".to_string(), 0.2),
                ("Class_VI".to_string(), 0.1),
            ]),
            Climate::Balanced => BTreeMap::from([
                ("Class_I".to_string(), 0.1),
                ("Class_II".to_string(), 0.2),
                ("Class_III".to_string(), 0.2),
                ("Class_IV".to_string(), 0.25),
                ("Class_V".to_string(), 0.15),
                ("Class_VI".to_string(), 0.1),
            ]),
        };
        let arable_mult = match self {
            Climate::Fertile => rng.gen_range(1.2..1.8),
            Climate::Desert => rng.gen_range(0.3..0.6),
            Climate::Mountainous => rng.gen_range(0.5..0.9),
            Climate::Balanced => rng.gen_range(0.8..1.2),
        };
        (profile, arable_mult)
    }

    fn mine_limits(&self, base_mines: i64, rng: &mut impl Rng) -> BTreeMap<String, i64> {
        let mut limits = base_mine_template();
        match self {
            Climate::Mountainous => {
                limits.insert("Kopalnia Węgla".to_string(), (base_mines as f64 * rng.gen_range(1.0..3.0)) as i64);
                limits.insert("Kopalnia Żelaza".to_string(), (base_mines as f64 * rng.gen_range(1.5..3.5)) as i64);
                limits.insert("Kopalnia Boksytu".to_string(), (base_mines as f64 * rng.gen_range(0.5..2.0)) as i64);
                limits.insert("Kopalnie Metali Kolorowych".to_string(), (base_mines as f64 * rng.gen_range(1.0..2.5)) as i64);
                limits.insert("Szyby Naftowe".to_string(), (base_mines as f64 * rng.gen_range(0.0..0.5)) as i64);
                limits.insert("Kopalnie Gazu Ziemnego".to_string(), (base_mines as f64 * rng.gen_range(0.0..0.5)) as i64);
            }
            Climate::Desert => {
                limits.insert("Kopalnia Węgla".to_string(), (base_mines as f64 * rng.gen_range(0.0..1.0)) as i64);
                limits.insert("Kopalnia Żelaza".to_string(), (base_mines as f64 * rng.gen_range(0.5..1.5)) as i64);
                limits.insert("Kopalnia Boksytu".to_string(), (base_mines as f64 * rng.gen_range(0.0..1.0)) as i64);
                limits.insert("Kopalnie Metali Kolorowych".to_string(), (base_mines as f64 * rng.gen_range(0.5..1.5)) as i64);
                limits.insert("Szyby Naftowe".to_string(), (base_mines as f64 * rng.gen_range(2.0..5.0)) as i64);
                limits.insert("Kopalnie Gazu Ziemnego".to_string(), (base_mines as f64 * rng.gen_range(1.5..4.0)) as i64);
            }
            Climate::Fertile => {
                limits.insert("Kopalnia Węgla".to_string(), (base_mines as f64 * rng.gen_range(1.0..2.0)) as i64);
                limits.insert("Kopalnia Żelaza".to_string(), (base_mines as f64 * rng.gen_range(0.5..1.5)) as i64);
                limits.insert("Kopalnia Boksytu".to_string(), (base_mines as f64 * rng.gen_range(0.0..0.5)) as i64);
                limits.insert("Kopalnie Metali Kolorowych".to_string(), (base_mines as f64 * rng.gen_range(0.2..1.0)) as i64);
                limits.insert("Szyby Naftowe".to_string(), (base_mines as f64 * rng.gen_range(0.2..1.0)) as i64);
                limits.insert("Kopalnie Gazu Ziemnego".to_string(), (base_mines as f64 * rng.gen_range(0.5..1.5)) as i64);
            }
            Climate::Balanced => {
                for key in limits.keys().cloned().collect::<Vec<_>>() {
                    let value = (base_mines as f64 * rng.gen_range(0.5..2.0)) as i64;
                    limits.insert(key, value);
                }
            }
        }
        limits
    }
}

fn base_mine_template() -> BTreeMap<String, i64> {
    BTreeMap::from([
        ("Kopalnia Węgla".to_string(), 0),
        ("Kopalnia Żelaza".to_string(), 0),
        ("Kopalnia Boksytu".to_string(), 0),
        ("Kopalnie Metali Kolorowych".to_string(), 0),
        ("Szyby Naftowe".to_string(), 0),
        ("Kopalnie Gazu Ziemnego".to_string(), 0),
    ])
}

/// Land sub-type for specific categories (Wastelands and WaterBodies).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LandSubType {
    #[default]
    /// Generic sub-type for most categories
    Generic,
    /// Freshwater (lakes, rivers)
    Freshwater,
    /// Saltwater (coastal waters, inland seas)
    Saltwater,
    /// Functional wasteland (tundra, semi-desert - permits limited infrastructure/mining)
    FunctionalWasteland,
    /// Non-functional wasteland (deep deserts, high peaks - absolutely zero construction allowed)
    NonFunctionalWasteland,
}

/// Soil class data within land categories (subordinate layer).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SoilClassData {
    /// Soil class identifier in English (e.g., "Class_I" through "Class_VI")
    #[serde(rename = "klasa_gleby")]
    pub soil_class: String,
    /// Area in hectares
    #[serde(rename = "hektary")]
    pub area_hectares: f64,
    /// Ownership distribution (reuses existing ClassLandDistribution)
    #[serde(rename = "własność")]
    pub ownership: ClassLandDistribution,
    /// Fertility index 0-1, crop yield multiplier
    #[serde(rename = "indeks_urodzajności")]
    pub fertility_index: f64,
    /// Erosion risk 0-1, degradation probability
    #[serde(rename = "ryzyko_erozji")]
    pub erosion_risk: f64,
}

/// Land category data for a specific land use type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LandCategoryData {
    /// Area in hectares
    #[serde(rename = "hektary")]
    pub area_hectares: f64,
    /// Soil profile (subordinate soil system)
    #[serde(rename = "profil_gleb", default)]
    pub soil_profile: BTreeMap<String, SoilClassData>,
    /// Ownership distribution (existing structure)
    #[serde(rename = "dystrybucja_własności", default)]
    pub ownership_distribution: ClassLandDistribution,
    /// Ecological health 0-1, affected by pollution
    #[serde(rename = "zdrowie_ekologiczne")]
    pub ecological_health: f64,
    /// Development potential 0-1, ease of transformation
    #[serde(rename = "potencja_rozwojowy")]
    pub development_potential: f64,
    /// Sub-type for specific categories
    #[serde(rename = "podtyp", default)]
    pub sub_type: LandSubType,
    /// Water pollution level 0-1 (for WaterBodies)
    #[serde(rename = "zanieczyszczenie_wody", default)]
    pub water_pollution: f64,
    /// Natural pollution decay rate (water self-cleans)
    #[serde(rename = "tempo_rozpadu_zanieczyszczeń", default)]
    pub pollution_decay_rate: f64,
    /// Sewage inflow this turn
    #[serde(rename = "przepływ_ścieków", default)]
    pub sewage_inflow: f64,
}

/// Land use category (primary layer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LandCategory {
    /// Cities, towns, suburbs
    Urbanized,
    /// Factories, refineries, mines
    Industrial,
    /// Natural and managed forests
    Forests,
    /// Pastures, meadows, steppes
    Grasslands,
    /// Cropland (where soil classes live)
    Agricultural,
    /// Swamps, marshes, bogs
    Wetlands,
    /// Freshwater lakes, rivers
    WaterBodies,
    /// Abandoned/contaminated land
    Wastelands,
}

/// Land use inventory for a region.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LandUseInventory {
    /// Total area in hectares
    #[serde(rename = "całkowita_powierzchnia")]
    pub total_area: f64,
    /// Land categories with their data
    #[serde(rename = "kategorie", default)]
    pub categories: BTreeMap<String, LandCategoryData>,
}

impl LandCategoryData {
    /// Process water pollution for one turn
    ///
    /// # Arguments
    /// * `sewage_inflow` - Sewage inflow this turn
    ///
    /// # Returns
    /// * Pollution increase (0-1)
    pub fn process_water_pollution(&mut self, sewage_inflow: f64) -> f64 {
        // Natural decay
        let natural_decay = self.water_pollution * self.pollution_decay_rate;
        self.water_pollution = (self.water_pollution - natural_decay).max(0.0);
        
        // Accumulate sewage if inflow exceeds decay capacity
        let decay_capacity = self.water_pollution * self.pollution_decay_rate * 10.0;
        if sewage_inflow > decay_capacity {
            let pollution_increase = (sewage_inflow - decay_capacity) * 0.01;
            self.water_pollution = (self.water_pollution + pollution_increase).min(1.0);
            pollution_increase
        } else {
            0.0
        }
    }
    
    /// Calculate pollution health impact
    ///
    /// # Returns
    /// * Disease incidence multiplier (0-1)
    pub fn calculate_pollution_health_impact(&self) -> f64 {
        // Water pollution increases disease risk
        if self.water_pollution > 0.7 {
            0.3 // 30% increase in disease incidence
        } else if self.water_pollution > 0.5 {
            0.15
        } else if self.water_pollution > 0.3 {
            0.05
        } else {
            0.0
        }
    }
}

impl LandUseInventory {
    /// Get data for a specific land category.
    pub fn get_category(&self, category: LandCategory) -> Option<&LandCategoryData> {
        let key = serde_json::to_string(&category).unwrap_or_default();
        self.categories.get(&key)
    }

    /// Get mutable data for a specific land category.
    pub fn get_category_mut(&mut self, category: LandCategory) -> Option<&mut LandCategoryData> {
        let key = serde_json::to_string(&category).unwrap_or_default();
        self.categories.get_mut(&key)
    }

    /// Ensure a category exists, creating it with default data if missing.
    pub fn ensure_category(&mut self, category: LandCategory) -> &mut LandCategoryData {
        let key = serde_json::to_string(&category).unwrap_or_default();
        self.categories.entry(key).or_default()
    }
}

/// Land ownership distribution by class within a soil quality class
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ClassLandDistribution {
    /// Hectares owned by Aristocracy (latifundia estates)
    #[serde(rename = "hektary_arystokracja", default)]
    pub aristocracy_hectares: i64,
    
    /// Hectares owned by Free Peasants (smallholdings)
    #[serde(rename = "hektary_wolni_chłopi", default)]
    pub free_peasant_hectares: i64,
    
    /// Hectares owned by State (crown lands)
    #[serde(rename = "hektary_skarb_panstwa", default)]
    pub state_hectares: i64,
    
    /// Hectares owned by Corporations (agricultural firms)
    #[serde(rename = "hektary_korporacje", default)]
    pub corporation_hectares: i64,
    
    /// Hectares owned by Communities/Cooperatives
    #[serde(rename = "hektary_wspólnoty", default)]
    pub community_hectares: i64,
    
    /// Hectares owned by Municipalities (JST)
    #[serde(rename = "hektary_miejskie", default)]
    pub municipal_hectares: i64,
}

impl ClassLandDistribution {
    /// Total hectares in this soil class
    pub fn total(&self) -> i64 {
        self.aristocracy_hectares + self.free_peasant_hectares 
            + self.state_hectares + self.corporation_hectares 
            + self.community_hectares + self.municipal_hectares
    }
    
    /// Aristocracy ownership share (0-1)
    pub fn aristocracy_share(&self) -> f64 {
        let total = self.total();
        if total == 0 { 0.0 } else { self.aristocracy_hectares as f64 / total as f64 }
    }
}

fn default_winter_mortality_multiplier() -> f64 {
    1.0
}

/// A single region with class-based land ownership
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Region {
    pub id: String,
    /// Phase 35: Human-readable display name (e.g., "Nordia Capital District",
    /// "Northern Valley"). Falls back to `id` if empty (for old saves).
    #[serde(default)]
    pub display_name: String,
    pub owner_country: String,
    pub population: i64,
    pub gdp: f64,
    pub gdp_pc: f64,
    #[serde(alias = "klimat")]
    pub climate: Climate,
    #[serde(alias = "profil_gleb")]
    pub soil_profile: BTreeMap<String, f64>,
    #[serde(alias = "ziemia_orna_max")]
    pub arable_land_max: i64,
    #[serde(alias = "ziemia_orna_wykorzystana")]
    pub arable_land_used: i64,
    #[serde(alias = "limity_wydobycia")]
    pub extraction_limits: BTreeMap<String, i64>,
    #[serde(alias = "limity_wykorzystane")]
    pub extraction_used: BTreeMap<String, i64>,
    #[serde(alias = "zasoby")]
    pub resources: Map<String, Value>,
    pub is_capital: bool,
    /// NEW: Graph node type (LandRegion, SeaNode, OceanNode)
    #[serde(rename = "typ_węzła", default)]
    pub node_type: NodeType,
    /// NEW: Structured edges replacing simple adjacency list
    #[serde(rename = "krawędzie", default)]
    pub edges: Vec<Edge>,
    /// DEPRECATED: Kept for backward compatibility during migration
    #[serde(rename = "sąsiedztwo", default)]
    pub adjacency: Vec<String>,
    
    // NEW: Regional class-based land distribution
    #[serde(rename = "dystrybucja_gruntu", default)]
    pub land_distribution: BTreeMap<String, ClassLandDistribution>,
    
    // NEW: Class demographics
    #[serde(rename = "demografia_klas", default)]
    pub class_demographics: RegionalClassDemographics,
    
    // NEW: Regional governance (JST)
    #[serde(rename = "zarzad_regionalne", default)]
    pub governance: Option<crate::politics::local_government::RegionalGovernance>,

    // NEW: Capacity pool generated by infrastructure companies
    #[serde(rename = "pojemność_regionalna", default)]
    pub capacity_pool: BTreeMap<crate::infrastructure::CapacityType, f64>,

    // NEW: Capacity utilization by type (0.0-1.0)
    #[serde(rename = "wykorzystanie_pojemności", default)]
    pub capacity_utilization: BTreeMap<crate::infrastructure::CapacityType, f64>,

    // NEW: Market prices for capacity units (for Private funding model)
    #[serde(rename = "ceny_pojemności", default)]
    pub capacity_prices: BTreeMap<crate::infrastructure::CapacityType, f64>,

    // NEW: Land use inventory (Phase 5.3)
    #[serde(rename = "inwentarz_ziemi", default)]
    pub land_use_inventory: LandUseInventory,
    
    // Phase 6.1: Climate profile for seasonal modifiers
    #[serde(rename = "profil_klimatyczny", default)]
    pub climate_profile: ClimateProfile,

    // NEW: Micro-regions within this region (Phase 6.1)
    #[serde(rename = "mikroregiony", default)]
    pub micro_regions: BTreeMap<String, MicroRegion>,

    // STAGE C: Regional treasury for cascading tax routing
    #[serde(rename = "skarbiec", default)]
    pub treasury: Treasury,

    // STAGE C: Microregion budgets for tax routing
    #[serde(rename = "budżety_mikroregionów", default)]
    pub microregion_budgets: HashMap<String, MicroRegionBudget>,

    // Phase 8: Winter mortality multiplier (1.0 = baseline, >1.0 = increased mortality)
    // Written by process_utility_consumption, read and reset by process_demographics_and_labor
    #[serde(rename = "współczynnik_śmiertelności_zimowej", default = "default_winter_mortality_multiplier")]
    pub winter_mortality_multiplier: f64,

    /// Phase 17A: Holy site trait for this region, if any.
    /// Links to a religion engine key and enables pilgrimage tourism.
    #[serde(rename = "święte_miejsce", default, skip_serializing_if = "Option::is_none")]
    pub holy_site: Option<HolySite>,

    /// Phase 23D: Geographic traits enabling maritime, riverine, and aviation transport.
    #[serde(rename = "cechy_geograficzne", default)]
    pub geographic_traits: GeographicTraits,

    /// Phase 30: 2D spatial coordinate X for geographic computations
    /// (air cargo routing, overflight fee calculation).
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub coord_x: f64,

    /// Phase 30: 2D spatial coordinate Y for geographic computations
    /// (air cargo routing, overflight fee calculation).
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub coord_y: f64,

    /// Phase 47: Regional development level (0.0 = underdeveloped backwater,
    /// 1.0 = highly developed urban hub). Assigned at genesis, evolves slowly
    /// during gameplay based on investment and migration.
    /// Drives: Services/LightIndustry company count bias, initial class savings,
    /// retail format selection, internal migration attractiveness.
    #[serde(default = "default_development_level")]
    pub development_level: f64,
    /// Phase 58: Parcel IDs in the country's Cadastre that belong to this region.
    /// Stored as a Vec of serialized ParcelId keys for serde compatibility.
    /// Filled during cadastre generation and updated when parcels are split/merged.
    #[serde(default)]
    pub parcel_ids: Vec<crate::society::cadastre::ParcelId>,
}

/// Phase 47: Default development level for old saves (conservative mid-low).
fn default_development_level() -> f64 {
    0.3
}

/// Holy site associated with a specific religion.
/// Enables pilgrimage tourism demand and boosts religious authority.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HolySite {
    /// Religion engine key this site is sacred to (e.g., "catholicism").
    #[serde(rename = "religia", default)]
    pub religion_key: String,
    /// Pilgrimage attractiveness (0.0–1.0, higher = more famous).
    #[serde(rename = "atrakcyjność_pielgrzymkowa", default)]
    pub pilgrimage_attractiveness: f64,
    /// Display name for localization (Polish).
    #[serde(rename = "nazwa", default)]
    pub display_name: String,
}

/// Phase 23D: Geographic traits that enable specialized transport modes.
///
/// These traits are assigned during world generation based on graph edges
/// (e.g., a region with a `Coastline` edge gets `has_coastline = true`).
/// They unlock ports, ships/barges, and airports for freight routing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GeographicTraits {
    /// Region borders a sea/ocean → enables ports and maritime freight.
    #[serde(rename = "wybrzeże", default)]
    pub has_coastline: bool,
    /// Region has a navigable river → enables barge/river freight.
    #[serde(rename = "rzeka_żeglowna", default)]
    pub has_navigable_river: bool,
    /// Region has a mountain pass → enables land freight but with high friction.
    #[serde(rename = "przełęcz_górska", default)]
    pub has_mountain_pass: bool,
    /// Region has an airport → enables aviation freight (late-game).
    /// Set by construction, not by geography, but stored here for routing.
    #[serde(rename = "lotnisko", default)]
    pub has_airport: bool,
}

/// Rural demographic class with distinct economic behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuralClass {
    /// Aristocracy - owns large estates, employs serfs/laborers
    Aristocracy,
    /// Free Peasants - own smallholdings, family labor
    FreePeasant,
    /// Serfs/Tied Peasants - tied to latifundia, unpaid labor
    Serf,
    /// Landless Laborers/Komornicy - work for wages, no land
    LandlessLaborer,
}

/// Health status for demographic classes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    #[default]
    /// 90-100% capacity, minimal healthcare needs
    Excellent,
    /// 75-89% capacity, routine maintenance
    Good,
    /// 50-74% capacity, periodic intervention needed
    Fair,
    /// 25-49% capacity, frequent care required
    Poor,
    /// 0-24% capacity, emergency/high-dependency care
    Critical,
}

/// Dependency level for demographic classes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DependencyLevel {
    #[default]
    /// Full self-care, can work normally
    Independent,
    /// Requires some assistance, limited work capacity
    PartiallyDependent,
    /// Requires 24/7 care, cannot work
    FullyDependent,
}

/// Political sentiment distribution for a demographic class
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PoliticalSentiment {
    /// Percentage of population that is Loyalist (pro-regime, 0-1)
    #[serde(rename = "lojalisci", default)]
    pub loyalists: f64,
    
    /// Percentage of population that is Undecided (swing voters, 0-1)
    #[serde(rename = "niezdecydowani", default)]
    pub undecided: f64,
    
    /// Percentage of population that is Radical (anti-regime, 0-1)
    #[serde(rename = "radykałowie", default)]
    pub radicals: f64,
    
    /// Sentiment volatility (0-1, higher = faster shifts)
    #[serde(rename = "zmienność", default)]
    pub volatility: f64,
    
    /// Turn when sentiment was last recalculated
    #[serde(rename = "turn_ostatniej_aktualizacji", default)]
    pub last_update_turn: u32,
}

impl PoliticalSentiment {
    /// Ensures the three components sum to 1.0 (normalization)
    pub fn normalize(&mut self) {
        let total = self.loyalists + self.undecided + self.radicals;
        if total > 0.0 {
            self.loyalists /= total;
            self.undecided /= total;
            self.radicals /= total;
        }
    }
}

/// Historical quality of life snapshot for YoY comparison (Phase 6.1)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HistoricalQualityOfLife {
    /// Savings per capita at this turn in previous year
    #[serde(rename = "oszczędności_na_osobę_rok_temu", default)]
    pub savings_per_capita_yoy: f64,
    
    /// Real wage at this turn in previous year
    #[serde(rename = "płaca_realna_rok_temu", default)]
    pub real_wage_yoy: f64,
    
    /// Inflation rate at this turn in previous year
    #[serde(rename = "inflacja_rok_temu", default)]
    pub inflation_yoy: f64,
    
    /// Turn when this snapshot was taken (for validation)
    #[serde(rename = "turn_zdjęcia", default)]
    pub snapshot_turn: u32,
}

/// Demographic and economic data for a rural class within a region
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ClassDemographics {
    /// Population count for this class
    /// NOTE: For Serf class, this is NOT stored statically - aggregated dynamically from Latifundia
    #[serde(rename = "populacja", default)]
    pub population: i64,

    /// Total personal savings/capital held by this class
    #[serde(rename = "oszczędności", default)]
    pub savings: f64,

    /// Phase 35: Outstanding consumer debt principal owed to banks.
    /// When a bank issues a B2C loan, `savings` increases and `debt` increases
    /// equally. Every turn, a portion of `savings` pays down `debt` plus
    /// interest (which flows back to the issuing bank as B2C revenue).
    #[serde(default)]
    pub debt: f64,

    /// Average per-capita savings
    #[serde(rename = "oszczędności_na_osobę", default)]
    pub savings_per_capita: f64,

    /// Subsistence consumption rate (0-1, fraction of income needed for survival)
    #[serde(rename = "konsumpcja_subsystencjalna", default)]
    pub subsistence_rate: f64,

    /// Current economic status
    #[serde(rename = "status_ekonomiczny", default)]
    pub economic_status: EconomicStatus,

    /// Labor force participation rate (0-1)
    #[serde(rename = "aktywność_zawodowa", default)]
    pub labor_participation: f64,

    /// Remaining labor time for subsistence farming (0-1, 1.0 = full time available)
    /// Used for Serf class to calculate survival based on corvée demands
    #[serde(rename = "czas_pracy_subsystencjalny", default)]
    pub subsistence_labor_time: f64,

    /// Average health status for this class
    #[serde(rename = "status_zdrowia", default)]
    pub health_status: HealthStatus,

    /// Health degradation rate per turn (age + labor factors)
    #[serde(rename = "współczynnik_degradacji", default)]
    pub health_degradation_rate: f64,

    /// Dependency level for this class
    #[serde(rename = "poziom_zależności", default)]
    pub dependency_level: DependencyLevel,

    /// Number of dependents requiring care
    #[serde(rename = "liczba_zależnych", default)]
    pub dependent_count: i64,

    /// Political sentiment distribution
    #[serde(rename = "sentyment_polityczny", default)]
    pub political_sentiment: PoliticalSentiment,

    /// Trade union affiliation (if any)
    #[serde(rename = "przynależność_związkowa", skip_serializing_if = "Option::is_none")]
    pub union_affiliation: Option<String>,
    
    /// Phase 6.1: Historical QoL snapshots for YoY comparison (indexed by turn 1-24)
    #[serde(rename = "historia_jakości_życia", default)]
    pub historical_qol: [HistoricalQualityOfLife; 24],

    /// Phase 6.1: Short-term memory: QoL snapshot from previous turn (for Turn-over-Turn fallback in Year 1)
    #[serde(rename = "qol_poprzedniego_turnu", default)]
    pub previous_turn_qol: HistoricalQualityOfLife,

    /// Phase 6.2: Available FTE this class can offer to the labor market
    /// Maximum: 1.5 * population (full-time + half-time secondary job)
    #[serde(rename = "dostępne_fte", default)]
    pub available_fte: f64,

    /// Phase 6.2: FTE currently allocated to companies this turn
    #[serde(rename = "przydzielone_fte", default)]
    pub allocated_fte: f64,

    /// Resurrection Phase 2: Brokerage account for direct retail securities trading.
    #[serde(rename = "rachunek_maklerski", skip_serializing_if = "Option::is_none", default)]
    pub brokerage_account: Option<crate::securities::BrokerageAccount>,

    /// Phase 13: Religion practiced by this demographic class (e.g., "Katolicyzm", "Islam").
    /// Defaults to country-level religion if empty (migration on load).
    #[serde(rename = "religia", default, skip_serializing_if = "String::is_empty")]
    pub religion: String,

    /// Phase 18A: Legal status of this class (Citizen, Resident, TemporaryWorker, Illegal).
    #[serde(rename = "status_prawny", default)]
    pub legal_status: crate::economy::legal_status::LegalStatus,

    /// Phase 18A: Undocumented/illegal population within this class.
    /// These are shadow workers not counted in official labor statistics.
    #[serde(rename = "nielegalna_populacja", default)]
    pub illegal_population: i64,

    /// Phase 24D: FTE permanently lost to OHS accidents / disasters.
    /// These workers are alive but unable to work (disabled/injured).
    /// Subtracted from `available_fte` when casualties occur.
    #[serde(rename = "niezdolne_do_pracy_fte", default)]
    pub unable_to_work: f64,

    /// Phase 24D: Cumulative count of class members killed by OHS accidents,
    /// building collapses, disasters, or pogroms. Subtracted from
    /// `available_fte` and `population` when deaths occur.
    #[serde(rename = "ofiar_śmiertelnych", default)]
    pub deceased: i64,

    /// Phase 24D: Cumulative count of class members disabled by OHS accidents
    /// or disasters. They remain in `population` but are excluded from
    /// `available_fte`.
    #[serde(rename = "niepełnosprawni", default)]
    pub active_disabled: i64,

    /// Phase 47: Persistent durable-goods holdings for this class.
    /// Mirrors FixedAssetCohort but for household consumption.
    /// Aggregated by (commodity, quality_bucket) to bound memory.
    #[serde(default)]
    pub household_durables: Vec<HouseholdDurableCohort>,
}

/// Phase 47: A cohort of household durable goods held by a demographic class.
/// Aggregated by (commodity, quality_bucket) where quality_bucket = (quality * 4.0).round() / 4.0
/// to bound the number of cohorts per class per commodity to ~4.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HouseholdDurableCohort {
    /// Commodity (Furniture, Cars, Televisions, Agd, Radio, Clothing, LuxuryFurniture, LuxuryClothing).
    pub commodity: crate::registries::enums::Commodity,
    /// Number of units held (per-capita normalized — count is per citizen).
    pub count: f64,
    /// Average condition in [0.0, 1.0]. Degrades by 1.0/durability per turn.
    pub condition: f64,
    /// Quality tier (1.0 = baseline, >1.0 = premium blueprint quality).
    pub quality: f64,
    /// Durability in turns (turns to fully degrade from 1.0 to 0.0).
    pub durability: f64,
    /// Turn acquired (for upgrade comparison and cohort merging).
    pub acquired_turn: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EconomicStatus {
    #[default]
    Prosperous,
    Stable,
    Struggling,
    Destitute,
}

impl ClassDemographics {
    /// Calculate YoY-adjusted sentiment drivers
    /// 
    /// # Arguments
    /// * `calendar` - Current calendar state
    /// * `current_savings_per_capita` - Current savings per capita (NOT total savings)
    /// * `current_real_wage` - Current real wage
    /// * `current_inflation` - Current inflation rate
    /// 
    /// # Returns
    /// SentimentDrivers with YoY-adjusted values, or Turn-over-Turn values if first year
    /// 
    /// # Rules
    /// * If snapshot_turn == 0 (first year), use Turn-over-Turn comparison against previous_turn_qol
    /// * This prevents "First-Year Paralysis" - society must react to immediate shocks
    /// * Explicit zero-guard on all divisions to prevent divide-by-zero panics
    /// * Unit consistency: current_savings_per_capita compared to historical.savings_per_capita_yoy
    pub fn calculate_yoy_drivers(
        &self,
        calendar: &crate::state::Calendar,
        current_savings_per_capita: f64,
        current_real_wage: f64,
        current_inflation: f64,
    ) -> crate::politics::chaos_config::SentimentDrivers {
        let turn_index = ((calendar.global_turn - 1) % 24) as usize;
        let historical = &self.historical_qol[turn_index];
        
        // FIRST-YEAR FALLBACK: If no YoY data exists, use Turn-over-Turn comparison
        // This prevents "First-Year Paralysis" - society must react to immediate shocks
        if historical.snapshot_turn == 0 {
            // Compare against previous turn (Turn-over-Turn / Month-over-Month)
            let real_wage_growth = if self.previous_turn_qol.real_wage_yoy > 0.0 {
                (current_real_wage - self.previous_turn_qol.real_wage_yoy) / self.previous_turn_qol.real_wage_yoy
            } else {
                0.0
            };
            
            let savings_depletion_rate = if self.previous_turn_qol.savings_per_capita_yoy > 0.0 {
                (self.previous_turn_qol.savings_per_capita_yoy - current_savings_per_capita) / self.previous_turn_qol.savings_per_capita_yoy
            } else {
                0.0
            };
            
            // Phase 6.2: Exploitation Penalty (Overwork + Poverty)
            let fte_ratio = if self.population > 0 {
                self.allocated_fte / self.population as f64
            } else {
                0.0
            };
            let is_overworked = fte_ratio > 1.2;
            let is_in_poverty = real_wage_growth < 0.0 || savings_depletion_rate > 0.1;
            let exploitation_penalty = if is_overworked && is_in_poverty { 5.0 } else { 1.0 };

            return crate::politics::chaos_config::SentimentDrivers {
                real_wage_growth,
                inflation_rate: current_inflation, // Use current inflation directly
                unemployment_rate: 0.0, // Calculated separately
                savings_depletion_rate,
                sse_success_rate: 0.0, // Calculated separately
                campaign_effectiveness: 0.0, // Calculated separately
                government_approval: 0.0, // Calculated separately
                exploitation_penalty,
            };
        }
        
        // YoY comparisons (current vs same turn last year)
        // EXPLICIT ZERO-GUARD: Prevent divide-by-zero panic
        let real_wage_growth = if historical.real_wage_yoy > 0.0 {
            (current_real_wage - historical.real_wage_yoy) / historical.real_wage_yoy
        } else {
            0.0
        };
        
        // UNIT CONSISTENCY: current_savings_per_capita (not total) compared to historical per capita
        // EXPLICIT ZERO-GUARD: Prevent divide-by-zero panic
        let savings_depletion_rate = if historical.savings_per_capita_yoy > 0.0 {
            (historical.savings_per_capita_yoy - current_savings_per_capita) / historical.savings_per_capita_yoy
        } else {
            0.0
        };
        
        // Inflation is already a rate, but we compare YoY trend
        let inflation_trend = current_inflation - historical.inflation_yoy;

        // Phase 6.2: Exploitation Penalty (Overwork + Poverty)
        let fte_ratio = if self.population > 0 {
            self.allocated_fte / self.population as f64
        } else {
            0.0
        };
        let is_overworked = fte_ratio > 1.2;
        let is_in_poverty = real_wage_growth < 0.0 || savings_depletion_rate > 0.1;
        let exploitation_penalty = if is_overworked && is_in_poverty { 5.0 } else { 1.0 };

        crate::politics::chaos_config::SentimentDrivers {
            real_wage_growth,
            inflation_rate: inflation_trend,
            unemployment_rate: 0.0, // Calculated separately
            savings_depletion_rate,
            sse_success_rate: 0.0, // Calculated separately
            campaign_effectiveness: 0.0, // Calculated separately
            government_approval: 0.0, // Calculated separately
            exploitation_penalty,
        }
    }
    
    /// Update historical snapshot at current turn and previous_turn_qol
    /// 
    /// # Arguments
    /// * `calendar` - Current calendar state
    /// * `current_savings_per_capita` - Current savings per capita (NOT total savings)
    /// * `current_real_wage` - Current real wage
    /// * `current_inflation` - Current inflation rate
    /// 
    /// # Rules
    /// * Updates both the ring buffer (historical_qol) and short-term memory (previous_turn_qol)
    /// * Unit consistency: current_savings_per_capita stored as per_capita in both structures
    pub fn update_historical_snapshot(
        &mut self,
        calendar: &crate::state::Calendar,
        current_savings_per_capita: f64,
        current_real_wage: f64,
        current_inflation: f64,
    ) {
        let turn_index = ((calendar.global_turn - 1) % 24) as usize;
        
        // Update ring buffer for YoY comparison
        self.historical_qol[turn_index] = HistoricalQualityOfLife {
            savings_per_capita_yoy: current_savings_per_capita,
            real_wage_yoy: current_real_wage,
            inflation_yoy: current_inflation,
            snapshot_turn: calendar.global_turn,
        };
        
        // Update short-term memory for Turn-over-Turn fallback
        self.previous_turn_qol = HistoricalQualityOfLife {
            savings_per_capita_yoy: current_savings_per_capita,
            real_wage_yoy: current_real_wage,
            inflation_yoy: current_inflation,
            snapshot_turn: calendar.global_turn,
        };
    }
}

/// Class demographics for a region
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RegionalClassDemographics {
    /// Demographics by rural class
    #[serde(rename = "klasy_wiejskie", default)]
    pub rural_classes: BTreeMap<String, ClassDemographics>,
    
    /// Demographics by urban class
    #[serde(rename = "klasy_miejskie", default)]
    pub urban_classes: BTreeMap<String, ClassDemographics>,
}

/// Micro-region type for nested administrative hierarchy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MicroRegionType {
    #[default]
    /// City district (Dzielnica)
    CityDistrict,
    /// Village (Sołectwo)
    Village,
    /// Rural settlement
    RuralSettlement,
    /// Industrial zone
    IndustrialZone,
}

/// Micro-region budget (sub-budget derived from local property taxes)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MicroRegionBudget {
    /// Liquid reserves
    #[serde(rename = "rezerwy_liquidne", default)]
    pub liquid_reserves: f64,
    
    /// Property tax revenue
    #[serde(rename = "podatek_nieruchomosci", default)]
    pub property_tax: f64,
    
    /// Local service fees
    #[serde(rename = "oplata_lokalna", default)]
    pub local_fees: f64,
    
    /// Transfer from parent Region
    #[serde(rename = "transfer_z_regionu", default)]
    pub regional_transfer: f64,
    
    /// Allocation for social housing
    #[serde(rename = "alokacja_mieszkania_spoleczne", default)]
    pub social_housing_allocation: f64,
}

/// Micro-region nested within a Region
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MicroRegion {
    /// Unique micro-region ID
    #[serde(rename = "id_mikroregionu", default)]
    pub id: String,
    
    /// Parent Region ID
    #[serde(rename = "region_id", default)]
    pub parent_region_id: String,
    
    /// Micro-region type
    #[serde(rename = "typ_mikroregionu")]
    pub micro_type: MicroRegionType,
    
    /// Name (e.g., "Warszawa-Śródmieście", "Wieś-Kowalówka")
    #[serde(rename = "nazwa", default)]
    pub name: String,
    
    /// Population within this micro-region
    #[serde(rename = "populacja", default)]
    pub population: i64,
    
    /// Sub-budget derived from local property taxes
    #[serde(rename = "budzet_lokalny", default)]
    pub sub_budget: MicroRegionBudget,
    
    /// Autonomy level 0-1 (affects independent spending power)
    #[serde(rename = "poziom_autonomii", default)]
    pub autonomy_level: f64,
}

impl RegionalClassDemographics {
    /// Get demographics for a specific class
    pub fn get_class(&self, class: RuralClass) -> Option<&ClassDemographics> {
        let key = serde_json::to_string(&class).unwrap_or_default();
        self.rural_classes.get(&key)
    }
    
    /// Get mutable demographics for a specific class
    pub fn get_class_mut(&mut self, class: RuralClass) -> Option<&mut ClassDemographics> {
        let key = serde_json::to_string(&class).unwrap_or_default();
        self.rural_classes.get_mut(&key)
    }
    
    /// Initialize class demographics if missing
    pub fn ensure_class(&mut self, class: RuralClass) -> &mut ClassDemographics {
        let key = serde_json::to_string(&class).unwrap_or_default();
        self.rural_classes.entry(key).or_default()
    }
    
    /// Dynamically aggregate serf population from all Latifundia in the region
    /// Serf population is NOT stored statically - LatifundiumData is the source of truth
    pub fn aggregate_serf_population(&mut self, latifundia: &[&crate::entities::legal_form::LatifundiumData]) {
        let total_serfs: u32 = latifundia.iter().map(|l| l.serf_population).sum();
        let serf_demographics = self.ensure_class(RuralClass::Serf);
        serf_demographics.population = total_serfs as i64;
    }
}

/// Update class demographics based on economic conditions
/// 
/// # Arguments
/// * `class_demographics` - Mutable reference to class demographics
/// * `class` - The rural class being updated
/// * `income` - Total income for this class
/// * `market_prices` - Market prices for subsistence basket
/// * `serf_labor_demand` - 0-1, fraction of serf time demanded by Latifundium (for Serf class only)
pub fn update_class_demographics(
    class_demographics: &mut ClassDemographics,
    class: RuralClass,
    income: f64,
    market_prices: &MarketPrices,
    serf_labor_demand: f64,
) {
    match class {
        RuralClass::Serf => {
            // Serfs feed themselves from their own plots
            // Economic status depends on remaining labor time for subsistence farming
            let subsistence_labor_time = 1.0 - serf_labor_demand; // 1.0 = full time available
            class_demographics.subsistence_labor_time = subsistence_labor_time;
            
            // If labor demand is too high, serfs cannot farm their plots -> starvation
            class_demographics.economic_status = if subsistence_labor_time < 0.3 {
                EconomicStatus::Destitute // High revolt risk
            } else if subsistence_labor_time < 0.5 {
                EconomicStatus::Struggling
            } else if subsistence_labor_time < 0.7 {
                EconomicStatus::Stable
            } else {
                EconomicStatus::Prosperous
            };
            
            // Serfs have no cash savings (subsistence economy)
            class_demographics.savings = 0.0;
            class_demographics.savings_per_capita = 0.0;
        }
        _ => {
            // Cash-owning classes interact with market prices
            let subsistence_cost = class_demographics.population as f64 
                * market_prices.subsistence_basket 
                * class_demographics.subsistence_rate;
            
            let disposable_income = income - subsistence_cost;
            
            if disposable_income > 0.0 {
                class_demographics.savings += disposable_income;
            } else {
                class_demographics.savings += disposable_income;
            }
            
            if class_demographics.population > 0 {
                class_demographics.savings_per_capita = class_demographics.savings / class_demographics.population as f64;
            }
            
            class_demographics.economic_status = match class_demographics.savings_per_capita {
                x if x >= 1000.0 => EconomicStatus::Prosperous,
                x if x >= 500.0 => EconomicStatus::Stable,
                x if x >= 100.0 => EconomicStatus::Struggling,
                _ => EconomicStatus::Destitute,
            };
        }
    }
}

/// Market prices for economic calculations
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MarketPrices {
    /// Cost of subsistence basket
    pub subsistence_basket: f64,
}

/// A megaregion grouping.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Megaregion {
    pub id: String,
    pub name: String,
    pub country: String,
    pub regions: Vec<String>,
    pub regional_budget: Map<String, Value>,
    pub population: i64,
    pub gdp: f64,
    
    // NEW: Megaregion governance
    #[serde(rename = "zarzad_megaregionu", default)]
    pub governance: Option<crate::politics::local_government::MegaregionGovernance>,
}

fn seed_geological_deposits(zasoby: &mut Map<String, Value>, gdp: f64, _rng: &mut impl Rng) {
    let goods = ["węgiel", "węgiel_brunatny", "ropa", "gaz_ziemny", "torf", "uran", "żelazo", "miedź", "cynk", "boksyt", "złoto", "srebro", "diamenty", "kamień", "piasek", "sól", "wapień"];
    for good in goods {
        let multiplier = geological_multiplier(&good);
        zasoby.insert(
            good.to_string(),
            serde_json::json!({
                "rezerwy_geologiczne": int(gdp * multiplier * 1000.0),
                "rezerwy": 0,
                "wydobycie_roczne": 0,
                "efektywność": 1.0,
                "krajowe_zuzycie": 0
            }),
        );
    }
}

fn geological_multiplier(good: &str) -> f64 {
    match resource_category(good) {
        "energetyczne" => 50.0,
        "metaliczne" => 30.0,
        "skalne" => 100.0,
        _ => 100.0,
    }
}

fn resource_category(good: &str) -> &'static str {
    match good {
        "węgiel" | "węgiel_brunatny" | "ropa" | "gaz_ziemny" | "torf" | "uran" => "energetyczne",
        "żelazo" | "miedź" | "cynk" | "boksyt" | "złoto" | "srebro" | "diamenty" => "metaliczne",
        _ => "skalne",
    }
}

/// Serde helper: returns true if the f64 is zero (for `skip_serializing_if`).
fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

fn int(x: f64) -> i64 {
    x as i64
}

fn region_count(population: i64, gdp_pc: f64) -> usize {
    let base = (population / 2_000_000).max(4).min(15) as f64;
    let multiplier = 1.0 + (gdp_pc / 100_000.0) * 0.3;
    ((base * multiplier) as i64).max(4).min(15) as usize
}

/// Phase 35: Generates a human-readable region display name.
///
/// For the capital region, returns "{Country} Capital District".
/// For other regions, picks a prefix from a geographic name pool and a suffix
/// (e.g., "Northern Valley", "Eastern Highlands", "Western Coast").
fn generate_region_name(country: &str, is_capital: bool, rng: &mut impl Rng) -> String {
    if is_capital {
        return format!("{} Capital District", country);
    }

    let prefixes = [
        "Northern", "Southern", "Eastern", "Western", "Central",
        "Upper", "Lower", "Greater", "Old", "New",
    ];
    let suffixes = [
        "Valley", "Highlands", "Coast", "Plains", "Ridge",
        "Basin", "Delta", "Frontier", "Heartland", "Marches",
    ];

    let prefix = prefixes.choose(rng).unwrap();
    let suffix = suffixes.choose(rng).unwrap();
    format!("{} {}", prefix, suffix)
}

/// Generates the regional topology for a country.
/// Phase 44: Now accepts `start_year` for era-aware demographics.
pub fn generate_regional_topology(country: &str, population: i64, gdp: f64, start_year: u32) -> HashMap<String, Region> {
    let mut rng = rand::thread_rng();
    let gdp_pc = gdp / population as f64;
    let count = region_count(population, gdp_pc);
    let mut regions = Vec::new();
    let mut remaining_pop = population as f64;
    let mut remaining_gdp = gdp;

    for i in 0..count {
        let (pop_share, gdp_share) = if i == 0 {
            (0.3 + rng.gen_range(0.0..0.2), 0.4 + rng.gen_range(0.0..0.2))
        } else {
            let denom = (count - 1) as f64;
            (
                (0.7 / denom) * rng.gen_range(0.5..1.5),
                (0.6 / denom) * rng.gen_range(0.5..1.5),
            )
        };
        let region_pop = (remaining_pop * pop_share).min(remaining_pop) as i64;
        let region_gdp = remaining_gdp * gdp_share;
        remaining_pop -= (region_pop as f64).min(remaining_pop);
        remaining_gdp -= region_gdp;

        let region_id = format!("{country}-Region{}", i + 1);
        // Phase 35: Generate a human-readable display name instead of "Region1".
        let region_display_name = generate_region_name(country, i == 0, &mut rng);
        let climate = Climate::random(&mut rng);
        let (soil, arable_mult) = climate.soil_profile(&mut rng);
        let arable_max = (region_pop as f64 * rng.gen_range(0.15..0.45) * arable_mult) as i64;
        let base_mines = (region_pop / 100_000) + 5;
        let mine_limits = climate.mine_limits(base_mines, &mut rng);
        let mut resources = Map::new();
        resources.insert("lasy".to_string(), serde_json::json!({"wyeksploatowanie": rng.gen_range(0.1..0.3)}));
        resources.insert("woda_slodka".to_string(), serde_json::json!({"dostepnosc": rng.gen_range(0.5..1.0)}));
        seed_geological_deposits(&mut resources, region_gdp, &mut rng);

        // Phase 47: Assign development_level per region.
        // Capital: heavily developed (0.85-0.95).
        // Provinces: varied (0.1-0.75), biased by relative population.
        let development_level = if i == 0 {
            rng.gen_range(0.85..0.95)
        } else {
            // Larger population regions tend to be more developed.
            let pop_factor = (region_pop as f64 / (population as f64 / count as f64))
                .clamp(0.3, 2.0);
            let base = rng.gen_range(0.15..0.55);
            (base * pop_factor).clamp(0.1, 0.75)
        };

        // Phase 51: Scale regional GDP by development level.
        // Developed regions produce more per capita than underdeveloped ones.
        let dev_multiplier = 0.3 + development_level * 1.4; // Range: ~0.44 to ~1.63
        let scaled_gdp = region_gdp * dev_multiplier;
        let scaled_gdp_pc = if region_pop > 0 { scaled_gdp / region_pop as f64 } else { gdp_pc };

        // Phase 51: Initialize governance for ALL regions.
        // No region is left without governance — low development means poverty, not anarchy.
        // Underdeveloped regions (development_level <= 0.2) get CommissaryAdministration.
        let mut gov = initialize_regional_governance(&region_id, country);
        if development_level <= 0.2 {
            gov.admin_status = AdministrativeStatus::CommissaryAdministration;
            gov.head_type = RegionalHeadType::CentralAdministrator;
        }
        // Scale budget by regional GDP.
        gov.budget.liquid_reserves = scaled_gdp * 0.02;
        gov.budget.tax_revenue = scaled_gdp * 0.01;
        gov.budget.property_tax = scaled_gdp * 0.005;
        gov.budget.local_expenditures = scaled_gdp * 0.015;
        gov.budget.budget_balance = scaled_gdp * 0.005;
        gov.debt.credit_rating = if development_level > 0.5 { "AA" } else if development_level > 0.3 { "A" } else { "BBB" }.to_string();
        // Initialize council seats proportional to population.
        gov.council.total_seats = ((region_pop / 50_000) as u32).max(5);
        // Faction distribution: developed regions lean Moderate, underdeveloped lean Populares.
        let total_seats = gov.council.total_seats;
        if development_level > 0.5 {
            gov.council.faction_distribution.moderates_count = (total_seats as f64 * 0.45) as u32;
            gov.council.faction_distribution.populares_count = (total_seats as f64 * 0.30) as u32;
            gov.council.faction_distribution.optimates_count = total_seats - gov.council.faction_distribution.moderates_count - gov.council.faction_distribution.populares_count;
        } else {
            gov.council.faction_distribution.populares_count = (total_seats as f64 * 0.50) as u32;
            gov.council.faction_distribution.moderates_count = (total_seats as f64 * 0.30) as u32;
            gov.council.faction_distribution.optimates_count = total_seats - gov.council.faction_distribution.populares_count - gov.council.faction_distribution.moderates_count;
        }
        let governance = Some(gov);

        // Phase 47: Assign varied climate profiles (currently all default to Temperate).
        // Capital tends to be temperate; provinces get varied climates.
        let climate_profile = if i == 0 {
            ClimateProfile::Temperate
        } else {
            pick_climate_profile(&mut rng)
        };

        regions.push(Region {
            id: region_id,
            display_name: region_display_name,
            owner_country: country.to_string(),
            population: region_pop,
            gdp: scaled_gdp,
            gdp_pc: scaled_gdp_pc,
            climate,
            soil_profile: soil,
            arable_land_max: arable_max,
            arable_land_used: 0,
            extraction_limits: mine_limits,
            climate_profile,
            extraction_used: base_mine_template().into_iter().map(|(k, _)| (k, 0)).collect(),
            resources,
            is_capital: i == 0,
            node_type: NodeType::LandRegion,
            edges: Vec::new(),
            adjacency: Vec::new(),
            land_distribution: BTreeMap::new(),
            class_demographics: generate_class_demographics(region_pop, start_year, development_level),
            governance,
            capacity_pool: BTreeMap::new(),
            capacity_utilization: BTreeMap::new(),
            capacity_prices: BTreeMap::new(),
            land_use_inventory: LandUseInventory::default(),
            micro_regions: BTreeMap::new(),
            treasury: Treasury::default(),
            microregion_budgets: HashMap::new(),
            winter_mortality_multiplier: 1.0,
            holy_site: None,
            geographic_traits: Default::default(),
            coord_x: 0.0,
            coord_y: 0.0,
            development_level,
            parcel_ids: Vec::new(),
        });
    }

    let adjacency = build_adjacency_graph(&regions);
    let structured_edges = build_structured_edges(&regions);
    for region in &mut regions {
        region.adjacency = adjacency.get(&region.id).cloned().unwrap_or_default();
        region.node_type = NodeType::LandRegion;
        region.edges = structured_edges.get(&region.id).cloned().unwrap_or_default();
        // Initialize land use inventory (Phase 5.3)
        initialize_default_land_inventory(region);
    }

    regions.into_iter().map(|r| (r.id.clone(), r)).collect()
}

/// Phase 25: Generate initial class demographics for a region based on its population.
/// Phase 44: Now era-aware — rural/urban split and class distribution scale by start_year.
/// Creates rural and urban classes with reasonable population splits and labor
/// Phase 47: Seed initial household durables for wealthy classes at Genesis.
/// `ownership_fraction` controls what fraction of the population owns each durable.
/// Earlier eras have fewer durables (no TVs before 1936, etc.).
fn seed_initial_durables(demo: &mut ClassDemographics, ownership_fraction: f64, year: u32) {
    use crate::registries::enums::Commodity;
    let pop = demo.population.max(1) as f64;
    let frac = ownership_fraction.max(0.0).min(1.0);

    // Furniture: always present for wealthy classes
    demo.household_durables.push(HouseholdDurableCohort {
        commodity: Commodity::Furniture,
        count: frac * pop * 0.8,
        condition: 0.7,
        quality: 0.7,
        durability: Commodity::Furniture.household_durable_turns(),
        acquired_turn: 0,
    });

    // Clothing: always present
    demo.household_durables.push(HouseholdDurableCohort {
        commodity: Commodity::Clothing,
        count: frac * pop * 2.0,
        condition: 0.6,
        quality: 0.6,
        durability: Commodity::Clothing.household_durable_turns(),
        acquired_turn: 0,
    });

    // LuxuryFurniture: only for very wealthy (aristocracy)
    if frac >= 0.4 {
        demo.household_durables.push(HouseholdDurableCohort {
            commodity: Commodity::LuxuryFurniture,
            count: frac * pop * 0.3,
            condition: 0.8,
            quality: 0.9,
            durability: Commodity::LuxuryFurniture.household_durable_turns(),
            acquired_turn: 0,
        });
    }

    // Radio: available from 1920
    if year >= 1920 {
        demo.household_durables.push(HouseholdDurableCohort {
            commodity: Commodity::Radio,
            count: frac * pop * 0.2,
            condition: 0.8,
            quality: 0.7,
            durability: Commodity::Radio.household_durable_turns(),
            acquired_turn: 0,
        });
    }

    // Cars: available from 1910, only for very wealthy
    if year >= 1910 && frac >= 0.4 {
        demo.household_durables.push(HouseholdDurableCohort {
            commodity: Commodity::Cars,
            count: frac * pop * 0.05,
            condition: 0.7,
            quality: 0.8,
            durability: Commodity::Cars.household_durable_turns(),
            acquired_turn: 0,
        });
    }
}

/// participation rates. This is the critical fix for the 100% unemployment bug —
/// without class demographics, the labor market clearing has no workers to hire.
fn generate_class_demographics(region_pop: i64, start_year: u32, development_level: f64) -> RegionalClassDemographics {
    let mut rural_classes = BTreeMap::new();
    let mut urban_classes = BTreeMap::new();

    // Phase 47: Development-driven savings multiplier.
    // High development → wealthier citizens (0.5x to 2.0x savings).
    let dev_savings_mult = 0.5 + development_level * 1.5;

    // Phase 44: Era-aware rural/urban split.
    // 1900: 80% rural, 20% urban (pre-industrial)
    // 1925: 65% rural, 35% urban (early industrialization)
    // 1950: 50% rural, 50% urban (industrialization)
    // 1975: 40% rural, 60% urban (post-industrial)
    let rural_share = match start_year {
        y if y <= 1900 => 0.80,
        y if y <= 1925 => 0.65,
        y if y <= 1950 => 0.50,
        _ => 0.40,
    };
    let rural_pop = (region_pop as f64 * rural_share) as i64;
    let urban_pop = region_pop - rural_pop;

    // Phase 44: Era-aware rural class distribution.
    // 1900: Serfs present (20%), FreePeasants (40%), LandlessLaborers (35%), Aristocracy (5%)
    // 1925: Serfs declining (10%), FreePeasants (50%), LandlessLaborers (35%), Aristocracy (5%)
    // 1950: No serfs, FreePeasants (55%), LandlessLaborers (40%), Aristocracy (5%)
    // 1975: No serfs, FreePeasants (60%), LandlessLaborers (35%), Aristocracy (5%)
    let (serf_pct, free_peasant_pct, landless_pct, aristocracy_pct) = match start_year {
        y if y <= 1900 => (0.20, 0.40, 0.35, 0.05),
        y if y <= 1925 => (0.10, 0.50, 0.35, 0.05),
        y if y <= 1950 => (0.00, 0.55, 0.40, 0.05),
        _ => (0.00, 0.60, 0.35, 0.05),
    };

    let serf_pop = (rural_pop as f64 * serf_pct) as i64;
    let free_peasant_pop = (rural_pop as f64 * free_peasant_pct) as i64;
    let landless_pop = (rural_pop as f64 * landless_pct) as i64;
    let aristocracy_pop = rural_pop - serf_pop - free_peasant_pop - landless_pop;

    if serf_pop > 0 {
        rural_classes.insert("Serf".to_string(), ClassDemographics {
            population: serf_pop,
            labor_participation: 0.65,
            savings: 0.0, // Serfs don't have cash savings
            ..Default::default()
        });
    }

    rural_classes.insert("FreePeasant".to_string(), ClassDemographics {
        population: free_peasant_pop,
        labor_participation: 0.55,
        savings: (free_peasant_pop as f64 * 100.0 * dev_savings_mult),
        ..Default::default()
    });
    rural_classes.insert("LandlessLaborer".to_string(), ClassDemographics {
        population: landless_pop,
        labor_participation: 0.60,
        savings: (landless_pop as f64 * 50.0 * dev_savings_mult),
        ..Default::default()
    });
    rural_classes.insert("Aristocracy".to_string(), {
        let mut demo = ClassDemographics {
            population: aristocracy_pop,
            labor_participation: 0.30,
            savings: (aristocracy_pop as f64 * 5000.0 * dev_savings_mult),
            ..Default::default()
        };
        // Phase 47: Seed initial household durables for wealthy classes.
        seed_initial_durables(&mut demo, 0.5, start_year);
        demo
    });

    // Urban classes:
    // - Workers: 70% of urban population
    // - Bourgeoisie: 30% of urban population
    let worker_pop = (urban_pop as f64 * 0.70) as i64;
    let middle_pop = urban_pop - worker_pop;

    urban_classes.insert("Worker".to_string(), ClassDemographics {
        population: worker_pop,
        labor_participation: 0.60,
        savings: (worker_pop as f64 * 200.0 * dev_savings_mult),
        ..Default::default()
    });
    urban_classes.insert("Bourgeoisie".to_string(), {
        let mut demo = ClassDemographics {
            population: middle_pop,
            labor_participation: 0.55,
            savings: (middle_pop as f64 * 1000.0 * dev_savings_mult),
            ..Default::default()
        };
        // Phase 47: Seed initial household durables for wealthy classes.
        seed_initial_durables(&mut demo, 0.3, start_year);
        demo
    });

    RegionalClassDemographics {
        rural_classes,
        urban_classes,
    }
}

fn build_adjacency_graph(regions: &[Region]) -> HashMap<String, Vec<String>> {
    let mut graph = HashMap::new();
    let n = regions.len();
    for (i, region) in regions.iter().enumerate() {
        let mut neighbors = Vec::new();
        if n > 1 {
            neighbors.push(regions[(i + 1) % n].id.clone());
            neighbors.push(regions[(i + n - 1) % n].id.clone());
        }
        if region.is_capital && n > 1 {
            for other in regions {
                if other.id != region.id && !neighbors.contains(&other.id) {
                    neighbors.push(other.id.clone());
                }
            }
        }
        graph.insert(region.id.clone(), neighbors);
    }
    graph
}

/// Build structured edges for regions (new graph-based system).
///
/// # Arguments
/// * `regions` - Slice of Region references
///
/// # Returns
/// HashMap mapping region IDs to their structured edges
fn build_structured_edges(regions: &[Region]) -> HashMap<String, Vec<Edge>> {
    let mut edges_map: HashMap<String, Vec<Edge>> = HashMap::new();
    let n = regions.len();

    for (i, region) in regions.iter().enumerate() {
        let mut edges = Vec::new();

        if n > 1 {
            // Circular adjacency with structured edges
            let next_region = &regions[(i + 1) % n];
            let prev_region = &regions[(i + n - 1) % n];

            edges.push(Edge {
                target_node: next_region.id.clone(),
                edge_type: EdgeType::LandBorder,
                distance: 100.0, // Default distance for land borders
                is_navigable: false,
                territorial_owner: None,
            });

            edges.push(Edge {
                target_node: prev_region.id.clone(),
                edge_type: EdgeType::LandBorder,
                distance: 100.0,
                is_navigable: false,
                territorial_owner: None,
            });
        }

        // Capital connects to all other regions
        if region.is_capital && n > 1 {
            for other in regions {
                if other.id != region.id && !edges.iter().any(|e| &e.target_node == &other.id) {
                    edges.push(Edge {
                        target_node: other.id.clone(),
                        edge_type: EdgeType::LandBorder,
                        distance: 150.0, // Longer distance for capital connections
                        is_navigable: false,
                        territorial_owner: None,
                    });
                }
            }
        }

        edges_map.insert(region.id.clone(), edges);
    }

    edges_map
}

/// Generate maritime nodes (SeaNode and OceanNode) for a country.
///
/// # Arguments
/// * `country` - Country name
/// * `coastal_regions` - IDs of regions that should have coastline connections
///
/// # Returns
/// HashMap of maritime node IDs to Region structures
pub fn generate_maritime_nodes(
    country: &str,
    coastal_regions: &[String],
) -> HashMap<String, Region> {
    let mut maritime_nodes = HashMap::new();

    // Generate one inland sea node (e.g., Baltic Sea equivalent)
    let sea_node_id = format!("{country}-Morze1");
    let sea_node = Region {
        id: sea_node_id.clone(),
        display_name: "Sea Lane".to_string(),
        owner_country: String::new(), // Maritime nodes have no owner
        population: 0,
        gdp: 0.0,
        gdp_pc: 0.0,
        climate: Climate::Balanced,
        soil_profile: BTreeMap::new(),
        arable_land_max: 0,
        arable_land_used: 0,
        extraction_limits: BTreeMap::new(),
        climate_profile: ClimateProfile::default(),
        extraction_used: BTreeMap::new(),
        resources: Map::new(),
        is_capital: false,
        node_type: NodeType::SeaNode,
        edges: Vec::new(),
        adjacency: Vec::new(),
        land_distribution: BTreeMap::new(),
        class_demographics: RegionalClassDemographics::default(),
        governance: None,
        capacity_pool: BTreeMap::new(),
        capacity_utilization: BTreeMap::new(),
        capacity_prices: BTreeMap::new(),
        land_use_inventory: LandUseInventory::default(),
        micro_regions: BTreeMap::new(),
        treasury: Treasury::default(),
        microregion_budgets: HashMap::new(),
        winter_mortality_multiplier: 1.0,
        holy_site: None,
        geographic_traits: Default::default(),
        coord_x: 0.0,
        coord_y: 0.0,
        development_level: 0.0,
        parcel_ids: Vec::new(),
    };
    maritime_nodes.insert(sea_node_id, sea_node);

    // Generate one ocean node (e.g., Atlantic equivalent)
    let ocean_node_id = format!("{country}-Ocean1");
    let ocean_node = Region {
        id: ocean_node_id.clone(),
        display_name: "Ocean".to_string(),
        owner_country: String::new(),
        population: 0,
        gdp: 0.0,
        gdp_pc: 0.0,
        climate: Climate::Balanced,
        soil_profile: BTreeMap::new(),
        arable_land_max: 0,
        arable_land_used: 0,
        extraction_limits: BTreeMap::new(),
        climate_profile: ClimateProfile::default(),
        extraction_used: BTreeMap::new(),
        resources: Map::new(),
        is_capital: false,
        node_type: NodeType::OceanNode,
        edges: Vec::new(),
        adjacency: Vec::new(),
        land_distribution: BTreeMap::new(),
        class_demographics: RegionalClassDemographics::default(),
        governance: None,
        capacity_pool: BTreeMap::new(),
        capacity_utilization: BTreeMap::new(),
        capacity_prices: BTreeMap::new(),
        land_use_inventory: LandUseInventory::default(),
        micro_regions: BTreeMap::new(),
        treasury: Treasury::default(),
        microregion_budgets: HashMap::new(),
        winter_mortality_multiplier: 1.0,
        holy_site: None,
        geographic_traits: Default::default(),
        coord_x: 0.0,
        coord_y: 0.0,
        development_level: 0.0,
        parcel_ids: Vec::new(),
    };
    maritime_nodes.insert(ocean_node_id, ocean_node);

    // Connect coastal regions to sea node via coastline edges
    for coastal_region_id in coastal_regions {
        if let Some(sea_node) = maritime_nodes.get_mut(&format!("{country}-Morze1")) {
            sea_node.edges.push(Edge {
                target_node: coastal_region_id.clone(),
                edge_type: EdgeType::Coastline,
                distance: 50.0, // Short distance for coastline
                is_navigable: true,
                territorial_owner: Some(country.to_string()),
            });
        }
    }

    // Connect sea node to ocean node via sea lane
    if let Some(sea_node) = maritime_nodes.get_mut(&format!("{country}-Morze1")) {
        sea_node.edges.push(Edge {
            target_node: format!("{country}-Ocean1"),
            edge_type: EdgeType::SeaLane,
            distance: 500.0, // Longer distance for sea lane
            is_navigable: true,
            territorial_owner: Some(country.to_string()),
        });
    }

    // Connect ocean node back to sea node (bidirectional)
    if let Some(ocean_node) = maritime_nodes.get_mut(&format!("{country}-Ocean1")) {
        ocean_node.edges.push(Edge {
            target_node: format!("{country}-Morze1"),
            edge_type: EdgeType::SeaLane,
            distance: 500.0,
            is_navigable: true,
            territorial_owner: Some(country.to_string()),
        });
    }

    maritime_nodes
}

/// Generate geological formations for a world.
///
/// # Arguments
/// * `region_ids` - All region IDs in the world
/// * `rng` - Random number generator
///
/// # Returns
/// Vector of geological formations
pub fn generate_geological_formations(
    region_ids: &[String],
    rng: &mut impl Rng,
) -> Vec<GeologicalFormation> {
    let mut formations = Vec::new();
    let formation_count = (region_ids.len() as f64 * 0.3).max(2.0).min(10.0) as usize;

    for i in 0..formation_count {
        let formation_type = match rng.gen_range(0..5) {
            0 => FormationType::MountainRange,
            1 => FormationType::SedimentaryBasin,
            2 => FormationType::RiftValley,
            3 => FormationType::VolcanicArc,
            _ => FormationType::ContinentalShelf,
        };

        let formation_id = format!("Formation-{i}");
        let formation_name = match formation_type {
            FormationType::MountainRange => format!("Góry {i}"),
            FormationType::SedimentaryBasin => format!("Basen {i}"),
            FormationType::RiftValley => format!("Rift {i}"),
            FormationType::VolcanicArc => format!("Wulkan {i}"),
            FormationType::ContinentalShelf => format!("Szelf {i}"),
        };

        // Select random regions to overlap (2-5 regions per formation)
        let overlap_count = rng.gen_range(2..=5).min(region_ids.len());
        let mut overlapping_regions = Vec::new();
        let mut used_indices = std::collections::HashSet::new();

        while overlapping_regions.len() < overlap_count {
            let idx = rng.gen_range(0..region_ids.len());
            if used_indices.insert(idx) {
                overlapping_regions.push(region_ids[idx].clone());
            }
        }

        // Generate resource deposits based on formation type
        let resource_deposits = generate_formation_resources(&formation_type, rng);

        // Calculate total area based on overlapping regions
        let total_area = overlap_count as f64 * rng.gen_range(5000.0..20000.0);

        formations.push(GeologicalFormation {
            id: formation_id,
            name: formation_name,
            formation_type,
            resource_deposits,
            overlapping_regions,
            total_area,
        });
    }

    formations
}

/// Generate resource deposits for a geological formation.
///
/// # Arguments
/// * `formation_type` - Type of geological formation
/// * `rng` - Random number generator
///
/// # Returns
/// BTreeMap of commodity string keys to ResourceDeposit structures.
/// Phase 21A: Completely rewritten to natively use `Commodity` enum variants.
/// No Polish strings, no mapping table.
fn generate_formation_resources(
    formation_type: &FormationType,
    rng: &mut impl Rng,
) -> BTreeMap<String, ResourceDeposit> {
    let mut deposits = BTreeMap::new();

    let possible: &[Commodity] = match formation_type {
        FormationType::MountainRange => &[
            Commodity::HardCoal, Commodity::Iron, Commodity::Copper,
            Commodity::Zinc, Commodity::Gold, Commodity::Silver,
        ],
        FormationType::SedimentaryBasin => &[
            Commodity::HardCoal, Commodity::Oil, Commodity::NaturalGas,
            Commodity::BrownCoal, Commodity::Peat,
        ],
        FormationType::RiftValley => &[
            Commodity::Oil, Commodity::NaturalGas, Commodity::Uranium,
        ],
        FormationType::VolcanicArc => &[
            Commodity::Sulfur, Commodity::Copper, Commodity::Tin,
            Commodity::Lead, Commodity::Zinc,
        ],
        FormationType::ContinentalShelf => &[
            Commodity::Oil, Commodity::NaturalGas, Commodity::Sand, Commodity::Gravel,
        ],
    };

    // Select 1-3 commodities from the possible pool.
    let resource_count = rng.gen_range(1..=3).min(possible.len());
    let mut used_indices = HashSet::new();

    while deposits.len() < resource_count {
        let idx = rng.gen_range(0..possible.len());
        if used_indices.insert(idx) {
            let commodity = possible[idx];
            let estimated_reserves = rng.gen_range(1_000_000.0..100_000_000.0);
            let extraction_cost = rng.gen_range(10.0..100.0);
            let quality = rng.gen_range(0.5..1.0);
            let depth = rng.gen_range(50.0..2000.0);
            // Shallow deposits (< 200m) start discovered; deep ones are hidden.
            let discovered = depth < 200.0 && rng.gen::<f64>() < 0.7;

            let key = commodity.to_string();
            deposits.insert(
                key,
                ResourceDeposit {
                    commodity,
                    estimated_reserves,
                    current_reserves: estimated_reserves,
                    extraction_cost,
                    quality,
                    current_quality: quality,
                    depth,
                    discovered,
                },
            );
        }
    }

    deposits
}

/// Check which formations intersect a given region.
///
/// # Arguments
/// * `region_id` - Region ID to check
/// * `formations` - All geological formations
///
/// # Returns
/// Vector of formations that intersect the region
pub fn get_formations_for_region<'a>(
    region_id: &str,
    formations: &'a [GeologicalFormation],
) -> Vec<&'a GeologicalFormation> {
    formations
        .iter()
        .filter(|f| f.overlapping_regions.contains(&region_id.to_string()))
        .collect()
}

/// Get all resources available to a region through geological formations.
///
/// # Arguments
/// * `region_id` - Region ID
/// * `formations` - All geological formations
///
/// # Returns
/// BTreeMap of resource types to ResourceDeposit
pub fn get_region_resources_from_formations(
    region_id: &str,
    formations: &[GeologicalFormation],
) -> BTreeMap<String, ResourceDeposit> {
    let mut resources: BTreeMap<String, ResourceDeposit> = BTreeMap::new();

    for formation in get_formations_for_region(region_id, formations) {
        for (resource_id, deposit) in &formation.resource_deposits {
            // If resource already exists, merge reserves
            if let Some(existing) = resources.get_mut(resource_id) {
                existing.estimated_reserves += deposit.estimated_reserves;
                existing.current_reserves += deposit.current_reserves;
                // Average the quality and cost
                existing.quality = (existing.quality + deposit.quality) / 2.0;
                existing.current_quality = (existing.current_quality + deposit.current_quality) / 2.0;
                existing.extraction_cost = (existing.extraction_cost + deposit.extraction_cost) / 2.0;
                // Keep the shallower depth and discovered status
                existing.depth = existing.depth.min(deposit.depth);
                existing.discovered = existing.discovered || deposit.discovered;
            } else {
                resources.insert(resource_id.clone(), deposit.clone());
            }
        }
    }

    resources
}

/// Migrate existing soil profile to new LandUseInventory structure.
///
/// This function preserves backward compatibility by converting the old
/// `soil_profile` (formerly `profil_gleb`) BTreeMap into the new nested soil class structure within
/// the Agricultural category.
///
/// # Arguments
/// * `region` - Mutable reference to the region
pub fn migrate_soil_profile_to_land_inventory(region: &mut Region) {
    // Initialize land use inventory if empty
    if region.land_use_inventory.categories.is_empty() {
        region.land_use_inventory.total_area = 100_000.0; // Default 100,000 hectares
    }

    // Ensure Agricultural category exists
    let agricultural = region.land_use_inventory.ensure_category(LandCategory::Agricultural);

    // Migrate soil profile from old soil_profile to new structure
    for (soil_class, percentage) in &region.soil_profile {
        if !agricultural.soil_profile.contains_key(soil_class) {
            // Calculate hectares based on percentage of total agricultural land
            let total_agricultural_hectares = agricultural.area_hectares.max(1.0);
            let soil_hectares = total_agricultural_hectares * percentage;

            // Create soil class data with default ownership distribution
            let soil_data = SoilClassData {
                soil_class: soil_class.clone(),
                area_hectares: soil_hectares,
                ownership: ClassLandDistribution::default(),
                fertility_index: match soil_class.as_str() {
                    "Class_I" => 1.0,
                    "Class_II" => 0.9,
                    "Class_III" => 0.75,
                    "Class_IV" => 0.6,
                    "Class_V" => 0.4,
                    "Class_VI" => 0.2,
                    _ => 0.5,
                },
                erosion_risk: match soil_class.as_str() {
                    "Class_I" => 0.1,
                    "Class_II" => 0.15,
                    "Class_III" => 0.2,
                    "Class_IV" => 0.3,
                    "Class_V" => 0.5,
                    "Class_VI" => 0.7,
                    _ => 0.4,
                },
            };

            agricultural.soil_profile.insert(soil_class.clone(), soil_data);
        }
    }

    // Set initial agricultural area based on arable_land_max
    agricultural.area_hectares = region.arable_land_max as f64;
}

/// Initialize default land use inventory for a region.
///
/// Creates reasonable default distributions for all 8 land categories
/// based on the region's climate and characteristics.
///
/// # Arguments
/// * `region` - Mutable reference to the region
pub fn initialize_default_land_inventory(region: &mut Region) {
    let total_area = 100_000.0; // Default 100,000 hectares per region
    region.land_use_inventory.total_area = total_area;

    // Default distribution based on climate
    let (urbanized, industrial, forests, grasslands, agricultural, wetlands, water_bodies, wastelands) = match region.climate {
        Climate::Fertile => (0.15, 0.10, 0.20, 0.15, 0.30, 0.05, 0.03, 0.02),
        Climate::Desert => (0.05, 0.05, 0.05, 0.10, 0.05, 0.00, 0.02, 0.68),
        Climate::Mountainous => (0.05, 0.05, 0.30, 0.20, 0.10, 0.05, 0.05, 0.20),
        Climate::Balanced => (0.12, 0.12, 0.18, 0.18, 0.25, 0.05, 0.05, 0.05),
    };

    // Initialize each category
    for (category, percentage) in [
        (LandCategory::Urbanized, urbanized),
        (LandCategory::Industrial, industrial),
        (LandCategory::Forests, forests),
        (LandCategory::Grasslands, grasslands),
        (LandCategory::Agricultural, agricultural),
        (LandCategory::Wetlands, wetlands),
        (LandCategory::WaterBodies, water_bodies),
        (LandCategory::Wastelands, wastelands),
    ] {
        let cat_data = region.land_use_inventory.ensure_category(category);
        cat_data.area_hectares = total_area * percentage;
        cat_data.ecological_health = 0.8;
        cat_data.development_potential = match category {
            LandCategory::Urbanized | LandCategory::Industrial => 0.3,
            LandCategory::Agricultural | LandCategory::Grasslands => 0.7,
            LandCategory::Forests | LandCategory::Wetlands => 0.5,
            LandCategory::WaterBodies => 0.1,
            LandCategory::Wastelands => 0.2,
        };

        // Set sub-types for specific categories
        cat_data.sub_type = match category {
            LandCategory::WaterBodies => LandSubType::Freshwater,
            LandCategory::Wastelands => LandSubType::FunctionalWasteland,
            _ => LandSubType::Generic,
        };
    }

    // Migrate existing soil profile
    migrate_soil_profile_to_land_inventory(region);
}

/// Land transformation project type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransformationType {
    /// Wetlands → Agricultural (Class I/II soils)
    Melioration,
    /// Wastelands → Forests
    Reforestation,
    /// Agricultural → Urbanized
    Urbanization,
    /// Agricultural → Industrial
    Industrialization,
    /// Industrial → Grasslands/Wetlands
    Restoration,
}

/// Land transformation megaproject.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LandTransformationProject {
    /// Unique project ID
    #[serde(rename = "id_projektu")]
    pub id: String,
    /// Type of transformation
    #[serde(rename = "typ_transformacji")]
    pub project_type: TransformationType,
    /// Source land category
    #[serde(rename = "kategoria_źródłowa")]
    pub source_category: LandCategory,
    /// Target land category
    #[serde(rename = "kategoria_docelowa")]
    pub target_category: LandCategory,
    /// Region where project is located
    #[serde(rename = "region_id")]
    pub region_id: String,
    /// Area to transform in hectares
    #[serde(rename = "obszar_transformacji")]
    pub area_to_transform: f64,
    /// Total cost
    #[serde(rename = "koszt_całkowity")]
    pub cost: f64,
    /// Duration in turns
    #[serde(rename = "czas_trwania")]
    pub duration_turns: u32,
    /// Progress 0-1
    #[serde(rename = "postęp")]
    pub progress: f64,
    /// Required infrastructure building types
    #[serde(rename = "wymagana_infrastruktura", default)]
    pub required_infrastructure: Vec<String>,
    /// Soil class to assign (for Agricultural target)
    #[serde(rename = "przypisana_klasa_gleby", default)]
    pub assigned_soil_class: Option<String>,
}

impl LandTransformationProject {
    /// Process one turn of the transformation project.
    ///
    /// # Arguments
    /// * cost_per_turn - Cost to deduct from budget this turn
    ///
    /// # Returns
    /// * true if project completed this turn
    /// * false if still in progress
    pub fn process_turn(&mut self, cost_per_turn: f64) -> bool {
        let progress_increment = 1.0 / self.duration_turns as f64;
        self.progress = (self.progress + progress_increment).min(1.0);
        self.progress >= 1.0
    }

    /// Apply the transformation to a region's land inventory.
    ///
    /// This physically moves hectares from source to target category
    /// and assigns appropriate soil classes if target is Agricultural.
    ///
    /// # Arguments
    /// * region - Mutable reference to the region
    pub fn apply_transformation(&self, region: &mut Region) {
        let source_key = serde_json::to_string(&self.source_category).unwrap_or_default();
        let target_key = serde_json::to_string(&self.target_category).unwrap_or_default();

        // Subtract from source category
        if let Some(source_cat) = region.land_use_inventory.categories.get_mut(&source_key) {
            source_cat.area_hectares = (source_cat.area_hectares - self.area_to_transform).max(0.0);
        }

        // Add to target category
        if let Some(target_cat) = region.land_use_inventory.categories.get_mut(&target_key) {
            target_cat.area_hectares += self.area_to_transform;

            // If target is Agricultural, assign soil class
            if self.target_category == LandCategory::Agricultural {
                let soil_class = self.assigned_soil_class.as_ref().unwrap_or(&"Class_III".to_string()).clone();

                // Create or update soil class data
                let soil_data = SoilClassData {
                    soil_class: soil_class.clone(),
                    area_hectares: self.area_to_transform,
                    ownership: ClassLandDistribution {
                        state_hectares: self.area_to_transform as i64, // Assign to state by default
                        ..Default::default()
                    },
                    fertility_index: match soil_class.as_str() {
                        "Class_I" => 1.0,
                        "Class_II" => 0.9,
                        "Class_III" => 0.75,
                        "Class_IV" => 0.6,
                        "Class_V" => 0.4,
                        "Class_VI" => 0.2,
                        _ => 0.5,
                    },
                    erosion_risk: match soil_class.as_str() {
                        "Class_I" => 0.1,
                        "Class_II" => 0.15,
                        "Class_III" => 0.2,
                        "Class_IV" => 0.3,
                        "Class_V" => 0.5,
                        "Class_VI" => 0.7,
                        _ => 0.4,
                    },
                };

                // Merge with existing soil profile
                if let Some(existing) = target_cat.soil_profile.get_mut(&soil_class) {
                    existing.area_hectares += self.area_to_transform;
                    existing.ownership.state_hectares += self.area_to_transform as i64;
                } else {
                    target_cat.soil_profile.insert(soil_class, soil_data);
                }

                // Update region's arable_land_max to reflect new agricultural land
                region.arable_land_max += self.area_to_transform as i64;
            }
        }
    }
}

/// Generate a land transformation project for melioration (wetlands to agricultural).
///
/// # Arguments
/// * `region_id` - Region ID
/// * `area_hectares` - Area to transform
/// * `soil_class` - Soil class to assign (e.g., "Class_I" for high-quality melioration)
/// * `rng` - Random number generator for unique ID
///
/// # Returns
/// Configured LandTransformationProject
pub fn create_melioration_project(
    region_id: String,
    area_hectares: f64,
    soil_class: String,
    rng: &mut impl Rng,
) -> LandTransformationProject {
    let cost_per_hectare = 5000.0; // Base cost per hectare
    let total_cost = area_hectares * cost_per_hectare;
    let duration_turns = (area_hectares / 1000.0).ceil() as u32; // 1 turn per 1000 hectares
    let unique_id: u64 = rng.gen();

    LandTransformationProject {
        id: format!("Melioration-{}-{}", region_id, unique_id),
        project_type: TransformationType::Melioration,
        source_category: LandCategory::Wetlands,
        target_category: LandCategory::Agricultural,
        region_id,
        area_to_transform: area_hectares,
        cost: total_cost,
        duration_turns: duration_turns.max(1),
        progress: 0.0,
        required_infrastructure: vec!["Systemy Odwadniające".to_string()],
        assigned_soil_class: Some(soil_class),
    }
}

/// Generate a land transformation project for reforestation.
///
/// # Arguments
/// * `region_id` - Region ID
/// * `area_hectares` - Area to transform
/// * `rng` - Random number generator for unique ID
///
/// # Returns
/// Configured LandTransformationProject
pub fn create_reforestation_project(
    region_id: String,
    area_hectares: f64,
    rng: &mut impl Rng,
) -> LandTransformationProject {
    let cost_per_hectare = 2000.0;
    let total_cost = area_hectares * cost_per_hectare;
    let duration_turns = (area_hectares / 2000.0).ceil() as u32;
    let unique_id: u64 = rng.gen();

    LandTransformationProject {
        id: format!("Reforestation-{}-{}", region_id, unique_id),
        project_type: TransformationType::Reforestation,
        source_category: LandCategory::Wastelands,
        target_category: LandCategory::Forests,
        region_id,
        area_to_transform: area_hectares,
        cost: total_cost,
        duration_turns: duration_turns.max(1),
        progress: 0.0,
        required_infrastructure: vec!["Szkółki Leśne".to_string()],
        assigned_soil_class: None,
    }
}

/// Generate a land transformation project for urbanization.
///
/// # Arguments
/// * `region_id` - Region ID
/// * `area_hectares` - Area to transform
/// * `rng` - Random number generator for unique ID
///
/// # Returns
/// Configured LandTransformationProject
pub fn create_urbanization_project(
    region_id: String,
    area_hectares: f64,
    rng: &mut impl Rng,
) -> LandTransformationProject {
    let cost_per_hectare = 10000.0;
    let total_cost = area_hectares * cost_per_hectare;
    let duration_turns = (area_hectares / 500.0).ceil() as u32;
    let unique_id: u64 = rng.gen();

    LandTransformationProject {
        id: format!("Urbanization-{}-{}", region_id, unique_id),
        project_type: TransformationType::Urbanization,
        source_category: LandCategory::Agricultural,
        target_category: LandCategory::Urbanized,
        region_id,
        area_to_transform: area_hectares,
        cost: total_cost,
        duration_turns: duration_turns.max(1),
        progress: 0.0,
        required_infrastructure: vec!["Sieci Miejskie".to_string(), "Transport Publiczny".to_string()],
        assigned_soil_class: None,
    }
}

/// Phase 37: Generates megaregions by clustering regions into groups of 3-5.
/// For small countries (≤3 regions), returns a single megaregion.
/// For larger countries, splits regions into `ceil(len / 4)` clusters.
/// Each cluster gets a unique generated name.
pub fn generate_megaregions(country: &str, region_ids: &[String]) -> Vec<Megaregion> {
    let mut rng = rand::thread_rng();

    // Small countries: single megaregion
    if region_ids.len() <= 3 {
        return vec![Megaregion {
            id: format!("MEG-{country}-01"),
            name: generate_megaregion_name(country, region_ids.len(), &mut rng),
            country: country.to_string(),
            regions: region_ids.to_vec(),
            regional_budget: Map::new(),
            population: 0,
            gdp: 0.0,
            governance: None,
        }];
    }

    // Larger countries: cluster into groups of ~4 regions
    let cluster_size = 4;
    let num_clusters = (region_ids.len() + cluster_size - 1) / cluster_size;
    let mut megaregions = Vec::new();

    for (i, chunk) in region_ids.chunks(cluster_size).enumerate() {
        let megaregion_idx = i + 1;
        megaregions.push(Megaregion {
            id: format!("MEG-{country}-{megaregion_idx:02}"),
            name: generate_megaregion_name(country, chunk.len(), &mut rng),
            country: country.to_string(),
            regions: chunk.to_vec(),
            regional_budget: Map::new(),
            population: 0,
            gdp: 0.0,
            governance: None,
        });
    }

    megaregions
}

/// Phase 36: Generate a human-readable megaregion name.
///
/// Combines a geographic prefix (e.g., "Northern", "Central") with a
/// descriptor (e.g., "Commonwealth", "Confederation", "Union") and the
/// country name. For small countries (≤4 regions), uses simpler names.
fn generate_megaregion_name(country: &str, region_count: usize, rng: &mut rand::rngs::ThreadRng) -> String {
    let geographic_prefixes = [
        "Northern", "Southern", "Eastern", "Western", "Central",
        "Upper", "Lower", "Greater",
    ];
    let descriptors_large = [
        "Commonwealth", "Confederation", "Union", "Republic", "Federation",
    ];
    let descriptors_small = [
        "Province", "Territory", "District", "Region",
    ];

    let prefix = geographic_prefixes.choose(rng).unwrap();
    let descriptor = if region_count > 4 {
        descriptors_large.choose(rng).unwrap()
    } else {
        descriptors_small.choose(rng).unwrap()
    };

    format!("{prefix} {country} {descriptor}")
}

/// Pathfinding result for maritime routes.
#[derive(Debug, Clone, PartialEq)]
pub struct PathResult {
    /// Sequence of node IDs from start to end
    pub path: Vec<String>,
    /// Total distance in kilometers
    pub total_distance: f64,
    /// Whether the path is navigable by ships
    pub is_navigable: bool,
}

/// Dijkstra's algorithm for finding shortest path in the graph.
///
/// # Arguments
/// * `graph` - HashMap of node ID to list of edges
/// * `start` - Starting node ID
/// * `end` - Target node ID
/// * `allow_maritime` - Whether to allow traversal through SeaNode/OceanNode
///
/// # Returns
/// * `Some(PathResult)` if a path exists
/// * `None` if no path exists
pub fn find_shortest_path(
    graph: &HashMap<String, Vec<Edge>>,
    start: &str,
    end: &str,
    allow_maritime: bool,
) -> Option<PathResult> {
    if start == end {
        return Some(PathResult {
            path: vec![start.to_string()],
            total_distance: 0.0,
            is_navigable: true,
        });
    }

    let mut distances: HashMap<String, f64> = HashMap::new();
    let mut previous: HashMap<String, String> = HashMap::new();
    let mut visited: HashSet<String> = HashSet::new();

    // Use a simple Vec as priority queue (manual sorting)
    let mut unvisited: Vec<(f64, String)> = Vec::new();

    distances.insert(start.to_string(), 0.0);
    unvisited.push((0.0, start.to_string()));

    while let Some((current_dist, current_node)) = unvisited.pop() {
        if visited.contains(&current_node) {
            continue;
        }

        if current_node == end {
            // Reconstruct path
            let mut path = Vec::new();
            let mut node = end.to_string();
            while node != start {
                path.push(node.clone());
                node = previous.get(&node)?.clone();
            }
            path.push(start.to_string());
            path.reverse();

            // Check if path is navigable
            let is_navigable = check_path_navigability(&path, graph);

            return Some(PathResult {
                path,
                total_distance: current_dist,
                is_navigable,
            });
        }

        visited.insert(current_node.clone());

        if let Some(edges) = graph.get(&current_node) {
            for edge in edges {
                if !allow_maritime {
                    // Skip maritime edges if not allowed
                    if matches!(edge.edge_type, EdgeType::Coastline | EdgeType::SeaLane) {
                        continue;
                    }
                }

                let neighbor = &edge.target_node;
                let new_dist = current_dist + edge.distance;

                if !distances.contains_key(neighbor) || new_dist < *distances.get(neighbor).unwrap() {
                    distances.insert(neighbor.clone(), new_dist);
                    previous.insert(neighbor.clone(), current_node.clone());
                    unvisited.push((new_dist, neighbor.clone()));
                }
            }
        }

        // Sort unvisited by distance (descending for pop() to get smallest)
        unvisited.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    }

    None
}

/// Calculate transport cost between two micro-regions (Phase 5.5).
///
/// # Arguments
/// * `from_region_id` - Source micro-region ID
/// * `to_region_id` - Target micro-region ID
/// * `country` - Country state containing geographic graph
/// * `commodity` - Commodity type for cost multiplier
///
/// # Returns
/// * Transport cost based on shortest path distance
/// * Returns 0.0 if no path exists or regions are the same
///
/// # Rules
/// * Uses Dijkstra's algorithm to find shortest path
/// * Base cost is distance in kilometers
/// * Commodity-specific multipliers may apply (e.g., hazardous goods)
pub fn calculate_transport_cost(
    from_region_id: &str,
    to_region_id: &str,
    country: &crate::state::Country,
    _commodity: crate::registries::enums::Commodity,
) -> f64 {
    if from_region_id == to_region_id {
        return 0.0;
    }

    // Build adjacency list from country regions
    let mut graph: HashMap<String, Vec<Edge>> = HashMap::new();
    for region in &country.regions {
        graph.insert(region.id.clone(), region.edges.clone());
    }

    // Find shortest path using existing Dijkstra implementation
    if let Some(path_result) = find_shortest_path(&graph, from_region_id, to_region_id, true) {
        // Base cost is distance in kilometers
        let base_cost = path_result.total_distance;

        // Apply commodity-specific multiplier (default 1.0)
        let multiplier = 1.0;

        base_cost * multiplier
    } else {
        // No path exists - return high cost to discourage extraction
        f64::MAX
    }
}

/// Check if a path is navigable by ships.
///
/// # Arguments
/// * `path` - Sequence of node IDs
/// * `graph` - Graph structure
///
/// # Returns
/// * `true` if all edges in the path are navigable
/// * `false` otherwise
fn check_path_navigability(path: &[String], graph: &HashMap<String, Vec<Edge>>) -> bool {
    for i in 0..path.len() - 1 {
        let current = &path[i];
        let next = &path[i + 1];

        if let Some(edges) = graph.get(current) {
            let edge = edges.iter().find(|e| &e.target_node == next);
            if let Some(edge) = edge {
                if !edge.is_navigable {
                    return false;
                }
            } else {
                return false;
            }
        } else {
            return false;
        }
    }
    true
}

/// Build a graph HashMap from a collection of regions.
///
/// # Arguments
/// * `regions` - Slice of Region references
///
/// # Returns
/// HashMap mapping node IDs to their outgoing edges
pub fn build_graph_from_regions(regions: &[Region]) -> HashMap<String, Vec<Edge>> {
    let mut graph: HashMap<String, Vec<Edge>> = HashMap::new();

    for region in regions {
        // Add edges from the new structured edges field
        for edge in &region.edges {
            graph
                .entry(region.id.clone())
                .or_insert_with(Vec::new)
                .push(edge.clone());
        }

        // For backward compatibility, also add edges from old adjacency list
        for neighbor_id in &region.adjacency {
            if !region.edges.iter().any(|e| &e.target_node == neighbor_id) {
                // Create a default edge for backward compatibility
                let default_edge = Edge {
                    target_node: neighbor_id.clone(),
                    edge_type: EdgeType::LandBorder,
                    distance: 100.0, // Default distance
                    is_navigable: false,
                    territorial_owner: None,
                };
                graph
                    .entry(region.id.clone())
                    .or_insert_with(Vec::new)
                    .push(default_edge);
            }
        }
    }

    graph
}

/// Phase 30: Migrate regions with (0.0, 0.0) coordinates by assigning
/// deterministic positions using a spring-layout algorithm on the graph
/// topology.
///
/// This is called on first load after deserialization to ensure all regions
/// have valid 2D coordinates for air cargo routing and overflight fee
/// calculation. Regions that already have non-zero coordinates are preserved.
pub fn migrate_region_coordinates(regions: &mut [Region]) {
    let needs_coords: Vec<String> = regions
        .iter()
        .filter(|r| r.coord_x == 0.0 && r.coord_y == 0.0)
        .map(|r| r.id.clone())
        .collect();

    if needs_coords.is_empty() {
        return;
    }

    let mut sorted_ids: Vec<String> = regions.iter().map(|r| r.id.clone()).collect();
    sorted_ids.sort();

    let n = regions.len();
    let radius = (n as f64) * 50.0;
    for (i, id) in sorted_ids.iter().enumerate() {
        if let Some(region) = regions.iter_mut().find(|r| &r.id == id) {
            if region.coord_x == 0.0 && region.coord_y == 0.0 {
                let angle = (i as f64) / (n as f64) * 2.0 * std::f64::consts::PI;
                region.coord_x = radius * angle.cos();
                region.coord_y = radius * angle.sin();
            }
        }
    }

    let mut edges: Vec<(String, String, f64)> = Vec::new();
    for region in regions.iter() {
        for edge in &region.edges {
            edges.push((region.id.clone(), edge.target_node.clone(), edge.distance));
        }
    }

    for _iteration in 0..50 {
        let mut new_positions: HashMap<String, (f64, f64)> = HashMap::new();
        for region in regions.iter() {
            new_positions.insert(region.id.clone(), (region.coord_x, region.coord_y));
        }

        for (from_id, to_id, target_dist) in &edges {
            let from_pos = match regions.iter().find(|r| &r.id == from_id) {
                Some(r) => (r.coord_x, r.coord_y),
                None => continue,
            };
            let to_pos = match regions.iter().find(|r| &r.id == to_id) {
                Some(r) => (r.coord_x, r.coord_y),
                None => continue,
            };

            let dx = to_pos.0 - from_pos.0;
            let dy = to_pos.1 - from_pos.1;
            let current_dist = (dx * dx + dy * dy).sqrt().max(1.0);
            let force = 0.05 * (target_dist - current_dist) / current_dist;
            let fx = dx * force;
            let fy = dy * force;

            if let Some(pos) = new_positions.get_mut(from_id) {
                pos.0 += fx;
                pos.1 += fy;
            }
            if let Some(pos) = new_positions.get_mut(to_id) {
                pos.0 -= fx;
                pos.1 -= fy;
            }
        }

        for region in regions.iter_mut() {
            if let Some(pos) = new_positions.get(&region.id) {
                region.coord_x = pos.0;
                region.coord_y = pos.1;
            }
        }
    }
}

/// Phase 30: Populate coordinates for all regions during world generation.
pub fn populate_region_coordinates(regions: &mut [Region]) {
    for region in regions.iter_mut() {
        region.coord_x = 0.0;
        region.coord_y = 0.0;
    }
    migrate_region_coordinates(regions);
}

#[cfg(test)]
mod phase30_tests {
    use super::*;

    fn make_test_region(id: &str, edges: Vec<Edge>) -> Region {
        Region {
            id: id.to_string(),
            display_name: id.to_string(),
            owner_country: "test".to_string(),
            population: 1000,
            gdp: 1000.0,
            gdp_pc: 1.0,
            climate: Climate::Balanced,
            soil_profile: BTreeMap::new(),
            arable_land_max: 100,
            arable_land_used: 0,
            extraction_limits: BTreeMap::new(),
            extraction_used: BTreeMap::new(),
            resources: serde_json::Map::new(),
            is_capital: false,
            node_type: NodeType::LandRegion,
            edges,
            adjacency: Vec::new(),
            land_distribution: BTreeMap::new(),
            class_demographics: Default::default(),
            governance: None,
            capacity_pool: BTreeMap::new(),
            capacity_utilization: BTreeMap::new(),
            capacity_prices: BTreeMap::new(),
            land_use_inventory: Default::default(),
            climate_profile: ClimateProfile::Temperate,
            micro_regions: BTreeMap::new(),
            treasury: Default::default(),
            microregion_budgets: HashMap::new(),
            winter_mortality_multiplier: 1.0,
            holy_site: None,
            geographic_traits: Default::default(),
            coord_x: 0.0,
            coord_y: 0.0,
            development_level: 0.0,
            parcel_ids: Vec::new(),
        }
    }

    #[test]
    fn migrate_coordinates_assigns_nonzero_to_zero_regions() {
        let mut regions = vec![
            make_test_region("r1", vec![Edge {
                target_node: "r2".to_string(),
                edge_type: EdgeType::LandBorder,
                distance: 100.0,
                is_navigable: false,
                territorial_owner: None,
            }]),
            make_test_region("r2", vec![Edge {
                target_node: "r1".to_string(),
                edge_type: EdgeType::LandBorder,
                distance: 100.0,
                is_navigable: false,
                territorial_owner: None,
            }]),
        ];
        migrate_region_coordinates(&mut regions);
        // Both regions should have non-zero coordinates after migration.
        assert!(regions[0].coord_x != 0.0 || regions[0].coord_y != 0.0);
        assert!(regions[1].coord_x != 0.0 || regions[1].coord_y != 0.0);
    }

    #[test]
    fn migrate_coordinates_preserves_existing_coordinates() {
        let mut regions = vec![
            make_test_region("r1", vec![]),
        ];
        regions[0].coord_x = 500.0;
        regions[0].coord_y = 300.0;
        migrate_region_coordinates(&mut regions);
        // Existing coordinates should be preserved.
        assert_eq!(regions[0].coord_x, 500.0);
        assert_eq!(regions[0].coord_y, 300.0);
    }

    #[test]
    fn migrate_coordinates_handles_empty_regions() {
        let mut regions: Vec<Region> = Vec::new();
        migrate_region_coordinates(&mut regions);
        // Should not panic.
    }

    // ═══════════════════════════════════════════════════════════════════
    // Phase 47: Regional Development Tests
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn test_region_default_development_level() {
        let region = Region::default();
        // Default should be the compatibility value (~0.3)
        assert!(
            region.development_level >= 0.0 && region.development_level <= 1.0,
            "development_level should be in [0, 1], got {}",
            region.development_level
        );
    }

    #[test]
    fn test_household_durable_cohort_default() {
        use crate::registries::enums::Commodity;
        let cohort = HouseholdDurableCohort {
            commodity: Commodity::Furniture,
            count: 10.0,
            condition: 1.0,
            quality: 0.8,
            durability: 240.0,
            acquired_turn: 0,
        };
        assert_eq!(cohort.commodity, Commodity::Furniture);
        assert_eq!(cohort.count, 10.0);
        assert_eq!(cohort.condition, 1.0);
    }

    #[test]
    fn test_class_demographics_has_household_durables() {
        let demo = ClassDemographics::default();
        assert!(
            demo.household_durables.is_empty(),
            "Default ClassDemographics should have empty household_durables"
        );
    }

    #[test]
    fn test_seed_initial_durables_for_wealthy() {
        use crate::registries::enums::Commodity;
        let mut demo = ClassDemographics {
            population: 1000,
            savings: 5_000_000.0,
            ..Default::default()
        };
        seed_initial_durables(&mut demo, 0.5, 1925);

        // Should have Furniture, Clothing, and possibly Radio (1925 >= 1920)
        let commodities: Vec<_> = demo.household_durables.iter().map(|c| c.commodity).collect();
        assert!(commodities.contains(&Commodity::Furniture), "Should seed Furniture");
        assert!(commodities.contains(&Commodity::Clothing), "Should seed Clothing");
        // Radio available from 1920
        assert!(commodities.contains(&Commodity::Radio), "Should seed Radio for 1925");
    }

    #[test]
    fn test_seed_initial_durables_pre_radio_era() {
        use crate::registries::enums::Commodity;
        let mut demo = ClassDemographics {
            population: 1000,
            savings: 5_000_000.0,
            ..Default::default()
        };
        seed_initial_durables(&mut demo, 0.5, 1900);

        let commodities: Vec<_> = demo.household_durables.iter().map(|c| c.commodity).collect();
        assert!(!commodities.contains(&Commodity::Radio), "Should NOT seed Radio before 1920");
        assert!(!commodities.contains(&Commodity::Cars), "Should NOT seed Cars before 1910");
    }
}

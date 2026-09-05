//! Housing and real estate system for Stage 6
//!
//! This module implements housing types, buildings, slots, and commercial real estate
//! with cascading assignment logic and administrative overhead penalties.

use serde::{Deserialize, Serialize};

use super::geography::RuralClass;
use crate::data::perishability_registry;
use crate::registries::enums::Commodity;

/// Housing building type with culturally and mechanically distinct properties
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HousingType {
    #[default]
    /// Peasant hut (multi-generational family)
    Hut,
    /// Slum (informal settlement)
    Slum,
    /// WorkersHousing (workers' housing, industrial era)
    WorkersHousing,
    /// SkilledHousing (higher standard WorkersHousing for specialists/skilled workers)
    SkilledHousing,
    /// Tenement (Kamienica - multi-story urban)
    Tenement,
    /// City palace (aristocratic urban residence)
    CityPalace,
    /// Palace (rural aristocratic estate)
    Palace,
    /// Rectory (Plebania - priest housing)
    Rectory,
    /// Monastery (Klasztor - monk housing)
    Monastery,
    /// Social housing (state-funded)
    SocialHousing,
    /// EstateHousing (Latifundium housing for serfs/landless laborers)
    EstateHousing,
}

/// Housing slots for a building
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HousingSlots {
    /// Total capacity (number of households)
    #[serde(default)]
    pub total_capacity: u32,

    /// Currently occupied slots
    #[serde(default)]
    pub occupied_slots: u32,

    /// Target class for these slots
    pub target_class: Option<RuralClass>,

    /// Rent per slot (if applicable)
    #[serde(default)]
    pub rent_per_slot: f64,
}

/// Utility connections for a building
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct UtilityConnections {
    /// Surface water connected (liters per turn)
    #[serde(default)]
    pub surface_water_capacity: f64,

    /// Groundwater connected (liters per turn)
    #[serde(default)]
    pub groundwater_capacity: f64,

    /// Sewage treatment connected (liters per turn)
    #[serde(default)]
    pub sewage_treatment_capacity: f64,

    /// District heating connected (GJ per turn)
    #[serde(default)]
    pub district_heating_capacity: f64,

    /// Electricity connected (kWh per turn)
    #[serde(default)]
    pub electricity_capacity: f64,

    /// Phase 83 (PARADIGM SHIFT): Quality of water the building actually
    /// received this turn (0.0-1.0). Set by the hydro grid distribution.
    /// PATCH 6 (Universal Water Sickness): biohazard penalty evaluates this
    /// per-building, regardless of source (grid or standalone well).
    #[serde(default)]
    pub water_quality_received: f64,
}

/// Blueprint 006: Physical water well constructed on a housing building
/// for off-grid water sourcing.
///
/// A well must be physically constructed (CAPEX) before a building can
/// use standalone water supply methods (Local Well, Hand Pump Well, etc.).
/// Without a constructed well, `active_water_supply` standalone methods
/// yield zero water — no water from thin air (Rule 1: conservation).
///
/// CAPEX is scaled by the building's total capacity (Rule 15 — no flat rates).
/// A well serving a 100-slot tenement costs more than one serving a 4-slot hut.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WaterWell {
    /// Whether the well has been constructed (false = no well, no water).
    #[serde(default)]
    pub constructed: bool,

    /// Construction progress (0.0 to 1.0). When >= 1.0, `constructed` = true.
    /// Increments per turn based on construction effort allocated.
    #[serde(default)]
    pub construction_progress: f64,

    /// Well depth in meters (scales CAPEX — deeper wells cost more).
    /// Determined by region groundwater depth at construction time.
    #[serde(default)]
    pub depth_m: f64,

    /// Maximum yield per turn in liters (scales with depth and aquifer quality).
    #[serde(default)]
    pub max_yield_liters: f64,

    /// Turn when construction was initiated (for tracking).
    #[serde(default)]
    pub construction_started_turn: u32,

    /// Total CAPEX spent on construction (for amortization, Rule 21).
    #[serde(default)]
    pub total_capex: f64,

    /// Blueprint 006: Maintenance cost per turn (scales by water_extracted
    /// last turn — Rule 15: physical scaling, no flat rates).
    #[serde(default)]
    pub maintenance_cost_per_turn: f64,

    /// Blueprint 006: Remaining yield lifetime (decrements with extraction).
    /// When zero → abandoned = true. Represents physical well degradation.
    #[serde(default)]
    pub remaining_yield_lifetime: f64,

    /// Blueprint 006: Abandoned flag (when true, yield = 0.0, well must be
    /// re-constructed or replaced). Rule 4: no immortal wells.
    #[serde(default)]
    pub abandoned: bool,

    /// Blueprint 006: Water extracted last turn (liters) — used to scale
    /// maintenance cost (Rule 15).
    #[serde(default)]
    pub last_turn_extracted_liters: f64,
}

impl WaterWell {
    /// Check if the well is operational (constructed, not abandoned, has yield).
    pub fn is_operational(&self) -> bool {
        self.constructed && !self.abandoned && self.max_yield_liters > 0.0
    }

    /// Compute the CAPEX cost for constructing a well, scaled by building
    /// capacity and well depth (Rule 15 — no flat rates).
    ///
    /// `building_capacity` = total housing slots (occupants to serve).
    /// `depth_m` = required well depth (from region geology).
    /// `avg_wage` = current average wage (Rule 2 — inflation-proof).
    ///
    /// Returns CAPEX in currency units.
    pub fn compute_capex(building_capacity: u32, depth_m: f64, avg_wage: f64) -> f64 {
        // Base cost scales with capacity (more occupants = bigger well shaft)
        // and depth (deeper = more excavation). Scaled by avg_wage (Rule 2).
        let capacity_factor = building_capacity.max(1) as f64;
        let depth_factor = depth_m.max(1.0);
        // ~2 days of wages per unit of capacity per meter of depth
        let base_labor_days = capacity_factor * depth_factor * 0.5;
        base_labor_days * avg_wage
    }

    /// Compute the physical BOM for well construction, scaled by capacity
    /// and depth (Rule 3 — physical quantities static, Rule 15 — scaled).
    ///
    /// Blueprint 006: Uses Steel (casing/lining), Cement (shaft seal), and
    /// ConstructionMachinery (pump head). NOT Stone/Timber/Bricks — those are
    /// pre-industrial materials inadequate for a deep well that must maintain
    /// structural integrity under groundwater pressure.
    ///
    /// Returns (Commodity, quantity) pairs for the construction materials.
    pub fn compute_capex_bom(
        building_capacity: u32,
        depth_m: f64,
    ) -> Vec<(Commodity, f64)> {
        let capacity = building_capacity.max(1) as f64;
        let depth = depth_m.max(1.0);
        // Steel casing/lining: scales with shaft circumference × depth.
        // ~0.01 tons of steel per slot per meter of depth.
        let steel = capacity * 0.01 * depth;
        // Cement shaft seal: scales with depth (borehole annulus volume).
        // ~0.005 units of cement per slot per meter.
        let cement = capacity * 0.005 * depth;
        // ConstructionMachinery (pump head): scales with capacity (bigger pump
        // for more occupants). ~0.001 machinery units per slot.
        let machinery = capacity * 0.001;

        let mut bom = Vec::new();
        if steel > 0.0 {
            bom.push((Commodity::Steel, steel));
        }
        if cement > 0.0 {
            bom.push((Commodity::Cement, cement));
        }
        if machinery > 0.0 {
            bom.push((Commodity::ConstructionMachinery, machinery));
        }
        bom
    }

    /// Compute maximum well yield based on depth and aquifer quality.
    /// Deeper wells in good aquifers yield more water.
    /// ~50 liters per occupant per turn is baseline demand.
    pub fn compute_max_yield(depth_m: f64, aquifer_quality: f64) -> f64 {
        let depth_bonus = (depth_m / 10.0).min(3.0); // 3x max from depth
        50.0 * depth_bonus * aquifer_quality.clamp(0.1, 1.0)
    }

    /// Blueprint 006: Record water extraction for this turn and update
    /// lifecycle state (maintenance cost, yield lifetime decrement).
    /// Rule 15: maintenance scales by water_extracted.
    /// Rule 4: yield lifetime decrements — wells deplete and can be abandoned.
    pub fn record_extraction(&mut self, liters_extracted: f64) {
        self.last_turn_extracted_liters = liters_extracted;
        // Maintenance cost: ~0.001 currency per liter extracted (Rule 15).
        // Scaled by physical wear on pump and casing.
        self.maintenance_cost_per_turn = liters_extracted * 0.001;
        // Yield lifetime: decrements by extraction volume / total_capacity.
        // A well serving 100 occupants at 50L/turn has ~5000L/turn extraction.
        // Lifetime of ~10000 turns at full load (realistic well lifespan).
        if self.max_yield_liters > 0.0 {
            let lifetime_decrement = liters_extracted / (self.max_yield_liters * 10000.0);
            self.remaining_yield_lifetime = (self.remaining_yield_lifetime - lifetime_decrement).max(0.0);
            if self.remaining_yield_lifetime <= 0.0 {
                self.abandoned = true;
            }
        }
    }

    /// Blueprint 006: Create a fully constructed well for world-generation
    /// off-grid buildings (Day-1 shock prevention — no B2B steel/concrete
    /// demand spike at Turn 0).
    pub fn new_constructed_at_world_gen(depth_m: f64, aquifer_quality: f64) -> Self {
        let max_yield = Self::compute_max_yield(depth_m, aquifer_quality);
        Self {
            constructed: true,
            construction_progress: 1.0,
            depth_m,
            max_yield_liters: max_yield,
            construction_started_turn: 0,
            total_capex: 0.0, // Sunk cost — not tracked for world-gen wells
            maintenance_cost_per_turn: 0.0,
            remaining_yield_lifetime: 10000.0, // Full lifespan
            abandoned: false,
            last_turn_extracted_liters: 0.0,
        }
    }
}

/// Phase 85: Workshop production method (Rule 13 — Technological Matrices).
/// Each method has distinct inputs, output multipliers, and CAPEX.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkshopMethod {
    /// Pre-industrial: hand tools, lowest output, lowest CAPEX
    #[default]
    ManualHandTool,
    /// Medieval: foot-powered machinery, moderate output
    FootPoweredLathe,
    /// Industrial: steam engine drive, high output, requires Coal
    SteamPowered,
    /// Modern: electric motor drive, highest output, requires Electricity
    ElectricMotor,
}

/// Phase 85: Reference to a workshop occupying a commercial slot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WorkshopRef {
    /// Guild company ID that coordinates this workshop
    #[serde(default)]
    pub guild_id: String,
    /// Demographic class ID that provides FTE
    #[serde(default)]
    pub craftsman_class_id: String,
    /// Output commodity produced by this workshop
    #[serde(default)]
    pub output_commodity: String,
    /// FTE allocated to this workshop this turn
    #[serde(default)]
    pub fte_allocated: f64,
}

/// Phase 85: Commercial slot on a HousingBuilding for ground-floor workshops.
/// Represents mixed-use zoning — residential above, workshop below.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CommercialSlot {
    /// Floor area available for workshop in sqm (clamped to building floor_area)
    #[serde(default)]
    pub capacity_sqm: f64,
    /// Current workshop occupying the slot (None = vacant)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_workshop: Option<WorkshopRef>,
    /// Which commodities can be produced here (based on zoning + utilities)
    #[serde(default)]
    pub allowed_crafts: Vec<String>,
    /// Current production method (Rule 13 — upgradeable)
    #[serde(default)]
    pub active_method: WorkshopMethod,
}

/// Housing building with capacity and utility connections
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HousingBuilding {
    /// Unique building ID
    #[serde(default)]
    pub id: String,

    /// Housing type
    pub housing_type: HousingType,

    /// Micro-region where building is located
    #[serde(default)]
    pub micro_region_id: String,

    /// Owner (LegalForm ID or individual)
    #[serde(default)]
    pub owner: String,

    /// Primary housing slots (for designated class)
    #[serde(default)]
    pub primary_slots: HousingSlots,

    /// Sublet slots (for landless laborers - huts only)
    pub sublet_slots: Option<HousingSlots>,

    /// Living standard 0-1 (affects health/satisfaction)
    #[serde(default)]
    pub living_standard: f64,

    /// Construction cost
    #[serde(default)]
    pub construction_cost: f64,

    /// Maintenance cost per turn
    #[serde(default)]
    pub maintenance_cost: f64,

    /// Current condition 0-1
    #[serde(default)]
    pub condition: f64,

    /// Utility connections
    #[serde(default)]
    pub utility_connections: UtilityConnections,

    /// Phase 81 Wave 2: Active lighting method (e.g., "Kerosene Lamps", "LED Lighting").
    /// Empty string = no lighting. Determines per-turn lighting commodity consumption.
    #[serde(default)]
    pub active_lighting: String,

    /// Phase 81 Wave 2: Active heating method (e.g., "Coal Stove", "Heat Pump").
    /// Empty string = no heating. Determines per-turn heating commodity consumption.
    #[serde(default)]
    pub active_heating: String,

    /// Phase 81 Wave 2: Active power generation method (e.g., "None", "Rooftop PV").
    /// Empty string = "None". Determines microgeneration output and CAPEX.
    #[serde(default)]
    pub active_power_generation: String,

    /// Phase 83: Active water supply method (e.g., "Local Well", "Municipal Mains").
    /// Empty string = "None". Determines whether the building draws from
    /// WaterReserveState (standalone) or WaterNetworkState (centralized).
    #[serde(default)]
    pub active_water_supply: String,

    /// Phase 83: Active sanitation method (e.g., "Open Defecation", "Municipal Sewer").
    /// Empty string = "None". Determines whether the building discharges to
    /// environment (standalone, biohazard) or SewerNetworkState (centralized).
    #[serde(default)]
    pub active_sanitation: String,

    /// Phase 84: Active waste disposal method (e.g., "Primitive Dumping",
    /// "Basic Homesteading", "Unsegregated Collection"). Empty string = "None".
    /// Determines whether waste is self-disposed (standalone, pollution) or
    /// collected by municipal WasteGridState (centralized).
    #[serde(default)]
    pub active_waste_disposal: String,

    /// Phase 81 Wave 2: Pending consumption-method upgrade (None if no upgrade
    /// in progress). Only one upgrade per building at a time. The active method
    /// string ONLY flips when `is_complete()` returns true (Flaw 2 correction).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_upgrade: Option<crate::construction::upgrade_project::UpgradeProject>,

    /// Phase 85: Commercial slot for ground-floor workshop (mixed-use zoning).
    /// None = purely residential. Some = mixed-use with workshop capacity.
    /// Capacity scales with building floor_area (Rule 15 — no flat rates).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commercial_slots: Option<CommercialSlot>,

    /// Blueprint 006: Physical water well for off-grid water sourcing.
    /// A well must be constructed (CAPEX) before standalone water supply
    /// methods can draw water. None = no well, no off-grid water.
    /// Centralized (municipal mains) buildings don't need a well.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub water_well: Option<WaterWell>,
}

/// Commercial building type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CommercialBuildingType {
    #[default]
    /// Office building
    Office,
    /// Shopping mall/retail center
    Retail,
    /// Mixed-use
    MixedUse,
    /// Industrial warehouse
    Warehouse,
    /// Phase 6.5: Traditional marketplace (open-air stalls, historical)
    Marketplace,
    /// Phase 6.5: Wholesale distribution center
    Wholesaler,
    /// Phase 6.5: Small independent retail store
    RetailStore,
    /// Phase 6.5: Modern supermarket (self-service, refrigerated)
    Supermarket,
    /// Phase 6.5: Department store (multi-category, large footprint)
    DepartmentStore,
    /// Phase 6.5: Shopping center (enclosed mall with multiple tenants)
    ShoppingCenter,
    /// Phase 9: Hotel (tourism accommodation)
    Hotel,
    /// Phase 9: Resort (luxury tourism accommodation + amenities)
    Resort,
    /// Phase 9: Restaurant (food service for tourists)
    Restaurant,
    /// Phase 9: Casino (entertainment venue)
    Casino,
    /// Phase 30: Gas station / fuel retail outlet
    GasStation,
}

/// Storage type for warehouses (Phase 5)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StorageType {
    #[default]
    /// General warehouse for dry goods
    GeneralWarehouse,
    /// Cold storage for perishable food/medicine
    ColdStorage,
    /// Liquid tanks for fuel/chemicals
    LiquidTanks,
    /// Hazardous material storage
    Hazardous,
}

/// Inventory batch for FEFO (First-Expired-First-Out) warehouse management (Phase 5.5)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InventoryBatch {
    /// Quantity of goods in this batch
    pub quantity: f64,

    /// Turn number when this batch was stored
    pub storage_turn: u32,

    /// Original owner company ID (producer retains ownership)
    pub owner_id: String,

    /// Accumulated storage fees owed to LogisticsCompany
    #[serde(default)]
    pub accumulated_fees: f64,

    /// Warehouse ID where this batch is stored
    pub warehouse_id: String,

    /// Fire-sale discount (0.0-1.0) for aging batches
    #[serde(default)]
    pub fire_sale_discount: f64,

    /// Phase 6.5: Acquisition cost per unit (for retail pricing)
    #[serde(default)]
    pub acquisition_cost_per_unit: f64,
}

/// Phase 6.5: Store profile type for retail buildings
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreProfile {
    /// Grocery store (food staples)
    Grocery,
    /// Butcher shop (protein)
    Butcher,
    /// Bakery (cereal-based goods)
    Bakery,
    /// Clothing store
    Clothing,
    /// Household goods
    Household,
    /// Electronics/appliances
    Electronics,
    /// Luxury goods
    Luxury,
    /// Phase 20: Car dealer (cars and trucks)
    CarDealer,
    /// Phase 30: Gas station (motor fuel retail)
    GasStation,
}

/// Phase 6.5: Retail upgrade for storefronts
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetailUpgrade {
    /// City-scale logistics network
    CityScales,
    /// Paved square for markets
    PavedSquare,
    /// Covered hall for weather protection
    CoveredHall,
    /// Cold counter for perishables
    ColdCounter,
    /// Advertising campaign
    Advertising,
}

/// Phase 6.5: Retail profile for storefronts (RetailStore, supermarket, DepartmentStore)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RetailProfile {
    /// Store profile types (e.g., Grocery, Butcher, Bakery)
    #[serde(default)]
    pub profiles: std::collections::BTreeSet<StoreProfile>,

    /// Base attractiveness from building template
    #[serde(default)]
    pub base_attractiveness: f64,

    /// Installed upgrades
    #[serde(default)]
    pub upgrades: std::collections::BTreeSet<RetailUpgrade>,

    /// Effective attractiveness (recomputed each turn in R2)
    #[serde(default)]
    pub effective_attractiveness: f64,

    /// Markup ratio set by R3 pricing
    #[serde(default)]
    pub markup_ratio: f64,

    /// Landlord building ID if tenant of ShoppingCenter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub landlord_building_id: Option<String>,

    /// Leased square meters
    #[serde(default)]
    pub leased_sqm: f64,

    /// Units sold last turn per commodity
    #[serde(default)]
    pub units_sold_last_turn: std::collections::BTreeMap<Commodity, f64>,

    /// Unmet demand last turn per commodity
    #[serde(default)]
    pub unmet_demand_last_turn: std::collections::BTreeMap<Commodity, f64>,

    /// Phase 6.5: Market share last turn per commodity (for consumer inertia)
    #[serde(default)]
    pub market_share_last_turn: std::collections::BTreeMap<Commodity, f64>,

    /// Phase 6.5: First active turn (for newcomer grace period)
    #[serde(default)]
    pub first_active_turn: u32,
}

/// Phase 6.5: Shopping center profile (enclosed mall)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ShoppingCenterProfile {
    /// Tenant building IDs (stores in this mall)
    #[serde(default)]
    pub tenant_building_ids: Vec<String>,

    /// Diversity bonus (0-1) for having varied store types
    #[serde(default)]
    pub diversity_bonus: f64,

    /// Anchor tenant (major store driving traffic)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_tenant: Option<String>,
}

/// Phase 6.5: Wholesale profile for distribution centers
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WholesaleProfile {
    /// Served micro-region IDs
    #[serde(default)]
    pub served_micro_regions: std::collections::BTreeSet<String>,

    /// Consolidation capacity tons per turn
    #[serde(default)]
    pub consolidation_capacity_tons: f64,

    /// Committed tons this turn
    #[serde(default)]
    pub committed_tons_this_turn: f64,

    /// Phase 6.5: Units sold to retailers last turn per commodity
    #[serde(default)]
    pub units_sold_to_retailers_last_turn: std::collections::BTreeMap<Commodity, f64>,

    /// Phase 6.5: Consecutive turns commodity has sat above stock target
    #[serde(default)]
    pub stale_turns: std::collections::BTreeMap<Commodity, u32>,
}

/// Phase 6.5: Retail lease agreement
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RetailLease {
    /// Tenant company ID
    pub tenant_id: String,

    /// Leased square meters
    pub leased_sqm: f64,

    /// Rent per sq meter
    pub rent_per_sqm: f64,

    /// Lease start turn
    pub start_turn: u32,

    /// Lease duration in turns
    pub duration_turns: u32,
}

/// Commercial building with office and retail space
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CommercialBuilding {
    /// Unique building ID
    #[serde(default)]
    pub id: String,

    /// Building type
    pub building_type: CommercialBuildingType,

    /// Micro-region location
    #[serde(default)]
    pub micro_region_id: String,

    /// Owner company ID (Phase 6.3.5 - for asset transfer during liquidation)
    #[serde(default)]
    pub owner_id: String,

    /// Office space capacity (sq meters)
    #[serde(default)]
    pub office_capacity: f64,

    /// Retail space capacity (sq meters)
    #[serde(default)]
    pub retail_capacity: f64,

    /// Currently leased by companies
    #[serde(default)]
    pub tenants: Vec<String>, // Company IDs

    /// Rent per sq meter
    #[serde(default)]
    pub rent_per_sqm: f64,

    /// Utility connections
    #[serde(default)]
    pub utility_connections: UtilityConnections,

    /// Phase 5: Storage capacity (for warehouses)
    #[serde(default)]
    pub storage_capacity: f64,

    /// Phase 5.5: Current inventory stored in this building (batched for FEFO)
    #[serde(default)]
    pub current_inventory: std::collections::BTreeMap<String, Vec<InventoryBatch>>,

    /// Phase 5: Storage type (for warehouses)
    #[serde(default)]
    pub storage_type: StorageType,

    /// Phase 5: Utilization rate (0-1) for storage
    #[serde(default)]
    pub utilization_rate: f64,

    /// Phase 6.5: Retail profile (for RetailStore, supermarket, DepartmentStore)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retail_profile: Option<RetailProfile>,

    /// Phase 6.5: Shopping center profile (for ShoppingCenter)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shopping_center_profile: Option<ShoppingCenterProfile>,

    /// Phase 6.5: Wholesale profile (for Wholesaler)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wholesale_profile: Option<WholesaleProfile>,

    /// Phase 6.5: Active retail leases (for ShoppingCenter landlords)
    #[serde(default)]
    pub retail_leases: Vec<RetailLease>,

    /// Phase 19B: Fixed-asset cohorts (retail fixtures, shelving, cold counters)
    /// installed in this commercial building. Empty = no fixtures. Cohorts are
    /// aggregated by blueprint+acquire turn+condition for RAM predictability.
    #[serde(default)]
    pub fixed_assets: Vec<crate::economy::fixed_assets::FixedAssetCohort>,

    /// Phase 81 Wave 2: Active lighting method (e.g., "Kerosene Lamps", "LED Lighting").
    /// Empty string = no lighting. Determines per-turn lighting commodity consumption.
    #[serde(default)]
    pub active_lighting: String,

    /// Phase 81 Wave 2: Active heating method (e.g., "Coal Stove", "Heat Pump").
    /// Empty string = no heating. Determines per-turn heating commodity consumption.
    #[serde(default)]
    pub active_heating: String,

    /// Phase 81 Wave 2: Active power generation method (e.g., "None", "Rooftop PV").
    /// Empty string = "None". Determines microgeneration output and CAPEX.
    #[serde(default)]
    pub active_power_generation: String,

    /// Phase 83: Active water supply method (e.g., "Local Well", "Municipal Mains").
    /// Empty string = "None". Determines whether the building draws from
    /// WaterReserveState (standalone) or WaterNetworkState (centralized).
    #[serde(default)]
    pub active_water_supply: String,

    /// Phase 83: Active sanitation method (e.g., "Open Defecation", "Municipal Sewer").
    /// Empty string = "None". Determines whether the building discharges to
    /// environment (standalone, biohazard) or SewerNetworkState (centralized).
    #[serde(default)]
    pub active_sanitation: String,

    /// Phase 84: Active waste disposal method (e.g., "Primitive Dumping",
    /// "Basic Homesteading", "Unsegregated Collection"). Empty string = "None".
    /// Determines whether waste is self-disposed (standalone, pollution) or
    /// collected by municipal WasteGridState (centralized).
    #[serde(default)]
    pub active_waste_disposal: String,

    /// Phase 81 Wave 2: Pending consumption-method upgrade (None if no upgrade
    /// in progress). Only one upgrade per building at a time. The active method
    /// string ONLY flips when `is_complete()` returns true (Flaw 2 correction).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_upgrade: Option<crate::construction::upgrade_project::UpgradeProject>,
}

/// Housing inventory for a micro-region
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HousingInventory {
    /// Housing buildings in this micro-region
    #[serde(default)]
    pub buildings: Vec<HousingBuilding>,
}

/// Commercial inventory for a micro-region
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CommercialInventory {
    /// Commercial buildings in this micro-region
    #[serde(default)]
    pub buildings: Vec<CommercialBuilding>,
}

impl HousingSlots {
    /// Calculate occupancy rate (0-1)
    pub fn occupancy_rate(&self) -> f64 {
        if self.total_capacity == 0 {
            0.0
        } else {
            self.occupied_slots as f64 / self.total_capacity as f64
        }
    }

    /// Available slots
    pub fn available_slots(&self) -> u32 {
        self.total_capacity.saturating_sub(self.occupied_slots)
    }
}

impl HousingBuilding {
    /// Calculate total housing capacity (primary + sublet)
    pub fn total_capacity(&self) -> u32 {
        let primary = self.primary_slots.total_capacity;
        let sublet = self
            .sublet_slots
            .as_ref()
            .map(|s| s.total_capacity)
            .unwrap_or(0);
        primary + sublet
    }

    /// Calculate total occupied slots
    pub fn total_occupied(&self) -> u32 {
        let primary = self.primary_slots.occupied_slots;
        let sublet = self
            .sublet_slots
            .as_ref()
            .map(|s| s.occupied_slots)
            .unwrap_or(0);
        primary + sublet
    }

    /// Check if building is overcrowded
    pub fn is_overcrowded(&self) -> bool {
        self.total_occupied() > self.total_capacity()
    }

    /// Blueprint 006: Check if this building can draw water from a standalone
    /// (off-grid) source. Returns true only if:
    /// 1. `active_water_supply` is a standalone method (not centralized)
    /// 2. A `WaterWell` exists and is constructed/operational
    ///
    /// Without a constructed well, standalone water supply yields zero water
    /// — no water from thin air (Rule 1: conservation).
    pub fn can_draw_standalone_water(&self) -> bool {
        use crate::utilities::consumption_bom::is_centralized_water_method;
        if self.active_water_supply.is_empty() || self.active_water_supply == "None" {
            return false;
        }
        if is_centralized_water_method(&self.active_water_supply) {
            return false; // Centralized — doesn't need a well
        }
        self.water_well
            .as_ref()
            .map(|w| w.is_operational())
            .unwrap_or(false)
    }

    /// Blueprint 006: Get the maximum standalone water yield available
    /// from this building's well (0.0 if no well, not constructed, or abandoned).
    pub fn standalone_water_yield(&self) -> f64 {
        if !self.can_draw_standalone_water() {
            return 0.0;
        }
        self.water_well
            .as_ref()
            .map(|w| {
                if w.abandoned {
                    0.0
                } else {
                    w.max_yield_liters
                }
            })
            .unwrap_or(0.0)
    }
}

impl CommercialBuilding {
    /// Calculate storage fee based on OPEX and utilization rate (Phase 5).
    ///
    /// # Returns
    /// * Storage fee per unit of stored goods
    ///
    /// # Rules
    /// * Base fee is derived from utility capacity divided by storage capacity
    /// * Fee increases with utilization rate (higher utilization = higher demand = higher price)
    /// * Cold storage and hazardous storage have multipliers (2.0x and 3.0x respectively)
    pub fn calculate_storage_fee(&self) -> f64 {
        if self.storage_capacity == 0.0 {
            return 0.0;
        }

        // Base fee from utility capacity (proxy for OPEX)
        let utility_capacity = self.utility_connections.electricity_capacity
            + self.utility_connections.district_heating_capacity
            + self.utility_connections.surface_water_capacity;

        let base_fee = utility_capacity / self.storage_capacity;

        // Storage type multiplier
        let type_multiplier = match self.storage_type {
            StorageType::GeneralWarehouse => 1.0,
            StorageType::ColdStorage => 2.0,
            StorageType::LiquidTanks => 1.5,
            StorageType::Hazardous => 3.0,
        };

        // Utilization-based pricing: higher utilization = higher fee
        // Fee scales from 0.5x at 0% utilization to 2.0x at 100% utilization
        let utilization_multiplier = 0.5 + (self.utilization_rate * 1.5);

        base_fee * type_multiplier * utilization_multiplier
    }

    /// Update utilization rate based on current inventory (Phase 5.5).
    pub fn update_utilization_rate(&mut self) {
        if self.storage_capacity == 0.0 {
            self.utilization_rate = 0.0;
            return;
        }

        let total_stored: f64 = self
            .current_inventory
            .values()
            .flat_map(|batches| batches.iter().map(|b| b.quantity))
            .sum();
        self.utilization_rate = (total_stored / self.storage_capacity).min(1.0);
    }

    /// Deposit inventory into this building with FEFO logic (Phase 6.3.5).
    ///
    /// # Arguments
    /// * `commodity_key` - Commodity identifier (e.g., "Cereal", "Vegetable")
    /// * `quantity` - Quantity to deposit
    /// * `owner_id` - Owner company ID
    /// * `current_turn` - Current turn number for batch tracking
    ///
    /// # Returns
    /// * Excess quantity that could not be stored (if building is full)
    ///
    /// # Rules
    /// * Creates new InventoryBatch with current turn for FEFO tracking
    /// * Checks capacity before depositing
    /// * Returns excess if building would exceed capacity
    pub fn deposit_inventory(
        &mut self,
        commodity_key: String,
        quantity: f64,
        owner_id: String,
        current_turn: u32,
    ) -> f64 {
        if quantity <= 0.0 {
            return 0.0;
        }

        // Calculate current stored quantity
        let current_stored: f64 = self
            .current_inventory
            .values()
            .flat_map(|batches| batches.iter().map(|b| b.quantity))
            .sum();

        // Check capacity
        let available_capacity = self.storage_capacity - current_stored;
        if available_capacity <= 0.0 {
            return quantity; // Building is full, return all as excess
        }

        // Deposit up to available capacity
        let deposit_amount = quantity.min(available_capacity);

        // Create new batch
        let batch = InventoryBatch {
            quantity: deposit_amount,
            storage_turn: current_turn,
            owner_id,
            accumulated_fees: 0.0,
            warehouse_id: self.id.clone(),
            fire_sale_discount: 0.0,
            acquisition_cost_per_unit: 0.0, // Will be set by B2B settlement
        };

        // Add to inventory
        self.current_inventory
            .entry(commodity_key)
            .or_default()
            .push(batch);

        // Return excess
        quantity - deposit_amount
    }

    /// Withdraw inventory from this building (oldest batches first) (Phase 6.3.5).
    ///
    /// # Arguments
    /// * `commodity_key` - Commodity identifier
    /// * `quantity` - Quantity to withdraw
    /// * `owner_id` - Owner company ID (only withdraw from this owner's batches)
    ///
    /// # Returns
    /// * Actual quantity withdrawn (clamped to available)
    ///
    /// # Rules
    /// * Uses FEFO (First-Expired-First-Out) - withdraw from oldest batches first
    /// * Only withdraws from batches matching the specified owner_id
    /// * Removes empty batches after withdrawal
    pub fn withdraw_inventory(
        &mut self,
        commodity_key: &str,
        quantity: f64,
        owner_id: &str,
    ) -> f64 {
        if quantity <= 0.0 {
            return 0.0;
        }

        let batches = self.current_inventory.get_mut(commodity_key);
        if batches.is_none() {
            return 0.0;
        }

        let batches = batches.unwrap();
        let mut remaining_to_withdraw = quantity;
        let mut total_withdrawn = 0.0;

        // Sort by storage_turn (oldest first) for FEFO
        batches.sort_by_key(|b| b.storage_turn);

        // Withdraw from batches (oldest first)
        for batch in batches.iter_mut() {
            if batch.owner_id != owner_id {
                continue; // Skip batches not owned by this company
            }

            if remaining_to_withdraw <= 0.0 {
                break;
            }

            let withdraw_from_batch = remaining_to_withdraw.min(batch.quantity);
            batch.quantity -= withdraw_from_batch;
            total_withdrawn += withdraw_from_batch;
            remaining_to_withdraw -= withdraw_from_batch;
        }

        // Remove empty batches
        batches.retain(|b| b.quantity > 0.0);

        // Remove commodity entry if no batches remain
        if batches.is_empty() {
            self.current_inventory.remove(commodity_key);
        }

        total_withdrawn
    }

    /// Apply perishability to stored goods with batch-based FEFO logic (Phase 5.5).
    ///
    /// # Arguments
    /// * `current_turn` - Current turn number for age calculation
    ///
    /// # Returns
    /// * `(f64, Vec<InventoryBatch>)` — total decayed quantity and destroyed batches for rot fee settlement
    ///
    /// # Rules
    /// * Uses static perishability registry for commodity-specific decay rates.
    /// * Agricultural commodities (Phase 6.3.5) have defined shelf lives.
    /// * Legacy Polish-keyed goods (Food, Medicine) use registry entries for compatibility.
    /// * Unknown inventory keys are treated as non-perishable (no decay).
    /// * Decayed goods are removed from inventory and returned for rot fee settlement.
    /// * Owner must pay accumulated fees to warehouse owner for rotted batches.
    pub fn apply_perishability(&mut self, current_turn: u32) -> (f64, Vec<InventoryBatch>) {
        let mut total_decayed = 0.0;
        let mut destroyed_batches = Vec::new();
        let registry = perishability_registry();

        for (commodity_str, batches) in self.current_inventory.iter_mut() {
            // Parse commodity key with fallback for unknown keys
            let commodity = Commodity::try_from(commodity_str.as_str()).ok();
            let profile = commodity.and_then(|c| registry.get(&c));

            let (max_turns, decay_rate) = if let Some(profile) = profile {
                if self.storage_type == StorageType::ColdStorage {
                    (profile.max_turns_cold, profile.decay_rate_cold)
                } else {
                    (profile.max_turns_general, profile.decay_rate_general)
                }
            } else {
                // Unknown or non-perishable commodity
                (u32::MAX, 0.0)
            };

            if decay_rate > 0.0 {
                for batch in batches.iter_mut() {
                    let age = current_turn.saturating_sub(batch.storage_turn);

                    if age >= max_turns {
                        // Batch has expired - full decay
                        let decayed = batch.quantity;
                        total_decayed += decayed;
                        destroyed_batches.push(InventoryBatch {
                            quantity: 0.0,
                            storage_turn: batch.storage_turn,
                            owner_id: batch.owner_id.clone(),
                            accumulated_fees: batch.accumulated_fees,
                            warehouse_id: batch.warehouse_id.clone(),
                            fire_sale_discount: batch.fire_sale_discount,
                            acquisition_cost_per_unit: batch.acquisition_cost_per_unit,
                        });
                        batch.quantity = 0.0;
                    } else if age > 0 {
                        // Partial decay based on age
                        let decayed = batch.quantity * decay_rate;
                        // Rule 20: Clamp to zero — inventory cannot go negative.
                        batch.quantity = (batch.quantity - decayed).max(0.0);
                        total_decayed += decayed;

                        if batch.quantity <= 0.0 {
                            destroyed_batches.push(InventoryBatch {
                                quantity: 0.0,
                                storage_turn: batch.storage_turn,
                                owner_id: batch.owner_id.clone(),
                                accumulated_fees: batch.accumulated_fees,
                                warehouse_id: batch.warehouse_id.clone(),
                                fire_sale_discount: batch.fire_sale_discount,
                                acquisition_cost_per_unit: batch.acquisition_cost_per_unit,
                            });
                        }
                    }
                }

                // Remove empty batches
                batches.retain(|b| b.quantity > 0.0);
            }
        }

        // Remove commodities with no remaining batches
        self.current_inventory
            .retain(|_, batches| !batches.is_empty());

        (total_decayed, destroyed_batches)
    }

    /// Evaluate fire-sale eligibility for an aging batch (Phase 5.5).
    ///
    /// # Arguments
    /// * `batch` - The inventory batch to evaluate
    /// * `current_turn` - Current turn number for age calculation
    /// * `commodity` - The commodity type (string key for inventory lookup)
    /// * `storage_type` - The storage type of the warehouse
    /// * `production_cost` - Base production cost for the commodity
    /// * `market_price` - Current market price for the commodity
    ///
    /// # Returns
    /// * Optional discount (0.0-1.0) if eligible for fire-sale, None otherwise
    ///
    /// # Rules
    /// * Only offer fire-sale on last turn before expiration
    /// * Hard rule: Never sell below 90% of production cost (unless bankruptcy imminent)
    /// * Typical fire-sale discount: 30%
    /// * Uses perishability_registry() for commodity-specific shelf life
    pub fn evaluate_fire_sale_eligibility(
        batch: &InventoryBatch,
        current_turn: u32,
        commodity: &str,
        storage_type: StorageType,
        production_cost: f64,
        market_price: f64,
    ) -> Option<f64> {
        // Try to parse commodity string to Commodity enum
        let commodity_enum = crate::registries::enums::Commodity::try_from(commodity).ok()?;

        // Look up perishability profile
        let profile =
            crate::data::perishability_registry::perishability_registry().get(&commodity_enum)?;

        let age = current_turn.saturating_sub(batch.storage_turn);
        let max_turns = if storage_type == StorageType::ColdStorage {
            profile.max_turns_cold
        } else {
            profile.max_turns_general
        };

        // Non-perishable goods (u32::MAX) don't need fire-sales
        if max_turns == u32::MAX {
            return None;
        }

        let remaining_turns = max_turns.saturating_sub(age);

        // Only offer fire-sale on last turn before expiration
        if remaining_turns == 1 {
            let min_price = production_cost * 0.9; // 10% loss tolerance

            if market_price > min_price {
                // Calculate discount to ensure quick sale
                let discount = 0.3; // 30% discount for fire-sale
                Some(discount)
            } else {
                None // Market already at floor - no fire-sale
            }
        } else {
            None // Not urgent enough
        }
    }

    /// Route decayed goods to Stage 6 Landfill system (Phase 5).
    ///
    /// # Arguments
    /// * `decayed_amount` - Amount of decayed goods to route
    /// * `landfill` - Optional reference to the nearest landfill
    /// * `region` - Mutable reference to the region for pollution fallback
    ///
    /// # Rules
    /// * If landfill exists and has capacity, call `landfill.process_waste(decayed_amount)`
    /// * If no landfill or landfill is full, convert decayed amount directly to region pollution
    /// * This integrates with Stage 6 Real Estate and Waste systems
    pub fn route_decayed_to_landfill(
        decayed_amount: f64,
        landfill: Option<&mut crate::utilities::waste::Landfill>,
        region: Option<&mut crate::society::geography::Region>,
    ) {
        if decayed_amount <= 0.0 {
            return;
        }

        if let Some(landfill) = landfill {
            if landfill.has_capacity() {
                // Route to landfill for processing
                landfill.process_waste(decayed_amount);
                return;
            }
        }

        // No landfill or landfill full - convert to pollution directly
        // Store pollution in the region's resources (resources) field as a fallback
        if let Some(region) = region {
            // This is a placeholder - actual pollution tracking would be in a dedicated field
            // For now, we store it in the resources map under "pollution" (pollution)
            let pollution_key = "pollution".to_string();
            let current_pollution = region
                .resources
                .get(&pollution_key)
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let new_pollution = (current_pollution + decayed_amount * 0.1).min(100.0);
            region.resources.insert(
                pollution_key,
                serde_json::Value::Number(
                    serde_json::Number::from_f64(new_pollution)
                        .unwrap_or(serde_json::Number::from(0)),
                ),
            );
        }
    }
}

// ============================================================================
// BLUEPRINT 007: HOUSING COOPERATIVE LIFECYCLE
// Complete lifecycle: genesis → growth → collapse → liquidation
// ============================================================================

/// Lifecycle stage of a housing cooperative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CooperativeLifecycleStage {
    /// Cooperative has been founded but is not yet operational (collecting
    /// share capital from founding members).
    #[default]
    Forming,
    /// Cooperative is operational — collecting fees, maintaining buildings,
    /// and accepting new members up to capacity.
    Operational,
    /// Cooperative is in financial distress — missed fee payments,
    /// declining reserves, but not yet insolvent.
    Distressed,
    /// Cooperative is insolvent — reserves depleted, fees uncollected,
    /// pending liquidation by the Syndic.
    Insolvent,
    /// Cooperative has been liquidated — assets sold, members displaced,
    /// ledger closed. Retained for historical tracking.
    Liquidated,
}

/// Wealth tier of a cooperative member — determines fee schedule,
/// emigration probability, and capital flight amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WealthTier {
    /// Lowest tier — subsistence income, minimal savings.
    #[default]
    Destitute,
    /// Working class — wage income, modest savings.
    Working,
    /// Middle class — stable income, significant savings.
    Middle,
    /// Upper class — high income, large liquid capital.
    Upper,
}

/// Individual cooperative ledger tracking assets, debts, and resident
/// obligations for a single housing cooperative (Rule 7 — individual
/// accountability).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CooperativeLedger {
    /// Total share capital paid by members (equity).
    #[serde(default)]
    pub share_capital: f64,

    /// Reserve fund for maintenance and emergencies.
    /// Scales by building age, floor_area, and maintenance history (Rule 15).
    #[serde(default)]
    pub reserve_fund: f64,

    /// Outstanding maintenance expenses owed to contractors.
    #[serde(default)]
    pub outstanding_maintenance_debt: f64,

    /// Utility bills owed to grid operators (water, heat, electricity).
    #[serde(default)]
    pub outstanding_utility_debt: f64,

    /// Per-member fee obligations: member_id → (amount owed, turns overdue).
    /// Unpaid fees trigger eviction and debt collection (Rule 8).
    #[serde(default)]
    pub member_fee_arrears: std::collections::BTreeMap<String, (f64, u32)>,

    /// Total fee revenue collected this turn.
    #[serde(default)]
    pub fee_revenue_this_turn: f64,

    /// Total maintenance costs this turn.
    #[serde(default)]
    pub maintenance_costs_this_turn: f64,

    /// Consecutive turns with negative net income (distress counter).
    #[serde(default)]
    pub consecutive_loss_turns: u32,

    /// Total building floor_area under management (sqm) — scales reserve
    /// requirements (Rule 15).
    #[serde(default)]
    pub total_floor_area_sqm: f64,

    /// Average building age under management (turns) — older buildings
    /// require higher reserve funds (Rule 15).
    #[serde(default)]
    pub avg_building_age_turns: f64,
}

impl CooperativeLedger {
    /// Compute the required minimum reserve fund, scaled by floor area,
    /// building age, and average wage (Rules 2, 15).
    pub fn required_reserve(&self, avg_wage: f64) -> f64 {
        // Base: 0.5 months of wages per 100 sqm (scales with physical size)
        let area_factor = self.total_floor_area_sqm / 100.0;
        // Age multiplier: 1.0 at age 0, +2% per turn of age (compounding decay)
        let age_mult = 1.0 + (self.avg_building_age_turns * 0.02);
        // Maintenance history factor: more arrears = need more reserves
        let arrears_factor = 1.0 + (self.outstanding_maintenance_debt / avg_wage.max(1.0)).min(5.0);
        area_factor * age_mult * arrears_factor * avg_wage * 0.5
    }

    /// Check if the cooperative is solvent (reserves + share capital > debts).
    pub fn is_solvent(&self) -> bool {
        let assets = self.share_capital + self.reserve_fund;
        let liabilities = self.outstanding_maintenance_debt + self.outstanding_utility_debt;
        assets > liabilities
    }

    /// Check if the cooperative is in distress (3+ consecutive loss turns
    /// or reserve fund below 50% of required).
    pub fn is_distressed(&self, avg_wage: f64) -> bool {
        if self.consecutive_loss_turns >= 3 {
            return true;
        }
        let required = self.required_reserve(avg_wage);
        self.reserve_fund < required * 0.5
    }

    /// Check if the cooperative is insolvent (cannot cover debts).
    pub fn is_insolvent(&self) -> bool {
        !self.is_solvent() && self.reserve_fund <= 0.0
    }

    /// Process fee collection from members for this turn.
    /// Returns (total_collected, total_uncollected).
    /// Fees scale by average_wage and floor_area (Rule 2 — no flat rates).
    pub fn process_fee_collection(
        &mut self,
        member_count: u32,
        avg_wage: f64,
        floor_area_per_member: f64,
    ) -> (f64, f64) {
        // Fee = 0.02 * avg_wage per member per turn (2% of monthly wage)
        // scaled by floor area (larger units pay more — Rule 15)
        let area_factor = (floor_area_per_member / 50.0).max(0.5);
        let fee_per_member = avg_wage * 0.02 * area_factor;
        let total_billed = fee_per_member * member_count as f64;

        // Collect from reserve fund capacity — if members can't pay,
        // the arrears accumulate (Rule 8 — rational actors)
        let collected = total_billed.min(self.share_capital + self.reserve_fund);
        let uncollected = total_billed - collected;

        self.fee_revenue_this_turn = collected;
        self.reserve_fund += collected;

        if uncollected > 0.0 {
            // Record arrears at the cooperative level (individual member
            // tracking is done via member_fee_arrears map)
            self.outstanding_maintenance_debt += uncollected;
        }

        (collected, uncollected)
    }
}

/// A housing cooperative with complete lifecycle (Rule 4).
/// Manages multiple buildings, collects fees from members, maintains
/// reserve funds, and can collapse/liquidate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HousingCooperative {
    /// Unique cooperative ID.
    #[serde(default)]
    pub id: String,

    /// Display name.
    #[serde(default)]
    pub name: String,

    /// Current lifecycle stage.
    #[serde(default)]
    pub lifecycle_stage: CooperativeLifecycleStage,

    /// Managed building IDs.
    #[serde(default)]
    pub managed_buildings: Vec<String>,

    /// Member household IDs with their wealth tier.
    /// Hard limit on membership (Rule 20 — overflow = waitlist).
    #[serde(default)]
    pub members: std::collections::BTreeMap<String, WealthTier>,

    /// Waitlist of prospective members (overflow behavior, Rule 20).
    #[serde(default)]
    pub waitlist: Vec<String>,

    /// Maximum member count (scaled by total building capacity).
    #[serde(default)]
    pub max_members: u32,

    /// Individual ledger tracking assets, debts, and obligations (Rule 7).
    #[serde(default)]
    pub ledger: CooperativeLedger,

    /// Turn when the cooperative was founded.
    #[serde(default)]
    pub founded_turn: u32,

    /// Turn when the cooperative collapsed (if applicable).
    #[serde(default)]
    pub collapsed_turn: Option<u32>,

    /// Turn when liquidation was completed (if applicable).
    #[serde(default)]
    pub liquidated_turn: Option<u32>,

    /// Utility economies of scale discount (0.0–1.0).
    /// Larger cooperatives negotiate better utility rates.
    #[serde(default)]
    pub utility_economies: f64,
}

impl HousingCooperative {
    /// Check if the cooperative can accept new members.
    pub fn has_capacity(&self) -> bool {
        (self.members.len() as u32) < self.max_members
    }

    /// Check if the cooperative should collapse based on financial distress.
    pub fn should_collapse(&self, avg_wage: f64) -> bool {
        match self.lifecycle_stage {
            CooperativeLifecycleStage::Operational => self.ledger.is_distressed(avg_wage),
            CooperativeLifecycleStage::Distressed => self.ledger.is_insolvent(),
            _ => false,
        }
    }

    /// Transition the cooperative to a new lifecycle stage.
    pub fn transition_to(&mut self, stage: CooperativeLifecycleStage, current_turn: u32) {
        match stage {
            CooperativeLifecycleStage::Distressed => {
                self.lifecycle_stage = CooperativeLifecycleStage::Distressed;
            }
            CooperativeLifecycleStage::Insolvent => {
                self.lifecycle_stage = CooperativeLifecycleStage::Insolvent;
                self.collapsed_turn = Some(current_turn);
            }
            CooperativeLifecycleStage::Liquidated => {
                self.lifecycle_stage = CooperativeLifecycleStage::Liquidated;
                self.liquidated_turn = Some(current_turn);
            }
            _ => {}
        }
    }

    /// Get the list of displaced member IDs when the cooperative is liquidated.
    /// These members need to be assigned a HomelessState.
    pub fn get_displaced_members(&self) -> Vec<(String, WealthTier)> {
        self.members
            .iter()
            .map(|(id, tier)| (id.clone(), *tier))
            .collect()
    }

    /// Compute the utility discount for this cooperative based on size
    /// (economies of scale). Larger cooperatives get bigger discounts.
    pub fn compute_utility_discount(&self) -> f64 {
        let member_count = self.members.len() as f64;
        // Discount scales logarithmically: 10 members = ~7%, 100 = ~14%
        (member_count.ln() * 0.03).min(0.20)
    }
}

/// Homeless state for a displaced cooperative member.
/// Tracks the member's transition from housed → homeless → emigrated or rehoused.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HomelessState {
    /// Member ID (matches the cooperative member ID).
    #[serde(default)]
    pub member_id: String,

    /// Former cooperative ID.
    #[serde(default)]
    pub former_cooperative_id: String,

    /// Turn when displacement occurred.
    #[serde(default)]
    pub displacement_turn: u32,

    /// Wealth tier at time of displacement (determines emigration probability).
    #[serde(default)]
    pub wealth_tier: WealthTier,

    /// Liquid capital balance at time of displacement (for capital flight).
    #[serde(default)]
    pub liquid_capital: f64,

    /// Emigration probability per turn (0.0–1.0, scales with wealth tier
    /// and turns homeless).
    #[serde(default)]
    pub emigration_probability: f64,

    /// Turns spent homeless (escalating mortality risk).
    #[serde(default)]
    pub turns_homeless: u32,

    /// Whether this member has been rehoused.
    #[serde(default)]
    pub rehoused: bool,

    /// Whether this member has emigrated (capital flight triggered).
    #[serde(default)]
    pub emigrated: bool,

    /// Blueprint 007-FIX: Remaining unconverted domestic capital from a
    /// partial forex fill. If forex reserves were insufficient, the
    /// emigration was partially filled and this amount stays with the
    /// member for retry next turn (persistent queue — Rule 20).
    #[serde(default)]
    pub remaining_unconverted_capital: f64,

    /// Blueprint 007-FIX: Whether this member is a welfare recipient
    /// (rehoused via poor_laws / state welfare). Used for UI tracking.
    #[serde(default)]
    pub welfare_recipient: bool,

    /// Blueprint 007-FIX: Region ID where the member was displaced.
    /// Used for finding rehousing vacancies in the same region.
    #[serde(default)]
    pub region_id: String,
}

impl HomelessState {
    /// Create a new homeless state for a displaced member.
    pub fn new(
        member_id: String,
        former_cooperative_id: String,
        displacement_turn: u32,
        wealth_tier: WealthTier,
        liquid_capital: f64,
    ) -> Self {
        // Base emigration probability scales with wealth tier:
        // Upper class emigrates most readily (they have resources to leave),
        // Destitute members emigrate least (they can't afford it).
        let base_prob = match wealth_tier {
            WealthTier::Upper => 0.15,
            WealthTier::Middle => 0.08,
            WealthTier::Working => 0.03,
            WealthTier::Destitute => 0.01,
        };
        Self {
            member_id,
            former_cooperative_id,
            displacement_turn,
            wealth_tier,
            liquid_capital,
            emigration_probability: base_prob,
            turns_homeless: 0,
            rehoused: false,
            emigrated: false,
            remaining_unconverted_capital: 0.0,
            welfare_recipient: false,
            region_id: String::new(),
        }
    }

    /// Update the homeless state for one turn.
    /// Increases emigration probability and mortality risk over time.
    /// Returns true if the member should emigrate this turn.
    pub fn update_turn(&mut self) -> bool {
        if self.rehoused || self.emigrated {
            return false;
        }
        self.turns_homeless += 1;
        // Emigration probability increases each turn homeless (desperation)
        self.emigration_probability = (self.emigration_probability + 0.02).min(0.50);
        // Roll: if probability exceeds threshold, member emigrates
        // Using deterministic threshold for simulation stability
        self.emigration_probability >= 0.25
    }

    /// Health penalty for being homeless (0.0–1.0, higher = worse).
    /// Scales with turns homeless — escalating mortality risk.
    pub fn health_penalty(&self) -> f64 {
        // 5% health degradation per turn homeless, capped at 80%
        (self.turns_homeless as f64 * 0.05).min(0.80)
    }

    /// Happiness penalty for being homeless (0.0–1.0, higher = worse).
    pub fn happiness_penalty(&self) -> f64 {
        // Immediate 40% happiness drop, +5% per turn
        (0.40 + self.turns_homeless as f64 * 0.05).min(0.95)
    }
}

/// Registry of all housing cooperatives in a country.
/// Keyed by cooperative ID for individual accountability (Rule 7).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CooperativeRegistry {
    /// All active and historical cooperatives.
    #[serde(default)]
    pub cooperatives: std::collections::BTreeMap<String, HousingCooperative>,

    /// Currently homeless members awaiting rehousing or emigration.
    #[serde(default)]
    pub homeless: Vec<HomelessState>,

    /// Total emigration capital outflow this turn (for UI snapshot, Rule 17).
    #[serde(default)]
    pub emigration_capital_outflow_this_turn: f64,

    /// Total forex reserve drain this turn (for UI snapshot, Rule 17).
    #[serde(default)]
    pub forex_reserve_drain_this_turn: f64,

    /// Cumulative emigration capital outflow (all turns).
    #[serde(default)]
    pub total_emigration_capital_outflow: f64,

    /// Cumulative forex reserve drain (all turns).
    #[serde(default)]
    pub total_forex_reserve_drain: f64,
}

impl CooperativeRegistry {
    /// Process cooperative lifecycle for one turn.
    /// Checks for collapse triggers, processes fee collection, and
    /// transitions cooperatives through lifecycle stages.
    pub fn process_lifecycle_turn(
        &mut self,
        avg_wage: f64,
        current_turn: u32,
    ) -> Vec<(String, Vec<(String, WealthTier)>)> {
        let mut displaced_batches = Vec::new();

        // Check each cooperative for collapse
        let coop_ids: Vec<String> = self.cooperatives.keys().cloned().collect();
        for coop_id in coop_ids {
            let coop = match self.cooperatives.get_mut(&coop_id) {
                Some(c) => c,
                None => continue,
            };

            // Skip already-liquidated cooperatives
            if coop.lifecycle_stage == CooperativeLifecycleStage::Liquidated {
                continue;
            }

            // Update utility economies discount
            coop.utility_economies = coop.compute_utility_discount();

            // Check for collapse
            if coop.should_collapse(avg_wage) {
                if coop.lifecycle_stage == CooperativeLifecycleStage::Operational {
                    coop.transition_to(CooperativeLifecycleStage::Distressed, current_turn);
                } else if coop.lifecycle_stage == CooperativeLifecycleStage::Distressed {
                    coop.transition_to(CooperativeLifecycleStage::Insolvent, current_turn);
                    // Collect displaced members
                    let displaced = coop.get_displaced_members();
                    if !displaced.is_empty() {
                        displaced_batches.push((coop_id.clone(), displaced.clone()));
                        // Create homeless states for each displaced member
                        for (member_id, wealth_tier) in &displaced {
                            // Estimate liquid capital from wealth tier
                            let liquid = match wealth_tier {
                                WealthTier::Upper => avg_wage * 100.0,
                                WealthTier::Middle => avg_wage * 20.0,
                                WealthTier::Working => avg_wage * 5.0,
                                WealthTier::Destitute => avg_wage * 0.5,
                            };
                            self.homeless.push(HomelessState::new(
                                member_id.clone(),
                                coop_id.clone(),
                                current_turn,
                                *wealth_tier,
                                liquid,
                            ));
                        }
                    }
                    // Transition to liquidated
                    coop.transition_to(CooperativeLifecycleStage::Liquidated, current_turn);
                }
            }
        }

        displaced_batches
    }

    /// Update all homeless members for one turn.
    /// Returns the list of members who should emigrate this turn
    /// (for capital flight processing). Members with
    /// `remaining_unconverted_capital > 0` from a previous partial fill
    /// are prioritized for retry (persistent queue — Rule 20).
    pub fn update_homeless_turn(&mut self) -> Vec<HomelessState> {
        let mut to_emigrate = Vec::new();
        for homeless in &mut self.homeless {
            if homeless.update_turn() {
                homeless.emigrated = true;
                to_emigrate.push(homeless.clone());
            }
        }
        // Remove emigrated and rehoused from active homeless list
        self.homeless.retain(|h| !h.emigrated && !h.rehoused);
        to_emigrate
    }

    /// Get the total homeless population count.
    pub fn homeless_count(&self) -> usize {
        self.homeless.len()
    }

    // ─────────────────────────────────────────────────────────────────────
    // BLUEPRINT 007-FIX: EVENT-BASED CACHE HOOKS
    // The registry is updated ONLY when a cooperative is explicitly created
    // or liquidated — NO per-turn O(N) scanning of all Company entities.
    // This is O(1) per create/liquidate event and O(K) per turn where K =
    // number of active cooperatives (typically small).
    // ─────────────────────────────────────────────────────────────────────

    /// Event hook: Called when a Company is assigned
    /// `LegalForm::HousingCooperative`. Inserts the cooperative into the
    /// registry. This is the ONLY way a cooperative enters the registry —
    /// the turn loop never scans all companies to rebuild it.
    ///
    /// # Arguments
    /// * `company_id` - The Company ID that has the HousingCooperative legal form.
    /// * `cooperative_data` - The HousingCooperativeData from the legal form.
    /// * `founded_turn` - Current turn number.
    pub fn on_cooperative_created(
        &mut self,
        company_id: String,
        name: String,
        managed_buildings: Vec<String>,
        member_households: u32,
        share_capital: f64,
        founded_turn: u32,
    ) {
        let max_members = member_households.max(1);
        let total_floor_area = managed_buildings.len() as f64 * 100.0; // estimate
        let coop = HousingCooperative {
            id: company_id.clone(),
            name,
            lifecycle_stage: CooperativeLifecycleStage::Operational,
            managed_buildings,
            members: std::collections::BTreeMap::new(),
            waitlist: Vec::new(),
            max_members,
            ledger: CooperativeLedger {
                share_capital,
                total_floor_area_sqm: total_floor_area,
                ..Default::default()
            },
            founded_turn,
            collapsed_turn: None,
            liquidated_turn: None,
            utility_economies: 0.0,
        };
        self.cooperatives.insert(company_id, coop);
    }

    /// Event hook: Called when a Company with `LegalForm::HousingCooperative`
    /// is liquidated (from the bankruptcy/liquidation code path). Transitions
    /// the cooperative to `Liquidated` stage and collects displaced members
    /// for homeless state assignment.
    ///
    /// Returns the list of displaced member IDs with their wealth tiers
    /// for the caller to create `HomelessState` entries.
    ///
    /// # Arguments
    /// * `company_id` - The Company ID being liquidated.
    /// * `current_turn` - Current turn number.
    pub fn on_cooperative_liquidated(
        &mut self,
        company_id: &str,
        current_turn: u32,
    ) -> Vec<(String, WealthTier)> {
        let coop = match self.cooperatives.get_mut(company_id) {
            Some(c) => c,
            None => return Vec::new(),
        };

        // Transition to Insolvent then Liquidated
        coop.transition_to(CooperativeLifecycleStage::Insolvent, current_turn);
        coop.transition_to(CooperativeLifecycleStage::Liquidated, current_turn);

        // Collect displaced members
        coop.get_displaced_members()
    }

    // ─────────────────────────────────────────────────────────────────────
    // BLUEPRINT 007-FIX: REHOUSING — 3-TIER AFFORDABILITY CASCADE
    // Market rent → Welfare/poor-laws → Homeless shelter/mortality
    // No citizen permanently trapped in homelessness (Rule 4, Rule 8).
    // ─────────────────────────────────────────────────────────────────────

    /// Attempt to rehouse a homeless member using the 3-tier cascade.
    ///
    /// **TIER 1 (Market Rent):** If the member's remaining liquid capital
    /// (after any capital-controls seizure) >= market rent for a vacancy,
    /// debit the member and credit the property owner. Set `rehoused = true`.
    ///
    /// **TIER 2 (Welfare / Poor Laws):** If the member has zero or
    /// insufficient savings AND the country has an active welfare program
    /// (treasury solvent), the state pays the rent: DEBIT
    /// `country.budget.liquid_reserves` → CREDIT property owner. Set
    /// `rehoused = true` and `welfare_recipient = true`.
    ///
    /// **TIER 3 (Homeless Shelter / Escalating Mortality):** If no vacancy
    /// exists OR no welfare program is active OR the treasury is insolvent,
    /// the member remains homeless with escalating mortality risk.
    ///
    /// # Arguments
    /// * `homeless` - The homeless member to rehouse.
    /// * `vacancies` - List of (building_id, owner_id, rent_per_slot) for
    ///   available housing units in the member's region.
    /// * `treasury` - Mutable treasury (for welfare payment in Tier 2).
    /// * `welfare_enabled` - Whether the country has an active poor_laws /
    ///   welfare program.
    ///
    /// # Returns
    /// `RehousingOutcome` indicating which tier was used or if the member
    /// remains homeless.
    pub fn try_rehouse(
        homeless: &mut HomelessState,
        vacancies: &[(String, String, f64)],
        treasury: &mut crate::state::Treasury,
        welfare_enabled: bool,
    ) -> RehousingOutcome {
        if vacancies.is_empty() {
            return RehousingOutcome::RemainsHomeless;
        }

        // Find the cheapest vacancy
        let (building_id, owner_id, rent) = vacancies
            .iter()
            .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap();

        // TIER 1: Market Rent — member pays from remaining liquid capital
        if homeless.liquid_capital >= *rent {
            homeless.liquid_capital -= *rent;
            homeless.rehoused = true;
            // The property owner is credited by the caller (who has access
            // to the company/building ledger). Here we just mark rehoused.
            return RehousingOutcome::MarketRent {
                building_id: building_id.clone(),
                owner_id: owner_id.clone(),
                rent_paid: *rent,
            };
        }

        // TIER 2: Welfare / Poor Laws — state pays rent for zero-savings citizens
        if welfare_enabled && treasury.liquid_reserves >= *rent {
            treasury.liquid_reserves -= *rent;
            homeless.rehoused = true;
            homeless.welfare_recipient = true;
            return RehousingOutcome::Welfare {
                building_id: building_id.clone(),
                owner_id: owner_id.clone(),
                rent_paid: *rent,
            };
        }

        // TIER 3: No rehousing possible — remains homeless with escalating mortality
        RehousingOutcome::RemainsHomeless
    }
}

/// Blueprint 007-FIX: Outcome of a rehousing attempt.
#[derive(Debug, Clone, PartialEq)]
pub enum RehousingOutcome {
    /// Tier 1: Member paid market rent from their own savings.
    MarketRent {
        building_id: String,
        owner_id: String,
        rent_paid: f64,
    },
    /// Tier 2: State welfare paid the rent (poor_laws fallback).
    Welfare {
        building_id: String,
        owner_id: String,
        rent_paid: f64,
    },
    /// Tier 3: No rehousing possible — member remains homeless.
    RemainsHomeless,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_well_yields_zero_water() {
        // Blueprint 006 invariant: Off-grid buildings with no constructed
        // well yield 0.0 water.
        let hb = HousingBuilding {
            active_water_supply: "Local Well".to_string(),
            water_well: None,
            ..Default::default()
        };
        assert_eq!(hb.standalone_water_yield(), 0.0);
        assert!(!hb.can_draw_standalone_water());
    }

    #[test]
    fn test_unconstructed_well_yields_zero_water() {
        // Blueprint 006 invariant: A well that exists but is not yet
        // constructed yields 0.0 water.
        let hb = HousingBuilding {
            active_water_supply: "Local Well".to_string(),
            water_well: Some(WaterWell {
                constructed: false,
                construction_progress: 0.5,
                depth_m: 20.0,
                max_yield_liters: 100.0,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(hb.standalone_water_yield(), 0.0);
        assert!(!hb.can_draw_standalone_water());
    }

    #[test]
    fn test_abandoned_well_yields_zero_water() {
        // Blueprint 006 invariant: An abandoned well yields 0.0 water.
        let hb = HousingBuilding {
            active_water_supply: "Local Well".to_string(),
            water_well: Some(WaterWell {
                constructed: true,
                abandoned: true,
                max_yield_liters: 100.0,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(hb.standalone_water_yield(), 0.0);
        assert!(!hb.can_draw_standalone_water());
    }

    #[test]
    fn test_constructed_well_yields_water() {
        // Blueprint 006 invariant: A constructed, non-abandoned well
        // yields its max_yield_liters.
        let hb = HousingBuilding {
            active_water_supply: "Local Well".to_string(),
            water_well: Some(WaterWell {
                constructed: true,
                abandoned: false,
                max_yield_liters: 150.0,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(hb.standalone_water_yield(), 150.0);
        assert!(hb.can_draw_standalone_water());
    }

    #[test]
    fn test_capex_bom_uses_steel_cement_machinery() {
        // Blueprint 006 invariant: CAPEX BOM uses Steel, Cement, and
        // ConstructionMachinery — NOT Stone/Timber/Bricks.
        let bom = WaterWell::compute_capex_bom(100, 30.0);
        let commodities: Vec<_> = bom.iter().map(|(c, _)| *c).collect();
        assert!(commodities.contains(&Commodity::Steel), "BOM must contain Steel");
        assert!(commodities.contains(&Commodity::Cement), "BOM must contain Cement");
        assert!(commodities.contains(&Commodity::ConstructionMachinery), "BOM must contain ConstructionMachinery");
        // Must NOT contain pre-industrial materials
        assert!(!commodities.contains(&Commodity::Stone), "BOM must NOT contain Stone");
        assert!(!commodities.contains(&Commodity::Timber), "BOM must NOT contain Timber");
        assert!(!commodities.contains(&Commodity::Bricks), "BOM must NOT contain Bricks");
    }

    #[test]
    fn test_world_gen_well_is_constructed() {
        // Blueprint 006 invariant: World-generation wells are fully
        // constructed (no Day-1 demand shock).
        let well = WaterWell::new_constructed_at_world_gen(20.0, 0.8);
        assert!(well.constructed);
        assert!(!well.abandoned);
        assert_eq!(well.construction_progress, 1.0);
        assert!(well.max_yield_liters > 0.0);
        assert_eq!(well.total_capex, 0.0); // Sunk cost, not tracked
        assert!(well.remaining_yield_lifetime > 0.0);
    }

    #[test]
    fn test_well_record_extraction_updates_lifecycle() {
        // Blueprint 006 invariant: Recording extraction updates maintenance
        // cost and decrements yield lifetime.
        let mut well = WaterWell {
            constructed: true,
            max_yield_liters: 100.0,
            remaining_yield_lifetime: 100.0,
            ..Default::default()
        };
        well.record_extraction(50.0);
        assert_eq!(well.last_turn_extracted_liters, 50.0);
        assert!(well.maintenance_cost_per_turn > 0.0);
        assert!(well.remaining_yield_lifetime < 100.0, "Lifetime must decrement");
    }

    #[test]
    fn test_well_abandonment_when_lifetime_reaches_zero() {
        // Blueprint 006 invariant: Wells deplete and can be abandoned.
        // With max_yield=100L and 10000-turn lifespan, total lifetime capacity
        // = 100 * 10000 = 1,000,000 L. Decrement per turn = extraction / 1M.
        // To abandon in one turn, extract more than remaining_lifetime * 1M.
        let mut well = WaterWell {
            constructed: true,
            max_yield_liters: 100.0,
            remaining_yield_lifetime: 0.0001, // nearly depleted
            ..Default::default()
        };
        // Extract 100L: decrement = 100 / (100 * 10000) = 0.0001
        // remaining = 0.0001 - 0.0001 = 0.0 → abandoned
        well.record_extraction(100.0);
        assert!(well.abandoned, "Well must be abandoned when lifetime reaches zero");
    }
}

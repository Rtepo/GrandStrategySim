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
    /// Familok (workers' housing, industrial era)
    Familok,
    /// Beamciok (higher standard Familok for specialists/skilled workers)
    Beamciok,
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
    /// FolwarkHousing (Czworaki - Latifundium housing for serfs/landless laborers)
    FolwarkHousing,
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
        let sublet = self.sublet_slots.as_ref().map(|s| s.total_capacity).unwrap_or(0);
        primary + sublet
    }
    
    /// Calculate total occupied slots
    pub fn total_occupied(&self) -> u32 {
        let primary = self.primary_slots.occupied_slots;
        let sublet = self.sublet_slots.as_ref().map(|s| s.occupied_slots).unwrap_or(0);
        primary + sublet
    }
    
    /// Check if building is overcrowded
    pub fn is_overcrowded(&self) -> bool {
        self.total_occupied() > self.total_capacity()
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

        let total_stored: f64 = self.current_inventory
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
        let current_stored: f64 = self.current_inventory
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
                        batch.quantity -= decayed;
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
        self.current_inventory.retain(|_, batches| !batches.is_empty());

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
        let profile = crate::data::perishability_registry::perishability_registry()
            .get(&commodity_enum)?;
        
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
            let min_price = production_cost * 0.9;  // 10% loss tolerance
            
            if market_price > min_price {
                // Calculate discount to ensure quick sale
                let discount = 0.3;  // 30% discount for fire-sale
                Some(discount)
            } else {
                None  // Market already at floor - no fire-sale
            }
        } else {
            None  // Not urgent enough
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
        // Store pollution in the region's zasoby (resources) field as a fallback
        if let Some(region) = region {
            // This is a placeholder - actual pollution tracking would be in a dedicated field
            // For now, we store it in the zasoby map under "zanieczyszczenie" (pollution)
            let pollution_key = "zanieczyszczenie".to_string();
            let current_pollution = region.resources.get(&pollution_key)
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let new_pollution = (current_pollution + decayed_amount * 0.1).min(100.0);
            region.resources.insert(pollution_key, serde_json::Value::Number(
                serde_json::Number::from_f64(new_pollution).unwrap_or(serde_json::Number::from(0))
            ));
        }
    }
}

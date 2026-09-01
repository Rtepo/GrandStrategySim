//! Phase 81: Core energy types — power plant types, grid tiers, grid state,
//! load shedding tiers, government priorities, cooling types, and overproduction tiers.
//!
//! These types model the physical electricity grid as a three-tier (LV/MV/HV)
//! region-level network with specialized generation, weather coupling, and
//! tiered load shedding.

#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Type of power plant, determining fuel inputs, weather coupling, and
/// geographic constraints. Includes primitive/rural plants for early-game
/// and remote electrification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PowerPlantType {
    /// Wood/Timber/Planks/Peat — early or remote regions. Available from 1880.
    BiomassFired,
    /// Agricultural waste/livestock byproducts — late-game rural. Available from 1930.
    BiogasPlant,
    /// HardCoal — high CAPEX, moderate OPEX, near deposits.
    #[default]
    CoalFired,
    /// BrownCoal — low CAPEX, high OPEX, near deposits.
    LigniteFired,
    /// Oil/NaturalGas — moderate CAPEX, fuel-price-dependent OPEX.
    OilGas,
    /// Uranium — very high CAPEX, low OPEX, needs water for cooling.
    Nuclear,
    /// No fuel — low CAPEX, weather-dependent (solar_multiplier).
    Solar,
    /// No fuel — moderate CAPEX, weather-dependent (wind_multiplier).
    Wind,
    /// Water — high CAPEX, needs river/coastline.
    Hydro,
    /// Energy arbitrage — needs elevation difference. Absorbs surplus, releases on demand.
    PumpedStorage,
    /// Battery storage — absorbs surplus, releases on demand. No geographic constraint.
    BatteryStorage,
    /// Geological — needs volcanic/geothermal zone.
    Geothermal,
}

impl PowerPlantType {
    /// Returns the string key used in production method registries.
    pub fn registry_key(&self) -> &'static str {
        match self {
            PowerPlantType::BiomassFired => "biomass_plant",
            PowerPlantType::BiogasPlant => "biogas_plant",
            PowerPlantType::CoalFired => "coal_fired_plant",
            PowerPlantType::LigniteFired => "lignite_fired_plant",
            PowerPlantType::OilGas => "oil_gas_plant",
            PowerPlantType::Nuclear => "nuclear_plant",
            PowerPlantType::Solar => "solar_plant",
            PowerPlantType::Wind => "wind_farm",
            PowerPlantType::Hydro => "hydro_plant",
            PowerPlantType::PumpedStorage => "pumped_storage",
            PowerPlantType::BatteryStorage => "battery_storage",
            PowerPlantType::Geothermal => "geothermal_plant",
        }
    }

    /// Returns true if this plant type is weather-coupled for generation.
    pub fn is_weather_coupled(&self) -> bool {
        matches!(
            self,
            PowerPlantType::Solar
                | PowerPlantType::Wind
                | PowerPlantType::Hydro
                | PowerPlantType::CoalFired
                | PowerPlantType::LigniteFired
                | PowerPlantType::OilGas
                | PowerPlantType::Nuclear
                | PowerPlantType::Geothermal
        )
    }

    /// Returns true if this plant type is a renewable (no fuel consumed).
    pub fn is_renewable(&self) -> bool {
        matches!(
            self,
            PowerPlantType::Solar | PowerPlantType::Wind | PowerPlantType::Hydro
        )
    }

    /// Returns true if this plant type is a storage plant (absorbs surplus).
    pub fn is_storage(&self) -> bool {
        matches!(
            self,
            PowerPlantType::PumpedStorage | PowerPlantType::BatteryStorage
        )
    }

    /// Returns true if this plant type uses cooling water (thermal plants).
    pub fn is_thermal(&self) -> bool {
        matches!(
            self,
            PowerPlantType::CoalFired
                | PowerPlantType::LigniteFired
                | PowerPlantType::OilGas
                | PowerPlantType::Nuclear
                | PowerPlantType::Geothermal
                | PowerPlantType::BiomassFired
                | PowerPlantType::BiogasPlant
        )
    }

    /// Returns the display name for UI snapshots.
    pub fn display_name(&self) -> &'static str {
        match self {
            PowerPlantType::BiomassFired => "BiomassFired",
            PowerPlantType::BiogasPlant => "BiogasPlant",
            PowerPlantType::CoalFired => "CoalFired",
            PowerPlantType::LigniteFired => "LigniteFired",
            PowerPlantType::OilGas => "OilGas",
            PowerPlantType::Nuclear => "Nuclear",
            PowerPlantType::Solar => "Solar",
            PowerPlantType::Wind => "Wind",
            PowerPlantType::Hydro => "Hydro",
            PowerPlantType::PumpedStorage => "PumpedStorage",
            PowerPlantType::BatteryStorage => "BatteryStorage",
            PowerPlantType::Geothermal => "Geothermal",
        }
    }
}

/// Grid voltage tier. HV is inter-regional, MV and LV are intra-regional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GridTier {
    /// Low voltage — local distribution to housing and small commercial.
    #[default]
    Lv,
    /// Medium voltage — regional aggregation, feeds LV substations.
    Mv,
    /// High voltage — inter-regional transmission lines.
    Hv,
}

/// A physical grid line connecting two regions (HV only; MV/LV are abstracted
/// as regional capacities in `PowerGridState`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GridLine {
    /// Unique line identifier.
    pub id: String,
    /// Source region ID.
    pub from_region: String,
    /// Destination region ID.
    pub to_region: String,
    /// Voltage tier (always Hv for inter-regional lines).
    pub tier: GridTier,
    /// Maximum transfer capacity in MW.
    pub capacity_mw: f64,
    /// Physical condition (0.0 = destroyed, 1.0 = perfect). Degrades over time.
    pub condition: f64,
    /// Distance in kilometers (from `Edge.distance`).
    pub distance_km: f64,
    /// True if this is a cross-border interconnector between different countries.
    pub is_interconnector: bool,
    /// Owning country ID.
    pub owner_country: String,
    /// Phase 81: Current flow magnitude (MW). Updated by DC flow balancing.
    #[serde(default)]
    pub current_flow_mw: f64,
}

/// Per-country state of the power grid. Stored on `Country`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PowerGridState {
    /// HV inter-regional transmission lines (including interconnectors).
    #[serde(default)]
    pub hv_lines: Vec<GridLine>,
    /// Region ID → LV distribution capacity (MW).
    #[serde(default)]
    pub region_lv_capacity: HashMap<String, f64>,
    /// Region ID → MV distribution capacity (MW).
    #[serde(default)]
    pub region_mv_capacity: HashMap<String, f64>,
    /// Region ID → LV grid condition (0.0-1.0).
    #[serde(default)]
    pub region_lv_condition: HashMap<String, f64>,
    /// Region ID → MV grid condition (0.0-1.0).
    #[serde(default)]
    pub region_mv_condition: HashMap<String, f64>,
    /// Region ID → current electricity spot price.
    #[serde(default)]
    pub spot_prices: HashMap<String, f64>,
    /// Region ID → current load shedding tier.
    #[serde(default)]
    pub load_shed_tiers: HashMap<String, LoadShedTier>,
    /// Region ID → current overproduction tier.
    #[serde(default)]
    pub overproduction_tiers: HashMap<String, OverproductionTier>,
    /// Phase 81: Region ID → current supply (MW) after grid balancing.
    #[serde(default)]
    pub region_supply_mw: HashMap<String, f64>,
    /// Phase 81: Region ID → current demand (MW).
    #[serde(default)]
    pub region_demand_mw: HashMap<String, f64>,
    /// Phase 81: Region ID → maximum production capacity (MW).
    #[serde(default)]
    pub region_max_capacity_mw: HashMap<String, f64>,
    /// Phase 81 Wave 2: Spot market state (merit-order clearing results).
    #[serde(default)]
    pub spot_market: SpotMarketState,
}

/// Phase 81 Wave 2: Spot market state — merit-order clearing results per turn.
///
/// Stores per-plant marginal costs, per-region clearing prices, the dispatch
/// order (merit order stack), and per-plant revenue distribution. All values
/// are recomputed each turn during `distribute_grid_power()`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SpotMarketState {
    /// Plant building ID → marginal cost per MWh for the current turn.
    #[serde(default)]
    pub marginal_costs: std::collections::BTreeMap<String, f64>,
    /// Region ID → clearing price per MWh (the marginal plant's cost).
    #[serde(default)]
    pub clearing_prices: std::collections::BTreeMap<String, f64>,
    /// Plant building IDs in merit order (cheapest first). Deterministic.
    #[serde(default)]
    pub dispatch_order: Vec<String>,
    /// Plant building ID → revenue for the current turn (MW * clearing price).
    #[serde(default)]
    pub revenue_distribution: std::collections::BTreeMap<String, f64>,
    /// Plant building ID → dispatched output (MW) for the current turn.
    #[serde(default)]
    pub dispatched_mw: std::collections::BTreeMap<String, f64>,
}

/// Phase 81 Wave 2: PPA contract status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PpaStatus {
    /// Contract is active and supplying energy.
    #[default]
    Active,
    /// Contract has reached its end turn.
    Expired,
    /// Contract was terminated early by one party (with break fee).
    Terminated,
}

/// Phase 81 Wave 2: Power Purchase Agreement — bilateral long-term contract
/// between a generator (seller) and an industrial consumer (buyer) at a
/// fixed price, hedging against spot market volatility.
///
/// # Price Discovery
/// The fixed price is set at negotiation time using exact formulas (Flaw 3):
/// - `seller_ask = marginal_cost_mwh * 1.15`
/// - `buyer_bid = moving_average_vwap(Commodity::Energy)`
/// - `execution_price = (seller_ask + buyer_bid) / 2.0` when `seller_ask <= buyer_bid`
///
/// # Lifecycle
/// 1. **Birth**: Negotiated during the corporate strategy phase.
/// 2. **Life**: Active for `start_turn..=end_turn`. Either party can terminate
///    early with a 20% break fee on remaining contract value.
/// 3. **Death**: Expires automatically at `end_turn`. No immortal contracts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PowerPurchaseAgreement {
    /// Unique PPA ID.
    pub id: String,
    /// Seller company ID (power plant owner).
    pub seller_company_id: String,
    /// Buyer company ID (industrial consumer).
    pub buyer_company_id: String,
    /// Specific plant building supplying energy.
    pub plant_building_id: String,
    /// Negotiated fixed price per MWh (see Flaw 3 formulas).
    pub fixed_price_per_mwh: f64,
    /// Allocated capacity in MW (pro-rata by bid quantity).
    pub contracted_mw: f64,
    /// Turn the PPA starts.
    pub start_turn: u32,
    /// Turn the PPA ends (inclusive). Fixed-term: 20-120 turns.
    pub end_turn: u32,
    /// Current contract status.
    pub status: PpaStatus,
}

/// Phase 81 Wave 2: PPA registry — all active and expired PPAs for a country.
/// Stored on `Country`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PpaRegistry {
    /// Active PPAs currently supplying energy.
    #[serde(default)]
    pub active_ppas: Vec<PowerPurchaseAgreement>,
    /// Expired or terminated PPAs (kept for historical record).
    #[serde(default)]
    pub expired_ppas: Vec<PowerPurchaseAgreement>,
}

/// Load shedding tier, escalating from minor cuts to total blackout.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum LoadShedTier {
    /// Full supply — no shedding.
    #[default]
    Normal,
    /// Non-essential industry cut (~5% reduction).
    Tier1,
    /// Heavy industry cut (~15% reduction).
    Tier2,
    /// Residential voltage reduction / brownout (~30% reduction).
    Tier3,
    /// Rolling outages (~50% reduction).
    Tier4,
    /// Total grid failure (100% reduction).
    Blackout,
}

impl LoadShedTier {
    /// Returns the display name for UI snapshots.
    pub fn display_name(&self) -> &'static str {
        match self {
            LoadShedTier::Normal => "Normal",
            LoadShedTier::Tier1 => "Tier1",
            LoadShedTier::Tier2 => "Tier2",
            LoadShedTier::Tier3 => "Tier3",
            LoadShedTier::Tier4 => "Tier4",
            LoadShedTier::Blackout => "Blackout",
        }
    }

    /// Returns the overall supply reduction fraction (0.0 = no reduction, 1.0 = total).
    pub fn reduction_factor(&self) -> f64 {
        match self {
            LoadShedTier::Normal => 0.0,
            LoadShedTier::Tier1 => 0.05,
            LoadShedTier::Tier2 => 0.15,
            LoadShedTier::Tier3 => 0.30,
            LoadShedTier::Tier4 => 0.50,
            LoadShedTier::Blackout => 1.0,
        }
    }
}

/// Government priority policy determining which sectors are shed first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GridPriority {
    /// Residential > Commercial > Heavy Industry. Default peacetime policy.
    #[default]
    Peacetime,
    /// Military > Heavy Industry > Residential. Wartime policy.
    Wartime,
    /// Residential Heating > Hospital > Everything else. Winter crisis policy.
    WinterCrisis,
    /// Heavy Industry > Commercial > Residential. Industrial priority policy.
    Industrial,
}

/// Cooling method for thermal power plants. Affects drought vulnerability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CoolingType {
    /// Once-through cooling — needs river/lake, vulnerable to drought.
    #[default]
    OnceThrough,
    /// Closed-loop cooling tower — drought-resistant (min 0.7 cooling water).
    ClosedLoop,
    /// Air-cooled condenser — no water needed, 5% efficiency penalty.
    AirCooled,
}

/// Overproduction tier for handling energy surplus (overfrequency).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum OverproductionTier {
    /// No remaining surplus after exports + storage.
    #[default]
    Normal,
    /// Small remaining surplus: local industry gets cheap energy buff.
    IndustrialBuff,
    /// Moderate remaining surplus: renewables curtailed, thermal throttled.
    Curtailment,
    /// Large remaining surplus: forced curtailment, grid condition degrades.
    GridDamage,
}

impl OverproductionTier {
    /// Returns the display name for UI snapshots.
    pub fn display_name(&self) -> &'static str {
        match self {
            OverproductionTier::Normal => "Normal",
            OverproductionTier::IndustrialBuff => "IndustrialBuff",
            OverproductionTier::Curtailment => "Curtailment",
            OverproductionTier::GridDamage => "GridDamage",
        }
    }
}

/// Metadata for a power plant building, stored in `Building.extra` as serialized JSON.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PowerPlantMetadata {
    /// Type of power plant (determines fuel, weather coupling, constraints).
    #[serde(default)]
    pub plant_type: PowerPlantType,
    /// Cooling method (affects drought vulnerability for thermal plants).
    #[serde(default)]
    pub cooling_type: CoolingType,
    /// Whether the plant has been upgraded with a cooling tower.
    #[serde(default)]
    pub has_cooling_upgrade: bool,
    /// Linked geological deposit ID for coal/lignite plants (None for non-fuel or market-sourced).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuel_source_deposit_id: Option<String>,
    /// Region providing cooling water (None for air-cooled or non-thermal plants).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub water_source_region: Option<String>,
    /// Theoretical maximum output in MW (nameplate capacity).
    #[serde(default)]
    pub nameplate_capacity_mw: f64,
    /// Historical average utilization (0.0-1.0). Used for UI display only.
    #[serde(default)]
    pub capacity_factor: f64,
}

impl PowerPlantMetadata {
    /// Serializes metadata to a JSON value for storage in `Building.extra`.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    /// Deserializes metadata from a `Building.extra` JSON value.
    pub fn from_json(value: &serde_json::Value) -> Option<Self> {
        serde_json::from_value(value.clone()).ok()
    }

    /// The `extra` map key under which power plant metadata is stored.
    pub const EXTRA_KEY: &'static str = "power_plant_metadata";
}

/// Result of grid power distribution per turn.
#[derive(Debug, Clone, Default)]
pub struct GridDistributionResult {
    /// Building ID → efficiency penalty (positive = load shedding, negative = industrial buff).
    pub building_efficiency_penalties: HashMap<String, f64>,
    /// Region ID → actual supply after HV balancing and curtailment (MW).
    pub region_supply_mw: HashMap<String, f64>,
    /// Region ID → actual demand (MW).
    pub region_demand_mw: HashMap<String, f64>,
    /// Region ID → max theoretical production capacity (MW).
    pub region_max_capacity_mw: HashMap<String, f64>,
    /// Region ID → spot price.
    pub region_spot_prices: HashMap<String, f64>,
    /// Region ID → storage absorbed energy (MW).
    pub region_storage_absorbed_mw: HashMap<String, f64>,
    /// Region ID → curtailed energy (MW).
    pub region_curtailed_mw: HashMap<String, f64>,
    /// Region ID → load shed tier.
    pub region_load_shed_tiers: HashMap<String, LoadShedTier>,
    /// Region ID → overproduction tier.
    pub region_overproduction_tiers: HashMap<String, OverproductionTier>,
    /// Interconnector flows: (from_region, to_region) → flow in MW.
    pub interconnector_flows: HashMap<(String, String), f64>,
}

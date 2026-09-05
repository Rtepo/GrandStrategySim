//! Phase 83: Hydrological grid infrastructure — the Water Quality Spectrum.
//!
//! PARADIGM SHIFT (Water Quality Spectrum): Water is a single continuous
//! physical mass that flows through the system, changing its **Quality**
//! (0.0 = Toxic Sludge → 1.0 = Pure/Potable) rather than transforming into
//! unrelated entities. No matter is created or destroyed — only its quality
//! changes.
//!
//! ## Key Physics
//!
//! - **Natural reserves**: Groundwater (quality 0.9, limited regen) and
//!   Surface Water (quality 0.6, high volume, dynamic quality).
//! - **Treatment = upgrading**: Water treatment plants intake environmental
//!   water, expend Chemicals/Energy, upgrade quality to ~1.0, push into grid.
//! - **Consumption = degrading**: Buildings intake water, utilize it, degrade
//!   quality to 0.05, discharge the same mass into the sewer.
//! - **Wastewater = filtering**: Wastewater plants extract pathogens into
//!   Fertilizers, discharge healed water back to surface water.
//! - **GUARDRAIL 1 (Infinite Flood)**: `natural_outflow_rate` prevents
//!   infinite mass accumulation from desalination + wastewater discharge.
//! - **PATCH 1 (Anti-Matter Sewage)**: Leakage uses exponential decay,
//!   never linear — prevents >100% leakage.

use serde::{Deserialize, Serialize};

// ============================================================================
// PHYSICAL CONSTANTS
// ============================================================================

/// Natural outflow/drainage rate for surface water (fraction/turn).
/// GUARDRAIL 1: Without this, desalination (PATCH 8) adds infinite mass
/// from the ocean, wastewater discharge (D.5) accumulates it, and
/// surface_water_volume overflows to infinity.
/// 5% per turn — rivers flow downstream, lakes drain, water evaporates.
pub const NATURAL_OUTFLOW_RATE: f64 = 0.05;

/// Groundwater outflow is slower — aquifers retain water longer than
/// surface rivers. Half the surface outflow rate.
pub const GROUNDWATER_OUTFLOW_RATE: f64 = 0.025;

/// Natural groundwater quality (soil-filtered, high quality).
pub const NATURAL_GROUNDWATER_QUALITY: f64 = 0.9;

/// Natural surface water quality (moderate — natural organic load).
pub const NATURAL_SURFACE_WATER_QUALITY: f64 = 0.6;

/// Quality of water after building consumption (blackwater).
/// Buildings degrade consumed water quality to this level.
pub const BLACKWATER_QUALITY: f64 = 0.05;

/// Quality threshold below which citizens consuming water get sick.
/// PATCH 6 (Universal Water Sickness): biohazard penalty evaluates
/// per-building `water_quality_received`, not grid-level quality.
pub const SAFE_WATER_QUALITY_THRESHOLD: f64 = 0.9;

/// Pathogen severity factor — fraction of quality deficit that manifests
/// as pathogenic load per liter consumed. Physical constant calibrated
/// from WHO cholera incidence data for untreated water consumption.
pub const PATHOGEN_SEVERITY_FACTOR: f64 = 0.5;

/// Dehydration severity — at 100% water deficit, death rate triples.
/// Physical constant: humans die in ~3 days without water; at 4 turns/year,
/// one turn is ~3 months, so total dehydration is catastrophic.
pub const DEHYDRATION_SEVERITY: f64 = 3.0;

/// Biological pollution decay rate per turn (3% — pathogens die off slower
/// than smog disperses, because waterborne pathogens persist in soil/water).
pub const BIO_DECAY_RATE: f64 = 0.03;

/// Industrial pump energy per liter (kWh/L). GUARDRAIL 3: Moving millions
/// of liters from a river into a blast furnace requires energy for intake
/// pumps. ~1 Wh/L at typical industrial pump efficiencies.
pub const PUMP_ENERGY_PER_LITER: f64 = 0.001;

/// Surface water quality threshold for industrial cross-substitution.
/// If surface water quality drops below this, industrial machinery suffers
/// corrosion and industries must buy from the municipal grid.
pub const INDUSTRIAL_WATER_QUALITY_THRESHOLD: f64 = 0.3;

// ============================================================================
// WATER RESERVE STATE
// ============================================================================

/// Natural water reserves for a region with dynamic quality.
///
/// PARADIGM SHIFT: Regions track two natural water reserves. Groundwater
/// has limited volume regeneration but high natural quality (0.9). Surface
/// water has high volume but dynamic quality (0.6, degrades if sewage dumped).
///
/// GUARDRAIL 1: `natural_outflow_rate` ensures surface water seeks an
/// equilibrium volume, preventing infinite accumulation from desalination.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WaterReserveState {
    /// Groundwater volume in liters (limited regeneration).
    #[serde(default)]
    pub groundwater_volume: f64,

    /// Groundwater quality (0.0-1.0). Natural default: 0.9 (high quality,
    /// filtered through soil/aquifer). Degrades if contaminated by sewage leakage.
    #[serde(default = "default_groundwater_quality")]
    pub groundwater_quality: f64,

    /// Surface water volume in liters (rivers/lakes, high volume).
    #[serde(default)]
    pub surface_water_volume: f64,

    /// Surface water quality (0.0-1.0). Natural default: 0.6 (moderate —
    /// natural organic load). Degrades if untreated sewage is dumped.
    /// Healed by wastewater treatment discharge.
    #[serde(default = "default_surface_water_quality")]
    pub surface_water_quality: f64,

    /// Per-turn groundwater regeneration rate (liters/turn, based on rainfall
    /// and aquifer recharge). Scales with climate_profile.
    #[serde(default)]
    pub groundwater_regen_rate: f64,

    /// Per-turn surface water inflow rate (liters/turn, from upstream rivers).
    /// Scales with geographic_traits (riverine).
    #[serde(default)]
    pub surface_water_inflow_rate: f64,

    /// GUARDRAIL 1: Natural outflow/drainage rate as a fraction of total
    /// volume per turn. Rivers flow downstream, lakes drain, water evaporates.
    /// Without this, desalination adds infinite mass, overflowing data types.
    /// Equilibrium: volume stabilizes where inflow = outflow * volume.
    #[serde(default = "default_natural_outflow_rate")]
    pub natural_outflow_rate: f64,
}

impl Default for WaterReserveState {
    fn default() -> Self {
        Self {
            groundwater_volume: 0.0,
            groundwater_quality: default_groundwater_quality(),
            surface_water_volume: 0.0,
            surface_water_quality: default_surface_water_quality(),
            groundwater_regen_rate: 0.0,
            surface_water_inflow_rate: 0.0,
            natural_outflow_rate: default_natural_outflow_rate(),
        }
    }
}

fn default_groundwater_quality() -> f64 {
    NATURAL_GROUNDWATER_QUALITY
}

fn default_surface_water_quality() -> f64 {
    NATURAL_SURFACE_WATER_QUALITY
}

fn default_natural_outflow_rate() -> f64 {
    NATURAL_OUTFLOW_RATE
}

impl WaterReserveState {
    /// Apply natural regeneration and outflow for one turn.
    ///
    /// GUARDRAIL 1: Surface water drains/evaporates proportionally —
    /// equilibrium at `inflow / outflow_rate`. Groundwater outflows at
    /// half the surface rate (aquifers retain water longer).
    /// Quality drifts toward natural defaults (soil filtration, biodegradation).
    ///
    /// Blueprint 006 v2 CORRECTION: Aquifer recharge is FRACTIONAL based on
    /// regional precipitation, NOT a full reset to capacity. Wells CAN deplete
    /// the aquifer if extraction exceeds recharge. The aquifer slowly recovers
    /// over many turns via fractional recharge.
    ///
    /// # Arguments
    /// * `aquifer_capacity` - Hard upper bound on groundwater_volume (Rule 20).
    /// * `precipitation_mm` - Regional precipitation in mm this turn.
    /// * `recharge_coefficient` - Fraction of precipitation that recharges the
    ///   aquifer (0.0-1.0, depends on soil permeability, vegetation, etc.).
    /// * `aquifer_recharge_area` - Surface area in m² that contributes to
    ///   aquifer recharge (region area × infiltration fraction).
    pub fn regenerate_fractional(
        &mut self,
        aquifer_capacity: f64,
        precipitation_mm: f64,
        recharge_coefficient: f64,
        aquifer_recharge_area: f64,
    ) {
        // Blueprint 006: Fractional recharge from precipitation.
        // recharge_volume = precipitation_mm * recharge_coefficient * aquifer_recharge_area
        // precipitation_mm is in mm; 1 mm over 1 m² = 1 liter of water.
        let recharge_volume = precipitation_mm.max(0.0)
            * recharge_coefficient.clamp(0.0, 1.0)
            * aquifer_recharge_area.max(0.0);
        self.groundwater_volume += recharge_volume;
        // Rule 20: Hard clamp to aquifer_capacity (upper bound only, NOT a reset)
        if self.groundwater_volume > aquifer_capacity {
            self.groundwater_volume = aquifer_capacity;
        }
        // Groundwater outflow (slower than surface)
        self.groundwater_volume -= self.groundwater_volume * GROUNDWATER_OUTFLOW_RATE;
        self.groundwater_volume = self.groundwater_volume.max(0.0);

        // Surface water volume: inflow + outflow
        self.surface_water_volume += self.surface_water_inflow_rate;
        self.surface_water_volume -= self.surface_water_volume * self.natural_outflow_rate;
        self.surface_water_volume = self.surface_water_volume.max(0.0);

        // Quality drift toward natural defaults
        let gw_drift = (NATURAL_GROUNDWATER_QUALITY - self.groundwater_quality) * 0.01;
        self.groundwater_quality += gw_drift;
        self.groundwater_quality = self.groundwater_quality.clamp(0.0, 1.0);

        let sw_drift = (NATURAL_SURFACE_WATER_QUALITY - self.surface_water_quality) * 0.01;
        self.surface_water_quality += sw_drift;
        self.surface_water_quality = self.surface_water_quality.clamp(0.0, 1.0);
    }

    /// Legacy regenerate — delegates to regenerate_fractional with zero
    /// precipitation (no recharge, only outflow and quality drift).
    /// Kept for backward compatibility with existing test code.
    pub fn regenerate(&mut self, aquifer_capacity: f64) {
        self.regenerate_fractional(aquifer_capacity, 0.0, 0.0, 0.0);
    }

    /// Draw water from groundwater reserve. Returns (actual_drawn, quality).
    ///
    /// GUARDRAIL 2 (Dry Well): If volume cannot meet demand, intake is
    /// hard-clamped to what is available. The caller computes the deficit
    /// and applies dehydration mortality.
    pub fn draw_groundwater(&mut self, demand: f64) -> (f64, f64) {
        let drawn = demand.min(self.groundwater_volume);
        self.groundwater_volume -= drawn;
        (drawn, self.groundwater_quality)
    }

    /// Draw water from surface water reserve. Returns (actual_drawn, quality).
    pub fn draw_surface_water(&mut self, demand: f64) -> (f64, f64) {
        let drawn = demand.min(self.surface_water_volume);
        self.surface_water_volume -= drawn;
        (drawn, self.surface_water_quality)
    }

    /// Discharge treated water back into surface water at a given quality.
    ///
    /// PARADIGM SHIFT (Pillar 5): Wastewater treatment heals the environment
    /// by adding higher-quality water back to the surface pool. Quality is
    /// updated by volume-weighted blending.
    pub fn discharge_to_surface(&mut self, volume: f64, quality: f64) {
        if self.surface_water_volume + volume > 0.0 {
            self.surface_water_quality = (self.surface_water_volume * self.surface_water_quality
                + volume * quality)
                / (self.surface_water_volume + volume);
        }
        self.surface_water_volume += volume;
        self.surface_water_quality = self.surface_water_quality.clamp(0.0, 1.0);
    }

    /// Contaminate surface water from sewage overflow/leakage.
    ///
    /// PARADIGM SHIFT: Untreated sewage degrades surface water quality,
    /// creating a feedback loop: polluted surface water → industrial
    /// corrosion → economic pressure → investment in sanitation.
    pub fn contaminate_surface(&mut self, contamination_mass: f64) {
        if self.surface_water_volume > 0.0 {
            let degradation = contamination_mass / self.surface_water_volume * 0.0001;
            self.surface_water_quality = (self.surface_water_quality - degradation).max(0.0);
        }
    }

    /// Contaminate groundwater from sewer leakage / septic tank leaks.
    pub fn contaminate_groundwater(&mut self, contamination_mass: f64) {
        if self.groundwater_volume > 0.0 {
            let degradation = contamination_mass / self.groundwater_volume * 0.0001;
            self.groundwater_quality = (self.groundwater_quality - degradation).max(0.0);
        }
    }
}

// ============================================================================
// WATER NETWORK STATE (Quality-Carrier Grid)
// ============================================================================

/// Water distribution network — carries water mass with a quality attribute.
///
/// PARADIGM SHIFT: The grid does NOT carry "PotableWater" as a distinct
/// commodity. It carries water mass at `current_quality` (set by treatment
/// plants). If treatment capacity is insufficient, raw environmental water
/// blends in and quality drops (cascading failure, Pillar 4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WaterNetworkState {
    /// Total pipe network length (km). Constructed by municipal CAPEX.
    #[serde(default)]
    pub pipe_network_km: f64,

    /// Pipe condition (0.0 = collapsed, 1.0 = pristine). Degrades per turn.
    #[serde(default = "default_pipe_condition")]
    pub pipe_condition: f64,

    /// PHYSICAL CONSTANT: 0.01 = 1% water loss per km through leakage
    /// (cast iron joints, aging gaskets, ~1% loss/km at 4 bar pressure).
    #[serde(default = "default_loss_per_km")]
    pub loss_per_km: f64,

    /// Current quality of water in the grid (0.0-1.0).
    /// Set to ~1.0 by treatment plants; degrades if treatment capacity is
    /// insufficient (cascading failure — raw environmental water blends in).
    #[serde(default)]
    pub current_quality: f64,

    /// Total water mass currently in the grid (liters per turn throughput).
    #[serde(default)]
    pub throughput_liters: f64,
}

impl Default for WaterNetworkState {
    fn default() -> Self {
        Self {
            pipe_network_km: 0.0,
            pipe_condition: default_pipe_condition(),
            loss_per_km: default_loss_per_km(),
            current_quality: 0.0,
            throughput_liters: 0.0,
        }
    }
}

fn default_pipe_condition() -> f64 {
    1.0
}

fn default_loss_per_km() -> f64 {
    0.01
}

impl WaterNetworkState {
    /// Compute the average delivery distance for water in this region.
    ///
    /// Water networks are more grid-like than heat (lower branching factor):
    /// `average_delivery_distance_km = (pipe_network_km / active_plants).sqrt() * 1.2`
    pub fn average_delivery_distance_km(&self, active_water_plants: usize) -> f64 {
        let plants = active_water_plants.max(1) as f64;
        (self.pipe_network_km / plants).sqrt() * 1.2
    }

    /// Compute transmission loss fraction (0.0 = no loss, 1.0 = total loss).
    ///
    /// Uses exponential decay (same as heat): never exceeds 1.0.
    pub fn transmission_loss(&self, active_water_plants: usize) -> f64 {
        if self.pipe_network_km <= 0.0 {
            return 1.0; // No pipes = total loss
        }
        let avg_distance = self.average_delivery_distance_km(active_water_plants);
        1.0 - (1.0 - self.loss_per_km).powf(avg_distance)
    }

    /// Compute effective water delivered after transmission losses.
    ///
    /// `effective_water = throughput * (1.0 - transmission_loss) * pipe_condition`
    pub fn effective_water_delivered(&self, active_water_plants: usize) -> f64 {
        let loss = self.transmission_loss(active_water_plants);
        self.throughput_liters * (1.0 - loss) * self.pipe_condition
    }

    /// Compute maximum connectable buildings based on pipe network and density.
    ///
    /// `max_connectable = pipe_network_km * (10.0 + development_level * 20.0)`
    /// Water mains serve more buildings per km than heat (smaller pipes,
    /// denser connections).
    pub fn max_connectable_buildings(&self, development_level: f64) -> usize {
        if self.pipe_network_km <= 0.0 {
            return 0;
        }
        let buildings_per_km = 10.0 + development_level * 20.0;
        (self.pipe_network_km * buildings_per_km) as usize
    }

    /// Degrade pipe condition by one turn.
    ///
    /// `degradation_rate = 0.001 * (1.0 + freeze_thaw_factor)`
    /// Water pipes degrade faster in cold climates (freeze-thaw cycles).
    pub fn degrade(&mut self, freeze_thaw_factor: f64) {
        let degradation = 0.001 * (1.0 + freeze_thaw_factor);
        self.pipe_condition = (self.pipe_condition - degradation).max(0.0);
    }
}

// ============================================================================
// SEWER NETWORK STATE (Quality-Carrier Grid)
// ============================================================================

/// Sewer collection network — carries degraded water (blackwater) from
/// buildings to wastewater treatment plants.
///
/// PARADIGM SHIFT: Buildings degrade consumed water to quality 0.05 and
/// discharge the same mass into this network. No matter is created —
/// only quality changes. Leaked blackwater contaminates groundwater
/// and surface water.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SewerNetworkState {
    /// Total pipe network length (km). Constructed by municipal CAPEX.
    #[serde(default)]
    pub pipe_network_km: f64,

    /// Pipe condition (0.0 = collapsed, 1.0 = pristine). Degrades per turn.
    #[serde(default = "default_sewer_pipe_condition")]
    pub pipe_condition: f64,

    /// PHYSICAL CONSTANT: 0.005 = 0.5% leakage per km through cracked
    /// clay/concrete pipes. Leaked water (quality ~0.05) contaminates
    /// groundwater and surface water.
    #[serde(default = "default_leakage_per_km")]
    pub leakage_per_km: f64,

    /// Current quality of water in the sewer (typically ~0.05, blackwater).
    #[serde(default = "default_sewer_quality")]
    pub current_quality: f64,

    /// Total water mass currently in the sewer (liters per turn throughput).
    #[serde(default)]
    pub throughput_liters: f64,
}

impl Default for SewerNetworkState {
    fn default() -> Self {
        Self {
            pipe_network_km: 0.0,
            pipe_condition: default_sewer_pipe_condition(),
            leakage_per_km: default_leakage_per_km(),
            current_quality: default_sewer_quality(),
            throughput_liters: 0.0,
        }
    }
}

fn default_sewer_pipe_condition() -> f64 {
    1.0
}

fn default_leakage_per_km() -> f64 {
    0.005
}

fn default_sewer_quality() -> f64 {
    BLACKWATER_QUALITY
}

impl SewerNetworkState {
    /// Compute the average delivery distance for sewage in this region.
    pub fn average_delivery_distance_km(&self, active_wastewater_plants: usize) -> f64 {
        let plants = active_wastewater_plants.max(1) as f64;
        (self.pipe_network_km / plants).sqrt() * 1.2
    }

    /// Compute leakage fraction (0.0 = no leakage, 1.0 = total leakage).
    ///
    /// PATCH 1 (Anti-Matter Sewage): Uses exponential decay — never exceeds 1.0.
    /// The previous linear formula `leakage_per_km * distance` could exceed 1.0
    /// (125% leakage at 250 km), creating sewage from nothing.
    pub fn leakage_fraction(&self, active_wastewater_plants: usize) -> f64 {
        if self.pipe_network_km <= 0.0 {
            return 1.0; // No pipes = all sewage leaks into environment
        }
        let avg_distance = self.average_delivery_distance_km(active_wastewater_plants);
        1.0 - (1.0 - self.leakage_per_km).powf(avg_distance)
    }

    /// Compute leaked water mass.
    ///
    /// Leaked blackwater (quality 0.05) contaminates groundwater and surface water.
    pub fn leaked_water_mass(&self, active_wastewater_plants: usize) -> f64 {
        self.throughput_liters * self.leakage_fraction(active_wastewater_plants)
    }

    /// Compute water delivered to treatment plants after leakage and pipe condition.
    pub fn water_delivered_to_treatment(&self, active_wastewater_plants: usize) -> f64 {
        let leakage = self.leakage_fraction(active_wastewater_plants);
        self.throughput_liters * (1.0 - leakage) * self.pipe_condition
    }

    /// Compute maximum connectable buildings.
    ///
    /// `max_connectable = pipe_network_km * (8.0 + development_level * 18.0)`
    /// Sewers are slightly less dense than water mains (larger pipes, fewer
    /// connections per km).
    pub fn max_connectable_buildings(&self, development_level: f64) -> usize {
        if self.pipe_network_km <= 0.0 {
            return 0;
        }
        let buildings_per_km = 8.0 + development_level * 18.0;
        (self.pipe_network_km * buildings_per_km) as usize
    }

    /// Degrade pipe condition by one turn.
    ///
    /// `degradation_rate = 0.0015` — sewers degrade at a constant rate
    /// (chemical corrosion, root intrusion).
    pub fn degrade(&mut self) {
        self.pipe_condition = (self.pipe_condition - 0.0015).max(0.0);
    }
}

// ============================================================================
// REGULATED PRICING (PATCH 5 — Hydro Bankruptcy Trap)
// ============================================================================

/// Compute the regulated cost-plus water price (per liter).
///
/// PATCH 5: Municipal Water utilities consume immense OPEX. Without billing,
/// they bankrupt instantly. Mirrors `compute_regulated_heat_price` from
/// Phase 82 exactly — takes `cost_plus_margin` and `average_wage` as
/// parameters (Rule 2: no hardcoded margins).
///
/// # Formula
/// `water_price = (chemicals_opex + energy_opex + labor_opex + maintenance_opex
///                 + amortized_capex) / smoothed_water_sales * cost_plus_margin`
pub fn compute_regulated_water_price(
    chemicals_opex: f64,
    energy_opex: f64,
    labor_opex: f64,
    maintenance_opex: f64,
    total_asset_value: f64,
    amortization_turns: f64,
    smoothed_water_sales_liters: f64,
    cost_plus_margin: f64,
    average_wage: f64,
) -> f64 {
    let amortized_capex = if amortization_turns > 0.0 {
        total_asset_value / amortization_turns
    } else {
        0.0
    };
    if smoothed_water_sales_liters > 0.0 {
        let total_cost =
            chemicals_opex + energy_opex + labor_opex + maintenance_opex + amortized_capex;
        (total_cost / smoothed_water_sales_liters) * cost_plus_margin
    } else {
        // Fallback: no sales history yet. Use wage-anchored price.
        average_wage * 0.5
    }
}

/// Compute the regulated cost-plus sewage price (per liter).
///
/// Same formula as water price — sewage collection and treatment is also
/// a natural monopoly requiring regulated pricing.
pub fn compute_regulated_sewage_price(
    chemicals_opex: f64,
    energy_opex: f64,
    labor_opex: f64,
    maintenance_opex: f64,
    total_asset_value: f64,
    amortization_turns: f64,
    smoothed_sewage_sales_liters: f64,
    cost_plus_margin: f64,
    average_wage: f64,
) -> f64 {
    compute_regulated_water_price(
        chemicals_opex,
        energy_opex,
        labor_opex,
        maintenance_opex,
        total_asset_value,
        amortization_turns,
        smoothed_sewage_sales_liters,
        cost_plus_margin,
        average_wage,
    )
}

// ============================================================================
// SALES HISTORY (for price smoothing — mirrors HeatSalesHistory)
// ============================================================================

/// Rolling window of water sold per turn (liters). Used for price smoothing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WaterSalesHistory {
    /// Rolling window of water sold per turn (liters).
    #[serde(default)]
    pub sales_history: Vec<f64>,
    /// Maximum window size (24 turns = 6 years).
    #[serde(default = "default_window_size")]
    pub window_size: usize,
}

fn default_window_size() -> usize {
    24
}

impl WaterSalesHistory {
    /// Create a new sales history with the default window size.
    pub fn new() -> Self {
        Self {
            sales_history: Vec::new(),
            window_size: default_window_size(),
        }
    }

    /// Record water sold this turn and maintain the rolling window.
    pub fn record(&mut self, liters: f64) {
        self.sales_history.push(liters);
        if self.sales_history.len() > self.window_size {
            self.sales_history.remove(0);
        }
    }

    /// Compute the smoothed (rolling average) water sales.
    pub fn smoothed_sales(&self) -> f64 {
        if self.sales_history.is_empty() {
            0.0
        } else {
            self.sales_history.iter().sum::<f64>() / self.sales_history.len() as f64
        }
    }
}

/// Rolling window of sewage collected/treated per turn (liters).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SewageSalesHistory {
    /// Rolling window of sewage collected per turn (liters).
    #[serde(default)]
    pub sales_history: Vec<f64>,
    /// Maximum window size (24 turns = 6 years).
    #[serde(default = "default_window_size")]
    pub window_size: usize,
}

impl SewageSalesHistory {
    /// Create a new sales history with the default window size.
    pub fn new() -> Self {
        Self {
            sales_history: Vec::new(),
            window_size: default_window_size(),
        }
    }

    /// Record sewage collected this turn and maintain the rolling window.
    pub fn record(&mut self, liters: f64) {
        self.sales_history.push(liters);
        if self.sales_history.len() > self.window_size {
            self.sales_history.remove(0);
        }
    }

    /// Compute the smoothed (rolling average) sewage collected.
    pub fn smoothed_sales(&self) -> f64 {
        if self.sales_history.is_empty() {
            0.0
        } else {
            self.sales_history.iter().sum::<f64>() / self.sales_history.len() as f64
        }
    }
}

// ============================================================================
// DEHYDRATION MORTALITY (GUARDRAIL 2 — Dry Well Exception)
// ============================================================================

/// Compute dehydration mortality multiplier from water deficit.
///
/// GUARDRAIL 2: When `WaterReserveState` volumes cannot meet local standalone
/// demand, water intake is hard-clamped to what is available. The resulting
/// deficit drastically spikes a dehydration mortality modifier.
///
/// `dehydration_mortality = 1.0 + (water_deficit / demand) * DEHYDRATION_SEVERITY`
///
/// At 100% deficit: death rate triples (DEHYDRATION_SEVERITY = 3.0).
/// At 50% deficit: death rate increases by 50%.
/// At 0% deficit: no effect (multiplier = 1.0).
pub fn compute_dehydration_mortality(water_deficit: f64, water_demand: f64) -> f64 {
    if water_demand <= 0.0 {
        return 1.0;
    }
    let deficit_fraction = (water_deficit / water_demand).clamp(0.0, 1.0);
    1.0 + deficit_fraction * DEHYDRATION_SEVERITY
}

// ============================================================================
// WATER TREATMENT PRODUCTION (PARADIGM SHIFT — quality upgrade, not creation)
// ============================================================================

/// Result of water treatment production for one turn.
#[derive(Debug, Clone, Default)]
pub struct WaterTreatmentResult {
    /// Total water mass intake from reserves (liters).
    pub total_intake: f64,
    /// Total water mass pushed into the grid (liters) — same as intake
    /// (mass conserved, only quality changes).
    pub total_output: f64,
    /// Quality of water pushed into the grid (0.0-1.0).
    pub output_quality: f64,
    /// Energy consumed by treatment (kWh).
    pub energy_consumed: f64,
    /// Chemicals consumed by treatment.
    pub chemicals_consumed: f64,
    /// Groundwater drawn (liters).
    pub groundwater_drawn: f64,
    /// Surface water drawn (liters).
    pub surface_water_drawn: f64,
    /// Desalination output (liters) — from infinite Ocean, not from reserves.
    pub desalination_output: f64,
}

/// Run water treatment production for a region.
///
/// PARADIGM SHIFT: Treatment plants draw water from `WaterReserveState`
/// (groundwater/surface water), upgrade its quality, and push it into
/// `WaterNetworkState`. They do NOT create water — only upgrade quality.
///
/// Desalination plants (PATCH 8) draw from the infinite Ocean, adding new
/// freshwater mass to the terrestrial system without depleting reserves.
///
/// # Arguments
/// * `reserves` - Mutable water reserves (groundwater/surface water drawn from)
/// * `network` - Mutable water network (quality and throughput updated)
/// * `plant_throughputs` - List of (throughput_liters, output_quality, is_desalination)
///   for each active water treatment plant in the region
/// * `energy_available` - Energy available for treatment (kWh). If insufficient,
///   throughput is reduced proportionally.
/// * `chemicals_available` - Chemicals available for treatment.
///
/// # Returns
/// `WaterTreatmentResult` with totals for accounting and diagnostics.
pub fn process_water_treatment(
    reserves: &mut WaterReserveState,
    network: &mut WaterNetworkState,
    plant_throughputs: &[(f64, f64, bool)],
    energy_available: f64,
    chemicals_available: f64,
) -> WaterTreatmentResult {
    let mut result = WaterTreatmentResult::default();

    if plant_throughputs.is_empty() {
        network.throughput_liters = 0.0;
        network.current_quality = 0.0;
        return result;
    }

    // Calculate total demand from all plants
    let total_demand: f64 = plant_throughputs.iter().map(|(t, _, _)| *t).sum();
    if total_demand <= 0.0 {
        network.throughput_liters = 0.0;
        network.current_quality = 0.0;
        return result;
    }

    // Energy and chemicals constraint: reduce throughput proportionally if insufficient
    // Each plant needs Energy and Chemicals proportional to its throughput share.
    // For simplicity, we check aggregate availability vs aggregate demand.
    // (In the full integration, each plant's BOM is resolved individually.)
    let energy_constraint = if energy_available > 0.0 {
        1.0_f64.min(energy_available / (total_demand * 0.001))
    } else {
        1.0 // No energy constraint if not tracked at this level
    };
    let chemicals_constraint = if chemicals_available > 0.0 {
        1.0_f64.min(chemicals_available / (total_demand * 0.001))
    } else {
        1.0
    };
    let supply_constraint = energy_constraint.min(chemicals_constraint);

    // Process each plant
    let mut total_quality_weighted: f64 = 0.0;
    let mut total_actual_output: f64 = 0.0;

    for &(throughput, quality, is_desalination) in plant_throughputs {
        let actual_throughput = throughput * supply_constraint;

        if is_desalination {
            // PATCH 8: Desalination draws from infinite Ocean — does NOT
            // deplete surface_water_volume or groundwater_volume.
            // Adds new freshwater mass to the terrestrial system.
            result.desalination_output += actual_throughput;
            result.total_output += actual_throughput;
            total_quality_weighted += actual_throughput * quality;
            total_actual_output += actual_throughput;
        } else {
            // Non-desalination: draw from reserves (groundwater first, then surface)
            let mut remaining = actual_throughput;

            // Draw from groundwater first (higher quality, preferred source)
            if remaining > 0.0 && reserves.groundwater_volume > 0.0 {
                let (drawn, _gw_quality) = reserves.draw_groundwater(remaining);
                result.groundwater_drawn += drawn;
                result.total_intake += drawn;
                result.total_output += drawn;
                total_quality_weighted += drawn * quality;
                total_actual_output += drawn;
                remaining -= drawn;
            }

            // Draw from surface water for remaining demand
            if remaining > 0.0 && reserves.surface_water_volume > 0.0 {
                let (drawn, _sw_quality) = reserves.draw_surface_water(remaining);
                result.surface_water_drawn += drawn;
                result.total_intake += drawn;
                result.total_output += drawn;
                total_quality_weighted += drawn * quality;
                total_actual_output += drawn;
            }
        }
    }

    // Update network state
    if total_actual_output > 0.0 {
        network.throughput_liters = total_actual_output;
        network.current_quality = (total_quality_weighted / total_actual_output).clamp(0.0, 1.0);
    } else {
        network.throughput_liters = 0.0;
        network.current_quality = 0.0;
    }

    result.output_quality = network.current_quality;
    result.energy_consumed = total_actual_output * 0.001; // Approximate
    result.chemicals_consumed = total_actual_output * 0.001; // Approximate

    result
}

// ============================================================================
// WATER DISTRIBUTION (Pro-rata, quality-aware — Rule 5)
// ============================================================================

/// Result of water distribution to buildings for one turn.
#[derive(Debug, Clone, Default)]
pub struct WaterDistributionResult {
    /// Total water delivered to all buildings (liters, after transmission loss).
    pub total_delivered: f64,
    /// Total water demand from all buildings (liters).
    pub total_demand: f64,
    /// Transmission loss (liters lost in pipes).
    pub transmission_loss: f64,
    /// Quality of water delivered (same as network quality, unless treatment
    /// failure causes raw water to blend in).
    pub delivered_quality: f64,
    /// Per-building (building_id, liters_received, quality_received).
    pub building_receipts: Vec<(String, f64, f64)>,
}

/// Distribute water from the `WaterNetworkState` to buildings pro-rata
/// based on each building's demand (Rule 5: no hardcoded splits).
///
/// PARADIGM SHIFT: If total demand exceeds grid throughput, each building
/// receives `demand_share = throughput * (building_demand / total_demand)`
/// — proportional rationing, not arbitrary caps.
///
/// Each building's `water_quality_received` is set to the network's
/// `current_quality` (or lower if treatment capacity is insufficient and
/// raw environmental water blends in — cascading failure, Pillar 4).
///
/// # Arguments
/// * `network` - Water network state (read-only — throughput/quality already set)
/// * `active_water_plants` - Number of active treatment plants (for transmission loss)
/// * `building_demands` - List of (building_id, water_demand_liters) for all
///   buildings connected to the water main in this region
pub fn distribute_water(
    network: &WaterNetworkState,
    active_water_plants: usize,
    building_demands: &[(String, f64)],
) -> WaterDistributionResult {
    let mut result = WaterDistributionResult::default();

    if building_demands.is_empty() || network.pipe_network_km <= 0.0 {
        // No pipes = no centralized water delivery
        result.delivered_quality = 0.0;
        return result;
    }

    let total_demand: f64 = building_demands.iter().map(|(_, d)| *d).sum();
    result.total_demand = total_demand;

    if total_demand <= 0.0 {
        return result;
    }

    // Compute effective water after transmission losses
    let loss_fraction = network.transmission_loss(active_water_plants);
    let effective_water =
        network.throughput_liters * (1.0 - loss_fraction) * network.pipe_condition;
    let transmission_loss = network.throughput_liters - effective_water;
    result.transmission_loss = transmission_loss;

    // Quality delivered = network quality (treatment sets this).
    // If treatment capacity is insufficient (throughput < demand),
    // the shortfall means some buildings get no water — but quality
    // of what IS delivered remains at treatment output quality.
    // (Cascading failure is handled by the turn loop: if throughput = 0,
    // buildings get 0 water and must rely on standalone sources.)
    result.delivered_quality = network.current_quality;
    result.total_delivered = effective_water.min(total_demand);

    // Pro-rata distribution: each building gets its demand share of available water
    let available = effective_water.min(total_demand);
    for (building_id, demand) in building_demands {
        let share = if total_demand > 0.0 {
            demand / total_demand
        } else {
            0.0
        };
        let received = available * share;
        result
            .building_receipts
            .push((building_id.clone(), received, result.delivered_quality));
    }

    result
}

// ============================================================================
// SEWAGE COLLECTION (Buildings discharge degraded water to sewer network)
// ============================================================================

/// Result of sewage collection for one turn.
#[derive(Debug, Clone, Default)]
pub struct SewageCollectionResult {
    /// Total sewage collected into the sewer network (liters).
    pub total_collected: f64,
    /// Sewage leaked into environment (liters) — contaminates groundwater/surface.
    pub leaked: f64,
    /// Sewage delivered to wastewater treatment plants (liters).
    pub delivered_to_treatment: f64,
    /// Quality of sewage in the network (typically 0.05 = blackwater).
    pub sewage_quality: f64,
}

/// Collect sewage from buildings into the `SewerNetworkState`.
///
/// PARADIGM SHIFT: Buildings degrade consumed water to quality 0.05
/// (BLACKWATER_QUALITY) and discharge the same mass into the sewer.
/// No matter is created or destroyed — only quality changes.
///
/// PATCH 1 (Anti-Matter Sewage): Leakage uses exponential decay,
/// never exceeding 1.0. Leaked blackwater contaminates groundwater
/// and surface water.
///
/// # Arguments
/// * `sewer` - Mutable sewer network state (throughput and quality updated)
/// * `active_wastewater_plants` - Number of active wastewater plants
///   (for leakage/delivery distance calculation)
/// * `building_discharges` - List of (building_id, discharge_liters) for all
///   buildings connected to the sewer in this region
pub fn collect_sewage(
    sewer: &mut SewerNetworkState,
    active_wastewater_plants: usize,
    building_discharges: &[(String, f64)],
) -> SewageCollectionResult {
    let mut result = SewageCollectionResult::default();

    if building_discharges.is_empty() || sewer.pipe_network_km <= 0.0 {
        // No pipes = all sewage goes to environment (standalone)
        sewer.throughput_liters = 0.0;
        return result;
    }

    let total_collected: f64 = building_discharges.iter().map(|(_, d)| *d).sum();
    result.total_collected = total_collected;

    if total_collected <= 0.0 {
        sewer.throughput_liters = 0.0;
        return result;
    }

    // Update sewer state
    sewer.throughput_liters = total_collected;
    sewer.current_quality = BLACKWATER_QUALITY;
    result.sewage_quality = BLACKWATER_QUALITY;

    // Compute leakage and delivery
    result.leaked = sewer.leaked_water_mass(active_wastewater_plants);
    result.delivered_to_treatment = sewer.water_delivered_to_treatment(active_wastewater_plants);

    result
}

// ============================================================================
// WASTEWATER TREATMENT (Filter blackwater, produce Fertilizers, heal surface)
// ============================================================================

/// Result of wastewater treatment for one turn.
#[derive(Debug, Clone, Default)]
pub struct WastewaterTreatmentResult {
    /// Sewage intake (liters).
    pub intake: f64,
    /// Fertilizers produced (kg) — extracted biosolids.
    pub fertilizers_produced: f64,
    /// Water discharged back to surface reserves (liters).
    pub water_discharged: f64,
    /// Quality of discharged water (0.0-1.0).
    pub discharge_quality: f64,
    /// Residual biohazard mass (pathogens not captured by treatment).
    pub residual_biohazard: f64,
    /// Energy consumed (kWh).
    pub energy_consumed: f64,
    /// Chemicals consumed.
    pub chemicals_consumed: f64,
}

/// Run wastewater treatment for a region.
///
/// PARADIGM SHIFT + REFINEMENT 4: Wastewater plants intake blackwater
/// (quality 0.05) from the sewer network, extract pathogens into
/// `Commodity::Fertilizers` (biosolids), and discharge the remaining
/// water mass back into the surface water pool at improved quality.
///
/// Mass balance: `water_in = Fertilizers_out + discharged_water + residual_biohazard`
/// (mass is conserved — pathogens become Fertilizers, water returns to environment)
///
/// # Arguments
/// * `reserves` - Mutable water reserves (discharged water added to surface)
/// * `sewer` - Sewer network (provides intake water)
/// * `plant_specs` - List of (treatment_efficiency, fertilizer_output_per_liter,
///   discharge_quality) for each active wastewater plant
/// * `active_wastewater_plants` - Number of active plants (for sewer delivery calc)
pub fn process_wastewater_treatment(
    reserves: &mut WaterReserveState,
    sewer: &SewerNetworkState,
    plant_specs: &[(f64, f64, f64)],
    active_wastewater_plants: usize,
) -> WastewaterTreatmentResult {
    let mut result = WastewaterTreatmentResult::default();

    if plant_specs.is_empty() {
        return result;
    }

    // Get water delivered to treatment plants (after sewer leakage)
    let water_delivered = sewer.water_delivered_to_treatment(active_wastewater_plants);
    if water_delivered <= 0.0 {
        return result;
    }

    result.intake = water_delivered;

    // Distribute intake among plants pro-rata by their treatment capacity
    // (each plant processes an equal share for simplicity; in full integration,
    // each plant's throughput capacity is resolved individually)
    let plants_count = plant_specs.len() as f64;
    let water_per_plant = water_delivered / plants_count;

    let mut total_fertilizers: f64 = 0.0;
    let mut total_discharged: f64 = 0.0;
    let mut total_quality_weighted: f64 = 0.0;
    let mut total_residual_biohazard: f64 = 0.0;

    for &(treatment_efficiency, fertilizer_per_liter, discharge_quality) in plant_specs {
        let intake = water_per_plant;

        // Fertilizers extracted (biosolids) — scaled by treatment efficiency
        let fertilizers = intake * fertilizer_per_liter * treatment_efficiency;
        total_fertilizers += fertilizers;

        // Water discharged back to surface (mass conservation: water - fertilizers)
        // Fertilizers are a small mass fraction; most water returns.
        let discharged = (intake - fertilizers * 0.01).max(0.0); // 1% mass conversion
        total_discharged += discharged;
        total_quality_weighted += discharged * discharge_quality;

        // Residual biohazard = pathogens not captured
        // (1 - treatment_efficiency) of the biohazard mass escapes
        let residual = intake * (1.0 - treatment_efficiency) * 0.001;
        total_residual_biohazard += residual;
    }

    // Discharge healed water back to surface reserves
    let avg_discharge_quality = if total_discharged > 0.0 {
        total_quality_weighted / total_discharged
    } else {
        BLACKWATER_QUALITY
    };

    reserves.discharge_to_surface(total_discharged, avg_discharge_quality);

    result.fertilizers_produced = total_fertilizers;
    result.water_discharged = total_discharged;
    result.discharge_quality = avg_discharge_quality;
    result.residual_biohazard = total_residual_biohazard;
    result.energy_consumed = water_delivered * 0.002; // Approximate
    result.chemicals_consumed = water_delivered * 0.001; // Approximate

    result
}

// ============================================================================
// COLD-START ENERGY FORECAST (Prevents Turn 1 deadlock)
// ============================================================================

/// Forecast energy demand for water treatment plants.
///
/// If prior throughput is zero (cold start), forecast using
/// `nameplate_capacity * 0.5` to avoid Turn 1 deadlock.
/// Otherwise, use the actual throughput from the previous turn.
///
/// # Arguments
/// * `nameplate_capacity` - Total nameplate throughput of all plants (liters/turn)
/// * `prior_throughput` - Actual throughput from previous turn (liters)
/// * `energy_per_liter` - Energy consumption per liter (kWh/L)
pub fn forecast_treatment_energy(
    nameplate_capacity: f64,
    prior_throughput: f64,
    energy_per_liter: f64,
) -> f64 {
    let effective_throughput = if prior_throughput > 0.0 {
        prior_throughput
    } else {
        // Cold start: 50% of nameplate to avoid deadlock
        nameplate_capacity * 0.5
    };
    effective_throughput * energy_per_liter
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Water Reserve State ──

    #[test]
    fn test_water_reserve_defaults() {
        let wrs = WaterReserveState::default();
        assert_eq!(wrs.groundwater_quality, 0.9);
        assert_eq!(wrs.surface_water_quality, 0.6);
        assert_eq!(wrs.natural_outflow_rate, 0.05);
    }

    #[test]
    fn test_groundwater_draw_clamps_to_available() {
        let mut wrs = WaterReserveState {
            groundwater_volume: 100.0,
            groundwater_quality: 0.9,
            ..Default::default()
        };
        let (drawn, quality) = wrs.draw_groundwater(150.0);
        assert_eq!(drawn, 100.0); // clamped to available
        assert_eq!(quality, 0.9);
        assert_eq!(wrs.groundwater_volume, 0.0);
    }

    #[test]
    fn test_surface_water_draw_clamps_to_available() {
        let mut wrs = WaterReserveState {
            surface_water_volume: 200.0,
            surface_water_quality: 0.6,
            ..Default::default()
        };
        let (drawn, quality) = wrs.draw_surface_water(300.0);
        assert_eq!(drawn, 200.0);
        assert_eq!(quality, 0.6);
        assert_eq!(wrs.surface_water_volume, 0.0);
    }

    #[test]
    fn test_surface_water_discharge_heals_quality() {
        let mut wrs = WaterReserveState {
            surface_water_volume: 1000.0,
            surface_water_quality: 0.3, // polluted
            ..Default::default()
        };
        // Discharge 500L at quality 0.85 (advanced MBR output)
        wrs.discharge_to_surface(500.0, 0.85);
        // Blended: (1000*0.3 + 500*0.85) / 1500 = (300 + 425) / 1500 = 0.4833...
        assert!((wrs.surface_water_quality - 0.4833).abs() < 0.01);
        assert_eq!(wrs.surface_water_volume, 1500.0);
    }

    #[test]
    fn test_surface_water_contamination_degrades_quality() {
        let mut wrs = WaterReserveState {
            surface_water_volume: 10000.0,
            surface_water_quality: 0.6,
            ..Default::default()
        };
        wrs.contaminate_surface(1000.0);
        assert!(wrs.surface_water_quality < 0.6);
        assert!(wrs.surface_water_quality > 0.0);
    }

    #[test]
    fn test_guardrail1_natural_outflow_reaches_equilibrium() {
        // With inflow 1000 and outflow_rate 0.05, the recurrence is:
        //   volume_new = (volume + inflow) * (1 - rate)
        // At equilibrium: volume = (volume + inflow) * (1 - rate)
        //   volume = volume*(1-rate) + inflow*(1-rate)
        //   volume * rate = inflow * (1 - rate)
        //   volume = inflow * (1 - rate) / rate = 1000 * 0.95 / 0.05 = 19000
        let mut wrs = WaterReserveState {
            surface_water_volume: 0.0,
            surface_water_inflow_rate: 1000.0,
            natural_outflow_rate: 0.05,
            ..Default::default()
        };
        // Simulate 1000 turns
        for _ in 0..1000 {
            wrs.regenerate(0.0);
        }
        // Should be near equilibrium 19000
        assert!((wrs.surface_water_volume - 19000.0).abs() < 100.0);
    }

    // ── Water Network State ──

    #[test]
    fn test_water_network_defaults() {
        let wn = WaterNetworkState::default();
        assert_eq!(wn.pipe_condition, 1.0);
        assert_eq!(wn.loss_per_km, 0.01);
        assert_eq!(wn.pipe_network_km, 0.0);
        assert_eq!(wn.current_quality, 0.0);
    }

    #[test]
    fn test_water_network_no_pipes_total_loss() {
        let wn = WaterNetworkState::default();
        assert_eq!(wn.transmission_loss(1), 1.0);
    }

    #[test]
    fn test_water_network_delivery_distance() {
        let wn = WaterNetworkState {
            pipe_network_km: 500.0,
            ..Default::default()
        };
        // 500/5 = 100, sqrt(100) = 10, * 1.2 = 12.0
        let dist = wn.average_delivery_distance_km(5);
        assert!((dist - 12.0).abs() < 0.01);
    }

    #[test]
    fn test_water_network_transmission_loss_exponential() {
        let wn = WaterNetworkState {
            pipe_network_km: 100.0,
            pipe_condition: 1.0,
            loss_per_km: 0.01,
            ..Default::default()
        };
        let loss = wn.transmission_loss(1);
        // At ~12 km with 1% loss/km: 1 - 0.99^12 ≈ 0.1136
        assert!(loss > 0.0 && loss < 1.0);
    }

    #[test]
    fn test_water_network_max_connectable_buildings() {
        let wn = WaterNetworkState {
            pipe_network_km: 100.0,
            ..Default::default()
        };
        // dev=0.5: 10 + 0.5*20 = 20 buildings/km → 2000
        assert_eq!(wn.max_connectable_buildings(0.5), 2000);
    }

    #[test]
    fn test_water_network_degradation() {
        let mut wn = WaterNetworkState {
            pipe_condition: 1.0,
            ..Default::default()
        };
        wn.degrade(2.0); // cold climate
                         // 0.001 * (1 + 2.0) = 0.003
        assert!((wn.pipe_condition - 0.997).abs() < 1e-9);
    }

    // ── Sewer Network State ──

    #[test]
    fn test_sewer_network_defaults() {
        let sn = SewerNetworkState::default();
        assert_eq!(sn.pipe_condition, 1.0);
        assert_eq!(sn.leakage_per_km, 0.005);
        assert_eq!(sn.current_quality, 0.05);
    }

    #[test]
    fn test_sewer_leakage_exponential_never_exceeds_one() {
        let sn = SewerNetworkState {
            pipe_network_km: 10000.0, // very long network
            pipe_condition: 1.0,
            leakage_per_km: 0.005,
            ..Default::default()
        };
        let leakage = sn.leakage_fraction(1);
        // Even at extreme distances, exponential decay never reaches 1.0
        assert!(leakage < 1.0);
        assert!(leakage > 0.0);
    }

    #[test]
    fn test_sewer_leakage_at_typical_distance() {
        let sn = SewerNetworkState {
            pipe_network_km: 100.0,
            pipe_condition: 1.0,
            leakage_per_km: 0.005,
            ..Default::default()
        };
        // avg distance = sqrt(100) * 1.2 = 12 km
        // leakage = 1 - 0.995^12 ≈ 0.0584
        let leakage = sn.leakage_fraction(1);
        assert!((leakage - 0.0584).abs() < 0.01);
    }

    #[test]
    fn test_sewer_no_pipes_all_leakage() {
        let sn = SewerNetworkState::default();
        assert_eq!(sn.leakage_fraction(1), 1.0);
    }

    #[test]
    fn test_sewer_delivered_to_treatment() {
        let sn = SewerNetworkState {
            pipe_network_km: 100.0,
            pipe_condition: 0.8,
            leakage_per_km: 0.005,
            throughput_liters: 1000.0,
            ..Default::default()
        };
        let delivered = sn.water_delivered_to_treatment(1);
        let leaked = sn.leaked_water_mass(1);
        // delivered + leaked/pipe_loss should be consistent
        assert!(delivered > 0.0 && delivered < 1000.0);
        assert!(leaked > 0.0 && leaked < 1000.0);
    }

    #[test]
    fn test_sewer_degradation() {
        let mut sn = SewerNetworkState {
            pipe_condition: 1.0,
            ..Default::default()
        };
        sn.degrade();
        assert!((sn.pipe_condition - 0.9985).abs() < 1e-9);
    }

    // ── Regulated Pricing (PATCH 5) ──

    #[test]
    fn test_regulated_water_price_normal() {
        let price = compute_regulated_water_price(
            100.0,   // chemicals_opex
            200.0,   // energy_opex
            300.0,   // labor_opex
            50.0,    // maintenance_opex
            50000.0, // total_asset_value
            160.0,   // amortization_turns
            1000.0,  // smoothed_water_sales_liters
            1.10,    // cost_plus_margin
            10.0,    // average_wage
        );
        // amortized_capex = 50000 / 160 = 312.5
        // total_cost = 100 + 200 + 300 + 50 + 312.5 = 962.5
        // price = 962.5 / 1000 * 1.10 = 1.05875
        assert!((price - 1.05875).abs() < 0.01);
    }

    #[test]
    fn test_regulated_water_price_no_sales_fallback() {
        let price = compute_regulated_water_price(
            100.0, 200.0, 300.0, 50.0, 50000.0, 160.0, 0.0, 1.10, 10.0,
        );
        // Fallback: average_wage * 0.5 = 5.0
        assert!((price - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_regulated_sewage_price_normal() {
        let price = compute_regulated_sewage_price(
            50.0, 100.0, 200.0, 30.0, 40000.0, 160.0, 800.0, 1.10, 10.0,
        );
        // amortized_capex = 40000 / 160 = 250.0
        // total_cost = 50 + 100 + 200 + 30 + 250 = 630.0
        // price = 630.0 / 800 * 1.10 = 0.86625
        assert!((price - 0.86625).abs() < 0.01);
    }

    // ── Sales History ──

    #[test]
    fn test_water_sales_history_rolling_window() {
        let mut h = WaterSalesHistory::new();
        for _ in 0..30 {
            h.record(100.0);
        }
        // Window size 24 — older entries dropped
        assert_eq!(h.sales_history.len(), 24);
        assert!((h.smoothed_sales() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_water_sales_history_empty() {
        let h = WaterSalesHistory::new();
        assert_eq!(h.smoothed_sales(), 0.0);
    }

    // ── Dehydration Mortality (GUARDRAIL 2) ──

    #[test]
    fn test_dehydration_mortality_no_deficit() {
        let m = compute_dehydration_mortality(0.0, 100.0);
        assert_eq!(m, 1.0);
    }

    #[test]
    fn test_dehydration_mortality_full_deficit() {
        let m = compute_dehydration_mortality(100.0, 100.0);
        // 1.0 + 1.0 * 3.0 = 4.0 (death rate quadruples)
        assert!((m - 4.0).abs() < 1e-9);
    }

    #[test]
    fn test_dehydration_mortality_half_deficit() {
        let m = compute_dehydration_mortality(50.0, 100.0);
        // 1.0 + 0.5 * 3.0 = 2.5
        assert!((m - 2.5).abs() < 1e-9);
    }

    #[test]
    fn test_dehydration_mortality_no_demand() {
        let m = compute_dehydration_mortality(50.0, 0.0);
        assert_eq!(m, 1.0); // no demand = no dehydration
    }

    // ── Blueprint 006: Fractional Aquifer Recharge ──

    #[test]
    fn test_fractional_recharge_does_not_reset_to_capacity() {
        // Blueprint 006 invariant: Fractional recharge must NOT reset
        // a depleted aquifer to full capacity. A depleted aquifer should
        // recover slowly over many turns.
        let mut wrs = WaterReserveState {
            groundwater_volume: 0.0, // fully depleted
            groundwater_quality: 0.8,
            ..Default::default()
        };
        let capacity = 1_000_000.0; // 1M liters
        // Moderate precipitation: 150mm, 10% recharge, 1000 m² area
        wrs.regenerate_fractional(capacity, 150.0, 0.10, 1000.0);
        // Recharge volume = 150 * 0.10 * 1000 = 15,000 liters
        // After outflow: 15000 * (1 - 0.025) = 14,625 liters
        // Must be MUCH less than capacity (1M)
        assert!(wrs.groundwater_volume < capacity * 0.05,
            "Fractional recharge must not reset to capacity: got {}",
            wrs.groundwater_volume);
        assert!(wrs.groundwater_volume > 0.0,
            "Fractional recharge must add some water");
    }

    #[test]
    fn test_depleted_aquifer_recovers_slowly() {
        // Blueprint 006 invariant: A depleted aquifer recovers slowly
        // over multiple turns, not in a single turn.
        let mut wrs = WaterReserveState {
            groundwater_volume: 0.0,
            groundwater_quality: 0.8,
            ..Default::default()
        };
        let capacity = 1_000_000.0;
        // Simulate 10 turns of recharge
        let mut volumes = Vec::new();
        for _ in 0..10 {
            wrs.regenerate_fractional(capacity, 150.0, 0.10, 1000.0);
            volumes.push(wrs.groundwater_volume);
        }
        // Volume must be monotonically increasing (recharge > outflow at low volumes)
        for i in 1..volumes.len() {
            assert!(volumes[i] >= volumes[i-1] || volumes[i].abs() < 1.0,
                "Aquifer should recover monotonically at low volumes");
        }
        // After 10 turns, still well below capacity
        assert!(wrs.groundwater_volume < capacity * 0.5,
            "Aquifer should not reach capacity in 10 turns: got {}",
            wrs.groundwater_volume);
    }

    #[test]
    fn test_well_depletion_drives_yield_to_zero() {
        // Blueprint 006 invariant: Aquifer depletion causes well yield
        // to reach zero. When groundwater_volume = 0, draw_groundwater
        // returns 0.0.
        let mut wrs = WaterReserveState {
            groundwater_volume: 50.0, // nearly depleted
            groundwater_quality: 0.8,
            ..Default::default()
        };
        // Draw more than available
        let (drawn1, _) = wrs.draw_groundwater(100.0);
        assert_eq!(drawn1, 50.0); // only 50L available
        assert_eq!(wrs.groundwater_volume, 0.0);
        // Now depleted — drawing more yields zero
        let (drawn2, _) = wrs.draw_groundwater(100.0);
        assert_eq!(drawn2, 0.0, "Depleted aquifer must yield 0.0 water");
    }
}

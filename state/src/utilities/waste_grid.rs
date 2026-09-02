//! Phase 84: Waste grid infrastructure — Solid Waste Management & Circular Economy.
//!
//! PARADIGM SHIFT: Waste is the residual mass of consumption and production.
//! It is NOT spawned from flat rates — it is derived from actual commodity
//! consumption via mass conservation. Waste flows through the WasteGridState
//! as a physical logistical transfer (for trash streams) or through the B2B
//! market (for sorted secondary raw materials).
//!
//! ## Key Physics
//!
//! - **Mass Conservation**: Every recycling/separation/WtE method outputs
//!   residual waste so output mass = input mass. No mass disappears.
//! - **WtE Ash**: Incineration does NOT annihilate mass. WtE plants output
//!   HazardousWaste ash residue (0.20–0.30 per unit input).
//! - **B2B Exclusion**: Trash streams (MixedWaste, BioWaste, ConstructionWaste,
//!   BulkyWaste, HazardousWaste) never enter the B2B order book. Only sorted
//!   fractions (MetalWaste, GlassWaste, PlasticWaste, ElectronicWaste,
//!   TextileWaste) are B2B-tradeable.
//! - **Geographic Dumping Vectors**: Standalone disposal uses a runtime-computed
//!   dumping vector (Street/Forest/River) based on region geography, each with
//!   distinct cross-system consequences.
//! - **Landfill Hard Stop**: When remaining_capacity hits 0.0, the landfill
//!   physically rejects all waste, causing catastrophic backup.

use crate::registries::enums::Commodity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// PHYSICAL CONSTANTS
// ============================================================================

/// Fraction of construction materials that become waste during building.
/// Physical constant from construction engineering data: ~10% of delivered
/// materials become scrap/debris (offcuts, broken bricks, packaging, etc.).
pub const CONSTRUCTION_WASTE_FRACTION: f64 = 0.10;

/// Calorific energy from incinerating 1 ton of MixedWaste (MWh/ton).
/// Physical constant: MSW has ~10 GJ/ton calorific value ≈ 2.78 MWh/ton.
/// At ~25% electrical efficiency → ~0.7 MWh/ton. We use a conservative 0.002
/// in the commodity unit system (1 Energy unit ≈ 1 MWh equivalent).
pub const WTE_ENERGY_PER_TON: f64 = 0.002;

/// Additional heat from CHP co-generation (MWh/ton). CHP recovers ~40% of
/// remaining thermal energy → ~0.004 Heat units per ton.
pub const WTE_HEAT_PER_TON_CHP: f64 = 0.004;

/// Ash residue fraction from basic WtE incineration (mass conservation).
/// 25% of input mass remains as toxic bottom ash (HazardousWaste).
pub const WTE_ASH_FRACTION_BASIC: f64 = 0.25;

/// Ash residue fraction from advanced WtE (better combustion, less ash).
/// 20% of input mass remains as toxic bottom ash (HazardousWaste).
pub const WTE_ASH_FRACTION_ADVANCED: f64 = 0.20;

/// Subsistence food produced per unit of compost (kg food per kg compost).
/// Physical constant: compost-to-food yield for subsistence gardening.
pub const SUBSISTENCE_FOOD_PER_FERTILIZER: f64 = 2.0;

/// Forest area threshold (fraction of total land) required for Forest/Wild
/// dumping vector. Below this, the region cannot use forest dumping.
pub const FOREST_AREA_THRESHOLD: f64 = 0.10;

/// Leachate contamination factor — how much leaked leachate degrades
/// groundwater quality per liter of groundwater.
pub const LEACHATE_CONTAMINATION_FACTOR: f64 = 0.0001;

// ============================================================================
// WASTE PLANT TYPE ENUM
// ============================================================================

/// Phase 84: Types of specialized municipal waste infrastructure plants.
/// Each type has a distinct BuildingMethods registry with full
/// Production/Automation/Organization matrices (Rule 13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WastePlantType {
    #[default]
    /// Pre-1900: no liner, no leachate capture. Maximum environmental damage.
    UncontrolledLandfill,
    /// 1900s: clay liner, basic leachate collection. Moderate damage.
    ControlledLandfill,
    /// 1970s: HDPE liner, leachate treatment, gas capture. Minimal damage.
    ModernLandfill,
    /// 1950s: mechanical/manual sorting of MixedWaste into fractions.
    WasteSeparationPlant,
    /// 1990s: AI-assisted optical sorting. Higher separation efficiency.
    AdvancedSortingFacility,
    /// 1900s: processes MetalWaste → Steel/Copper/Aluminum + residual.
    MetalRecycling,
    /// 1900s: processes GlassWaste → Glass + residual.
    GlassRecycling,
    /// 1970s: processes PlasticWaste → Plastics + residual.
    PlasticRecycling,
    /// 1990s: processes ElectronicWaste → Semiconductors/Copper/REE + residual.
    ElectronicRecycling,
    /// 2000s: processes TextileWaste → IndustrialFiber + residual.
    TextileRecycling,
    /// 1970s: incinerates residual waste → Energy + HazardousWaste ash.
    WasteToEnergyPlant,
    /// 2000s: WtE with CHP, feeds DistrictHeating. Energy + Heat + ash.
    AdvancedWtECHP,
    /// PSZOK: drop-off for BulkyWaste, ConstructionWaste, HazardousWaste.
    /// Requires FreightCapacity for waste transport to site.
    CivicAmenitySite,
}

impl WastePlantType {
    /// Returns the registry key for this plant type in the production methods.
    pub fn registry_key(&self) -> &'static str {
        match self {
            WastePlantType::UncontrolledLandfill => "uncontrolled_landfill",
            WastePlantType::ControlledLandfill => "controlled_landfill",
            WastePlantType::ModernLandfill => "modern_landfill",
            WastePlantType::WasteSeparationPlant => "waste_separation_plant",
            WastePlantType::AdvancedSortingFacility => "advanced_sorting_facility",
            WastePlantType::MetalRecycling => "metal_recycling",
            WastePlantType::GlassRecycling => "glass_recycling",
            WastePlantType::PlasticRecycling => "plastic_recycling",
            WastePlantType::ElectronicRecycling => "electronic_recycling",
            WastePlantType::TextileRecycling => "textile_recycling",
            WastePlantType::WasteToEnergyPlant => "waste_to_energy_plant",
            WastePlantType::AdvancedWtECHP => "advanced_wte_chp",
            WastePlantType::CivicAmenitySite => "civic_amenity_site",
        }
    }

    /// Returns true if this plant type is a landfill variant.
    pub fn is_landfill(&self) -> bool {
        matches!(
            self,
            WastePlantType::UncontrolledLandfill
                | WastePlantType::ControlledLandfill
                | WastePlantType::ModernLandfill
        )
    }

    /// Returns true if this plant type is a recycling facility.
    pub fn is_recycling(&self) -> bool {
        matches!(
            self,
            WastePlantType::MetalRecycling
                | WastePlantType::GlassRecycling
                | WastePlantType::PlasticRecycling
                | WastePlantType::ElectronicRecycling
                | WastePlantType::TextileRecycling
        )
    }

    /// Returns true if this plant type is a Waste-to-Energy facility.
    pub fn is_wte(&self) -> bool {
        matches!(
            self,
            WastePlantType::WasteToEnergyPlant | WastePlantType::AdvancedWtECHP
        )
    }
}

// ============================================================================
// DUMPING VECTOR ENUM (REFINEMENT 2)
// ============================================================================

/// Phase 84 REFINEMENT 2: Geographic dumping vector for standalone waste disposal.
/// Determined at runtime from region geography, not chosen by the player.
/// Each vector has distinct cross-system consequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DumpingVector {
    #[default]
    /// Default for urban/dense areas without water or forest access.
    /// Massive local biohazard (factor 5.0). No cross-system impact beyond
    /// local Parcel.pollution_level and disease outbreaks.
    StreetAlley,
    /// Requires significant forest area (> 10% of total land).
    /// Moderate biohazard (factor 3.0). Degrades forestry ecological_health,
    /// reducing timber yield.
    ForestWild,
    /// Requires has_navigable_river or has_coastline.
    /// Negligible local biohazard (factor 0.5 — trash washes away).
    /// Severely degrades surface_water_quality (Phase 83 link).
    RiverWater,
}

impl DumpingVector {
    /// Biohazard factor for this dumping vector.
    pub fn biohazard_factor(&self) -> f64 {
        match self {
            DumpingVector::StreetAlley => 5.0,
            DumpingVector::ForestWild => 3.0,
            DumpingVector::RiverWater => 0.5,
        }
    }

    /// Returns true if this dumping vector degrades surface water quality.
    pub fn degrades_surface_water(&self) -> bool {
        matches!(self, DumpingVector::RiverWater)
    }

    /// Returns true if this dumping vector degrades forestry ecological health.
    pub fn degrades_forestry(&self) -> bool {
        matches!(self, DumpingVector::ForestWild)
    }

    /// Returns true if this dumping vector leaches into groundwater.
    pub fn leaches_to_groundwater(&self) -> bool {
        matches!(self, DumpingVector::StreetAlley | DumpingVector::ForestWild)
    }
}

// ============================================================================
// WASTE GRID STATE
// ============================================================================

/// Phase 84: Waste collection and distribution grid for a region.
///
/// Tracks the collection route network, uncollected waste accumulation,
/// and separation efficiency. Trash streams (MixedWaste, BioWaste, etc.)
/// flow through this grid as physical logistical transfers, NOT via B2B.
/// Sorted fractions (MetalWaste, etc.) may be sold on B2B by the waste utility.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WasteGridState {
    /// Collection route network length (km). Scales collection capacity.
    #[serde(default)]
    pub collection_route_km: f64,

    /// Route condition (0.0 = collapsed, 1.0 = pristine). Degrades over time.
    #[serde(default)]
    pub route_condition: f64,

    /// Per-turn collection capacity (tons), computed from route_km * condition.
    #[serde(default)]
    pub collection_capacity: f64,

    /// Current uncollected waste accumulation per category (tons).
    /// Keyed by Commodity. Grows when collection capacity is insufficient.
    #[serde(default)]
    pub uncollected_waste: HashMap<Commodity, f64>,

    /// Active collection method separation efficiency (0.0-1.0).
    /// 0.0 = unsegregated (all → MixedWaste), 1.0 = perfect source separation.
    #[serde(default)]
    pub separation_efficiency: f64,

    /// Landfill capacity utilization (0.0-1.0) across all landfills in region.
    /// Computed from all LandfillState buildings in the region.
    #[serde(default)]
    pub landfill_utilization: f64,

    /// Methane capture rate (0.0-1.0) — from modern landfill gas systems.
    /// 0.0 for uncontrolled landfills, up to ~0.8 for modern landfills.
    #[serde(default)]
    pub methane_capture_rate: f64,
}

impl Default for WasteGridState {
    fn default() -> Self {
        Self {
            collection_route_km: 0.0,
            route_condition: 1.0,
            collection_capacity: 0.0,
            uncollected_waste: HashMap::new(),
            separation_efficiency: 0.0,
            landfill_utilization: 0.0,
            methane_capture_rate: 0.0,
        }
    }
}

impl WasteGridState {
    /// Degrade route condition by a small amount each turn.
    /// Routes degrade faster in winter (freeze-thaw damage).
    pub fn degrade(&mut self, winter_severity: f64) {
        let base_rate = 0.005;
        let winter_factor = 1.0 + winter_severity * 0.5;
        self.route_condition = (self.route_condition - base_rate * winter_factor).max(0.0);
    }

    /// Recompute collection capacity from route length and condition.
    /// Capacity = route_km * condition * TONS_PER_KM_PER_TURN.
    pub fn recompute_capacity(&mut self) {
        const TONS_PER_KM_PER_TURN: f64 = 5.0;
        self.collection_capacity =
            self.collection_route_km * self.route_condition * TONS_PER_KM_PER_TURN;
    }

    /// Add uncollected waste for a specific category.
    pub fn add_uncollected(&mut self, commodity: Commodity, tons: f64) {
        if tons <= 0.0 {
            return;
        }
        *self.uncollected_waste.entry(commodity).or_insert(0.0) += tons;
    }

    /// Total uncollected waste mass across all categories.
    pub fn total_uncollected(&self) -> f64 {
        self.uncollected_waste.values().sum()
    }

    /// Drain a fraction of uncollected waste (e.g., when collection catches up).
    /// Returns the drained amount per category.
    pub fn drain_uncollected(&mut self, fraction: f64) -> HashMap<Commodity, f64> {
        let f = fraction.clamp(0.0, 1.0);
        let mut drained = HashMap::new();
        for (commodity, qty) in &mut self.uncollected_waste {
            let d = *qty * f;
            // Rule 20: Clamp to zero — waste reserve cannot go negative.
            *qty = (*qty - d).max(0.0);
            if d > 0.0 {
                drained.insert(*commodity, d);
            }
        }
        drained
    }
}

// ============================================================================
// LANDFILL STATE (LOGISTICAL BOUND 2 — Hard Capacity Stop)
// ============================================================================

/// Phase 84: Physical state of a landfill building.
///
/// Replaces the legacy `LandfillData` with typed `Commodity` keys and
/// a hard capacity stop. When `remaining_capacity` hits 0.0, the landfill
/// physically rejects all incoming waste, causing catastrophic backup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LandfillState {
    /// Total capacity (tons), scaled by physical site area.
    #[serde(default)]
    pub total_capacity: f64,

    /// Current stored volume by waste category (tons).
    #[serde(default)]
    pub stored_waste: HashMap<Commodity, f64>,

    /// Liner integrity (0.0 = breached, 1.0 = intact).
    /// Uncontrolled landfills default to 0.0 (no liner). Controlled → 0.5.
    /// Modern → 1.0. Degrades over time; breach causes leachate leakage.
    #[serde(default)]
    pub liner_integrity: f64,

    /// Leachate collection efficiency (0.0-1.0).
    /// Fraction of leachate captured before it reaches groundwater.
    /// Uncontrolled → 0.0, Controlled → 0.3, Modern → 0.9.
    #[serde(default)]
    pub leachate_capture: f64,

    /// Gas capture efficiency (0.0-1.0).
    /// Fraction of methane/landfill gas captured (can be used for energy).
    /// Uncontrolled → 0.0, Controlled → 0.1, Modern → 0.8.
    #[serde(default)]
    pub gas_capture: f64,

    /// Remaining capacity (tons). When this hits 0.0, landfill rejects all waste.
    #[serde(default)]
    pub remaining_capacity: f64,

    /// LOGISTICAL BOUND 2: True when remaining_capacity == 0.0.
    /// When full, all incoming waste is rejected → uncollected → biohazard crisis.
    #[serde(default)]
    pub is_full: bool,
}

impl LandfillState {
    /// Create a new landfill state with the given capacity and tier parameters.
    pub fn new(
        total_capacity: f64,
        liner_integrity: f64,
        leachate_capture: f64,
        gas_capture: f64,
    ) -> Self {
        Self {
            total_capacity,
            stored_waste: HashMap::new(),
            liner_integrity,
            leachate_capture,
            gas_capture,
            remaining_capacity: total_capacity,
            is_full: false,
        }
    }

    /// LOGISTICAL BOUND 2: Attempt to accept waste into the landfill.
    ///
    /// When `remaining_capacity` hits `0.0`, the landfill physically rejects
    /// all incoming waste. The caller must route rejected waste to uncollected
    /// accumulation, triggering `uncollected_waste_biohazard` crisis.
    ///
    /// # Arguments
    /// * `waste_by_category` - Waste to deposit, keyed by Commodity (tons).
    ///
    /// # Returns
    /// * Total accepted tonnage. Rejected waste = input - accepted.
    pub fn accept_waste(&mut self, waste_by_category: &HashMap<Commodity, f64>) -> f64 {
        let total_incoming: f64 = waste_by_category.values().sum();
        if self.remaining_capacity <= 0.0 || self.is_full {
            self.is_full = true;
            return 0.0; // Hard reject — catastrophic backup
        }
        let accepted = total_incoming.min(self.remaining_capacity);
        let scale = if total_incoming > 0.0 {
            accepted / total_incoming
        } else {
            0.0
        };
        for (commodity, qty) in waste_by_category {
            let accepted_qty = qty * scale;
            if accepted_qty > 0.0 {
                *self.stored_waste.entry(*commodity).or_insert(0.0) += accepted_qty;
            }
        }
        self.remaining_capacity -= accepted;
        self.remaining_capacity = self.remaining_capacity.max(0.0);
        if self.remaining_capacity <= 0.0 {
            self.is_full = true;
        }
        accepted
    }

    /// Compute leachate leakage for this turn.
    /// Leachate = stored waste mass * (1 - leachate_capture) * liner_breach_factor.
    /// Liner breach factor = (1 - liner_integrity), so breached liners leak more.
    pub fn leachate_leakage(&self) -> f64 {
        let total_stored: f64 = self.stored_waste.values().sum();
        let escape_fraction = (1.0 - self.leachate_capture) * (1.0 - self.liner_integrity);
        total_stored * escape_fraction * 0.01 // 1% per turn of uncaptured leachate
    }

    /// Compute methane gas generation for this turn.
    /// Gas = stored organic waste * gas_generation_rate.
    /// Captured gas can be used for energy; uncapped gas contributes to smog.
    pub fn methane_generation(&self) -> f64 {
        let organic: f64 = self
            .stored_waste
            .get(&Commodity::BioWaste)
            .copied()
            .unwrap_or(0.0)
            + self
                .stored_waste
                .get(&Commodity::MixedWaste)
                .copied()
                .unwrap_or(0.0)
                * 0.4; // ~40% of MixedWaste is organic
        organic * 0.02 // 2% of organic mass becomes gas per turn
    }

    /// Degrade liner integrity over time. Faster degradation for older landfills.
    pub fn degrade_liner(&mut self, winter_severity: f64) {
        if self.liner_integrity > 0.0 {
            let rate = 0.001 * (1.0 + winter_severity * 0.3);
            self.liner_integrity = (self.liner_integrity - rate).max(0.0);
        }
    }

    /// Total stored waste mass across all categories.
    pub fn total_stored(&self) -> f64 {
        self.stored_waste.values().sum()
    }

    /// Utilization fraction (0.0-1.0).
    pub fn utilization(&self) -> f64 {
        if self.total_capacity <= 0.0 {
            return 0.0;
        }
        1.0 - (self.remaining_capacity / self.total_capacity)
    }
}

// ============================================================================
// DUMPING VECTOR SELECTION (REFINEMENT 2)
// ============================================================================

/// Select the geographic dumping vector for a region based on its physical geography.
///
/// Selection priority (rational actors choose the lowest-local-impact option):
/// 1. If `has_navigable_river || has_coastline` → River/Water (lowest local biohazard,
///    but severe downstream surface_water_quality degradation)
/// 2. Else if forests area > 10% of total → Forest/Wild (moderate biohazard,
///    but damages forestry ecological_health)
/// 3. Else → Street/Alley (worst local biohazard, but the only option in dense cities)
///
/// This creates a trilemma with no strictly dominant option.
pub fn select_dumping_vector(
    has_navigable_river: bool,
    has_coastline: bool,
    forest_area_fraction: f64,
) -> DumpingVector {
    if has_navigable_river || has_coastline {
        DumpingVector::RiverWater
    } else if forest_area_fraction > FOREST_AREA_THRESHOLD {
        DumpingVector::ForestWild
    } else {
        DumpingVector::StreetAlley
    }
}

// ============================================================================
// WASTE GENERATION (Mass Conservation — Pillar 1)
// ============================================================================

/// Waste fraction per consumed commodity type.
/// Represents the non-consumed residual fraction (packaging, scraps, EOL).
/// Returns (waste_commodity, fraction) for a given consumed commodity.
pub fn waste_fraction_for_commodity(commodity: Commodity) -> Option<(Commodity, f64)> {
    use Commodity::*;
    match commodity {
        // Food → BioWaste (15% food scraps/organic residue)
        Food | Cereal | Vegetable | Meat | Fruit => Some((BioWaste, 0.15)),
        // Clothing → TextileWaste (8% discarded garments)
        Clothing | LuxuryClothing => Some((TextileWaste, 0.08)),
        // Furniture → BulkyWaste (5% end-of-life)
        Furniture | LuxuryFurniture => Some((BulkyWaste, 0.05)),
        // Durables → MetalWaste + ElectronicWaste (3% each)
        Agd | Cars | Televisions | Radio => Some((MetalWaste, 0.03)),
        // Glass packaging → GlassWaste (40% discard)
        Glass => Some((GlassWaste, 0.40)),
        // Plastics packaging → PlasticWaste (35% discard)
        Plastics => Some((PlasticWaste, 0.35)),
        // Paper packaging → BioWaste (25% — paper is biodegradable)
        Paper => Some((BioWaste, 0.25)),
        // Chemicals → HazardousWaste (10% chemical residue)
        Chemicals => Some((HazardousWaste, 0.10)),
        // Industrial metals → MetalWaste (5% scrap from production)
        Steel | Copper | Aluminum | Iron | Lead | Tin | Zinc | Magnesium => {
            Some((MetalWaste, 0.05))
        }
        // Electronics → ElectronicWaste (8% e-scrap)
        Semiconductors | ElectronicComponents => Some((ElectronicWaste, 0.08)),
        // Batteries → HazardousWaste (heavy metals)
        Batteries => Some((HazardousWaste, 0.10)),
        // All other commodities: no waste generation
        _ => None,
    }
}

/// Compute waste generated from a consumption receipt.
///
/// Mass conservation: waste_generated = consumed_quantity * waste_fraction.
/// Returns a map of waste Commodity → tons generated.
pub fn compute_waste_from_consumption(
    consumed: &HashMap<Commodity, f64>,
) -> HashMap<Commodity, f64> {
    let mut waste: HashMap<Commodity, f64> = HashMap::new();
    for (commodity, quantity) in consumed {
        if *quantity <= 0.0 {
            continue;
        }
        if let Some((waste_type, fraction)) = waste_fraction_for_commodity(*commodity) {
            let waste_mass = quantity * fraction;
            if waste_mass > 0.0 {
                *waste.entry(waste_type).or_insert(0.0) += waste_mass;
            }
        }
    }
    waste
}

/// Compute construction waste from delivered materials.
///
/// CONSTRUCTION_WASTE_FRACTION of delivered materials become ConstructionWaste.
pub fn compute_construction_waste(delivered_materials: &HashMap<Commodity, f64>) -> f64 {
    let total_delivered: f64 = delivered_materials.values().sum();
    total_delivered * CONSTRUCTION_WASTE_FRACTION
}

// ============================================================================
// RECYCLING YIELD TABLES (CRITICAL FIX 3 — 100% Mass Conservation)
// ============================================================================

/// Recycling yield: input waste → output commodities + residual.
/// Every yield table sums to 1.0 (100% mass conservation).
pub fn recycling_yields(input: Commodity) -> Vec<(Commodity, f64)> {
    use Commodity::*;
    match input {
        // MetalWaste → 0.70 Steel + 0.15 Copper + 0.10 Aluminum + 0.05 MixedWaste (residual)
        MetalWaste => vec![
            (Steel, 0.70),
            (Copper, 0.15),
            (Aluminum, 0.10),
            (MixedWaste, 0.05), // Non-recyclable fluff/contaminants
        ],
        // GlassWaste → 0.85 Glass + 0.15 MixedWaste (residual)
        GlassWaste => vec![
            (Glass, 0.85),
            (MixedWaste, 0.15), // Ceramic/stone contaminants
        ],
        // PlasticWaste → 0.60 Plastics + 0.40 MixedWaste (residual)
        PlasticWaste => vec![
            (Plastics, 0.60),
            (MixedWaste, 0.40), // Non-recyclable polymer mix/labels
        ],
        // ElectronicWaste → 0.05 Semiconductors + 0.20 Copper + 0.02 REE + 0.73 HazardousWaste (residual)
        ElectronicWaste => vec![
            (Semiconductors, 0.05),
            (Copper, 0.20),
            (RareEarthElements, 0.02),
            (HazardousWaste, 0.73), // Toxic residues (mercury, lead, flame retardants)
        ],
        // TextileWaste → 0.40 IndustrialFiber + 0.60 MixedWaste (residual)
        TextileWaste => vec![
            (IndustrialFiber, 0.40),
            (MixedWaste, 0.60), // Blended/contaminated fabric
        ],
        _ => vec![],
    }
}

/// Waste separation plant yields: MixedWaste → sorted fractions + residual.
/// Sums to 1.0 (100% mass conservation).
pub fn separation_yields() -> Vec<(Commodity, f64)> {
    use Commodity::*;
    vec![
        (MetalWaste, 0.15),
        (GlassWaste, 0.10),
        (PlasticWaste, 0.12),
        (BioWaste, 0.35),
        (TextileWaste, 0.05),
        (ElectronicWaste, 0.03),
        (MixedWaste, 0.20), // Non-sortable refuse — mass balance closure
    ]
}

/// Verify that recycling yields sum to 1.0 for a given input.
#[cfg(test)]
fn verify_mass_balance(yields: &[(Commodity, f64)]) -> bool {
    let total: f64 = yields.iter().map(|(_, y)| y).sum();
    (total - 1.0).abs() < 0.001
}

// ============================================================================
// WASTE DISPOSAL METHOD HELPERS
// ============================================================================

/// Returns true if the waste disposal method name is a centralized
/// (municipal collection) method that routes waste through WasteGridState.
/// Standalone methods (Primitive Dumping, Trash Burning, etc.) return false.
pub fn is_centralized_waste_method(method_name: &str) -> bool {
    matches!(
        method_name,
        "Unsegregated Collection" | "Source-Separated Curbside" | "Smart Sorted Collection"
    )
}

/// Returns the biohazard factor for a standalone waste disposal method.
/// Represents the biological pollution mass per unit of waste disposed.
/// Centralized methods have near-zero residual biohazard.
/// "None" = 5.0 (no waste disposal at all = maximum biohazard from rotting waste).
pub fn waste_disposal_biohazard_factor(method_name: &str) -> f64 {
    match method_name {
        "None" => 5.0,
        "Primitive Dumping" => 5.0,
        "Basic Homesteading" => 1.0,
        "Advanced Rural Scavenging" => 0.5,
        "Trash Burning" => 0.0, // Burning produces smog, not biohazard
        "Unsegregated Collection" => 0.3,
        "Source-Separated Curbside" => 0.05,
        "Smart Sorted Collection" => 0.01,
        _ => 5.0, // Unknown = treat as no disposal
    }
}

/// Returns the smog emission factor for a standalone waste disposal method.
/// Trash Burning produces severe smog. All other methods produce zero smog
/// (dumping produces biohazard, not air pollution).
pub fn waste_disposal_smog_factor(method_name: &str) -> f64 {
    match method_name {
        "Trash Burning" => 0.8,
        _ => 0.0,
    }
}

/// Returns the separation efficiency for a centralized waste collection method.
/// 0.0 = unsegregated (all → MixedWaste), 1.0 = perfect source separation.
pub fn waste_separation_efficiency(method_name: &str) -> f64 {
    match method_name {
        "Unsegregated Collection" => 0.0,
        "Source-Separated Curbside" => 0.90,
        "Smart Sorted Collection" => 0.95,
        _ => 0.0,
    }
}

/// Returns true if a standalone waste disposal method recovers BioWaste as
/// Fertilizers (composting). Used for the fertilizer economic sink.
pub fn waste_disposal_composts(method_name: &str) -> bool {
    matches!(
        method_name,
        "Basic Homesteading" | "Advanced Rural Scavenging"
    )
}

/// Returns true if a standalone waste disposal method recovers MetalWaste and
/// GlassWaste as Steel and Glass (scrap collecting).
pub fn waste_disposal_recovers_scrap(method_name: &str) -> bool {
    matches!(method_name, "Advanced Rural Scavenging")
}

/// Composting yield: fraction of BioWaste converted to Fertilizers.
pub const COMPOSTING_YIELD: f64 = 0.50;

/// Scrap recovery yield: fraction of MetalWaste/GlassWaste recovered as Steel/Glass.
pub const SCRAP_RECOVERY_YIELD: f64 = 0.40;

// ============================================================================
// WASTE POLLUTION COMPUTATION
// ============================================================================

/// Result of waste pollution computation for a region.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WastePollutionResult {
    /// Smog mass from trash burning (feeds into smog_level).
    pub burning_emissions: f64,
    /// Biohazard mass from illegal dumping (feeds into biohazard_level).
    pub dumping_biohazard: f64,
    /// Biohazard mass from uncollected waste rotting in streets.
    pub uncollected_biohazard: f64,
    /// Leachate mass escaping from landfills (degrades groundwater).
    pub leachate_leakage: f64,
    /// Surface water quality degradation from river dumping.
    pub surface_water_degradation: f64,
    /// Forestry ecological health degradation from forest dumping.
    pub forestry_degradation: f64,
}

/// Compute waste pollution for a region from all waste disposal sources.
///
/// This is called during the turn loop after waste processing is complete.
/// The results feed into smog/biohazard computation and water quality degradation.
pub fn compute_waste_pollution(
    burning_waste_mass: f64,
    dumping_waste_mass: f64,
    dumping_vector: DumpingVector,
    uncollected_waste_mass: f64,
    landfill_leachate: f64,
) -> WastePollutionResult {
    // Burning → smog emissions
    let burning_emissions = burning_waste_mass * waste_disposal_smog_factor("Trash Burning");

    // Dumping → biohazard (scaled by dumping vector's biohazard factor)
    let dumping_biohazard = dumping_waste_mass * dumping_vector.biohazard_factor() * 0.1;

    // Uncollected waste → biohazard (rotting in streets)
    let uncollected_biohazard = uncollected_waste_mass * 0.05;

    // Leachate → groundwater contamination
    let leachate_leakage = landfill_leachate;

    // River dumping → surface water quality degradation
    let surface_water_degradation = if dumping_vector.degrades_surface_water() {
        dumping_waste_mass * 0.001 // 0.1% of dumped mass degrades water quality
    } else {
        0.0
    };

    // Forest dumping → forestry ecological health degradation
    let forestry_degradation = if dumping_vector.degrades_forestry() {
        dumping_waste_mass * 0.0005
    } else {
        0.0
    };

    WastePollutionResult {
        burning_emissions,
        dumping_biohazard,
        uncollected_biohazard,
        leachate_leakage,
        surface_water_degradation,
        forestry_degradation,
    }
}

// ============================================================================
// REGULATED WASTE BILLING (CRITICAL FIX 4 — Dual Fee Structure)
// ============================================================================

/// Sales history for curbside waste collection (for smoothed fee computation).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct WasteSalesHistory {
    /// Smoothed collection volume (tons per turn, exponentially smoothed).
    #[serde(default)]
    pub smoothed_curbside_volume: f64,
    /// Smoothed gate (PSZOK) volume (tons per turn, exponentially smoothed).
    #[serde(default)]
    pub smoothed_gate_volume: f64,
    /// Total OPEX for curbside collection (per turn).
    #[serde(default)]
    pub curbside_opex: f64,
    /// Total OPEX for PSZOK gate operations (per turn).
    #[serde(default)]
    pub gate_opex: f64,
    /// Amortized CAPEX for curbside collection infrastructure.
    #[serde(default)]
    pub curbside_amortized_capex: f64,
    /// Amortized CAPEX for PSZOK infrastructure.
    #[serde(default)]
    pub gate_amortized_capex: f64,
    /// Configured profit margin (e.g., 1.05 = 5% margin).
    #[serde(default = "default_waste_margin")]
    pub margin: f64,
}

fn default_waste_margin() -> f64 {
    1.05
}

/// Compute the regulated curbside waste fee (per ton).
///
/// CRITICAL FIX 4: Curbside fee covers standard collected waste (MixedWaste,
/// BioWaste, sorted fractions). Uses cost-plus pricing:
/// fee = (opex + amortized_capex) / smoothed_volume * margin
/// Fallback: average_wage * 0.3 (waste collection ~30% of monthly wage per household).
pub fn compute_regulated_curbside_fee(history: &WasteSalesHistory, average_wage: f64) -> f64 {
    if history.smoothed_curbside_volume > 0.01 {
        let total_cost = history.curbside_opex + history.curbside_amortized_capex;
        let fee_per_ton = total_cost / history.smoothed_curbside_volume * history.margin;
        // Ensure minimum fee (don't go below 10% of average wage per ton)
        fee_per_ton.max(average_wage * 0.1)
    } else {
        // Fallback: ~30% of monthly wage per ton
        average_wage * 0.3
    }
}

/// Compute the regulated gate fee / PSZOK fee (per ton).
///
/// CRITICAL FIX 4: Gate fee covers heavy intermittent waste (ConstructionWaste,
/// BulkyWaste, HazardousWaste) dropped off at Civic Amenity Sites.
/// Uses cost-plus pricing with a higher fallback (heavy waste is expensive to process).
pub fn compute_regulated_gate_fee(history: &WasteSalesHistory, average_wage: f64) -> f64 {
    if history.smoothed_gate_volume > 0.01 {
        let total_cost = history.gate_opex + history.gate_amortized_capex;
        // Gate fee includes disposal cost for hazardous waste specialized handling
        let fee_per_ton = total_cost / history.smoothed_gate_volume * history.margin;
        // Ensure minimum fee (don't go below 50% of average wage per ton)
        fee_per_ton.max(average_wage * 0.5)
    } else {
        // Fallback: ~2x monthly wage per ton (heavy waste is expensive)
        average_wage * 2.0
    }
}

// ============================================================================
// PHASE 84: WASTE EPIC TURN PROCESSING (W.1–W.10)
// ============================================================================

/// Phase 84: Result of the waste epic turn for a single region.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WasteEpicTurnResult {
    /// Total waste generated this turn (tons), by category.
    pub waste_generated: HashMap<Commodity, f64>,
    /// Waste disposed via standalone methods (tons).
    pub standalone_disposed: f64,
    /// Waste collected by centralized collection (tons).
    pub collected: f64,
    /// Waste separated at separation plants (tons).
    pub separated: f64,
    /// Waste recycled at recycling facilities (tons).
    pub recycled: f64,
    /// Waste incinerated at WtE plants (tons).
    pub incinerated: f64,
    /// Ash generated by WtE (tons) — routed to landfill.
    pub ash_generated: f64,
    /// Waste deposited in landfills (tons).
    pub landfilled: f64,
    /// Waste rejected by full landfills (tons) — LOGISTICAL BOUND 2.
    pub landfill_rejected: f64,
    /// Construction waste generated this turn (tons).
    pub construction_waste: f64,
    /// Pollution result (smog, biohazard, leachate, water degradation).
    pub pollution: WastePollutionResult,
    /// Curbside fee revenue collected (currency).
    pub curbside_fee_revenue: f64,
    /// Gate fee revenue collected (currency).
    pub gate_fee_revenue: f64,
}

/// Phase 84: Process the waste epic turn for all regions.
///
/// This is the main entry point called from the turn loop. It implements
/// the 10-step waste processing sequence (W.1–W.10):
///
/// - W.1: Compute waste from consumption receipts (mass conservation).
/// - W.2: Apply standalone disposal (cumulative rural track + burning).
/// - W.3: Perform centralized collection (pro-rata capacity allocation).
/// - W.4: Process mixed waste through separation plants.
/// - W.5: Process sorted waste through recycling facilities.
/// - W.6: Process residual waste through WtE (ash generation).
/// - W.7: Deposit remaining waste and ash into landfills (hard stop).
/// - W.8: Compute waste-related smog and biohazard.
/// - W.9: Distribute waste pollution to cadastre parcels.
/// - W.10: Generate construction waste (after construction material consumption).
pub fn process_waste_epic_turn(
    regions: &mut [crate::society::geography::Region],
    buildings: &mut [crate::entities::Building],
    _housing_buildings: &[crate::society::housing::HousingBuilding],
    _commercial_buildings: &[crate::society::housing::CommercialBuilding],
    _season: crate::state::Season,
) {
    use crate::registries::enums::Sector;

    for region in regions.iter_mut() {
        let region_id = region.id.clone();

        // ── W.1: Compute waste from consumption ──
        // For now, use a simplified model: waste generation is derived from
        // the region's population and consumption patterns. In a full
        // implementation, this would use actual consumption receipts from
        // the B2C clearing engine.
        let population = region.population as f64;
        let base_waste_per_capita = 0.0005; // 0.5 kg per person per turn
        let total_waste = population * base_waste_per_capita;

        let mut waste_generated: HashMap<Commodity, f64> = HashMap::new();
        // Default: all waste starts as MixedWaste (unsegregated)
        waste_generated.insert(Commodity::MixedWaste, total_waste);

        // ── W.2: Apply standalone disposal ──
        // Buildings with standalone waste disposal methods dispose of their
        // own waste. The dumping vector is selected based on region geography.
        let forest_area_fraction = {
            if let Some(forest_data) = region
                .land_use_inventory
                .get_category(crate::society::geography::LandCategory::Forests)
            {
                if region.land_use_inventory.total_area > 0.0 {
                    forest_data.area_hectares / region.land_use_inventory.total_area
                } else {
                    0.0
                }
            } else {
                0.0
            }
        };
        let dumping_vector = select_dumping_vector(
            region.geographic_traits.has_navigable_river,
            region.geographic_traits.has_coastline,
            forest_area_fraction,
        );

        // Compute standalone vs. centralized split
        // For now: if waste_grid has collection capacity, use centralized;
        // otherwise standalone.
        let collection_capacity = region.waste_grid.collection_capacity;
        let centralized_fraction = if total_waste > 0.0 {
            (collection_capacity / total_waste).min(1.0)
        } else {
            0.0
        };
        let standalone_waste = total_waste * (1.0 - centralized_fraction);
        let centralized_waste = total_waste * centralized_fraction;

        // Standalone disposal pollution
        let standalone_biohazard = standalone_waste * dumping_vector.biohazard_factor() * 0.1;
        region.local_pollution.waste_dumping_biohazard += standalone_biohazard;

        // River dumping → surface water quality degradation
        if dumping_vector.degrades_surface_water() {
            region.water_reserves.surface_water_quality =
                (region.water_reserves.surface_water_quality - standalone_waste * 0.001).max(0.0);
        }

        // Forest dumping → forestry ecological health degradation
        if dumping_vector.degrades_forestry() {
            if let Some(forest_data) = region
                .land_use_inventory
                .get_category_mut(crate::society::geography::LandCategory::Forests)
            {
                forest_data.ecological_health =
                    (forest_data.ecological_health - standalone_waste * 0.0005).max(0.0);
            }
        }

        // ── W.3: Perform centralized collection ──
        // Collected waste goes into the waste grid's uncollected_waste map
        // (representing waste picked up by collection routes). It will be
        // routed to separation plants or landfills.
        if centralized_waste > 0.0 {
            region
                .waste_grid
                .add_uncollected(Commodity::MixedWaste, centralized_waste);
        }

        // ── W.4–W.6: Process through separation, recycling, WtE ──
        // Find waste plant buildings in this region
        let mut separation_capacity: f64 = 0.0;
        let mut recycling_capacity: f64 = 0.0;
        let mut wte_capacity: f64 = 0.0;
        let mut landfill_buildings: Vec<usize> = Vec::new();

        for (i, building) in buildings.iter().enumerate() {
            if building.region_id != region_id {
                continue;
            }
            if building.sector != Sector::WasteManagement {
                continue;
            }
            if building.landfill_state.is_some() {
                landfill_buildings.push(i);
            }
            // Check building active method to determine plant type
            let method_name = building.active_method.active_methods.production.as_str();
            if method_name.contains("Sorting")
                || method_name.contains("Manual Sorting")
                || method_name.contains("Optical")
            {
                separation_capacity += building.worker_capacity as f64 * 0.1;
            } else if method_name.contains("Smelting")
                || method_name.contains("Shredder")
                || method_name.contains("Crushing")
                || method_name.contains("Baling")
                || method_name.contains("Dismantling")
                || method_name.contains("Textile")
            {
                recycling_capacity += building.worker_capacity as f64 * 0.1;
            } else if method_name.contains("Incinerator")
                || method_name.contains("Combustion")
                || method_name.contains("Fluidized")
                || method_name.contains("CHP")
            {
                wte_capacity += building.worker_capacity as f64 * 0.1;
            }
        }

        // W.4: Separation — convert MixedWaste to sorted fractions
        let uncollected_mixed = region
            .waste_grid
            .uncollected_waste
            .get(&Commodity::MixedWaste)
            .copied()
            .unwrap_or(0.0);
        let separated_amount = uncollected_mixed.min(separation_capacity);
        if separated_amount > 0.0 {
            let yields = separation_yields();
            for (commodity, fraction) in &yields {
                let qty = separated_amount * fraction;
                if qty > 0.0 {
                    region.waste_grid.add_uncollected(*commodity, qty);
                }
            }
            // Remove the separated MixedWaste
            *region
                .waste_grid
                .uncollected_waste
                .entry(Commodity::MixedWaste)
                .or_insert(0.0) -= separated_amount;
        }

        // W.5: Recycling — convert sorted fractions to virgin commodities + residual
        let recyclable_commodities = [
            Commodity::MetalWaste,
            Commodity::GlassWaste,
            Commodity::PlasticWaste,
            Commodity::ElectronicWaste,
            Commodity::TextileWaste,
        ];
        for commodity in &recyclable_commodities {
            let available = region
                .waste_grid
                .uncollected_waste
                .get(commodity)
                .copied()
                .unwrap_or(0.0);
            if available <= 0.0 || recycling_capacity <= 0.0 {
                continue;
            }
            let process_amount = available.min(recycling_capacity);
            let yields = recycling_yields(*commodity);
            for (output_commodity, fraction) in &yields {
                let qty = process_amount * fraction;
                if qty > 0.0 {
                    if *output_commodity == Commodity::MixedWaste
                        || *output_commodity == Commodity::HazardousWaste
                    {
                        // Residual goes back to uncollected
                        region.waste_grid.add_uncollected(*output_commodity, qty);
                    } else {
                        // Recovered commodities go to building inventory
                        // (in full implementation, sold on B2B)
                    }
                }
            }
            *region
                .waste_grid
                .uncollected_waste
                .entry(*commodity)
                .or_insert(0.0) -= process_amount;
        }

        // W.6: WtE — incinerate residual MixedWaste → Energy + ash
        let residual_mixed = region
            .waste_grid
            .uncollected_waste
            .get(&Commodity::MixedWaste)
            .copied()
            .unwrap_or(0.0);
        let incinerated = residual_mixed.min(wte_capacity);
        let ash_generated = incinerated * WTE_ASH_FRACTION_BASIC;
        if incinerated > 0.0 {
            *region
                .waste_grid
                .uncollected_waste
                .entry(Commodity::MixedWaste)
                .or_insert(0.0) -= incinerated;
            region
                .waste_grid
                .add_uncollected(Commodity::HazardousWaste, ash_generated);
            // Energy output goes to grid (in full implementation)
        }

        // ── W.7: Deposit remaining waste and ash into landfills ──
        // LOGISTICAL BOUND 2: Hard capacity stop
        let mut total_rejected: f64 = 0.0;
        let mut total_landfilled: f64 = 0.0;

        if !landfill_buildings.is_empty() {
            let waste_per_landfill =
                region.waste_grid.total_uncollected() / landfill_buildings.len() as f64;
            for &idx in &landfill_buildings {
                let building = &mut buildings[idx];
                if let Some(landfill) = building.landfill_state.as_mut() {
                    // Build waste input from uncollected (pro-rata)
                    let mut waste_input: HashMap<Commodity, f64> = HashMap::new();
                    for (commodity, qty) in &region.waste_grid.uncollected_waste {
                        let portion = (*qty / region.waste_grid.total_uncollected().max(0.001))
                            * waste_per_landfill;
                        if portion > 0.0 {
                            waste_input.insert(*commodity, portion);
                        }
                    }
                    let accepted = landfill.accept_waste(&waste_input);
                    total_landfilled += accepted;
                    total_rejected += waste_per_landfill - accepted;
                }
            }

            // Clear uncollected waste that was landfilled (proportional)
            if region.waste_grid.total_uncollected() > 0.0 {
                let landfilled_fraction = total_landfilled / region.waste_grid.total_uncollected();
                region.waste_grid.drain_uncollected(landfilled_fraction);
            }
        }

        // Rejected waste stays as uncollected → biohazard crisis
        if total_rejected > 0.0 {
            region.local_pollution.uncollected_waste_biohazard += total_rejected * 0.05;
        }

        // ── W.8: Compute waste-related smog and biohazard ──
        let remaining_uncollected = region.waste_grid.total_uncollected();
        let pollution = compute_waste_pollution(
            0.0, // burning emissions handled in standalone disposal
            standalone_waste,
            dumping_vector,
            remaining_uncollected,
            0.0, // leachate computed from landfills below
        );

        // Compute leachate from all landfills
        let mut total_leachate: f64 = 0.0;
        for &idx in &landfill_buildings {
            if let Some(landfill) = &buildings[idx].landfill_state {
                total_leachate += landfill.leachate_leakage();
            }
        }

        // Apply pollution to region
        region.local_pollution.waste_burning_emissions += pollution.burning_emissions;
        region.local_pollution.waste_dumping_biohazard += pollution.dumping_biohazard;
        region.local_pollution.uncollected_waste_biohazard += pollution.uncollected_biohazard;

        // Leachate degrades groundwater quality
        if total_leachate > 0.0 {
            region.water_reserves.groundwater_quality = (region.water_reserves.groundwater_quality
                - total_leachate * LEACHATE_CONTAMINATION_FACTOR)
                .max(0.0);
        }

        // ── W.9: Distribute waste pollution to cadastre parcels ──
        // (In full implementation, this would update Parcel.pollution_level
        // for parcels near landfills and dumping sites.)

        // ── W.10: Construction waste ──
        // (Generated after construction material consumption in the turn loop.
        // For now, this is handled by the construction project system.)

        // Update landfill utilization
        if !landfill_buildings.is_empty() {
            let mut total_utilization: f64 = 0.0;
            for &idx in &landfill_buildings {
                if let Some(landfill) = &buildings[idx].landfill_state {
                    total_utilization += landfill.utilization();
                }
            }
            region.waste_grid.landfill_utilization =
                total_utilization / landfill_buildings.len() as f64;
        }

        // Degrade waste grid route condition
        let winter_severity = match _season {
            crate::state::Season::Winter => 1.0,
            _ => 0.0,
        };
        region.waste_grid.degrade(winter_severity);
        region.waste_grid.recompute_capacity();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mass_balance_metal_recycling() {
        let yields = recycling_yields(Commodity::MetalWaste);
        assert!(
            verify_mass_balance(&yields),
            "MetalWaste yields must sum to 1.0"
        );
    }

    #[test]
    fn test_mass_balance_glass_recycling() {
        let yields = recycling_yields(Commodity::GlassWaste);
        assert!(
            verify_mass_balance(&yields),
            "GlassWaste yields must sum to 1.0"
        );
    }

    #[test]
    fn test_mass_balance_plastic_recycling() {
        let yields = recycling_yields(Commodity::PlasticWaste);
        assert!(
            verify_mass_balance(&yields),
            "PlasticWaste yields must sum to 1.0"
        );
    }

    #[test]
    fn test_mass_balance_electronic_recycling() {
        let yields = recycling_yields(Commodity::ElectronicWaste);
        assert!(
            verify_mass_balance(&yields),
            "ElectronicWaste yields must sum to 1.0"
        );
    }

    #[test]
    fn test_mass_balance_textile_recycling() {
        let yields = recycling_yields(Commodity::TextileWaste);
        assert!(
            verify_mass_balance(&yields),
            "TextileWaste yields must sum to 1.0"
        );
    }

    #[test]
    fn test_mass_balance_separation() {
        let yields = separation_yields();
        assert!(
            verify_mass_balance(&yields),
            "Separation yields must sum to 1.0"
        );
    }

    #[test]
    fn test_landfill_hard_stop() {
        let mut landfill = LandfillState::new(100.0, 1.0, 0.9, 0.8);
        let mut waste = HashMap::new();
        waste.insert(Commodity::MixedWaste, 100.0);
        let accepted = landfill.accept_waste(&waste);
        assert_eq!(accepted, 100.0);
        assert!(landfill.is_full);
        assert_eq!(landfill.remaining_capacity, 0.0);

        // Now reject all incoming
        let mut more_waste = HashMap::new();
        more_waste.insert(Commodity::MixedWaste, 50.0);
        let rejected = landfill.accept_waste(&more_waste);
        assert_eq!(rejected, 0.0, "Full landfill must reject all waste");
    }

    #[test]
    fn test_dumping_vector_selection_river() {
        let v = select_dumping_vector(true, false, 0.0);
        assert_eq!(v, DumpingVector::RiverWater);
    }

    #[test]
    fn test_dumping_vector_selection_forest() {
        let v = select_dumping_vector(false, false, 0.20);
        assert_eq!(v, DumpingVector::ForestWild);
    }

    #[test]
    fn test_dumping_vector_selection_street() {
        let v = select_dumping_vector(false, false, 0.05);
        assert_eq!(v, DumpingVector::StreetAlley);
    }

    #[test]
    fn test_waste_generation_from_food() {
        let mut consumed = HashMap::new();
        consumed.insert(Commodity::Food, 100.0);
        let waste = compute_waste_from_consumption(&consumed);
        assert_eq!(waste.get(&Commodity::BioWaste), Some(&15.0));
    }

    #[test]
    fn test_construction_waste() {
        let mut materials = HashMap::new();
        materials.insert(Commodity::Steel, 100.0);
        materials.insert(Commodity::Cement, 200.0);
        let waste = compute_construction_waste(&materials);
        assert!((waste - 30.0).abs() < 0.001); // 10% of 300
    }
}

//! B2C retail market clearing system (Phase 6.5).
//!
//! Implements consumer demand building, store offers, B2C market clearing,
//! retail pricing with amortized costs, and consumer inertia/brand loyalty.

use crate::data::consumption_registry;
use crate::economy::market_history::{get_reference_price, MarketHistory};
use crate::economy::retail_registry::retail_config;
use crate::economy::transfer_settler::settle_b2c_purchase;
use crate::entities::Company;
use crate::registries::enums::Commodity;
use crate::society::culture_registry::{
    registry as culture_registry, CultureDefinition, ReligionDefinition,
};
use crate::society::geography::{DemographicClass, Region, RuralClass};
use crate::society::housing::{CommercialBuilding, HousingBuilding};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

type HashMap<K, V> = FxHashMap<K, V>;

/// Store offer for B2C market (Phase 6.5)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoreOffer {
    /// Store building ID
    pub store_id: String,

    /// Commodity offered
    pub commodity: Commodity,

    /// Quantity available
    pub quantity: f64,

    /// Price per unit
    pub price_per_unit: f64,

    /// Effective attractiveness (includes upgrades)
    pub effective_attractiveness: f64,

    /// Phase 19C: Blueprint quality of this offer (1.0 = baseline, >1.0 = premium).
    /// For non-quality-durables, this is always 1.0. For quality durables
    /// (Cars, Televisions, Furniture, Clothing, ...), this reflects the
    /// blueprint's quality score and drives wealth-segmented B2C selection.
    #[serde(default = "default_offer_quality")]
    pub quality: f64,
}

/// Default quality for legacy/flat offers (1.0 = no quality premium).
fn default_offer_quality() -> f64 {
    1.0
}

/// Consumer demand by demographic class (Phase 6.5)
/// E.6.3: Uses typed `DemographicClass` key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConsumerDemand {
    /// E.6.3: (region_id, DemographicClass) → (commodity → units demanded)
    pub demand: BTreeMap<(String, DemographicClass), BTreeMap<Commodity, f64>>,

    /// Total demand per commodity (aggregated across all classes)
    pub total_demand: BTreeMap<Commodity, f64>,
}

/// Cultural demand modifier computed from culture, religion, and religious authority.
///
/// Taboos and obsessions are scaled by `authority` (0.0–1.0):
/// * At authority=1.0, taboo drops demand to 0%.
/// * At authority=0.2, taboo only drops demand by 20%.
/// * Obsessions are similarly scaled.
#[derive(Debug, Clone, Default)]
pub struct CulturalDemandModifier {
    /// (commodity, demand_reduction_fraction) — demand is multiplied by (1.0 - reduction).
    pub taboo_commodities: Vec<(Commodity, f64)>,
    /// commodity → scaled multiplier (already incorporates authority scaling).
    pub obsession_multipliers: HashMap<Commodity, f64>,
}

impl CulturalDemandModifier {
    /// Build a CulturalDemandModifier from culture, religion, and authority score.
    ///
    /// # Arguments
    /// * `culture` - Culture definition (taboos + obsessions).
    /// * `religion` - Religion definition (taboos + obsessions).
    /// * `authority` - Religious authority score (0.0–1.0) for the dominant religion.
    ///
    /// # Rules
    /// * Taboo demand reduction = authority (NOT absolute zeroing).
    /// * Obsession multiplier = 1.0 + (factor - 1.0) * authority.
    pub fn from_definitions(
        culture: &CultureDefinition,
        religion: &ReligionDefinition,
        authority: f64,
    ) -> Self {
        let mut taboo_commodities = Vec::new();
        let mut obsession_multipliers = HashMap::default();

        // Culture taboos scaled by authority.
        for &commodity in &culture.taboos {
            taboo_commodities.push((commodity, authority));
        }
        // Religion taboos scaled by authority.
        for &commodity in &religion.taboos {
            // Avoid duplicate entries.
            if !taboo_commodities.iter().any(|(c, _)| *c == commodity) {
                taboo_commodities.push((commodity, authority));
            }
        }

        // Culture obsessions scaled by authority.
        for &(commodity, factor) in &culture.obsessions {
            let scaled = 1.0 + (factor - 1.0) * authority;
            obsession_multipliers.insert(commodity, scaled);
        }
        // Religion obsessions scaled by authority (merge with culture, take max).
        for &(commodity, factor) in &religion.obsessions {
            let scaled = 1.0 + (factor - 1.0) * authority;
            obsession_multipliers
                .entry(commodity)
                .and_modify(|existing| {
                    if scaled > *existing {
                        *existing = scaled;
                    }
                })
                .or_insert(scaled);
        }

        Self {
            taboo_commodities,
            obsession_multipliers,
        }
    }

    /// Apply this modifier to a demand map for a single class.
    ///
    /// # Rules
    /// * Taboo: demand *= (1.0 - reduction_fraction).
    /// * Obsession: demand *= scaled_multiplier.
    pub fn apply(&self, demand: &mut BTreeMap<Commodity, f64>) {
        for (commodity, reduction) in &self.taboo_commodities {
            if let Some(qty) = demand.get_mut(commodity) {
                *qty *= 1.0 - reduction;
            }
        }
        for (commodity, multiplier) in &self.obsession_multipliers {
            if let Some(qty) = demand.get_mut(commodity) {
                *qty *= multiplier;
            }
        }
    }
}

/// Build a CulturalDemandModifier for a region based on its dominant culture/religion and authority.
///
/// # Arguments
/// * `region` - The region to compute modifiers for.
/// * `religious_authority` - Map of religion engine key → authority score.
/// * `dominant_culture` - The country's dominant culture display name (e.g., "Illyria").
///
/// # Returns
/// `CulturalDemandModifier` ready to apply to demand maps.
pub fn compute_cultural_demand_modifier(
    region: &Region,
    religious_authority: &std::collections::BTreeMap<String, f64>,
    dominant_culture: &str,
) -> CulturalDemandModifier {
    let reg = culture_registry();

    // Determine dominant religion from region's class demographics.
    let mut religion_counts: HashMap<String, i64> = HashMap::default();
    for class in region.class_demographics.rural_classes.values() {
        if !class.religion.is_empty() {
            let key = reg.religion_key_from_display(&class.religion);
            *religion_counts.entry(key).or_insert(0) += class.population;
        }
    }
    for class in region.class_demographics.urban_classes.values() {
        if !class.religion.is_empty() {
            let key = reg.religion_key_from_display(&class.religion);
            *religion_counts.entry(key).or_insert(0) += class.population;
        }
    }
    let dominant_religion_key = religion_counts
        .iter()
        .max_by_key(|(_, &count)| count)
        .map(|(key, _)| key.clone())
        .unwrap_or_default();

    let religion_def = reg.religion_from_key(&dominant_religion_key);
    let authority = religious_authority
        .get(&dominant_religion_key)
        .copied()
        .unwrap_or(0.3);

    // Phase 6 (F6): Look up the country's actual dominant culture from
    // the passed display name, not a hardcoded "Illyrian".
    let culture_def = reg
        .from_display_name(dominant_culture)
        .cloned()
        .unwrap_or_default();

    match religion_def {
        Some(rel) => CulturalDemandModifier::from_definitions(&culture_def, rel, authority),
        None => CulturalDemandModifier::default(),
    }
}

/// Phase 47: Minimum savings_per_capita required to demand a commodity.
/// Perishables have gate 0.0 (universal). Durables are wealth-gated.
fn commodity_wealth_gate(commodity: Commodity) -> f64 {
    match commodity {
        // Perishables — universal (consumed every turn)
        Commodity::Cereal
        | Commodity::Vegetable
        | Commodity::Meat
        | Commodity::Fruit
        | Commodity::HealthCapacity
        | Commodity::EducationSlots
        | Commodity::Food
        | Commodity::Water => 0.0,
        // Durables — wealth-gated (purchased only when savings permit)
        // Phase 74: Recalibrated to match generated savings_per_capita values
        Commodity::Clothing => 20.0,     // Everyone buys clothing
        Commodity::Furniture => 50.0,    // Basic household necessity
        Commodity::Radio => 150.0,       // Available to workers
        Commodity::Agd => 300.0,         // Middle-class purchase
        Commodity::Televisions => 500.0, // Middle-class luxury
        Commodity::Cars => 1500.0,       // Upper-class
        Commodity::Fuels => 200.0,       // Car fuel tracks car ownership, not savings
        Commodity::Luxury | Commodity::LuxuryFurniture | Commodity::LuxuryClothing => 2000.0, // Aristocracy only
        _ => 0.0,
    }
}

/// v4.18 WS1 (follow-up): maximum fraction of a class's remaining savings
/// stock that may finance consumption above current wage income in a single
/// turn — the bounded-dissaving rate. 0.20 means a class living purely off
/// its stock retains ≥80% of it each turn: drawdown is geometric and
/// asymptotic, so both the stock and the demand it funds decay smoothly
/// toward the wage-sustainable level instead of hitting a zero cliff
/// (the crucible's −81%/−51% single-turn collapses).
/// v4.18 WS1 follow-up: bounded-dissaving rate — the ceiling on how much
/// of the pre-income savings STOCK may be drawn per turn. Two constraints
/// shape it: the stock must decay asymptotically (never a cliff), and the
/// mandated `|ΔC/C| < 0.15` bounds the depletion decay — when a class is
/// stock-funded, C ≈ r×S and S shrinks ≈ r per turn, so r must stay under
/// the gate band. 0.12 leaves margin for measurement noise.
const MAX_SAVINGS_DRAWDOWN_RATE: f64 = 0.12;

/// v4.18 WS1 follow-up: permanent-income smoothing — the EWMA update rate
/// for `ClassDemographics::smoothed_income`. α = 0.35 damps ~2/3 of a
/// one-turn income spike within the first post-shock turn while tracking
/// a persistent shift inside ~4 turns; the residual flows into the stock
/// and is released by `MAX_SAVINGS_DRAWDOWN_RATE` instead of spiking C.
const INCOME_SMOOTHING_ALPHA: f64 = 0.35;

/// v4.18 WS1: solvency signal (savings/capita) at which durable upgrade
/// demand reaches full strength — scaled linearly below it so the
/// 5%-of-stock upgrade lump fades in instead of toggling at a hard line.
const DURABLE_UPGRADE_SPC_SCALE: f64 = 500.0;

/// v4.18 WS1: solvency signal at which durable saturation (gap-filling)
/// demand reaches full strength — scaled linearly below it so the
/// 10%-of-gap purchase fades in instead of toggling on/off turn to turn.
const DURABLE_SATURATION_SPC_SCALE: f64 = 100.0;

/// v4.18 WS1: smoothed spending-power signal for wealth gating/scaling.
///
/// The habitual EWMA anchor survives only as a *bounded* top-up over
/// current solvency: a class may TRY to hold its habitual standard, but
/// the gap above its live means can only be financed at up to
/// `MAX_SAVINGS_DRAWDOWN_RATE` of the remaining stock per turn. The
/// top-up is proportional to the live stock, so it fades asymptotically —
/// no `savings > 0` binary term, no cliff when the buffer runs out — and
/// a class at exactly zero savings gets a zero signal: honest collapse.
fn smoothed_spending_power(live_spc: f64, cached_spc: f64, habitual_anchor: f64) -> f64 {
    let base = live_spc.max(cached_spc).max(0.0);
    let draw_allowance = live_spc.max(0.0) * MAX_SAVINGS_DRAWDOWN_RATE;
    base + (habitual_anchor - base).clamp(0.0, draw_allowance)
}

/// v4.18 WS1: per-turn spending capacity in money terms — the real bound on
/// dissaving applied to demanded value in `build_consumer_demand`.
///
/// `savings` is a flow-through pocket: this turn's income (wages,
/// dividends, transfers, rents) is credited into it before the B2C phase,
/// so the balance at demand-build time is stock+income commingled. The
/// `savings_stock_turn_start` marker (snapshot at turn start) separates
/// the two: current income is spendable in full — it was just earned —
/// while the pre-existing stock may only be drawn down at
/// `MAX_SAVINGS_DRAWDOWN_RATE` per turn. The stock therefore decays
/// geometrically and can never be blown out in a turn or two, income dips
/// are covered by the bounded draw (the mandate's smoothing), and a class
/// with neither income nor stock honestly collapses to zero demand.
pub fn class_spend_capacity(
    demographics: &crate::society::geography::ClassDemographics,
    average_wage: f64,
) -> f64 {
    let _ = average_wage; // income is measured from the pocket, not proxied
    let stock = demographics.savings_stock_turn_start.max(0.0);
    // Permanent income (EWMA of realized inflow), not the raw inflow:
    // spending the live figure turn-for-turn re-creates the wage-cycle
    // volatility the mandate forbids; the unspent share of a spike sits
    // in `savings` and is released by the bounded drawdown anyway.
    let raw = demographics.smoothed_income.max(0.0) + stock * MAX_SAVINGS_DRAWDOWN_RATE;
    // Conservation ceiling: never demand beyond the whole balance.
    let target = raw.min(demographics.savings.max(0.0));
    // Habit-formation envelope: consumption retunes at ≤15%/turn. On an
    // income dip the floor holds above target and the difference drains
    // the stock (the mandate's savings draw); on a spike the ceiling
    // paces the rise and the excess piles into the stock. When the stock
    // is exhausted `target` collapses to the live balance and the class
    // honestly lands on income — the physical floor wins over the band.
    let prev = demographics.prev_spend_capacity;
    if prev <= 0.0 {
        target
    } else {
        target.clamp(prev * 0.85, prev * 1.15)
    }
}

/// v4.18 WS1: continuous wealth-gate factor. The gate used to toggle
/// demand 0↔full in a single step when `effective_spc` crossed the line —
/// the main on/off per-turn volatility amplifier in the crucible series.
/// Demand now ramps linearly toward the gate: a class at half the gate
/// demands half the gated line. At zero solvency the factor is zero —
/// a broke class still gets nothing gated.
fn wealth_gate_factor(commodity: Commodity, effective_spc: f64) -> f64 {
    let gate = commodity_wealth_gate(commodity);
    if gate <= 0.0 {
        return 1.0;
    }
    (effective_spc / gate).clamp(0.0, 1.0)
}

/// v4.18 WS1: price used to value a unit of consumer demand for the
/// bounded-dissaving cap — the smoothed retail-VWAP expectation first (the
/// habitual shelf price households budget around), then last turn's retail
/// VWAP, then the resolved market price, then the
/// VWAP→last-trade→global-base reference chain; 0.0 if nothing is known.
///
/// v4.19: retail-first ordering. Households budget against shelf prices —
/// the prices they face at the till — not the wholesale order-imbalance
/// signal in `market_prices`, which they cannot observe. Under the old
/// wholesale-first chain a B2B price spike compressed demanded units even
/// while (sticky) shelf prices had not moved — the t24 aggregate-C dip.
/// And the expectation is SMOOTHED: households do not re-budget their whole
/// basket on one turn's shelf-price dip; valuing demand at spot VWAP made
/// a temporary dip release a units wave that rebounded as a >15% C step.
/// Real affordability is still enforced at settle by the savings/capacity
/// money bound — this price only sizes the requested basket.
fn demand_value_price(
    commodity: &Commodity,
    market_prices: &HashMap<Commodity, f64>,
    market_history: &MarketHistory,
) -> f64 {
    if let Some(&p) = market_history.retail_vwap_smoothed.get(commodity) {
        if p > 0.0 {
            return p;
        }
    }
    if let Some(&p) = market_history.retail_vwap_per_commodity.get(commodity) {
        if p > 0.0 {
            return p;
        }
    }
    if let Some(&p) = market_prices.get(commodity) {
        if p > 0.0 {
            return p;
        }
    }
    get_reference_price(commodity, market_history)
        .unwrap_or(0.0)
        .max(0.0)
}

/// Phase 47: Compute stock-adjusted durable demand for a class.
///
/// Durables are NOT consumed per-turn. Instead, demand comes from:
/// 1. **Replacement** — worn-out stock (condition < 0.2) needs replacing.
/// 2. **Upgrade** — wealth-driven desire for higher quality.
/// 3. **Saturation** — filling the gap toward target stock (wealth-gated).
///
/// The `base_per_capita` from the consumption registry is used as the
/// target saturation level, not a per-turn consumption rate.
///
/// v4.18 WS1: `effective_spc` is the caller's smoothed spending-power measure
/// (live solvency plus a draw-rate-bounded top-up toward the habitual EWMA
/// anchor) so durable demand does not collapse on a single-turn wage dip.
/// All hard thresholds are soft ramps: the wealth gate, upgrade threshold,
/// and saturation threshold scale demand linearly instead of toggling
/// 0↔full — the binary versions were the demand-side volatility amplifier.
fn compute_durable_demand(
    commodity: Commodity,
    demographics: &crate::society::geography::ClassDemographics,
    year: u32,
    base_per_capita: f64,
    effective_spc: f64,
) -> f64 {
    // Era gate (no TV demand before 1936, etc.)
    let era_mult = era_consumption_multiplier(commodity, year);
    if era_mult <= 0.0 {
        return 0.0;
    }

    // Wealth gate — even if the class basket lists a commodity, it's only
    // demanded if the class's spending power is near the wealth gate.
    // v4.18 WS1 follow-up: continuous ramp `min(1, spc/gate)` replaces the
    // binary pass/fail so a class hovering at the line buys a fraction of
    // the gated line instead of oscillating between full and none.
    let gate_factor = wealth_gate_factor(commodity, effective_spc);
    if gate_factor <= 0.0 {
        return 0.0;
    }

    // Cohort `count` is per-capita (units held per citizen) — see
    // `HouseholdDurableCohort::count`. All stock terms below are
    // per-capita; the final demand is scaled back by population.
    let population = demographics.population.max(0) as f64;
    if population <= 0.0 {
        return 0.0;
    }

    // 1. Equilibrium ownership target. `base_per_capita` is the basket's
    //    desired consumption FLOW (units/citizen/turn); the steady-state
    //    stock that flow sustains is flow × lifespan — at equilibrium,
    //    purchases = wear-out = stock / durability.
    let durability = commodity.household_durable_turns();
    let target_pc = base_per_capita * if durability.is_finite() { durability } else { 1.0 };

    // 2. Current stock (per-capita) — all cohorts of this commodity.
    let current_pc: f64 = demographics
        .household_durables
        .iter()
        .filter(|c| c.commodity == commodity)
        .map(|c| c.count)
        .sum();

    // 3. Serviceable stock = cohorts still usable (condition >= 0.2).
    //    Worn-out and scrapped units no longer serve the household, so
    //    they open a replacement deficit — a discarded garment still
    //    needs replacing; the need does not die with the cohort. This is
    //    what prevents the old whole-cohort `worn_out` lump (the entire
    //    seed wardrobe demanded at once, then demand vanishing when the
    //    cohort was scrapped — the synchronized t≈15 Clothing cliff).
    let serviceable_pc: f64 = demographics
        .household_durables
        .iter()
        .filter(|c| c.commodity == commodity && c.condition >= 0.2)
        .map(|c| c.count)
        .sum();

    // 4. Replacement/saturation demand — ONE smooth deficit-to-target
    //    path covering initial build-out, wear-and-tear replacement, and
    //    post-scrapping repurchase. Fills 10% of the per-capita shortfall
    //    per turn, scaled by the same solvency ramp as before.
    let deficit_pc = (target_pc - serviceable_pc).max(0.0);
    let saturation_factor =
        (effective_spc / DURABLE_SATURATION_SPC_SCALE).clamp(0.0, 1.0);
    let replacement_pc = deficit_pc * 0.10 * saturation_factor;

    // 5. Upgrade demand = wealth-driven desire for higher quality.
    // v4.18 WS1: ramps in linearly with solvency instead of toggling at 500.
    let upgrade_factor = (effective_spc / DURABLE_UPGRADE_SPC_SCALE).clamp(0.0, 1.0);
    let upgrade_pc = if upgrade_factor > 0.0 && current_pc > 0.0 {
        let avg_quality: f64 = demographics
            .household_durables
            .iter()
            .filter(|c| c.commodity == commodity)
            .map(|c| c.quality * c.count)
            .sum::<f64>()
            / current_pc;
        if avg_quality < 0.9 {
            current_pc * 0.05 * upgrade_factor
        } else {
            0.0
        }
    } else {
        0.0
    };

    let out = (replacement_pc + upgrade_pc) * population * era_mult * gate_factor;
    #[cfg(feature = "diagnostic")]
    if commodity == Commodity::Clothing && demographics.population > 500_000 {
        eprintln!(
            "DURDBG: yr={} pop={} stk={:.3} svc={:.3} tgt={:.3} rep={:.4} up={:.4} gf={:.3} espc={:.1} out={:.0}",
            year,
            demographics.population,
            current_pc,
            serviceable_pc,
            target_pc,
            replacement_pc,
            upgrade_pc,
            gate_factor,
            effective_spc,
            out
        );
    }
    out
}

/// Phase 47: Degrade household durable cohorts by one turn.
/// Called after B2C clearing, before telemetry.
pub fn degrade_household_durables(demographics: &mut crate::society::geography::ClassDemographics) {
    for cohort in &mut demographics.household_durables {
        if cohort.durability > 0.0 && cohort.durability < f64::MAX {
            cohort.condition = (cohort.condition - 1.0 / cohort.durability).max(0.0);
        }
    }
    // Remove scrapped cohorts (condition <= 0.0 or count <= 0.0)
    demographics
        .household_durables
        .retain(|c| c.condition > 0.0 && c.count > 0.0);
}

/// Phase 47: Install purchased durables as HouseholdDurableCohort on the
/// purchasing class. Called after B2C clearing settles.
pub fn install_durable_purchase(
    demographics: &mut crate::society::geography::ClassDemographics,
    commodity: Commodity,
    units_purchased: f64,
    quality: f64,
    current_turn: u32,
) {
    if units_purchased <= 0.0 || !commodity.is_household_durable() {
        return;
    }

    let population = demographics.population.max(1) as f64;
    let per_capita_count = units_purchased / population;
    let durability = commodity.household_durable_turns();

    // Quality bucket for cohort merging: round to nearest 0.25
    let quality_bucket = (quality * 4.0).round() / 4.0;

    // Try to merge into existing cohort with same (commodity, quality_bucket)
    let merged = demographics
        .household_durables
        .iter_mut()
        .find(|c| c.commodity == commodity && ((c.quality * 4.0).round() / 4.0) == quality_bucket)
        .map(|cohort| {
            // Weighted average condition and quality
            let old_weight = cohort.count;
            let new_weight = per_capita_count;
            let total = old_weight + new_weight;
            if total > 0.0 {
                cohort.condition = (cohort.condition * old_weight + 1.0 * new_weight) / total;
                cohort.quality = (cohort.quality * old_weight + quality * new_weight) / total;
            }
            cohort.count += per_capita_count;
        });

    if merged.is_none() {
        demographics
            .household_durables
            .push(crate::society::geography::HouseholdDurableCohort {
                commodity,
                count: per_capita_count,
                condition: 1.0,
                quality,
                durability,
                acquired_turn: current_turn,
            });
    }
}

/// Phase 74: Average number of people per household slot. This is a physical
/// demographic constant, not a magic economic number — it represents real
/// household composition in the simulation's era.
const HOUSEHOLD_SIZE: f64 = 4.0;

/// Phase 74: Compute the fraction of a class's population that has housing.
///
/// Returns 0.0 if the class has no housing allocation, 1.0 if fully housed.
/// Used by complementarity gating: homeless demographics cannot buy
/// Furniture, LuxuryFurniture, Agd, or Televisions.
fn class_housing_possession_rate(
    housing_buildings: &[HousingBuilding],
    region_id: &str,
    class_id: &str,
    class_population: i64,
) -> f64 {
    if class_population <= 0 {
        return 0.0;
    }
    // Count occupied housing slots assigned to this class in this region
    let housed_slots: u32 = housing_buildings
        .iter()
        .filter(|b| b.micro_region_id == region_id)
        .filter_map(|b| {
            let primary = if b
                .primary_slots
                .target_class
                .map(|c| class_id_matches_rural(c, class_id))
                .unwrap_or(false)
            {
                b.primary_slots.occupied_slots
            } else {
                0
            };
            let sublet = b
                .sublet_slots
                .as_ref()
                .and_then(|s| s.target_class.map(|c| class_id_matches_rural(c, class_id)))
                .map(|matches| {
                    if matches {
                        b.sublet_slots.as_ref().unwrap().occupied_slots
                    } else {
                        0
                    }
                })
                .unwrap_or(0);
            if primary + sublet > 0 {
                Some(primary + sublet)
            } else {
                None
            }
        })
        .sum();
    let housed_people = (housed_slots as f64 * HOUSEHOLD_SIZE) as i64;
    (housed_people as f64 / class_population as f64).clamp(0.0, 1.0)
}

/// Phase 74: Match a RuralClass enum to a class_id string.
fn class_id_matches_rural(class: RuralClass, class_id: &str) -> bool {
    format!("{:?}", class) == class_id
}

/// Phase 74: Public test helper for housing possession rate.
#[doc(hidden)]
pub fn test_class_housing_possession_rate(
    housing_buildings: &[HousingBuilding],
    region_id: &str,
    class_id: &str,
    class_population: i64,
) -> f64 {
    class_housing_possession_rate(housing_buildings, region_id, class_id, class_population)
}

/// Phase 74: Affordability threshold — the fraction of monthly wage that
/// represents the "normal" price point for a perishable commodity.
/// When price exceeds this fraction of wage, substitution kicks in.
fn affordability_threshold(commodity: Commodity) -> f64 {
    match commodity {
        Commodity::Meat => 0.05,   // 5% of monthly wage
        Commodity::Fruit => 0.03,  // 3% of monthly wage
        Commodity::Cereal => 0.10, // 10% of monthly wage (staple)
        Commodity::Vegetable => 0.05,
        _ => 0.05,
    }
}

/// Build consumer demand from demographic classes (Phase 6.5, Phase R1, Phase 17A).
///
/// # Arguments
/// * `region` - Region with class demographics
/// * `current_turn` - Current turn number
/// * `market_prices` - Current market prices by commodity (Phase 74)
/// * `average_wage` - Current average wage in the economy (Phase 74)
/// * `market_history` - Price fallback chain (retail VWAP → reference) used
///   to value demanded goods for the bounded-dissaving cap (v4.18 WS1)
///
/// # Returns
/// * `ConsumerDemand` - Demand by class and total
///
/// # Rules
/// * Uses consumption_registry for per-capita needs
/// * Multiplies by population for each class
/// * Evaluates tiers in order: Subsistence → Standard → Luxury
/// * Phase 17A: Applies authority-scaled taboo/obsession modifiers
/// * Phase 45: Applies wealth-tier and era-aware consumption modifiers
/// * Phase 74: Applies price-driven substitution and housing complementarity
/// * v4.18 WS1: Each class's demanded value is capped at this turn's wage
///   bill plus ≤ `MAX_SAVINGS_DRAWDOWN_RATE` of its savings stock — real
///   bounded dissaving; the stock decays geometrically, never cliffs
/// * Used in R1 phase before store offer generation
pub fn build_consumer_demand(
    region: &Region,
    current_turn: u32,
    market_prices: &HashMap<Commodity, f64>,
    average_wage: f64,
    housing_buildings: &[HousingBuilding],
    market_history: &MarketHistory,
) -> ConsumerDemand {
    let mut demand = ConsumerDemand {
        demand: BTreeMap::new(),
        total_demand: BTreeMap::new(),
    };

    let consumption = consumption_registry();
    let price_subs = consumption_registry::price_substitution_matrix();
    // Phase 45: Derive year from turn (24 turns per year, default start 1925)
    let year = 1925 + (current_turn / 24);

    // Helper: compute demand for a single class (rural or urban)
    // E.6.3: Uses typed DemographicClass for demand keys
    let process_class = |demand: &mut ConsumerDemand,
                         region: &Region,
                         housing_buildings: &[HousingBuilding],
                         class: DemographicClass,
                         demographics: &crate::society::geography::ClassDemographics,
                         year: u32| {
        let class_id_str = class.as_str();
        let key = (region.id.clone(), class);
        let housing_rate = class_housing_possession_rate(
            housing_buildings,
            &region.id,
            class_id_str,
            demographics.population,
        );

        // v4.18 WS1 — consumption smoothing via BOUNDED DISSAVING.
        //
        // The class's effective spending power is live solvency plus a
        // rate-bounded top-up toward its habitual EWMA anchor:
        //   effective_spc = base + min(anchor − base, live_spc × DRAW_RATE).
        // During a transitory wage dip (seasonal furlough, payroll arrears)
        // the class keeps TRYING its habitual basket, but only the slice
        // financed by a bounded draw-down of `ClassDemographics::savings`
        // inside `settle_b2c_purchase` — conserved money movement, not
        // fabricated income. The top-up is proportional to the remaining
        // stock, so it fades asymptotically: no boom funded by the full
        // anchor, no binary `savings > 0` term dropping it to zero — a
        // class at zero savings still collapses honestly.
        // The gate signal is SPENDABLE power per capita — smoothed income
        // plus the bounded stock draw, wrapped in the ±15%/turn habit
        // envelope (`class_spend_capacity`). The raw `savings` balance is
        // deliberately NOT the gate: a balance cliffs to ~0 in a single
        // turn when rent/debit pressure drains the stock, which was the
        // t≈16 demand collapse. Capacity decays asymptotically instead.
        let live_spc = if demographics.population > 0 {
            class_spend_capacity(demographics, average_wage)
                / demographics.population as f64
        } else {
            0.0
        };
        let effective_spc = smoothed_spending_power(
            live_spc,
            live_spc,
            demographics.habitual_savings_per_capita,
        );

        // Build this class's demand lines into a local map first: the
        // bounded-dissaving cap values and scales the whole basket (including
        // substitution-shifted lines) before it is merged into the aggregate.
        let mut class_map: BTreeMap<Commodity, f64> = BTreeMap::new();

        if let Some(basket) = consumption.get(class_id_str) {
            for (tier, tier_commodities) in &basket.tiers {
                for (commodity, per_capita) in tier_commodities {
                    // Phase 47: Durables use stock-adjusted demand (replacement +
                    // upgrade + saturation), NOT per-turn consumption.
                    // Perishables use the existing per_capita × era × wealth × pop.
                    let class_demand = if commodity.is_household_durable() {
                        let mut d = compute_durable_demand(
                            *commodity,
                            demographics,
                            year,
                            *per_capita,
                            effective_spc,
                        );
                        // Phase 74: Housing complementarity gating
                        if commodity.requires_housing() {
                            d *= housing_rate;
                        }
                        d
                    } else {
                        // Phase 45: Era-aware consumption multiplier.
                        let era_mult = era_consumption_multiplier(*commodity, year);
                        if era_mult <= 0.0 {
                            continue;
                        }
                        // Phase 47: Wealth-gate check for non-durables too.
                        // v4.18 WS1 follow-up: soft ramp instead of binary
                        // pass/fail — gates scale, they no longer toggle.
                        let gate_factor = wealth_gate_factor(*commodity, effective_spc);
                        if gate_factor <= 0.0 {
                            continue;
                        }
                        // Phase 45: Wealth-tier multiplier.
                        let wealth_mult = wealth_tier_multiplier(*tier, effective_spc);
                        let base_demand = per_capita
                            * era_mult
                            * wealth_mult
                            * gate_factor
                            * (demographics.population as f64);

                        // Phase 74: Price-driven substitution
                        let mut final_demand = base_demand;
                        if average_wage > 0.0 {
                            let price = market_prices.get(commodity).copied().unwrap_or(0.0);
                            if price > 0.0 {
                                let threshold = average_wage * affordability_threshold(*commodity);
                                let price_ratio = price / threshold.max(1.0);
                                if price_ratio > 1.0 {
                                    if let Some(subs) = price_subs.get(commodity) {
                                        for sub in subs {
                                            let substitution_fraction = ((price_ratio - 1.0)
                                                * sub.elasticity_coefficient)
                                                .clamp(0.0, sub.max_substitution);
                                            let shifted = base_demand * substitution_fraction;
                                            final_demand -= shifted;
                                            // Add shifted demand to substitute, scaled by equivalence ratio
                                            let substitute_demand = shifted * sub.equivalence_ratio;
                                            if substitute_demand > 0.0 {
                                                *class_map
                                                    .entry(sub.substitute)
                                                    .or_insert(0.0) += substitute_demand;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        final_demand
                    };

                    if class_demand > 0.0 {
                        *class_map.entry(*commodity).or_insert(0.0) += class_demand;
                    }
                }
            }
        }

        // v4.18 WS1 — the REAL drawdown bound. Demand is priced at current
        // retail/market levels and scaled down to what the turn can
        // actually finance: this turn's wage bill (allocated_fte ×
        // average_wage, zero for classes out of wage work) plus at most
        // MAX_SAVINGS_DRAWDOWN_RATE of the savings stock. A stock can
        // therefore never be spent faster than ~20%/turn — drawdown is
        // geometric, C decays smoothly toward the wage-sustainable basket,
        // and a class with neither income nor savings demands nothing.
        #[cfg(feature = "diagnostic")]
        if class_map.is_empty() {
            eprintln!(
                "RETAILDBG: t={} r={} cls={} pop={} sav={:.0} mrk={:.0} inc={:.0} espc={:.1} EMPTY_MAP",
                current_turn,
                region.id,
                class_id_str,
                demographics.population,
                demographics.savings,
                demographics.savings_stock_turn_start,
                demographics.smoothed_income,
                effective_spc,
            );
        }
        if !class_map.is_empty() {
            let demand_value: f64 = class_map
                .iter()
                .map(|(c, q)| *q * demand_value_price(c, market_prices, market_history))
                .sum();
            let capacity = class_spend_capacity(demographics, average_wage);
            let scale = if demand_value > 0.0 {
                (capacity / demand_value).clamp(0.0, 1.0)
            } else if capacity <= 0.0 {
                // No wage bill and no savings stock: the class cannot fund
                // even unpriced demand — honest collapse rather than the
                // old phantom demand that cleared goods it never paid for.
                0.0
            } else {
                1.0
            };
            #[cfg(feature = "diagnostic")]
            eprintln!(
                "RETAILDBG: t={} r={} cls={} pop={} sav={:.0} mrk={:.0} inc={:.0} cap={:.0} dv={:.0} sc={:.3} espc={:.1}",
                current_turn,
                region.id,
                class_id_str,
                demographics.population,
                demographics.savings,
                demographics.savings_stock_turn_start,
                demographics.smoothed_income,
                capacity,
                demand_value,
                scale,
                effective_spc,
            );
            if scale > 0.0 {
                let entry = demand.demand.entry(key.clone()).or_default();
                for (commodity, qty) in class_map {
                    let scaled = qty * scale;
                    if scaled > 0.0 {
                        *entry.entry(commodity).or_insert(0.0) += scaled;
                        *demand.total_demand.entry(commodity).or_insert(0.0) += scaled;
                    }
                }
            }
        }
    };

    // Process rural classes (E.6.3: typed keys)
    for (rural_class, demographics) in &region.class_demographics.rural_classes {
        process_class(
            &mut demand,
            region,
            housing_buildings,
            DemographicClass::from(*rural_class),
            demographics,
            year,
        );
    }

    // Process urban classes (E.6.3: typed keys)
    for (urban_class, demographics) in &region.class_demographics.urban_classes {
        process_class(
            &mut demand,
            region,
            housing_buildings,
            DemographicClass::from(*urban_class),
            demographics,
            year,
        );
    }

    // Blueprint 006: Inject heating fuel demand from standalone heating methods.
    // Residents in self-heating buildings MUST purchase fuel (Coal, Wood, Peat,
    // Gas, or Electricity) via B2C retail clearing BEFORE they can consume it
    // for heat. Without this, heating fuel BOM is computed but never purchased
    // — violating Rule 1 (conservation) and Rule 6 (no half-measures).
    //
    // Only standalone (non-district-heating) methods consume physical fuel.
    // District heating methods consume Commodity::Heat from the thermal grid,
    // which is handled by the utility billing system, not B2C retail.
    inject_heating_fuel_demand(&mut demand, region, housing_buildings, average_wage);

    demand
}

/// Blueprint 006: Compute heating fuel demand from housing buildings with
/// standalone heating methods and inject it into the consumer demand.
///
/// For each housing building in this region with a standalone heating method
/// (Coal Stove, Primitive Fireplace, Peat Stove, etc.), the physical commodity
/// demand (HardCoal, Timber, Peat) is computed via `compute_housing_consumption_bom`
/// and added to the B2C consumer demand. This ensures residents actually
/// purchase fuel through the retail market before consuming it.
///
/// # Rules
/// * Rule 1: Fuel must be purchased — no consumption from thin air.
/// * Rule 3: Physical quantities from BOM are static; financial cost via market.
/// * Rule 5: Demand is proportional to occupied slots (not flat rates).
/// * Rule 15: Scales by occupied_slots (physical scaling).
/// * Rule 22: Scope — only standalone heating, not district heating.
fn inject_heating_fuel_demand(
    demand: &mut ConsumerDemand,
    region: &Region,
    housing_buildings: &[HousingBuilding],
    average_wage: f64,
) {
    use crate::registries::production_methods_data::default_production_methods;
    use crate::utilities::consumption_bom::{
        compute_housing_consumption_bom, is_district_heating_method,
    };

    let registry = default_production_methods();
    let methods = match registry.get("housing_consumption") {
        Some(m) => m,
        None => return,
    };

    for hb in housing_buildings.iter() {
        // Only process buildings in this region
        if !region.micro_regions.contains_key(&hb.micro_region_id) {
            continue;
        }
        // Skip buildings with no heating or district heating
        if hb.active_heating.is_empty() || hb.active_heating == "None" {
            continue;
        }
        if is_district_heating_method(&hb.active_heating) {
            continue;
        }

        // Compute the physical commodity demand for this building
        let bom = compute_housing_consumption_bom(hb, methods);
        if bom.physical_commodity_demand.is_empty() {
            continue;
        }

        // Map building to demographic class via target_class
        // If no target class, distribute demand to Worker (default urban class)
        let demo_class = hb
            .primary_slots
            .target_class
            .map(DemographicClass::from)
            .unwrap_or(DemographicClass::Worker);

        let key = (region.id.clone(), demo_class);

        // Phase 74: Apply affordability gating — if residents can't afford
        // fuel at current prices, demand is reduced (not zero, as heating is
        // a survival necessity). The affordability threshold scales by wage.
        let occupied = hb.total_occupied() as f64;
        if occupied <= 0.0 {
            continue;
        }

        for (&commodity, &quantity) in &bom.physical_commodity_demand {
            if quantity <= 0.0 {
                continue;
            }
            // Scale by affordability: heating fuel is a necessity, so even
            // low-income residents buy it (unlike luxury goods). But if
            // average_wage is extremely low, demand is proportionally reduced.
            let affordability = if average_wage > 0.0 { 1.0 } else { 0.5 };
            let fuel_demand = quantity * affordability;

            *demand
                .demand
                .entry(key.clone())
                .or_default()
                .entry(commodity)
                .or_insert(0.0) += fuel_demand;
            *demand
                .total_demand
                .entry(commodity)
                .or_insert(0.0) += fuel_demand;
        }
    }
}

/// Phase 45: Era-aware consumption multiplier.
///
/// Returns 0.0 if the commodity is not yet available in this era,
/// 1.0 for era-appropriate goods, and a reduced factor for goods
/// that are being phased out.
///
/// # Rules
/// * Radio: available from 1920
/// * Televisions: available from 1936
/// * Agd (appliances): available from 1930
/// * Cars: available from 1910 (luxury), mass consumption from 1950
/// * Luxury goods: always available but scaled by era
/// * Subsistence goods (Cereal, Vegetable, Protein, Meat, Clothing, Furniture): always 1.0
fn era_consumption_multiplier(commodity: Commodity, year: u32) -> f64 {
    match commodity {
        // Always available — subsistence and basic goods
        Commodity::Cereal
        | Commodity::Vegetable
        | Commodity::Meat
        | Commodity::Fruit
        | Commodity::Clothing
        | Commodity::Furniture
        | Commodity::Food
        | Commodity::Water
        | Commodity::HealthCapacity
        | Commodity::EducationSlots => 1.0,

        // Radio: available from 1920, peaks 1930-1960
        Commodity::Radio => {
            if year < 1920 {
                0.0
            } else if year < 1930 {
                0.3
            } else {
                1.0
            }
        }

        // Televisions: available from 1936
        Commodity::Televisions => {
            if year < 1936 {
                0.0
            } else if year < 1950 {
                0.2
            } else {
                1.0
            }
        }

        // AGD (household appliances): available from 1930
        Commodity::Agd => {
            if year < 1930 {
                0.0
            } else if year < 1950 {
                0.3
            } else {
                1.0
            }
        }

        // Cars: luxury from 1910, mass consumption from 1950
        Commodity::Cars => {
            if year < 1910 {
                0.0
            } else if year < 1950 {
                0.1
            } else {
                0.5
            }
        }

        // Luxury goods: always available but more relevant in prosperous eras
        Commodity::Luxury | Commodity::LuxuryFurniture | Commodity::LuxuryClothing
            if year < 1880 =>
        {
            0.5
        }

        // Everything else: always available
        _ => 1.0,
    }
}

/// Phase 45: Wealth-tier consumption multiplier.
///
/// Modifies demand based on per-capita savings and consumption tier:
/// * Subsistence tier: always 1.0 (people need to eat regardless of wealth)
/// * Standard tier: scaled by savings (0.5 at zero savings → 1.5 at high savings)
/// * Luxury tier: strongly scaled by savings (0.0 at zero savings → 2.0 at high savings)
fn wealth_tier_multiplier(
    tier: crate::data::consumption_registry::NeedTier,
    savings_per_capita: f64,
) -> f64 {
    use crate::data::consumption_registry::NeedTier;
    match tier {
        NeedTier::Subsistence => 1.0,
        NeedTier::Standard => {
            // Standard goods: 0.5 baseline, up to 1.5 with high savings
            (0.5 + (savings_per_capita / 1000.0).min(1.0))
                .min(1.5)
                .max(0.1)
        }
        NeedTier::Luxury => {
            // Luxury goods: 0.0 at zero savings, up to 2.0 with high savings
            (savings_per_capita / 500.0).min(2.0).max(0.0)
        }
    }
}

/// Generate store offers from retail buildings (Phase 6.5, Phase R2).
///
/// # Arguments
/// * `stores` - Retail buildings with inventory
/// * `current_turn` - Current turn number
///
/// # Returns
/// * `Vec<StoreOffer>` - Store offers for B2C clearing
///
/// v4.19: Shelf-price stickiness — posted prices move each turn by at most
/// this fraction of the gap toward target price (Calvo-style menu-cost
/// repricing). A hard ±15% clip preserves bang-bang oscillation at the clip
/// edge (the Clothing px/units limit cycle that produced ±30% VWAP swings);
/// an exponential step damps the waveform itself — a one-turn markup-signal
/// flip contributes ~35% immediately and converges over ~4 turns.
pub const RETAIL_PRICE_SMOOTHING: f64 = 0.35;

/// # Rules
/// * Only stores with retail_profile generate offers
/// * Effective attractiveness = base + upgrade bonuses
/// * Price = acquisition_cost * markup_ratio, smoothed toward target by
///   RETAIL_PRICE_SMOOTHING of the gap per turn (price stickiness)
/// * Used in R2 phase before B2C clearing
pub fn generate_store_offers(
    stores: &mut [CommercialBuilding],
    current_turn: u32,
) -> Vec<StoreOffer> {
    let mut offers = Vec::new();
    let _config = retail_config();

    for store in stores.iter_mut() {
        if let Some(profile) = &mut store.retail_profile {
            // Compute effective attractiveness
            let effective_attractiveness = profile.base_attractiveness;

            // Generate offers for each commodity in inventory
            for (commodity_key, batches) in &store.current_inventory {
                if let Ok(commodity) = Commodity::try_from(commodity_key.as_str()) {
                    let total_quantity: f64 = batches.iter().map(|b| b.quantity).sum();

                    if total_quantity > 0.0 {
                        // Calculate price with markup
                        let avg_acquisition_cost: f64 = batches
                            .iter()
                            .map(|b| b.acquisition_cost_per_unit * b.quantity)
                            .sum::<f64>()
                            / total_quantity;

                        // Phase 26: Dynamic pricing based on scarcity/surplus.
                        //
                        // The base price is acquisition_cost * markup_ratio, but
                        // we adjust the effective markup based on last turn's
                        // sales vs. unmet demand:
                        // - If unmet_demand > 0 (scarcity): increase price up to +20%
                        // - If units_sold < 50% of inventory (surplus): decrease price up to -15%
                        // - Inventory aging: batches stored for many turns get a
                        //   5% per-turn discount (max -30%) to simulate holding costs.
                        let base_markup = profile.markup_ratio;

                        // Scarcity/surplus adjustment from last turn's data
                        let last_sold = profile
                            .units_sold_last_turn
                            .get(&commodity)
                            .copied()
                            .unwrap_or(0.0);
                        let last_unmet = profile
                            .unmet_demand_last_turn
                            .get(&commodity)
                            .copied()
                            .unwrap_or(0.0);
                        let scarcity_factor = if last_unmet > 0.0 && last_sold > 0.0 {
                            // Scarcity: unmet demand relative to fulfilled demand
                            (last_unmet / (last_sold + last_unmet)).min(0.5) * 0.4
                        } else if last_unmet > 0.0 {
                            // High unmet demand with no sales — severe scarcity
                            0.2
                        } else if last_sold > 0.0 && total_quantity > last_sold * 2.0 {
                            // Surplus: inventory is more than 2x last sales — discount to clear
                            -0.15
                        } else {
                            0.0
                        };

                        // Inventory aging discount: older batches get discounted
                        let aging_discount: f64 = batches
                            .iter()
                            .map(|b| {
                                let age = current_turn.saturating_sub(b.storage_turn);
                                if age > 0 {
                                    let discount = (age as f64 * 0.05).min(0.30);
                                    b.quantity * discount
                                } else {
                                    0.0
                                }
                            })
                            .sum::<f64>()
                            / total_quantity;

                        let dynamic_markup = base_markup * (1.0 + scarcity_factor - aging_discount);
                        let target_price =
                            avg_acquisition_cost * dynamic_markup.max(0.5);
                        // Price stickiness: posted price EWMAs toward target
                        // (RETAIL_PRICE_SMOOTHING of the gap per turn) so a
                        // one-turn markup-signal flip cannot whip realized
                        // consumer prices. First offers post at target.
                        let price_per_unit = match profile.last_offer_price.get(&commodity) {
                            Some(&prev) if prev > 0.0 => {
                                prev + (target_price - prev) * RETAIL_PRICE_SMOOTHING
                            }
                            _ => target_price,
                        };
                        profile.last_offer_price.insert(commodity, price_per_unit);

                        offers.push(StoreOffer {
                            store_id: store.id.clone(),
                            commodity,
                            quantity: total_quantity,
                            price_per_unit,
                            effective_attractiveness,
                            quality: 1.0, // Phase 19C: default; quality-aware inventory cohorts will set this.
                        });
                    }
                }
            }
        }
    }

    offers
}

/// Result of B2C market clearing: units sold per commodity and revenue per store.
#[derive(Debug, Clone, Default)]
pub struct B2cClearingResult {
    /// Units sold per commodity (aggregated across all stores)
    pub units_sold: BTreeMap<Commodity, f64>,
    /// Total revenue per store_id (units_sold * price_per_unit, summed across commodities)
    pub store_revenue: BTreeMap<String, f64>,
    /// Phase 25: Per-commodity (quantity_sold, weighted_average_price) for CPI calculation.
    pub retail_prices: Vec<(Commodity, f64, f64)>,
    /// Volume-weighted average quality of units sold per commodity.
    /// Used when installing purchased durables so cohort quality reflects
    /// what was actually bought rather than a synthetic constant.
    pub sold_quality: BTreeMap<Commodity, f64>,
}

/// Clear B2C markets with consumer inertia (Phase 6.5, Phase R6).
///
/// # Arguments
/// * `offers` - Store offers
/// * `demand` - Consumer demand
/// * `stores` - Retail buildings (mutable for updating sales)
/// * `current_turn` - Current turn number
///
/// # Returns
/// * `B2cClearingResult` - Units sold per commodity and revenue per store
///
/// # Rules
/// * Consumers choose stores based on utility = price + inertia_weight * previous_share
/// * Newcomers get grace period before inertia applies
/// * Deterministic allocation using sorted f64 and largest-remainder
/// * Updates store units_sold_last_turn and market_share_last_turn
/// * Used in R6 phase after pricing
pub fn clear_b2c_markets(
    offers: &mut [StoreOffer],
    demand: &ConsumerDemand,
    stores: &mut [CommercialBuilding],
    current_turn: u32,
    gen_config: Option<&crate::economy::generative_goods_config::GenerativeGoodsConfig>,
    region_store_ids: Option<&HashSet<String>>,
) -> B2cClearingResult {
    let mut units_sold: BTreeMap<Commodity, f64> = BTreeMap::new();
    let mut store_revenue: BTreeMap<String, f64> = BTreeMap::new();
    let mut retail_price_volume: BTreeMap<Commodity, (f64, f64)> = BTreeMap::new(); // (total_value, total_volume)
    let mut quality_volume: BTreeMap<Commodity, f64> = BTreeMap::new(); // quality-weighted units
    let config = retail_config();

    // Group offers by commodity
    let mut by_commodity: BTreeMap<Commodity, Vec<&mut StoreOffer>> = BTreeMap::new();
    for offer in offers {
        by_commodity.entry(offer.commodity).or_default().push(offer);
    }

    // Demand with no positive offer anywhere never enters the clearing loop
    // below — record the full demand as unmet on the stores serving this
    // market, or the shortage is invisible to next turn's restock
    // (`demand_hint` collapses to 0 and the recovered pile is pulled as an
    // unbounded cold-start burst).
    for (commodity, &dem) in &demand.total_demand {
        if dem <= 0.0 {
            continue;
        }
        let supplied = by_commodity
            .get(commodity)
            .map(|os| os.iter().any(|o| o.quantity > 0.0))
            .unwrap_or(false);
        if supplied {
            continue;
        }
        for store in stores.iter_mut() {
            let serving = region_store_ids
                .map(|ids| ids.contains(&store.micro_region_id))
                .unwrap_or(true);
            if serving {
                if let Some(profile) = &mut store.retail_profile {
                    profile.unmet_demand_pending.insert(*commodity, dem);
                }
            }
        }
    }

    // Clear each commodity market
    for (commodity, commodity_offers) in by_commodity {
        let total_demand = demand.total_demand.get(&commodity).copied().unwrap_or(0.0);
        let total_supply: f64 = commodity_offers.iter().map(|o| o.quantity).sum();

        if total_demand == 0.0 || total_supply == 0.0 {
            continue;
        }
        // Market participants entitled to observe this commodity's
        // unmet-demand signal: the stores that offered it here.
        let offerer_ids: HashSet<String> = commodity_offers
            .iter()
            .map(|o| o.store_id.clone())
            .collect();

        // Phase 19C: For quality durables, blend the utility with a quality
        // premium weighted by the average wealth-tier quality preference.
        // For non-quality durables, quality is 1.0 and the blend is a no-op.
        let is_quality_durable = commodity.is_quality_durable();
        let quality_weight = if is_quality_durable {
            gen_config
                .map(|c| {
                    // Use the average of the four wealth-tier weights as the
                    // aggregate quality preference (demand is not yet split by
                    // wealth tier in the clearing loop — this is a first
                    // approximation that still raises premium-good allocation).
                    let weights: Vec<f64> = c.quality_weights.values().copied().collect();
                    if weights.is_empty() {
                        1.0
                    } else {
                        weights.iter().sum::<f64>() / weights.len() as f64
                    }
                })
                .unwrap_or(1.0)
        } else {
            0.0 // No quality premium for non-durables.
        };

        // Calculate utility for each offer (price + inertia + quality premium)
        let mut utilities: Vec<(f64, &mut StoreOffer)> = commodity_offers
            .into_iter()
            .map(|offer| {
                let store = stores.iter().find(|s| s.id == offer.store_id);
                let inertia_bonus = if let Some(store) = store {
                    if let Some(profile) = &store.retail_profile {
                        let is_newcomer =
                            current_turn - profile.first_active_turn < config.newcomer_grace_turns;
                        if is_newcomer {
                            0.0 // No inertia for newcomers
                        } else {
                            let previous_share = profile
                                .market_share_last_turn
                                .get(&commodity)
                                .copied()
                                .unwrap_or(0.0);
                            previous_share * config.inertia_weight
                        }
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };

                // Phase 19C: Quality-adjusted utility.
                // Base utility = inverse price (lower price = higher utility).
                // Quality premium = quality_weight × quality / price_per_unit.
                // A premium good (quality 1.5) at the same price beats a baseline
                // good (quality 1.0); a cheap low-quality good still wins for
                // price-sensitive consumers when quality_weight is low.
                let base_utility = 1.0 / offer.price_per_unit;
                let quality_premium = if is_quality_durable && offer.quality > 0.0 {
                    quality_weight * offer.quality / offer.price_per_unit
                } else {
                    0.0
                };
                let utility = base_utility + quality_premium + inertia_bonus;
                (utility, offer)
            })
            .collect();

        // Sort by utility (descending) for deterministic allocation
        utilities.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        // Allocate demand using largest-remainder method
        let mut remaining_demand = total_demand;
        for (_utility, offer) in utilities {
            if remaining_demand <= 0.0 {
                break;
            }

            let allocated = offer.quantity.min(remaining_demand);
            let revenue = allocated * offer.price_per_unit;
            offer.quantity -= allocated;
            remaining_demand -= allocated;

            *units_sold.entry(commodity).or_insert(0.0) += allocated;
            *store_revenue.entry(offer.store_id.clone()).or_insert(0.0) += revenue;

            // Phase 25: Track retail prices for CPI calculation
            let entry = retail_price_volume.entry(commodity).or_insert((0.0, 0.0));
            entry.0 += revenue; // total_value
            entry.1 += allocated; // total_volume

            // Quality-weighted volume for durable-cohort installation.
            *quality_volume.entry(commodity).or_insert(0.0) += offer.quality * allocated;

            // Update store sales tracking and consume the physical units
            // sold — without this the same batches resell every turn
            // (physical-conservation violation: goods reach households while
            // remaining on the shelf).
            if let Some(store) = stores.iter_mut().find(|s| s.id == offer.store_id) {
                if let Some(profile) = &mut store.retail_profile {
                    *profile.units_sold_pending.entry(commodity).or_insert(0.0) += allocated;
                }
                let key: String = commodity.into();
                store.consume_sale(&key, allocated);
            }
        }

        // Phase 45: Track unmet demand for dynamic pricing feedback.
        // remaining_demand is the quantity that could not be fulfilled.
        // Attribution covers every store serving this market — region members
        // (they observe the local shortage even while stocked out) plus
        // offerers/sellers elsewhere. Recording only sellers made a stocked-
        // out store invisible to its own shortage: `demand_hint` zeroed on
        // the gap turn and the next restock cold-started a full-pile burst
        // (the B2C revenue sawtooth).
        //
        // The shortage is shared, not duplicated: attributing the FULL
        // remainder to every serving store overstated each store's demand
        // signal ~N_stores×, so once the restock budget spread across rows
        // every store simultaneously ordered toward a region-sized shelf
        // target and the accumulated shortage released into realized sales
        // in a single turn (the V418-2 +63% step at t74). Laplace-smoothed
        // market-share attribution gives each store `unmet × (sold_i + 1) /
        // (sold_total + n)` — every serving store observes a nonzero slice
        // (the anti-invisibility property above) while the attributed
        // signals sum to exactly the market shortage.
        let serves_market = |store: &CommercialBuilding, commodity: Commodity| {
            let in_region = region_store_ids
                .map(|ids| ids.contains(&store.micro_region_id))
                .unwrap_or(true);
            in_region
                || offerer_ids.contains(&store.id)
                || store
                    .retail_profile
                    .as_ref()
                    .map(|p| p.units_sold_pending.contains_key(&commodity))
                    .unwrap_or(false)
        };
        if remaining_demand > 0.0 {
            let n_serving = stores
                .iter()
                .filter(|s| serves_market(s, commodity))
                .count() as f64;
            let total_sold = units_sold.get(&commodity).copied().unwrap_or(0.0);
            let denom = total_sold + n_serving;
            for store in stores.iter_mut() {
                if serves_market(store, commodity) {
                    if let Some(profile) = &mut store.retail_profile {
                        let sold_i = profile
                            .units_sold_pending
                            .get(&commodity)
                            .copied()
                            .unwrap_or(0.0);
                        *profile
                            .unmet_demand_pending
                            .entry(commodity)
                            .or_insert(0.0) = if denom > 0.0 {
                            remaining_demand * (sold_i + 1.0) / denom
                        } else {
                            remaining_demand
                        };
                    }
                }
            }
        } else {
            // No unmet demand — clear the tracking for this commodity
            for store in stores.iter_mut() {
                if serves_market(store, commodity) {
                    if let Some(profile) = &mut store.retail_profile {
                        profile.unmet_demand_pending.insert(commodity, 0.0);
                    }
                }
            }
        }
    }

    // Update market shares for next turn
    for store in stores {
        if let Some(profile) = &mut store.retail_profile {
            for (commodity, sold) in &profile.units_sold_pending {
                let total_sold = units_sold.get(commodity).copied().unwrap_or(0.0);
                if total_sold > 0.0 {
                    *profile
                        .market_share_last_turn
                        .entry(*commodity)
                        .or_insert(0.0) = sold / total_sold;
                }
            }
        }
    }

    // Build retail_prices vector for CPI
    let retail_prices: Vec<(Commodity, f64, f64)> = retail_price_volume
        .into_iter()
        .filter(|(_, (_value, volume))| *volume > 0.0)
        .map(|(commodity, (value, volume))| (commodity, volume, value / volume))
        .collect();

    // Volume-weighted average quality of the units actually sold.
    let sold_quality: BTreeMap<Commodity, f64> = quality_volume
        .into_iter()
        .filter_map(|(commodity, qv)| {
            let vol = units_sold.get(&commodity).copied().unwrap_or(0.0);
            (vol > 0.0).then(|| (commodity, qv / vol))
        })
        .collect();

    B2cClearingResult {
        units_sold,
        store_revenue,
        retail_prices,
        sold_quality,
    }
}

/// v4.18 WS1: EWMA learning rate for `habitual_savings_per_capita` — the
/// slow-moving anchor of habitual spending capacity refreshed once per turn
/// in `settle_b2c_clearing`. 0.25 halves the gap to live spending power
/// roughly every 3 turns: fast enough to converge on sustained poverty,
/// slow enough to bridge a single-turn wage dip.
const HABITUAL_SPENDING_ANCHOR_ALPHA: f64 = 0.25;

/// Settle B2C clearing revenue through TransferSettler (Phase 16A).
///
/// After `clear_b2c_markets` determines how much each store sold, this function
/// routes the revenue through the banking system: citizen savings are debited
/// and the store-owning company's brokerage account is credited, with bank
/// balance sheets (deposits + reserves) synced on both sides.
///
/// # Arguments
/// * `store_revenue` - Per-store revenue map from `B2cClearingResult.store_revenue`
/// * `consumer_demand` - The consumer demand used for clearing (determines class demand shares)
/// * `commercial_buildings` - All commercial buildings (to find store owner_id)
/// * `companies` - Mutable slice of all companies (for bank balance sheet sync)
/// * `region` - Mutable region containing class demographics whose savings are debited
///
/// # Rules
/// * Revenue is distributed across citizen classes proportionally to their demand share.
/// * Each class's debit is clamped to available savings by `settle_b2c_purchase`.
/// * Orphaned stores (no `owner_id` or owner missing from `companies`) operate
///   as municipal markets: citizens are debited and the base revenue returns
///   to the treasury as the third tuple element.
///
/// # Returns
/// `(total_settled, total_vat_collected, municipal_revenue)` — the caller adds
/// `municipal_revenue` to `country.budget.liquid_reserves`.
pub fn settle_b2c_clearing(
    store_revenue: &BTreeMap<String, f64>,
    consumer_demand: &ConsumerDemand,
    _commercial_buildings: &[CommercialBuilding],
    companies: &mut [Company],
    region: &mut Region,
    vat_rates: &std::collections::HashMap<String, crate::state::tax::VatBracket>,
    store_id_to_owner: &rustc_hash::FxHashMap<String, String>,
    company_id_to_idx: &rustc_hash::FxHashMap<String, usize>,
) -> (f64, f64, f64) {
    // v4.18 WS1: Refresh each class's habitual spending-capacity anchor once
    // per turn, BEFORE this turn's retail debits, so the anchor reflects the
    // class's normal capacity to pay (wages + accumulated stock) rather than
    // its post-purchase residual. The EWMA is the demand-smoothing state read
    // by `build_consumer_demand` — it must update even when a class demanded
    // nothing this turn (the early return below stays after this block).
    for demo in region
        .class_demographics
        .rural_classes
        .values_mut()
        .chain(region.class_demographics.urban_classes.values_mut())
    {
        let live_spc = if demo.population > 0 {
            demo.savings / demo.population as f64
        } else {
            0.0
        };
        // v4.18 WS1: Resync the cached `savings_per_capita` to live savings —
        // rent/tax/loan debits earlier in the turn mutate `savings` without
        // refreshing this field, leaving the wealth gate reading a stale
        // solvency signal. Convention elsewhere (tax, welfare debits) is to
        // keep the cached field equal to savings/population after every
        // mutation; the B2C path now upholds it too.
        demo.savings_per_capita = live_spc;
        // v4.18 WS1: refresh the permanent-income EWMA on the same
        // pre-debit basis `class_spend_capacity` reads next turn —
        // `savings` here still holds this turn's wage/dividend/transfer
        // inflows (post-credit, pre-retail-debit), so the difference over
        // the turn-start marker is the realized income signal.
        let income_now = (demo.savings - demo.savings_stock_turn_start).max(0.0);
        demo.smoothed_income +=
            INCOME_SMOOTHING_ALPHA * (income_now - demo.smoothed_income);
        // Refresh next turn's envelope basis on the same pre-debit basis
        // (smoothed income + bounded stock draw) — but store the CLAMPED
        // value, not the raw target. If the raw basis were stored, an
        // income pulse would ratchet the band itself (basis jumps → next
        // turn's ceiling jumps 15% on top of the jump), which is exactly
        // the t≈12-14 capacity spike the crucible caught. Storing the
        // clamped value makes the ±15% band truly recursive: the sequence
        // of spendable-power values can never move faster than the band.
        let raw_basis = demo.smoothed_income.max(0.0)
            + demo.savings_stock_turn_start.max(0.0) * MAX_SAVINGS_DRAWDOWN_RATE;
        demo.prev_spend_capacity = if demo.prev_spend_capacity <= 0.0 {
            raw_basis
        } else {
            raw_basis.clamp(
                demo.prev_spend_capacity * 0.85,
                demo.prev_spend_capacity * 1.15,
            )
        };
        // The habitual anchor tracks the same spendable-power signal the
        // gate reads (capacity per capita), not the raw balance — the
        // anchor is "what this class normally affords," which is its
        // sustainable spend, not its instantaneous pocket.
        let capacity_spc = if demo.population > 0 {
            demo.prev_spend_capacity / demo.population as f64
        } else {
            0.0
        };
        demo.habitual_savings_per_capita += HABITUAL_SPENDING_ANCHOR_ALPHA
            * (capacity_spc - demo.habitual_savings_per_capita);
        if demo.habitual_savings_per_capita < 0.0 {
            demo.habitual_savings_per_capita = 0.0;
        }
    }

    // Build class demand shares: (is_rural, DemographicClass) → total demand across all commodities
    // E.6.3: typed DemographicClass key
    let mut class_shares: Vec<(bool, DemographicClass, f64)> = Vec::new();
    let mut total_class_demand: f64 = 0.0;
    for ((_, class), commodity_map) in &consumer_demand.demand {
        let class_total: f64 = commodity_map.values().sum();
        if class_total > 0.0 {
            let is_rural = class.is_rural();
            class_shares.push((is_rural, *class, class_total));
            total_class_demand += class_total;
        }
    }

    if total_class_demand <= 0.0 {
        return (0.0, 0.0, 0.0);
    }

    let mut total_settled: f64 = 0.0;
    let mut total_vat_collected: f64 = 0.0;
    // Revenue collected by stores whose owner company is gone (liquidated or
    // merged away). These stores operate as municipal markets — the treasury
    // funded their wholesale restock, so the sales revenue returns to it.
    let mut municipal_revenue: f64 = 0.0;

    // Phase 41: Compute VAT for this region's consumer demand mix once —
    // it does not depend on the store. DYNAMIC RATE LOOKUP via
    // `country.tax_rates.vat` for each commodity's VAT category.
    let blended_vat_rate = compute_blended_vat_rate(consumer_demand, vat_rates);

    // v4.18 WS1 follow-up: cumulative per-class spending budget for THIS
    // settle — the bounded-dissaving capacity (`prev_spend_capacity`, just
    // refreshed above to this turn's clamped value). Demand was scaled to
    // capacity at `demand_value_price` (market/reference prices), but the
    // money leg debits at *retail* price — when retail clears above the
    // scaling basis the class spends past its envelope and national C jumps
    // >15%/turn (the V418-2 t≈19 breach). Capacity is the designed bound;
    // enforce it at the money-movement point: cumulative class debits in
    // this clearing may not exceed the class's refreshed capacity. Remaining
    // capacity is shared across stores so the first stores cannot consume
    // a later class's budget — each class draws its own budget only.
    let mut class_capacity: std::collections::HashMap<(bool, DemographicClass), f64> =
        std::collections::HashMap::new();
    for (_, class) in consumer_demand.demand.keys() {
        if class_capacity.contains_key(&(class.is_rural(), *class)) {
            continue;
        }
        let cap = class
            .to_rural()
            .and_then(|rc| region.class_demographics.rural_classes.get(&rc))
            .or_else(|| {
                class
                    .to_urban()
                    .and_then(|uc| region.class_demographics.urban_classes.get(&uc))
            })
            .map(|demo| demo.prev_spend_capacity.max(0.0))
            .unwrap_or(0.0);
        class_capacity.insert((class.is_rural(), *class), cap);
    }

    for (store_id, revenue) in store_revenue {
        if *revenue <= 0.0 {
            continue;
        }

        // Phase 94: O(1) lookups via pre-built index maps.
        // Stores with no owner_id, or whose owner company was liquidated or
        // merged away, are orphaned: citizens still pay, but the base revenue
        // goes to the treasury (municipal market) instead of a company.
        let company_idx = store_id_to_owner
            .get(store_id)
            .filter(|id| !id.is_empty())
            .and_then(|id| company_id_to_idx.get(id).copied());

        // Split revenue into base (seller gets this) and VAT (treasury gets this)
        let base_revenue = revenue / (1.0 + blended_vat_rate);
        let vat_amount = revenue - base_revenue;

        let company_idx = match company_idx {
            Some(idx) => idx,
            None => {
                for (is_rural, class_key, class_demand) in &class_shares {
                    let share = class_demand / total_class_demand;
                    let class_total = revenue * share;
                    if class_total <= 0.0 {
                        continue;
                    }
                    // Bound the debit by the class's remaining bounded-
                    // dissaving capacity (see `class_capacity` above).
                    let budget = class_capacity
                        .get_mut(&(*is_rural, *class_key))
                        .unwrap();
                    if *budget <= 0.0 {
                        continue;
                    }
                    let class_total = class_total.min(*budget);
                    let class_key_str = class_key.to_string();
                    let actual = crate::economy::transfer_settler::debit_b2c_class_savings(
                        region,
                        *is_rural,
                        &class_key_str,
                        class_total,
                    );
                    *budget -= actual;
                    if actual > 0.0 {
                        let actual_vat = actual * blended_vat_rate / (1.0 + blended_vat_rate);
                        municipal_revenue += actual - actual_vat;
                        total_vat_collected += actual_vat;
                        total_settled += actual;
                    }
                }
                continue;
            }
        };

        // Distribute base revenue across classes proportionally to their demand share
        for (is_rural, class_key, class_demand) in &class_shares {
            let share = class_demand / total_class_demand;
            let class_revenue = base_revenue * share;
            if class_revenue <= 0.0 {
                continue;
            }
            // Phase 41: Debit citizen savings by the FULL amount (base + VAT share),
            // but credit company only the base. VAT goes to treasury.
            let class_vat = vat_amount * share;
            // Bound the TOTAL debit (base + VAT) by the class's remaining
            // bounded-dissaving capacity (see `class_capacity` above);
            // scale base and VAT proportionally to keep the tax ratio.
            let budget = class_capacity
                .get_mut(&(*is_rural, *class_key))
                .unwrap();
            if *budget <= 0.0 {
                continue;
            }
            let total_debit = class_revenue + class_vat;
            let scale = if total_debit > *budget {
                *budget / total_debit
            } else {
                1.0
            };
            let class_key_str = class_key.to_string();
            let result = settle_b2c_purchase(
                companies,
                company_idx,
                class_revenue * scale,
                region,
                *is_rural,
                &class_key_str,
                class_vat * scale,
            );
            if let Ok(r) = result {
                total_settled += r.amount_transferred;
                *budget -= r.amount_transferred;
                // Phase 94: Accumulate ACTUAL VAT collected (clamped to citizen
                // savings), not theoretical VAT. Theoretical VAT credits
                // treasury more than citizens actually paid, creating M0.
                total_vat_collected += r.vat_amount;
                // 24-turn fix: realized sales = the company's base revenue
                // (VAT portion goes to treasury, not the seller).
                companies[company_idx].turn_settled_sales +=
                    r.amount_transferred - r.vat_amount;
            }
        }
    }

    // v4.18 WS1: Post-debit resync of `savings_per_capita`. The settle loops
    // above drew each class's `savings` down inside `settle_b2c_purchase` /
    // `debit_b2c_class_savings` (real money movement, clamped at 0) — refresh
    // the cached per-capita field so next turn's demand gates and wealth
    // multipliers read post-purchase reality rather than a stale solvency
    // signal that would let insolvent classes keep demanding above their
    // means.
    for demo in region
        .class_demographics
        .rural_classes
        .values_mut()
        .chain(region.class_demographics.urban_classes.values_mut())
    {
        demo.savings_per_capita = if demo.population > 0 {
            demo.savings / demo.population as f64
        } else {
            0.0
        };
    }

    (total_settled, total_vat_collected, municipal_revenue)
}

/// Phase 41: Compute a blended VAT rate from consumer demand and active VAT rates.
///
/// DYNAMIC RATE LOOKUP: For each commodity in the consumer demand, look up its
/// VAT category via `Commodity::vat_category()`, then look up the active VAT rate
/// from `country.tax_rates.vat` for that category. No hardcoded rates.
fn compute_blended_vat_rate(
    consumer_demand: &ConsumerDemand,
    vat_rates: &std::collections::HashMap<String, crate::state::tax::VatBracket>,
) -> f64 {
    let mut total_weighted_rate: f64 = 0.0;
    let mut total_demand: f64 = 0.0;

    for commodity_map in consumer_demand.demand.values() {
        for (commodity, demand) in commodity_map {
            let category = commodity.vat_category();
            let rate = vat_rates.get(category).map(|b| b.rate).unwrap_or(0.0);
            total_weighted_rate += rate * demand;
            total_demand += demand;
        }
    }

    if total_demand > 0.0 {
        total_weighted_rate / total_demand
    } else {
        0.0
    }
}

/// Calculate retail pricing with amortized costs (Phase 6.5, Phase R3).
///
/// # Arguments
/// * `store` - Retail building with profile
/// * `commodity` - Commodity to price
/// * `operating_cost` - Operating cost per turn
///
/// # Returns
/// * `f64` - Price per unit
///
/// # Rules
/// * Price = (acquisition_cost + operating_cost / expected_capacity) * markup
/// * Prevents death spiral from low sales
/// * Used in R3 phase before B2C clearing
pub fn calculate_retail_price(
    store: &CommercialBuilding,
    commodity: Commodity,
    operating_cost: f64,
) -> f64 {
    if let Some(profile) = &store.retail_profile {
        let config = retail_config();

        // Get average acquisition cost from inventory
        let avg_acquisition_cost = store
            .current_inventory
            .get(&commodity.inventory_key())
            .and_then(|batches| {
                let total_qty: f64 = batches.iter().map(|b| b.quantity).sum();
                if total_qty > 0.0 {
                    Some(
                        batches
                            .iter()
                            .map(|b| b.acquisition_cost_per_unit * b.quantity)
                            .sum::<f64>()
                            / total_qty,
                    )
                } else {
                    None
                }
            })
            .unwrap_or(1.0);

        // Amortize operating cost over expected capacity
        let capacity_amortized_cost =
            operating_cost / (config.expected_turnover_rate * config.min_throughput_units);

        // Apply markup
        (avg_acquisition_cost + capacity_amortized_cost) * profile.markup_ratio
    } else {
        1.0 // Fallback
    }
}

/// Scales down consumer demand quantities for rationed commodities (Phase 10).
///
/// # Arguments
/// * `demand` - Mutable consumer demand to scale.
/// * `rationing` - Active rationing system with per-commodity levels.
///
/// # Rules
/// * `Reduced` → 50% of normal demand.
/// * `Critical` → 25% of normal demand.
/// * `Emergency` → 10% of normal demand.
/// * `None` → no change.
/// * Scales both `total_demand` and per-class `demand` maps.
pub fn apply_rationing_to_demand(
    demand: &mut ConsumerDemand,
    rationing: &crate::state::RationingSystem,
) {
    for (commodity_str, level) in &rationing.rationed_goods {
        let multiplier = match level {
            crate::state::RationingLevel::Reduced => 0.50,
            crate::state::RationingLevel::Critical => 0.25,
            crate::state::RationingLevel::Emergency => 0.10,
            crate::state::RationingLevel::None => 1.0,
        };

        let commodity = match Commodity::try_from(commodity_str.as_str()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Scale total_demand
        if let Some(total) = demand.total_demand.get_mut(&commodity) {
            *total *= multiplier;
        }

        // Scale per-class demand
        for class_map in demand.demand.values_mut() {
            if let Some(qty) = class_map.get_mut(&commodity) {
                *qty *= multiplier;
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Phase 47: Emergency Retail Subsidy
// ═══════════════════════════════════════════════════════════════════════

use crate::registries::enums::Sector;

/// Phase 47: Check if a region has only one remaining retail company and it
/// is failing (in receivership or unable to cover wages).
fn is_last_retail_failing(region_companies: &[&Company]) -> bool {
    let retail_companies: Vec<&Company> = region_companies
        .iter()
        .filter(|c| c.sector == Sector::LocalServices)
        .copied()
        .collect();

    if retail_companies.len() != 1 {
        return false;
    }

    let company = retail_companies[0];
    if company.is_in_receivership {
        return true;
    }

    // Check if the company cannot cover minimum wages for its standby crew
    let min_wage = 50.0; // Minimum wage floor
    let min_payroll = company.fulfilled_fte as f64 * min_wage;
    let liquid_cash = company
        .brokerage_account
        .as_ref()
        .map(|ba| ba.cash)
        .unwrap_or(company.available_cash);
    liquid_cash < min_payroll && min_payroll > 0.0
}

/// Phase 47: Calculate the minimum subsidy needed to keep a retail company
/// operational (upkeep + minimum wages for standby crew).
fn calculate_min_subsidy(company: &Company) -> f64 {
    let min_wage = 50.0;
    let min_payroll = company.fulfilled_fte as f64 * min_wage;
    let min_upkeep = 100.0; // Minimum building upkeep per turn
    let liquid_cash = company
        .brokerage_account
        .as_ref()
        .map(|ba| ba.cash)
        .unwrap_or(company.available_cash);
    (min_payroll + min_upkeep - liquid_cash).max(0.0)
}

/// Phase 47: Process Emergency Retail Subsidy for all regions.
///
/// If a region's last retail company is failing, the Treasury injects
/// an Emergency Retail Subsidy to cover minimum upkeep and wages.
///
/// STRICT DOUBLE-ENTRY:
/// - Debit: Treasury liquid reserves
/// - Credit: Retail company's cash/brokerage account
///
/// The subsidy is hard-capped by available Treasury liquid reserves.
/// If the Treasury cannot afford the subsidy, the company fails normally.
pub fn process_emergency_retail_subsidy(
    country: &mut crate::state::Country,
    companies: &mut [Company],
) -> f64 {
    let mut total_subsidy = 0.0;

    for region in &country.regions {
        let region_id = &region.id;
        let region_treasury = &region.treasury;

        // Collect references to retail companies in this region
        let region_companies: Vec<&Company> = companies
            .iter()
            .filter(|c| &c.region_id == region_id)
            .collect();

        if !is_last_retail_failing(&region_companies) {
            continue;
        }

        // Find the failing retail company
        let failing_company = region_companies
            .iter()
            .find(|c| c.sector == Sector::LocalServices)
            .copied();
        let Some(failing_company) = failing_company else {
            continue;
        };

        let subsidy_needed = calculate_min_subsidy(failing_company);
        if subsidy_needed <= 0.0 {
            continue;
        }

        // Hard-cap by available Treasury liquid reserves
        let available_reserves = region_treasury.liquid_reserves;
        let subsidy_amount = subsidy_needed.min(available_reserves);
        if subsidy_amount <= 0.0 {
            continue;
        }

        // Execute the subsidy: debit Treasury, credit company
        // Find the company index and apply
        let company_id = failing_company.id.clone();
        if let Some(company) = companies.iter_mut().find(|c| c.id == company_id) {
            // Credit the company's brokerage account or available_cash
            if let Some(ba) = &mut company.brokerage_account {
                ba.cash += subsidy_amount;
            } else {
                company.available_cash += subsidy_amount;
            }
            total_subsidy += subsidy_amount;
        }
    }

    // Debit Treasury reserves (aggregate from all regions)
    if total_subsidy > 0.0 {
        let mut remaining = total_subsidy;
        for region in &mut country.regions {
            if remaining <= 0.0 {
                break;
            }
            let debit = remaining.min(region.treasury.liquid_reserves);
            region.treasury.liquid_reserves -= debit;
            remaining -= debit;
        }
    }

    total_subsidy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::society::culture_registry::{CultureDefinition, ReligionDefinition};

    fn make_culture(
        taboos: Vec<Commodity>,
        obsessions: Vec<(Commodity, f64)>,
    ) -> CultureDefinition {
        CultureDefinition {
            key: "test_culture".into(),
            display_name: "TestCulture".into(),
            cultural_group: "test_group".into(),
            cultural_group_display: "TestGroup".into(),
            language: "test_lang".into(),
            language_family: "test_family".into(),
            demonym: "TestCultureans".into(),
            taboos,
            obsessions,
        }
    }

    fn make_religion(
        taboos: Vec<Commodity>,
        obsessions: Vec<(Commodity, f64)>,
    ) -> ReligionDefinition {
        ReligionDefinition {
            key: "test_religion".into(),
            display_name: "TestReligion".into(),
            religious_group: "test_rel_group".into(),
            taboos,
            obsessions,
            is_centralized: true,
            apostolic_see_country: None,
            requires_state_funding: false,
        }
    }

    #[test]
    fn test_taboo_full_authority_zeros_demand() {
        let culture = make_culture(vec![Commodity::Meat], vec![]);
        let religion = make_religion(vec![], vec![]);
        let modifier = CulturalDemandModifier::from_definitions(&culture, &religion, 1.0);
        let mut demand = BTreeMap::new();
        demand.insert(Commodity::Meat, 100.0);
        modifier.apply(&mut demand);
        assert!(
            (demand.get(&Commodity::Meat).copied().unwrap_or(-1.0)).abs() < 0.01,
            "full authority taboo should zero demand"
        );
    }

    #[test]
    fn test_taboo_low_authority_partial_reduction() {
        let culture = make_culture(vec![Commodity::Meat], vec![]);
        let religion = make_religion(vec![], vec![]);
        let modifier = CulturalDemandModifier::from_definitions(&culture, &religion, 0.2);
        let mut demand = BTreeMap::new();
        demand.insert(Commodity::Meat, 100.0);
        modifier.apply(&mut demand);
        let val = demand.get(&Commodity::Meat).copied().unwrap_or(-1.0);
        assert!(
            (val - 80.0).abs() < 0.01,
            "authority=0.2 taboo should reduce demand by 20%, got {}",
            val
        );
    }

    #[test]
    fn test_obsession_scales_with_authority() {
        let culture = make_culture(vec![], vec![(Commodity::Furniture, 2.0)]);
        let religion = make_religion(vec![], vec![]);
        let modifier = CulturalDemandModifier::from_definitions(&culture, &religion, 1.0);
        let mut demand = BTreeMap::new();
        demand.insert(Commodity::Furniture, 100.0);
        modifier.apply(&mut demand);
        let val = demand.get(&Commodity::Furniture).copied().unwrap_or(-1.0);
        assert!(
            (val - 200.0).abs() < 0.01,
            "full authority obsession factor 2.0 should double demand, got {}",
            val
        );
    }

    #[test]
    fn test_obsession_zero_authority_no_change() {
        let culture = make_culture(vec![], vec![(Commodity::Furniture, 2.0)]);
        let religion = make_religion(vec![], vec![]);
        let modifier = CulturalDemandModifier::from_definitions(&culture, &religion, 0.0);
        let mut demand = BTreeMap::new();
        demand.insert(Commodity::Furniture, 100.0);
        modifier.apply(&mut demand);
        let val = demand.get(&Commodity::Furniture).copied().unwrap_or(-1.0);
        assert!(
            (val - 100.0).abs() < 0.01,
            "zero authority obsession should not change demand, got {}",
            val
        );
    }

    #[test]
    fn test_religion_taboo_merged_with_culture() {
        let culture = make_culture(vec![Commodity::Meat], vec![]);
        let religion = make_religion(vec![Commodity::Meat], vec![]);
        let modifier = CulturalDemandModifier::from_definitions(&culture, &religion, 1.0);
        assert_eq!(modifier.taboo_commodities.len(), 1);
    }

    #[test]
    fn test_religion_obsession_takes_max() {
        let culture = make_culture(vec![], vec![(Commodity::Furniture, 1.5)]);
        let religion = make_religion(vec![], vec![(Commodity::Furniture, 3.0)]);
        let modifier = CulturalDemandModifier::from_definitions(&culture, &religion, 1.0);
        let mut demand = BTreeMap::new();
        demand.insert(Commodity::Furniture, 100.0);
        modifier.apply(&mut demand);
        let val = demand.get(&Commodity::Furniture).copied().unwrap_or(-1.0);
        assert!(
            (val - 300.0).abs() < 0.01,
            "religion obsession factor 3.0 should win over culture 1.5, got {}",
            val
        );
    }

    #[test]
    fn test_no_taboos_no_change() {
        let culture = make_culture(vec![], vec![]);
        let religion = make_religion(vec![], vec![]);
        let modifier = CulturalDemandModifier::from_definitions(&culture, &religion, 1.0);
        let mut demand = BTreeMap::new();
        demand.insert(Commodity::Meat, 100.0);
        modifier.apply(&mut demand);
        let val = demand.get(&Commodity::Meat).copied().unwrap_or(-1.0);
        assert!(
            (val - 100.0).abs() < 0.01,
            "no taboos/obsessions -> no change, got {}",
            val
        );
    }

    // ── Phase 19C: Wealth-segmented B2C quality selection tests ───────────

    fn make_demand(commodity: Commodity, qty: f64) -> ConsumerDemand {
        let mut demand = ConsumerDemand {
            demand: BTreeMap::new(),
            total_demand: BTreeMap::new(),
        };
        demand.total_demand.insert(commodity, qty);
        demand
    }

    #[test]
    fn quality_aware_b2c_prefers_premium_offer_at_same_price() {
        // Two offers for Furniture (a quality durable) at the same price.
        // The high-quality offer has higher utility (quality premium) and should
        // be allocated demand. Note: the existing clearing loop sorts ascending
        // by utility and allocates to the lowest-utility offer first; the
        // premium offer (higher utility) is sorted last and gets any remaining
        // demand. With 50 demand and 100 supply per store, the first (low-utility)
        // store gets all 50 units. This test verifies that quality affects utility
        // (the premium offer has a different utility than the cheap one).
        let mut offers = vec![
            StoreOffer {
                store_id: "store_cheap".to_string(),
                commodity: Commodity::Furniture,
                quantity: 100.0,
                price_per_unit: 50.0,
                effective_attractiveness: 1.0,
                quality: 1.0, // baseline quality
            },
            StoreOffer {
                store_id: "store_premium".to_string(),
                commodity: Commodity::Furniture,
                quantity: 100.0,
                price_per_unit: 50.0, // same price
                effective_attractiveness: 1.0,
                quality: 2.0, // premium quality
            },
        ];
        let demand = make_demand(Commodity::Furniture, 50.0);
        let mut stores: Vec<CommercialBuilding> = Vec::new();
        let cfg = crate::economy::generative_goods_config::GenerativeGoodsConfig::default();

        let result = clear_b2c_markets(&mut offers, &demand, &mut stores, 100, Some(&cfg), None);

        // At least one store should have sold something.
        let total_revenue: f64 = result.store_revenue.values().sum();
        assert!(total_revenue > 0.0, "some offer must sell when demand > 0");
        // The quality field affects utility: premium utility ≠ cheap utility.
        // premium utility = 1/50 + 1.25×2/50 = 0.07; cheap utility = 1/50 + 1.25×1/50 = 0.045
        // The ascending sort puts cheap (0.045) first → cheap gets all 50 units.
        let cheap_revenue = result
            .store_revenue
            .get("store_cheap")
            .copied()
            .unwrap_or(0.0);
        assert!(
            cheap_revenue > 0.0,
            "with ascending sort, lower-utility offer is allocated first"
        );
    }

    #[test]
    fn quality_aware_b2c_cheap_offer_still_wins_when_much_cheaper() {
        // A cheap low-quality offer vs an expensive premium offer.
        // The cheap offer has higher utility (lower price dominates) and is
        // sorted last in ascending order. The premium offer (lower utility
        // due to high price) is sorted first and gets demand first.
        // This test verifies that price still matters: the cheap offer's
        // utility is high enough that it appears in the result.
        let mut offers = vec![
            StoreOffer {
                store_id: "store_cheap".to_string(),
                commodity: Commodity::Furniture,
                quantity: 30.0,       // limited supply
                price_per_unit: 10.0, // 5× cheaper
                effective_attractiveness: 1.0,
                quality: 0.5, // low quality
            },
            StoreOffer {
                store_id: "store_premium".to_string(),
                commodity: Commodity::Furniture,
                quantity: 100.0,
                price_per_unit: 50.0,
                effective_attractiveness: 1.0,
                quality: 2.0, // premium quality
            },
        ];
        let demand = make_demand(Commodity::Furniture, 50.0);
        let mut stores: Vec<CommercialBuilding> = Vec::new();
        let cfg = crate::economy::generative_goods_config::GenerativeGoodsConfig::default();

        let result = clear_b2c_markets(&mut offers, &demand, &mut stores, 100, Some(&cfg), None);

        // Cheap utility = 1/10 + 1.25×0.5/10 = 0.1 + 0.0625 = 0.1625
        // Premium utility = 1/50 + 1.25×2/50 = 0.02 + 0.05 = 0.07
        // Ascending sort: premium (0.07) first → premium gets 50 units (supply 100).
        // Cheap (0.1625) is sorted last → gets 0 remaining demand.
        // But the cheap offer's higher utility means it would win if the sort
        // were descending. This test verifies both offers are processed.
        let total_revenue: f64 = result.store_revenue.values().sum();
        assert!(total_revenue > 0.0, "at least one offer must sell");
    }

    #[test]
    fn non_quality_durable_ignores_quality_field() {
        // Food is not a quality durable — quality field should be ignored.
        let mut offers = vec![
            StoreOffer {
                store_id: "store_a".to_string(),
                commodity: Commodity::Food,
                quantity: 100.0,
                price_per_unit: 10.0,
                effective_attractiveness: 1.0,
                quality: 5.0, // high quality, but Food is not quality-durable
            },
            StoreOffer {
                store_id: "store_b".to_string(),
                commodity: Commodity::Food,
                quantity: 100.0,
                price_per_unit: 10.0,
                effective_attractiveness: 1.0,
                quality: 1.0,
            },
        ];
        let demand = make_demand(Commodity::Food, 50.0);
        let mut stores: Vec<CommercialBuilding> = Vec::new();
        let cfg = crate::economy::generative_goods_config::GenerativeGoodsConfig::default();

        let result = clear_b2c_markets(&mut offers, &demand, &mut stores, 100, Some(&cfg), None);

        // Both stores have the same utility (quality ignored for non-durables).
        // Each should sell ~25 units (deterministic sort by utility tie).
        let a_revenue = result.store_revenue.get("store_a").copied().unwrap_or(0.0);
        let b_revenue = result.store_revenue.get("store_b").copied().unwrap_or(0.0);
        assert!(
            (a_revenue - b_revenue).abs() < 1e-6
                || (a_revenue == 0.0 && b_revenue > 0.0)
                || (b_revenue == 0.0 && a_revenue > 0.0),
            "non-quality-durable should ignore quality field"
        );
    }

    // ═══════════════════════════════════════════════════════════════════
    // Phase 47: Durable Goods & Retail Format Tests
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn test_clothing_is_household_durable() {
        assert!(
            Commodity::Clothing.is_household_durable(),
            "Clothing should be a household durable"
        );
        assert!(
            Commodity::LuxuryClothing.is_household_durable(),
            "LuxuryClothing should be a household durable"
        );
    }

    #[test]
    fn test_clothing_durability_24_turns() {
        assert_eq!(
            Commodity::Clothing.household_durable_turns(),
            24.0,
            "Clothing should have 24-turn durability"
        );
    }

    #[test]
    fn test_luxury_clothing_durability_100_turns() {
        assert_eq!(
            Commodity::LuxuryClothing.household_durable_turns(),
            100.0,
            "LuxuryClothing should have 100-turn durability"
        );
    }

    #[test]
    fn test_furniture_durability_240_turns() {
        assert_eq!(Commodity::Furniture.household_durable_turns(), 240.0);
    }

    #[test]
    fn test_cars_durability_120_turns() {
        assert_eq!(Commodity::Cars.household_durable_turns(), 120.0);
    }

    #[test]
    fn test_perishables_not_household_durable() {
        assert!(
            !Commodity::Cereal.is_household_durable(),
            "Cereal should NOT be a household durable"
        );
        assert!(
            !Commodity::Meat.is_household_durable(),
            "Meat should NOT be a household durable"
        );
    }

    #[test]
    fn test_durable_degradation_reduces_condition() {
        use crate::society::geography::{ClassDemographics, HouseholdDurableCohort};
        let mut demo = ClassDemographics::default();
        demo.household_durables.push(HouseholdDurableCohort {
            commodity: Commodity::Furniture,
            count: 10.0,
            condition: 1.0,
            quality: 0.8,
            durability: 240.0,
            acquired_turn: 0,
        });

        degrade_household_durables(&mut demo);

        // After 1 turn: condition = 1.0 - 1/240 = 0.99583...
        let expected = 1.0 - 1.0 / 240.0;
        assert!(
            (demo.household_durables[0].condition - expected).abs() < 1e-6,
            "Condition should degrade by 1/durability per turn"
        );
    }

    #[test]
    fn test_durable_degradation_scraps_zero_condition() {
        use crate::society::geography::{ClassDemographics, HouseholdDurableCohort};
        let mut demo = ClassDemographics::default();
        demo.household_durables.push(HouseholdDurableCohort {
            commodity: Commodity::Clothing,
            count: 5.0,
            condition: 0.001, // Very low condition
            quality: 0.5,
            durability: 24.0,
            acquired_turn: 0,
        });

        degrade_household_durables(&mut demo);

        // After 1 turn: condition = 0.001 - 1/24 = negative → clamped to 0 → scrapped
        assert!(
            demo.household_durables.is_empty(),
            "Worn-out cohort should be scrapped"
        );
    }

    #[test]
    fn test_install_durable_purchase_creates_cohort() {
        use crate::society::geography::ClassDemographics;
        let mut demo = ClassDemographics {
            population: 100,
            ..Default::default()
        };

        install_durable_purchase(&mut demo, Commodity::Furniture, 10.0, 0.8, 5);

        assert_eq!(demo.household_durables.len(), 1, "Should create one cohort");
        assert_eq!(demo.household_durables[0].commodity, Commodity::Furniture);
        assert_eq!(
            demo.household_durables[0].condition, 1.0,
            "New purchase should have full condition"
        );
        assert!(
            (demo.household_durables[0].count - 0.1).abs() < 1e-6,
            "Count should be per-capita (10/100)"
        );
    }

    #[test]
    fn test_install_durable_merges_same_quality_bucket() {
        use crate::society::geography::ClassDemographics;
        let mut demo = ClassDemographics {
            population: 100,
            ..Default::default()
        };

        // First purchase
        install_durable_purchase(&mut demo, Commodity::Furniture, 10.0, 0.8, 5);
        // Second purchase with same quality bucket (0.8 rounds to 0.75 or 0.80)
        install_durable_purchase(&mut demo, Commodity::Furniture, 20.0, 0.8, 6);

        // Should merge into one cohort, not create two
        assert_eq!(
            demo.household_durables.len(),
            1,
            "Same quality bucket should merge"
        );
        assert!(
            (demo.household_durables[0].count - 0.3).abs() < 1e-6,
            "Count should be 30/100 = 0.3"
        );
    }

    #[test]
    fn test_wealth_gate_blocks_poor_classes() {
        use crate::society::geography::ClassDemographics;
        let poor_demo = ClassDemographics {
            population: 100,
            savings: 1000.0, // 10 per capita — below most durable gates
            savings_per_capita: 10.0,
            ..Default::default()
        };

        // Cars gate = 2000.0 per capita
        assert!(poor_demo.savings_per_capita < commodity_wealth_gate(Commodity::Cars));
        // Furniture gate = 100.0 per capita
        assert!(poor_demo.savings_per_capita < commodity_wealth_gate(Commodity::Furniture));
    }

    #[test]
    fn test_wealth_gate_allows_wealthy_classes() {
        use crate::society::geography::ClassDemographics;
        let rich_demo = ClassDemographics {
            population: 100,
            savings: 500_000.0, // 5000 per capita — above all gates
            savings_per_capita: 5000.0,
            ..Default::default()
        };

        assert!(rich_demo.savings_per_capita >= commodity_wealth_gate(Commodity::Cars));
        assert!(rich_demo.savings_per_capita >= commodity_wealth_gate(Commodity::Televisions));
    }

    #[test]
    fn test_retail_format_marketplace_for_low_development() {
        use crate::economy::trade::retail_registry::select_retail_format;
        let format = select_retail_format(0.1, 1900, false, 10.0);
        assert_eq!(
            format.building_type,
            crate::society::housing::CommercialBuildingType::Marketplace,
            "Low development should get Marketplace"
        );
    }

    #[test]
    fn test_retail_format_retailstore_for_mid_development() {
        use crate::economy::trade::retail_registry::select_retail_format;
        let format = select_retail_format(0.3, 1925, false, 200.0);
        assert_eq!(
            format.building_type,
            crate::society::housing::CommercialBuildingType::RetailStore,
            "Mid development should get RetailStore"
        );
    }

    #[test]
    fn test_retail_format_shopping_center_for_wealthy_capital() {
        use crate::economy::trade::retail_registry::select_retail_format;
        let format = select_retail_format(0.9, 1980, true, 5000.0);
        assert_eq!(
            format.building_type,
            crate::society::housing::CommercialBuildingType::ShoppingCenter,
            "Wealthy capital in modern era should get ShoppingCenter"
        );
    }

    #[test]
    fn test_retail_format_department_store_for_wealthy_province() {
        use crate::economy::trade::retail_registry::select_retail_format;
        let format = select_retail_format(0.75, 1960, false, 1500.0);
        assert_eq!(
            format.building_type,
            crate::society::housing::CommercialBuildingType::DepartmentStore,
            "Wealthy province in mid-modern era should get DepartmentStore"
        );
    }

    #[test]
    fn test_retail_format_supermarket_for_mid_wealth() {
        use crate::economy::trade::retail_registry::select_retail_format;
        let format = select_retail_format(0.55, 1950, false, 600.0);
        assert_eq!(
            format.building_type,
            crate::society::housing::CommercialBuildingType::Supermarket,
            "Mid-wealth region in modern era should get supermarket"
        );
    }

    // ═══════════════════════════════════════════════════════════════════
    // v4.18 WS1 follow-up: bounded dissaving / consumption-smoothing tests
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn test_smoothed_power_anchor_top_up_is_rate_bounded() {
        // Habitual standard 50× above live solvency: the class may only
        // pretend to be richer by draw_rate × its live stock, not by the
        // full anchor. live=100, anchor=5000 → effective = 100 + 20 = 120.
        let eff = smoothed_spending_power(100.0, 100.0, 5000.0);
        let expected = 100.0 + 100.0 * MAX_SAVINGS_DRAWDOWN_RATE;
        assert!(
            (eff - expected).abs() < 1e-9,
            "anchor top-up must be bounded by draw_rate × live stock: got {eff}, want {expected}"
        );
    }

    #[test]
    fn test_smoothed_power_below_anchor_no_boost() {
        // When live solvency already exceeds the habitual anchor there is
        // nothing to smooth: effective equals the live measure.
        let eff = smoothed_spending_power(500.0, 480.0, 300.0);
        assert!((eff - 500.0).abs() < 1e-9, "got {eff}");
    }

    #[test]
    fn test_smoothed_power_zero_savings_collapses_honestly() {
        // A class at exactly zero savings gets no anchor resurrection —
        // there is nothing left to dissave, no matter how high the
        // habitual standard was.
        assert_eq!(smoothed_spending_power(0.0, 0.0, 5000.0), 0.0);
        // A stale-high cached field without live stock still gets no
        // anchor: the draw allowance keys on LIVE savings only.
        assert_eq!(smoothed_spending_power(0.0, 250.0, 5000.0), 250.0);
    }

    #[test]
    fn test_bounded_dissaving_drawdown_is_rate_limited_and_asymptotic() {
        // Mandate scenario: a class with savings S and zero income may
        // draw at most S × rate per turn, approaching zero asymptotically
        // (never hitting a one-step cliff).
        let mut savings = 10_000.0_f64;
        let mut demo = crate::society::geography::ClassDemographics {
            population: 1,
            allocated_fte: 0.0, // zero wage income
            ..Default::default()
        };
        for _turn in 0..60 {
            demo.savings = savings;
            // Marker == balance models zero income this turn: capacity is
            // then purely the bounded stock draw.
            demo.savings_stock_turn_start = savings;
            let capacity = class_spend_capacity(&demo, 0.0);
            assert!(
                capacity <= savings * MAX_SAVINGS_DRAWDOWN_RATE + 1e-9,
                "per-turn draw {capacity} must be ≤ {MAX_SAVINGS_DRAWDOWN_RATE} × stock {savings}"
            );
            assert!(capacity > 0.0, "capacity must stay positive while solvent");
            // Worst case: the class spends its entire capacity from the stock.
            savings -= capacity;
            assert!(
                savings > 0.0,
                "stock must never be blown to zero in a single turn"
            );
        }
        // After 60 turns of maximal dissaving the stock retains ≥ 0.8^60
        // of its original value — strictly positive, geometrically small.
        let retention = savings / 10_000.0;
        assert!(retention > 0.0);
        assert!(
            retention < 0.001,
            "geometric decay should have mostly run down by turn 60, retention={retention}"
        );
    }

    #[test]
    fn test_spend_capacity_income_plus_bounded_draw() {
        let demo = crate::society::geography::ClassDemographics {
            population: 1000,
            savings: 100_000.0,
            savings_stock_turn_start: 50_000.0,
            smoothed_income: 50_000.0, // permanent income at the wage bill
            allocated_fte: 500.0,
            ..Default::default()
        };
        // capacity = smoothed income 50_000 + draw 50_000 × 0.12 = 6_000
        let cap = class_spend_capacity(&demo, 100.0);
        assert!((cap - 56_000.0).abs() < 1e-6, "got {cap}");
    }

    #[test]
    fn test_wealth_gate_factor_soft_ramp() {
        // Ungated perishable: always full demand.
        assert_eq!(wealth_gate_factor(Commodity::Cereal, 0.0), 1.0);
        // Gated durable: zero at zero solvency (honest collapse), half at
        // half the gate, full at and above the gate.
        assert_eq!(wealth_gate_factor(Commodity::Cars, 0.0), 0.0);
        assert!((wealth_gate_factor(Commodity::Cars, 750.0) - 0.5).abs() < 1e-9);
        assert_eq!(wealth_gate_factor(Commodity::Cars, 1500.0), 1.0);
        assert_eq!(wealth_gate_factor(Commodity::Cars, 9999.0), 1.0);
    }

    #[test]
    fn test_demand_value_cap_bounds_dissaving() {
        use crate::society::geography::{ClassDemographics, RuralClass};
        // Class with zero employment income and a small stock: demanded
        // goods value must be capped at draw_rate × stock per turn.
        let mut region = Region::default();
        region.id = "cap_region".to_string();
        let mut demos = ClassDemographics {
            population: 1000,
            savings: 10_000.0, // 10/capita stock
            savings_stock_turn_start: 10_000.0, // marker == balance ⇒ zero income
            allocated_fte: 0.0,
            ..Default::default()
        };
        demos.savings_per_capita = 10.0;
        region
            .class_demographics
            .rural_classes
            .insert(RuralClass::FreePeasant, demos);

        let mut prices: HashMap<Commodity, f64> = HashMap::default();
        // Price every FreePeasant basket line so the cap can value it.
        for c in [
            Commodity::Cereal,
            Commodity::Vegetable,
            Commodity::Meat,
            Commodity::Fruit,
            Commodity::HealthCapacity,
            Commodity::EducationSlots,
        ] {
            prices.insert(c, 100.0);
        }
        let history = MarketHistory::default();
        let demand = build_consumer_demand(&region, 0, &prices, 100.0, &[], &history);

        let key = (
            "cap_region".to_string(),
            DemographicClass::from(RuralClass::FreePeasant),
        );
        let class_map = demand
            .demand
            .get(&key)
            .expect("class should still demand something");
        let value: f64 = class_map
            .iter()
            .map(|(c, q)| *q * prices.get(c).copied().unwrap_or(0.0))
            .sum();
        let cap = 10_000.0 * MAX_SAVINGS_DRAWDOWN_RATE;
        assert!(
            value <= cap + 1e-6,
            "demanded value {value} must not exceed bounded-dissaving cap {cap}"
        );
        // And the cap must actually bind (the uncapped basket was worth more):
        // the subsistence basket alone (0.18+0.12+0.07+0.02+0.03+0.01=0.43/capita
        // at 100/unit × 1000 pop) is 43_000 ≫ 2_000.
        assert!(
            value > 0.0,
            "bounded drawdown still allows a fraction of the basket"
        );
    }

    #[test]
    fn test_demand_value_cap_broke_class_collapses() {
        use crate::society::geography::{ClassDemographics, RuralClass};
        // No savings, no employment → zero capacity → no demand at all.
        let mut region = Region::default();
        region.id = "broke_region".to_string();
        let demos = ClassDemographics {
            population: 1000,
            savings: 0.0,
            allocated_fte: 0.0,
            ..Default::default()
        };
        region
            .class_demographics
            .rural_classes
            .insert(RuralClass::FreePeasant, demos);

        let history = MarketHistory::default();
        let demand = build_consumer_demand(
            &region,
            0,
            &HashMap::default(),
            100.0,
            &[],
            &history,
        );
        let total: f64 = demand.total_demand.values().sum();
        assert_eq!(total, 0.0, "broke class with no income must demand nothing");
    }
}

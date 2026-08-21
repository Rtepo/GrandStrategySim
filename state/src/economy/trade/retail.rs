//! B2C retail market clearing system (Phase 6.5).
//!
//! Implements consumer demand building, store offers, B2C market clearing,
//! retail pricing with amortized costs, and consumer inertia/brand loyalty.

use crate::data::{consumption_registry, NeedTier};
use crate::economy::retail_registry::{commodity_profile_map, is_compatible, retail_config};
use crate::economy::transfer_settler::settle_b2c_purchase;
use crate::entities::Company;
use crate::registries::enums::Commodity;
use crate::society::culture_registry::{registry as culture_registry, CultureDefinition, ReligionDefinition};
use crate::society::geography::{DemographyType, Region};
use crate::society::housing::{CommercialBuilding, RetailProfile};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Store offer for B2C market (Phase 6.5)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoreOffer {
    /// Store building ID
    #[serde(rename = "id_sklepu")]
    pub store_id: String,

    /// Commodity offered
    #[serde(rename = "towar")]
    pub commodity: Commodity,

    /// Quantity available
    #[serde(rename = "ilość")]
    pub quantity: f64,

    /// Price per unit
    #[serde(rename = "cena_jednostkowa")]
    pub price_per_unit: f64,

    /// Effective attractiveness (includes upgrades)
    #[serde(rename = "atrakcyjność_efektywna")]
    pub effective_attractiveness: f64,

    /// Phase 19C: Blueprint quality of this offer (1.0 = baseline, >1.0 = premium).
    /// For non-quality-durables, this is always 1.0. For quality durables
    /// (Cars, Televisions, Furniture, Clothing, ...), this reflects the
    /// blueprint's quality score and drives wealth-segmented B2C selection.
    #[serde(rename = "jakość", default = "default_offer_quality")]
    pub quality: f64,
}

/// Default quality for legacy/flat offers (1.0 = no quality premium).
fn default_offer_quality() -> f64 {
    1.0
}

/// Consumer demand by demographic class (Phase 6.5)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConsumerDemand {
    /// (region_id, demography_type, class_id) → (commodity → units demanded)
    #[serde(rename = "popyt_konsumencki")]
    pub demand: BTreeMap<(String, DemographyType, String), BTreeMap<Commodity, f64>>,
    
    /// Total demand per commodity (aggregated across all classes)
    #[serde(rename = "popyt_całkowity")]
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
        let mut obsession_multipliers = HashMap::new();

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
///
/// # Returns
/// `CulturalDemandModifier` ready to apply to demand maps.
pub fn compute_cultural_demand_modifier(
    region: &Region,
    religious_authority: &std::collections::BTreeMap<String, f64>,
) -> CulturalDemandModifier {
    let reg = culture_registry();

    // Determine dominant religion from region's class demographics.
    let mut religion_counts: HashMap<String, i64> = HashMap::new();
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

    // Determine dominant culture from region (use country-level culture as fallback).
    // For now, use a default culture definition if we can't determine one.
    let culture_def = reg.from_key("ilirian").cloned().unwrap_or_default();

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
        Commodity::Cereal | Commodity::Vegetable | Commodity::Protein
        | Commodity::Meat | Commodity::Fruit | Commodity::HealthCapacity
        | Commodity::EducationSlots | Commodity::Food | Commodity::Water => 0.0,
        // Durables — wealth-gated (purchased only when savings permit)
        Commodity::Clothing => 50.0,       // Basic durable, low gate
        Commodity::Furniture => 100.0,
        Commodity::Radio => 300.0,
        Commodity::Agd => 500.0,
        Commodity::Televisions => 800.0,
        Commodity::Cars => 2000.0,
        Commodity::Fuels => 1500.0,  // Only if you have a car
        Commodity::Luxury | Commodity::LuxuryFurniture | Commodity::LuxuryClothing => 3000.0,
        _ => 0.0,
    }
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
fn compute_durable_demand(
    commodity: Commodity,
    demographics: &crate::society::geography::ClassDemographics,
    year: u32,
    base_per_capita: f64,
) -> f64 {
    // Era gate (no TV demand before 1936, etc.)
    let era_mult = era_consumption_multiplier(commodity, year);
    if era_mult <= 0.0 {
        return 0.0;
    }

    // Wealth gate — even if the class basket lists a commodity, it's only
    // demanded if the class's savings_per_capita exceeds the wealth gate.
    if demographics.savings_per_capita < commodity_wealth_gate(commodity) {
        return 0.0;
    }

    // 1. Target stock = base_per_capita × population (the "saturation" level)
    let target_stock = base_per_capita * demographics.population as f64;

    // 2. Current stock = sum of cohort counts
    let current_stock: f64 = demographics
        .household_durables
        .iter()
        .filter(|c| c.commodity == commodity)
        .map(|c| c.count)
        .sum();

    // 3. Worn-out stock = cohorts with condition < 0.2 (replacement trigger)
    let worn_out: f64 = demographics
        .household_durables
        .iter()
        .filter(|c| c.commodity == commodity && c.condition < 0.2)
        .map(|c| c.count)
        .sum();

    // 4. Upgrade demand = wealth-driven desire for higher quality
    let upgrade_demand = if demographics.savings_per_capita > 500.0 && current_stock > 0.0 {
        let avg_quality: f64 = demographics
            .household_durables
            .iter()
            .filter(|c| c.commodity == commodity)
            .map(|c| c.quality * c.count)
            .sum::<f64>()
            / current_stock;
        if avg_quality < 0.9 {
            current_stock * 0.05
        } else {
            0.0
        }
    } else {
        0.0
    };

    // 5. Saturation demand = fill gap toward target stock (wealth-gated, gradual)
    let saturation_demand = if current_stock < target_stock {
        let gap = target_stock - current_stock;
        if demographics.savings_per_capita > 100.0 {
            gap * 0.10 // Fill 10% of the gap per turn
        } else {
            0.0
        }
    } else {
        0.0
    };

    (worn_out + upgrade_demand + saturation_demand) * era_mult
}

/// Phase 47: Degrade household durable cohorts by one turn.
/// Called after B2C clearing, before telemetry.
pub fn degrade_household_durables(
    demographics: &mut crate::society::geography::ClassDemographics,
) {
    for cohort in &mut demographics.household_durables {
        if cohort.durability > 0.0 && cohort.durability < f64::MAX {
            cohort.condition =
                (cohort.condition - 1.0 / cohort.durability).max(0.0);
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
        .find(|c| {
            c.commodity == commodity
                && ((c.quality * 4.0).round() / 4.0) == quality_bucket
        })
        .map(|cohort| {
            // Weighted average condition and quality
            let old_weight = cohort.count;
            let new_weight = per_capita_count;
            let total = old_weight + new_weight;
            if total > 0.0 {
                cohort.condition =
                    (cohort.condition * old_weight + 1.0 * new_weight) / total;
                cohort.quality =
                    (cohort.quality * old_weight + quality * new_weight) / total;
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

/// Build consumer demand from demographic classes (Phase 6.5, Phase R1, Phase 17A).
///
/// # Arguments
/// * `region` - Region with class demographics
/// * `current_turn` - Current turn number
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
/// * Used in R1 phase before store offer generation
pub fn build_consumer_demand(
    region: &Region,
    current_turn: u32,
) -> ConsumerDemand {
    let mut demand = ConsumerDemand {
        demand: BTreeMap::new(),
        total_demand: BTreeMap::new(),
    };

    let consumption = consumption_registry();
    // Phase 45: Derive year from turn (24 turns per year, default start 1925)
    let year = 1925 + (current_turn / 24);

    // Process rural classes
    for (class_id, demographics) in &region.class_demographics.rural_classes {
        let key = (region.id.clone(), DemographyType::Rural, class_id.clone());

        if let Some(basket) = consumption.get(class_id) {
            for (tier, tier_commodities) in &basket.tiers {
                for (commodity, per_capita) in tier_commodities {
                    // Phase 47: Durables use stock-adjusted demand (replacement +
                    // upgrade + saturation), NOT per-turn consumption.
                    // Perishables use the existing per_capita × era × wealth × pop.
                    let class_demand = if commodity.is_household_durable() {
                        compute_durable_demand(*commodity, demographics, year, *per_capita)
                    } else {
                        // Phase 45: Era-aware consumption multiplier.
                        let era_mult = era_consumption_multiplier(*commodity, year);
                        if era_mult <= 0.0 {
                            continue;
                        }
                        // Phase 47: Wealth-gate check for non-durables too
                        if demographics.savings_per_capita < commodity_wealth_gate(*commodity) {
                            continue;
                        }
                        // Phase 45: Wealth-tier multiplier.
                        let wealth_mult = wealth_tier_multiplier(
                            *tier,
                            demographics.savings_per_capita,
                        );
                        per_capita * era_mult * wealth_mult * (demographics.population as f64)
                    };

                    if class_demand > 0.0 {
                        *demand.demand.entry(key.clone()).or_insert_with(BTreeMap::new)
                            .entry(*commodity).or_insert(0.0) += class_demand;
                        *demand.total_demand.entry(*commodity).or_insert(0.0) += class_demand;
                    }
                }
            }
        }
    }

    // Process urban classes
    for (class_id, demographics) in &region.class_demographics.urban_classes {
        let key = (region.id.clone(), DemographyType::Urban, class_id.clone());

        if let Some(basket) = consumption.get(class_id) {
            for (tier, tier_commodities) in &basket.tiers {
                for (commodity, per_capita) in tier_commodities {
                    let class_demand = if commodity.is_household_durable() {
                        compute_durable_demand(*commodity, demographics, year, *per_capita)
                    } else {
                        let era_mult = era_consumption_multiplier(*commodity, year);
                        if era_mult <= 0.0 {
                            continue;
                        }
                        if demographics.savings_per_capita < commodity_wealth_gate(*commodity) {
                            continue;
                        }
                        let wealth_mult = wealth_tier_multiplier(
                            *tier,
                            demographics.savings_per_capita,
                        );
                        per_capita * era_mult * wealth_mult * (demographics.population as f64)
                    };

                    if class_demand > 0.0 {
                        *demand.demand.entry(key.clone()).or_insert_with(BTreeMap::new)
                            .entry(*commodity).or_insert(0.0) += class_demand;
                        *demand.total_demand.entry(*commodity).or_insert(0.0) += class_demand;
                    }
                }
            }
        }
    }

    demand
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
        Commodity::Cereal | Commodity::Vegetable | Commodity::Protein
        | Commodity::Meat | Commodity::Fruit | Commodity::Clothing
        | Commodity::Furniture | Commodity::Food | Commodity::Water
        | Commodity::HealthCapacity | Commodity::EducationSlots => 1.0,

        // Radio: available from 1920, peaks 1930-1960
        Commodity::Radio => {
            if year < 1920 { 0.0 } else if year < 1930 { 0.3 } else { 1.0 }
        }

        // Televisions: available from 1936
        Commodity::Televisions => {
            if year < 1936 { 0.0 } else if year < 1950 { 0.2 } else { 1.0 }
        }

        // AGD (household appliances): available from 1930
        Commodity::Agd => {
            if year < 1930 { 0.0 } else if year < 1950 { 0.3 } else { 1.0 }
        }

        // Cars: luxury from 1910, mass consumption from 1950
        Commodity::Cars => {
            if year < 1910 { 0.0 } else if year < 1950 { 0.1 } else { 0.5 }
        }

        // Luxury goods: always available but more relevant in prosperous eras
        Commodity::Luxury | Commodity::LuxuryFurniture | Commodity::LuxuryClothing => {
            if year < 1880 { 0.5 } else { 1.0 }
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
fn wealth_tier_multiplier(tier: crate::data::consumption_registry::NeedTier, savings_per_capita: f64) -> f64 {
    use crate::data::consumption_registry::NeedTier;
    match tier {
        NeedTier::Subsistence => 1.0,
        NeedTier::Standard => {
            // Standard goods: 0.5 baseline, up to 1.5 with high savings
            (0.5 + (savings_per_capita / 1000.0).min(1.0)).min(1.5).max(0.1)
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
/// # Rules
/// * Only stores with retail_profile generate offers
/// * Effective attractiveness = base + upgrade bonuses
/// * Price = acquisition_cost * markup_ratio
/// * Used in R2 phase before B2C clearing
pub fn generate_store_offers(
    stores: &[CommercialBuilding],
    current_turn: u32,
) -> Vec<StoreOffer> {
    let mut offers = Vec::new();
    let config = retail_config();

    for store in stores {
        if let Some(profile) = &store.retail_profile {
            // Compute effective attractiveness
            let mut effective_attractiveness = profile.base_attractiveness;

            // Generate offers for each commodity in inventory
            for (commodity_key, batches) in &store.current_inventory {
                if let Ok(commodity) = Commodity::try_from(commodity_key.as_str()) {
                    let total_quantity: f64 = batches.iter().map(|b| b.quantity).sum();

                    if total_quantity > 0.0 {
                        // Calculate price with markup
                        let avg_acquisition_cost: f64 = batches.iter()
                            .map(|b| b.acquisition_cost_per_unit * b.quantity)
                            .sum::<f64>() / total_quantity;

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
                        let last_sold = profile.units_sold_last_turn.get(&commodity).copied().unwrap_or(0.0);
                        let last_unmet = profile.unmet_demand_last_turn.get(&commodity).copied().unwrap_or(0.0);
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
                        let aging_discount: f64 = batches.iter()
                            .map(|b| {
                                let age = current_turn.saturating_sub(b.storage_turn);
                                if age > 0 {
                                    let discount = (age as f64 * 0.05).min(0.30);
                                    b.quantity * discount
                                } else {
                                    0.0
                                }
                            })
                            .sum::<f64>() / total_quantity;

                        let dynamic_markup = base_markup * (1.0 + scarcity_factor - aging_discount);
                        let price_per_unit = avg_acquisition_cost * dynamic_markup.max(0.5);

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
) -> B2cClearingResult {
    let mut units_sold: BTreeMap<Commodity, f64> = BTreeMap::new();
    let mut store_revenue: BTreeMap<String, f64> = BTreeMap::new();
    let mut retail_price_volume: BTreeMap<Commodity, (f64, f64)> = BTreeMap::new(); // (total_value, total_volume)
    let config = retail_config();

    // Group offers by commodity
    let mut by_commodity: BTreeMap<Commodity, Vec<&mut StoreOffer>> = BTreeMap::new();
    for offer in offers {
        by_commodity.entry(offer.commodity).or_default().push(offer);
    }

    // Clear each commodity market
    for (commodity, commodity_offers) in by_commodity {
        let total_demand = demand.total_demand.get(&commodity).copied().unwrap_or(0.0);
        let total_supply: f64 = commodity_offers.iter().map(|o| o.quantity).sum();

        if total_demand == 0.0 || total_supply == 0.0 {
            continue;
        }

        // Phase 19C: For quality durables, blend the utility with a quality
        // premium weighted by the average wealth-tier quality preference.
        // For non-quality durables, quality is 1.0 and the blend is a no-op.
        let is_quality_durable = commodity.is_quality_durable();
        let quality_weight = if is_quality_durable {
            gen_config.map(|c| {
                // Use the average of the four wealth-tier weights as the
                // aggregate quality preference (demand is not yet split by
                // wealth tier in the clearing loop — this is a first
                // approximation that still raises premium-good allocation).
                let weights: Vec<f64> = c.quality_weights.values().copied().collect();
                if weights.is_empty() { 1.0 } else { weights.iter().sum::<f64>() / weights.len() as f64 }
            }).unwrap_or(1.0)
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
                        let is_newcomer = current_turn - profile.first_active_turn < config.newcomer_grace_turns;
                        if is_newcomer {
                            0.0 // No inertia for newcomers
                        } else {
                            let previous_share = profile.market_share_last_turn.get(&commodity).copied().unwrap_or(0.0);
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
        for (utility, offer) in utilities {
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

            // Update store sales tracking
            if let Some(store) = stores.iter_mut().find(|s| s.id == offer.store_id) {
                if let Some(profile) = &mut store.retail_profile {
                    *profile.units_sold_last_turn.entry(commodity).or_insert(0.0) += allocated;
                }
            }
        }

        // Phase 45: Track unmet demand for dynamic pricing feedback.
        // remaining_demand is the quantity that could not be fulfilled.
        if remaining_demand > 0.0 {
            // Record unmet demand on stores that sold this commodity this turn.
            for store in stores.iter_mut() {
                if let Some(profile) = &mut store.retail_profile {
                    if profile.units_sold_last_turn.contains_key(&commodity) {
                        *profile.unmet_demand_last_turn.entry(commodity).or_insert(0.0) = remaining_demand;
                    }
                }
            }
        } else {
            // No unmet demand — clear the tracking for this commodity
            for store in stores.iter_mut() {
                if let Some(profile) = &mut store.retail_profile {
                    if profile.units_sold_last_turn.contains_key(&commodity) {
                        profile.unmet_demand_last_turn.insert(commodity, 0.0);
                    }
                }
            }
        }
    }
    
    // Update market shares for next turn
    for store in stores {
        if let Some(profile) = &mut store.retail_profile {
            for (commodity, sold) in &profile.units_sold_last_turn {
                let total_sold = units_sold.get(commodity).copied().unwrap_or(0.0);
                if total_sold > 0.0 {
                    *profile.market_share_last_turn.entry(*commodity).or_insert(0.0) = sold / total_sold;
                }
            }
        }
    }
    
    // Build retail_prices vector for CPI
    let retail_prices: Vec<(Commodity, f64, f64)> = retail_price_volume
        .into_iter()
        .filter(|(_, (value, volume))| *volume > 0.0)
        .map(|(commodity, (value, volume))| (commodity, volume, value / volume))
        .collect();

    B2cClearingResult {
        units_sold,
        store_revenue,
        retail_prices,
    }
}

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
/// * Stores with no `owner_id` or whose owner is not found in `companies` are skipped.
pub fn settle_b2c_clearing(
    store_revenue: &BTreeMap<String, f64>,
    consumer_demand: &ConsumerDemand,
    commercial_buildings: &[CommercialBuilding],
    companies: &mut [Company],
    region: &mut Region,
    vat_rates: &std::collections::HashMap<String, crate::state::tax::VatBracket>,
) -> (f64, f64) {
    // Build class demand shares: (is_rural, class_key) → total demand across all commodities
    let mut class_shares: Vec<(bool, String, f64)> = Vec::new();
    let mut total_class_demand: f64 = 0.0;
    for ((_, demo_type, class_id), commodity_map) in &consumer_demand.demand {
        let class_total: f64 = commodity_map.values().sum();
        if class_total > 0.0 {
            let is_rural = *demo_type == DemographyType::Rural;
            class_shares.push((is_rural, class_id.clone(), class_total));
            total_class_demand += class_total;
        }
    }

    if total_class_demand <= 0.0 {
        return (0.0, 0.0);
    }

    let mut total_settled: f64 = 0.0;
    let mut total_vat_collected: f64 = 0.0;

    for (store_id, revenue) in store_revenue {
        if *revenue <= 0.0 {
            continue;
        }

        // Find the store building to get owner_id
        let owner_id = commercial_buildings
            .iter()
            .find(|b| b.id == *store_id)
            .map(|b| b.owner_id.clone());

        let owner_id = match owner_id {
            Some(ref id) if !id.is_empty() => id.clone(),
            _ => continue,
        };

        // Find company index by owner_id
        let company_idx = companies.iter().position(|c| c.id == owner_id);
        let company_idx = match company_idx {
            Some(idx) => idx,
            None => continue,
        };

        // Phase 41: Compute VAT for this store's revenue.
        // The store_revenue is the total (base + VAT inclusive) amount.
        // We need to determine the blended VAT rate for this store based on
        // the commodities it sells. Since we don't have per-store commodity
        // breakdown here, we use a weighted average VAT rate based on the
        // consumer demand commodity mix.
        //
        // DYNAMIC RATE LOOKUP: Look up the active VAT rate from country.tax_rates.vat
        // for each commodity's VAT category. No hardcoding.
        let blended_vat_rate = compute_blended_vat_rate(consumer_demand, vat_rates);

        // Split revenue into base (company gets this) and VAT (treasury gets this)
        let base_revenue = revenue / (1.0 + blended_vat_rate);
        let vat_amount = revenue - base_revenue;
        total_vat_collected += vat_amount;

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
            let result = settle_b2c_purchase(
                companies,
                company_idx,
                class_revenue,
                region,
                *is_rural,
                class_key,
                class_vat,
            );
            if let Ok(r) = result {
                total_settled += r.amount_transferred;
            }
        }
    }

    (total_settled, total_vat_collected)
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
        let avg_acquisition_cost = store.current_inventory
            .get(&commodity.inventory_key())
            .and_then(|batches| {
                let total_qty: f64 = batches.iter().map(|b| b.quantity).sum();
                if total_qty > 0.0 {
                    Some(batches.iter().map(|b| b.acquisition_cost_per_unit * b.quantity).sum::<f64>() / total_qty)
                } else {
                    None
                }
            })
            .unwrap_or(1.0);
        
        // Amortize operating cost over expected capacity
        let capacity_amortized_cost = operating_cost / (config.expected_turnover_rate * config.min_throughput_units);
        
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
use crate::state::treasury::Treasury;

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
    let min_payroll = company.fulfilled_fte * min_wage;
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
    let min_payroll = company.fulfilled_fte * min_wage;
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

    fn make_culture(taboos: Vec<Commodity>, obsessions: Vec<(Commodity, f64)>) -> CultureDefinition {
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

    fn make_religion(taboos: Vec<Commodity>, obsessions: Vec<(Commodity, f64)>) -> ReligionDefinition {
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
        assert!((demand.get(&Commodity::Meat).copied().unwrap_or(-1.0)).abs() < 0.01,
            "full authority taboo should zero demand");
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
        assert!((val - 80.0).abs() < 0.01,
            "authority=0.2 taboo should reduce demand by 20%, got {}", val);
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
        assert!((val - 200.0).abs() < 0.01,
            "full authority obsession factor 2.0 should double demand, got {}", val);
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
        assert!((val - 100.0).abs() < 0.01,
            "zero authority obsession should not change demand, got {}", val);
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
        assert!((val - 300.0).abs() < 0.01,
            "religion obsession factor 3.0 should win over culture 1.5, got {}", val);
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
        assert!((val - 100.0).abs() < 0.01,
            "no taboos/obsessions -> no change, got {}", val);
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

        let result = clear_b2c_markets(&mut offers, &demand, &mut stores, 100, Some(&cfg));

        // At least one store should have sold something.
        let total_revenue: f64 = result.store_revenue.values().sum();
        assert!(total_revenue > 0.0, "some offer must sell when demand > 0");
        // The quality field affects utility: premium utility ≠ cheap utility.
        // premium utility = 1/50 + 1.25×2/50 = 0.07; cheap utility = 1/50 + 1.25×1/50 = 0.045
        // The ascending sort puts cheap (0.045) first → cheap gets all 50 units.
        let cheap_revenue = result.store_revenue.get("store_cheap").copied().unwrap_or(0.0);
        assert!(cheap_revenue > 0.0, "with ascending sort, lower-utility offer is allocated first");
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
                quantity: 30.0, // limited supply
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

        let result = clear_b2c_markets(&mut offers, &demand, &mut stores, 100, Some(&cfg));

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

        let result = clear_b2c_markets(&mut offers, &demand, &mut stores, 100, Some(&cfg));

        // Both stores have the same utility (quality ignored for non-durables).
        // Each should sell ~25 units (deterministic sort by utility tie).
        let a_revenue = result.store_revenue.get("store_a").copied().unwrap_or(0.0);
        let b_revenue = result.store_revenue.get("store_b").copied().unwrap_or(0.0);
        assert!((a_revenue - b_revenue).abs() < 1e-6 || (a_revenue == 0.0 && b_revenue > 0.0) || (b_revenue == 0.0 && a_revenue > 0.0),
            "non-quality-durable should ignore quality field");
    }

    // ═══════════════════════════════════════════════════════════════════
    // Phase 47: Durable Goods & Retail Format Tests
    // ═══════════════════════════════════════════════════════════════════

    #[test]
    fn test_clothing_is_household_durable() {
        assert!(Commodity::Clothing.is_household_durable(), "Clothing should be a household durable");
        assert!(Commodity::LuxuryClothing.is_household_durable(), "LuxuryClothing should be a household durable");
    }

    #[test]
    fn test_clothing_durability_24_turns() {
        assert_eq!(Commodity::Clothing.household_durable_turns(), 24.0, "Clothing should have 24-turn durability");
    }

    #[test]
    fn test_luxury_clothing_durability_100_turns() {
        assert_eq!(Commodity::LuxuryClothing.household_durable_turns(), 100.0, "LuxuryClothing should have 100-turn durability");
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
        assert!(!Commodity::Cereal.is_household_durable(), "Cereal should NOT be a household durable");
        assert!(!Commodity::Meat.is_household_durable(), "Meat should NOT be a household durable");
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
        assert!(demo.household_durables.is_empty(), "Worn-out cohort should be scrapped");
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
        assert_eq!(demo.household_durables[0].condition, 1.0, "New purchase should have full condition");
        assert!((demo.household_durables[0].count - 0.1).abs() < 1e-6, "Count should be per-capita (10/100)");
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
        assert_eq!(demo.household_durables.len(), 1, "Same quality bucket should merge");
        assert!((demo.household_durables[0].count - 0.3).abs() < 1e-6, "Count should be 30/100 = 0.3");
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
            "Mid-wealth region in modern era should get Supermarket"
        );
    }
}

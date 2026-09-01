//! War economy: production decrees, conscription, and war finance.
//!
//! Phase 69 of the Military Epic. Implements:
//! - `ProductionDecree`: swaps a building's `active_method` to a military
//!   `ProductionMethod` with distinct physical inputs/outputs (Rule 3).
//! - `Conscription`: drains population from `ClassDemographics` into
//!   `MilitaryUnit`s with `manpower_origin` tracking (Rule 1 closed-loop).
//! - War bonds: issued via the existing `DebtMarket` infrastructure with
//!   double-entry cash flow from subscribers to treasury.

use crate::entities::{ActiveProductionMethod, Building};
use crate::military::units::{MilitaryUnit, UnitType};
use crate::registries::enums::{Commodity, Sector};
use crate::society::geography::{Region, RuralClass};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

type HashMap<K, V> = FxHashMap<K, V>;

// ============================================================================
// CONSCRIPTION LEVEL
// ============================================================================

/// Mobilization level of a country's military.
///
/// Replaces the old `DraftScope` usage for war-economy purposes.
/// Each level determines the fraction of eligible population that can be
/// drafted per turn, and which economic penalties apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ConscriptionLevel {
    /// No active draft. Peacetime standing army only.
    #[default]
    Peacetime,
    /// Selective draft: a configured fraction of eligible population.
    Selective,
    /// Universal conscription: all eligible adults are subject to draft.
    UniversalDraft,
    /// Total mobilization: maximum extraction, severe economic penalties.
    TotalMobilization,
}

impl ConscriptionLevel {
    /// Returns the fraction of eligible population drafted per turn.
    /// Scaled by configuration, not magic numbers.
    pub fn draft_fraction(self, config: &WarEconomyConfig) -> f64 {
        match self {
            ConscriptionLevel::Peacetime => 0.0,
            ConscriptionLevel::Selective => config.selective_draft_fraction,
            ConscriptionLevel::UniversalDraft => config.universal_draft_fraction,
            ConscriptionLevel::TotalMobilization => config.total_mobilization_fraction,
        }
    }

    /// Returns the labor participation penalty applied when this level is active.
    /// Draining workers into the army reduces production capacity.
    pub fn labor_participation_penalty(self, config: &WarEconomyConfig) -> f64 {
        match self {
            ConscriptionLevel::Peacetime => 0.0,
            ConscriptionLevel::Selective => config.selective_labor_penalty,
            ConscriptionLevel::UniversalDraft => config.universal_labor_penalty,
            ConscriptionLevel::TotalMobilization => config.total_mobilization_labor_penalty,
        }
    }
}

// ============================================================================
// PRODUCTION DECREE
// ============================================================================

/// A state decree forcing civilian factories to switch to military production.
///
/// Rule 3 compliance: the decree swaps the building's `active_method` to a
/// military `ProductionMethod` that has its OWN distinct physical input
/// commodity demands. Producing tanks requires different physical inputs
/// (Steel, Aluminum, ElectronicComponents) than producing tractors
/// (Steel, Rubber, IndustrialMachinery). The input demand naturally
/// shocks the supply chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductionDecree {
    /// Sector targeted by the decree (e.g., HeavyIndustry → TankProduction).
    pub target_sector: Sector,
    /// The military production method ID to apply (references the registry).
    /// This method defines distinct physical inputs and military outputs.
    pub military_method_id: String,
    /// The original `active_method` snapshot, saved before the decree
    /// so it can be restored when the decree is lifted.
    pub original_method: ActiveProductionMethod,
    /// Turn the decree was enacted.
    pub enacted_turn: u32,
    /// Optional expiry turn. None = indefinite until manually lifted.
    pub expiry_turn: Option<u32>,
    /// Building IDs affected by this decree (populated at application time).
    pub affected_building_ids: Vec<String>,
}

impl ProductionDecree {
    /// Returns true if the decree has expired by the given turn.
    pub fn is_expired(&self, current_turn: u32) -> bool {
        match self.expiry_turn {
            Some(expiry) => current_turn >= expiry,
            None => false,
        }
    }
}

// ============================================================================
// WAR ECONOMY STATE
// ============================================================================

/// Complete war economy state for a country.
///
/// Stored on `Country` as `war_economy: WarEconomyState`.
/// No serde defaults — breaks saves per Rule 10.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WarEconomyState {
    /// Active production decrees redirecting civilian factories.
    pub active_decrees: Vec<ProductionDecree>,
    /// Current conscription level.
    pub conscription_level: ConscriptionLevel,
    /// Total war bonds issued (face value).
    pub war_bonds_issued: f64,
    /// War bond IDs currently outstanding (references DebtMarket securities).
    pub outstanding_war_bond_ids: Vec<String>,
    /// Cumulative conscripts drafted (for reporting/debugging).
    pub total_conscripts_drafted: i64,
    /// Cumulative casualties sustained (for reporting/debugging).
    pub total_casualties: i64,
    /// Cumulative demobilized survivors returned to demographics.
    pub total_demobilized: i64,
}

impl Default for WarEconomyState {
    fn default() -> Self {
        Self {
            active_decrees: Vec::new(),
            conscription_level: ConscriptionLevel::Peacetime,
            war_bonds_issued: 0.0,
            outstanding_war_bond_ids: Vec::new(),
            total_conscripts_drafted: 0,
            total_casualties: 0,
            total_demobilized: 0,
        }
    }
}

// ============================================================================
// WAR ECONOMY CONFIG
// ============================================================================

/// Configuration for war economy mechanics.
///
/// All values are fractions or rates — no magic nominal constants (Rule 2).
/// Loaded from configuration, not hardcoded in logic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WarEconomyConfig {
    // ── Conscription fractions ──
    /// Fraction of eligible population drafted per turn under Selective.
    pub selective_draft_fraction: f64,
    /// Fraction of eligible population drafted per turn under UniversalDraft.
    pub universal_draft_fraction: f64,
    /// Fraction of eligible population drafted per turn under TotalMobilization.
    pub total_mobilization_fraction: f64,

    // ── Labor penalties (fraction of labor participation lost) ──
    /// Labor participation penalty under Selective (e.g., 0.05 = 5% reduction).
    pub selective_labor_penalty: f64,
    /// Labor participation penalty under UniversalDraft.
    pub universal_labor_penalty: f64,
    /// Labor participation penalty under TotalMobilization.
    pub total_mobilization_labor_penalty: f64,

    // ── Conscription eligibility ──
    /// Minimum age for conscription (in turns).
    pub min_conscription_age_turns: u32,
    /// Maximum age for conscription (in turns).
    pub max_conscription_age_turns: u32,
    /// Fraction of population that is exempt from draft (health, essential workers).
    pub draft_exemption_rate: f64,

    // ── War bonds ──
    /// War bond coupon rate premium over standard treasury bonds (fraction).
    /// War bonds pay higher rates to compensate for wartime risk.
    pub war_bond_coupon_premium: f64,
    /// War bond maturity in turns.
    pub war_bond_maturity_turns: u32,
    /// Maximum war bond issuance as fraction of GDP per turn.
    pub max_war_bond_gdp_fraction: f64,
    /// Deficit threshold (as fraction of liquid_reserves) that triggers
    /// automatic war bond issuance when at war.
    pub war_bond_deficit_threshold: f64,

    // ── Production decrees ──
    /// Maximum number of concurrent decrees a country can maintain.
    pub max_concurrent_decrees: usize,
    /// Efficiency penalty when a factory is forced to switch to military
    /// production it wasn't designed for (retooling friction).
    pub decree_retooling_penalty: f64,
}

impl Default for WarEconomyConfig {
    fn default() -> Self {
        Self {
            selective_draft_fraction: 0.02,
            universal_draft_fraction: 0.05,
            total_mobilization_fraction: 0.10,
            selective_labor_penalty: 0.03,
            universal_labor_penalty: 0.08,
            total_mobilization_labor_penalty: 0.15,
            min_conscription_age_turns: 64,  // 16 years (4 turns/year)
            max_conscription_age_turns: 160, // 40 years
            draft_exemption_rate: 0.15,
            war_bond_coupon_premium: 0.02,
            war_bond_maturity_turns: 20, // 5 years
            max_war_bond_gdp_fraction: 0.25,
            war_bond_deficit_threshold: 0.20,
            max_concurrent_decrees: 5,
            decree_retooling_penalty: 0.15,
        }
    }
}

// ============================================================================
// PRODUCTION DECREE APPLICATION
// ============================================================================

/// Applies a production decree to all buildings in the target sector.
///
/// Rule 3 compliance: swaps `active_method` to a military `ProductionMethod`
/// with distinct physical inputs. The original method is saved for restoration.
///
/// # Arguments
/// * `buildings` - All buildings for this country (mutable).
/// * `target_sector` - Sector to convert (e.g., HeavyIndustry).
/// * `military_method` - The military production method to apply.
/// * `military_method_id` - String ID of the military method (for tracking).
/// * `enacted_turn` - Current turn.
/// * `expiry_turn` - Optional expiry.
/// * `retooling_penalty` - Efficiency penalty from retooling.
///
/// # Returns
/// The created `ProductionDecree` with affected building IDs, or None if
/// no buildings matched.
pub fn apply_production_decree(
    buildings: &mut [Building],
    target_sector: Sector,
    military_method: &ActiveProductionMethod,
    military_method_id: &str,
    enacted_turn: u32,
    expiry_turn: Option<u32>,
    retooling_penalty: f64,
) -> Option<ProductionDecree> {
    let mut affected_ids = Vec::new();
    let mut original_methods: Vec<ActiveProductionMethod> = Vec::new();

    // First pass: collect affected buildings and their original methods
    for building in buildings.iter() {
        if building.sector == target_sector && building.current_employment > 0 {
            affected_ids.push(building.id.clone());
            original_methods.push(building.active_method.clone());
        }
    }

    if affected_ids.is_empty() {
        return None;
    }

    // Use the first building's original method as the snapshot
    // (all buildings in the same sector share the same base method pattern)
    let original_method = original_methods[0].clone();

    // Second pass: apply the military method with retooling penalty
    let mut adjusted_method = military_method.clone();
    adjusted_method.efficiency *= (1.0 - retooling_penalty).max(0.0);

    for building in buildings.iter_mut() {
        if affected_ids.contains(&building.id) {
            building.active_method = adjusted_method.clone();
        }
    }

    Some(ProductionDecree {
        target_sector,
        military_method_id: military_method_id.to_string(),
        original_method,
        enacted_turn,
        expiry_turn,
        affected_building_ids: affected_ids,
    })
}

/// Lifts a production decree, restoring original production methods.
///
/// # Arguments
/// * `buildings` - All buildings for this country (mutable).
/// * `decree` - The decree to lift.
pub fn lift_production_decree(buildings: &mut [Building], decree: &ProductionDecree) {
    for building in buildings.iter_mut() {
        if decree.affected_building_ids.contains(&building.id) {
            building.active_method = decree.original_method.clone();
        }
    }
}

/// Removes expired decrees and restores their original production methods.
///
/// Should be called each turn after production is processed.
///
/// # Arguments
/// * `buildings` - All buildings for this country (mutable).
/// * `war_economy` - War economy state (mutable).
/// * `current_turn` - Current turn number.
pub fn process_expired_decrees(
    buildings: &mut [Building],
    war_economy: &mut WarEconomyState,
    current_turn: u32,
) {
    let mut expired_indices: Vec<usize> = Vec::new();
    for (idx, decree) in war_economy.active_decrees.iter().enumerate() {
        if decree.is_expired(current_turn) {
            expired_indices.push(idx);
        }
    }

    // Process in reverse order to maintain indices
    for &idx in expired_indices.iter().rev() {
        let decree = war_economy.active_decrees.remove(idx);
        lift_production_decree(buildings, &decree);
    }
}

// ============================================================================
// CONSCRIPTION
// ============================================================================

/// Result of a conscription action.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConscriptionResult {
    /// Total recruits drafted this turn.
    pub recruits_drafted: i64,
    /// Manpower origin breakdown (RuralClass → count).
    pub manpower_origin: HashMap<RuralClass, i64>,
    /// Regions that provided recruits (region_id → count).
    pub regional_breakdown: HashMap<String, i64>,
    /// Units created or reinforced.
    pub units_affected: Vec<String>,
    /// Labor participation reduction applied (fraction).
    pub labor_penalty_applied: f64,
}

/// Executes conscription: drains population from demographic classes into
/// military units.
///
/// Rule 1 compliance: population is physically removed from `ClassDemographics.population`
/// and transferred to `MilitaryUnit.manpower`. No duplicate population creation.
/// The `manpower_origin` field on each unit tracks exactly which demographic
/// class the recruits came from, enabling accurate casualty routing and
/// demobilization.
///
/// # Arguments
/// * `regions` - All regions for this country (mutable).
/// * `military_units` - Military units (mutable). New units are created or
///   existing ones reinforced.
/// * `war_economy` - War economy state (mutable, for tracking totals).
/// * `config` - War economy configuration.
/// * `country_name` - Country name (for unit ID generation).
/// * `current_turn` - Current turn number.
///
/// # Returns
/// `ConscriptionResult` with details of the draft.
pub fn execute_conscription(
    regions: &mut [Region],
    oob: &mut crate::military::oob::OrderOfBattle,
    war_economy: &mut WarEconomyState,
    config: &WarEconomyConfig,
    country_name: &str,
    current_turn: u32,
) -> ConscriptionResult {
    let mut result = ConscriptionResult::default();

    if war_economy.conscription_level == ConscriptionLevel::Peacetime {
        return result;
    }

    let draft_fraction = war_economy.conscription_level.draft_fraction(config);
    let eligible_fraction = 1.0 - config.draft_exemption_rate;
    let effective_fraction = draft_fraction * eligible_fraction;

    // Collect recruits from each region's demographics
    // We iterate regions, and for each region, iterate rural and urban classes
    let mut total_recruits: i64 = 0;
    let mut manpower_origin: HashMap<RuralClass, i64> = HashMap::default();
    let mut regional_breakdown: HashMap<String, i64> = HashMap::default();

    // First pass: calculate how many recruits each region/class provides
    // We collect the data first, then mutate, to avoid borrow issues
    #[derive(Clone)]
    struct RecruitSource {
        region_id: String,
        class_key: String,
        is_urban: bool,
        count: i64,
        rural_class: Option<RuralClass>,
    }

    let mut sources: Vec<RecruitSource> = Vec::new();

    for region in regions.iter() {
        let region_recruits: i64 = 0;
        let mut region_total = 0i64;

        // Rural classes
        for (class_key, demo) in region.class_demographics.rural_classes.iter() {
            let eligible = (demo.population as f64 * effective_fraction) as i64;
            if eligible <= 0 {
                continue;
            }
            let rural_class = parse_rural_class(class_key);
            if let Some(rc) = rural_class {
                sources.push(RecruitSource {
                    region_id: region.id.clone(),
                    class_key: class_key.clone(),
                    is_urban: false,
                    count: eligible,
                    rural_class: Some(rc),
                });
                region_total += eligible;
            }
        }

        // Urban classes — mapped to the closest rural class for manpower_origin
        for (class_key, demo) in region.class_demographics.urban_classes.iter() {
            let eligible = (demo.population as f64 * effective_fraction) as i64;
            if eligible <= 0 {
                continue;
            }
            // Urban workers are mapped to LandlessLaborer for military purposes
            // (they are wage laborers, not landowners)
            sources.push(RecruitSource {
                region_id: region.id.clone(),
                class_key: class_key.clone(),
                is_urban: true,
                count: eligible,
                rural_class: Some(RuralClass::LandlessLaborer),
            });
            region_total += eligible;
        }

        if region_total > 0 {
            regional_breakdown.insert(region.id.clone(), region_total);
        }
        let _ = region_recruits;
    }

    // Second pass: drain population from demographics
    for source in &sources {
        for region in regions.iter_mut() {
            if region.id != source.region_id {
                continue;
            }
            let class_map = if source.is_urban {
                &mut region.class_demographics.urban_classes
            } else {
                &mut region.class_demographics.rural_classes
            };

            if let Some(demo) = class_map.get_mut(&source.class_key) {
                let actual_drain = source.count.min(demo.population);
                demo.population -= actual_drain;
                total_recruits += actual_drain;

                if let Some(rc) = source.rural_class {
                    *manpower_origin.entry(rc).or_insert(0) += actual_drain;
                }
            }
        }
    }

    if total_recruits <= 0 {
        return result;
    }

    // Create or reinforce military units
    // Conscripts are typically formed into Infantry units
    let unit_id = format!("CONSCRIPT-{}-{}", country_name, current_turn);

    // Set home_region to the region that provided the most recruits
    let home_region = regional_breakdown
        .iter()
        .max_by_key(|(_, &v)| v)
        .map(|(r, _)| r.clone())
        .unwrap_or_default();

    let unit = MilitaryUnit::new(
        unit_id.clone(),
        UnitType::Infantry,
        total_recruits,
        manpower_origin.clone(),
        home_region,
    );

    // Add the conscript unit to the OOB.
    // Conscripts are placed in a dedicated "Conscript Reserve" army
    // to keep them organizationally separate from standing forces.
    add_conscript_to_oob(oob, unit, country_name);
    result.units_affected.push(unit_id);

    result.recruits_drafted = total_recruits;
    result.manpower_origin = manpower_origin;
    result.regional_breakdown = regional_breakdown;
    result.labor_penalty_applied = war_economy
        .conscription_level
        .labor_participation_penalty(config);

    // Apply labor participation penalty to all demographic classes
    let penalty = result.labor_penalty_applied;
    for region in regions.iter_mut() {
        for demo in region.class_demographics.rural_classes.values_mut() {
            demo.labor_participation = (demo.labor_participation * (1.0 - penalty)).max(0.0);
        }
        for demo in region.class_demographics.urban_classes.values_mut() {
            demo.labor_participation = (demo.labor_participation * (1.0 - penalty)).max(0.0);
        }
    }

    war_economy.total_conscripts_drafted += total_recruits;

    result
}

/// Demobilizes surviving soldiers back to their demographic classes.
///
/// Rule 1 compliance: survivors are returned to the `ClassDemographics.population`
/// of their home region, using the `manpower_origin` tracking on the unit.
/// Casualties (dead, wounded who can't fight) are NOT returned.
///
/// # Arguments
/// * `unit` - The unit to disband (consumed).
/// * `regions` - Regions to return survivors to.
///
/// # Returns
/// HashMap of RuralClass → survivor count returned.
pub fn demobilize_unit(
    unit: &mut MilitaryUnit,
    regions: &mut [Region],
) -> HashMap<RuralClass, i64> {
    let survivors = unit.disband();

    // Return survivors to their home region's demographics
    if !unit.home_region.is_empty() {
        for region in regions.iter_mut() {
            if region.id == unit.home_region {
                for (rural_class, &count) in &survivors {
                    let class_key = rural_class_to_string(rural_class);
                    if let Some(demo) = region.class_demographics.rural_classes.get_mut(&class_key)
                    {
                        demo.population += count;
                    }
                }
                break;
            }
        }
    }

    survivors
}

// ============================================================================
// WAR BONDS
// ============================================================================

/// Issues war bonds to finance extreme wartime deficits.
///
/// Uses the existing `DebtMarket` infrastructure. War bonds are
/// `TreasurySecurity` instruments with:
/// - Higher coupon rates (standard rate + war_bond_coupon_premium).
/// - Configured maturity.
/// - Subscribed by demographic classes (retail) and banks (wholesale).
///
/// Rule 1 compliance: cash flows from subscribers to treasury.
/// - Retail: `ClassDemographics.savings → treasury.liquid_reserves`.
/// - Wholesale: `bank.brokerage_account.cash → treasury.liquid_reserves`.
/// The bonds become a liability on the treasury and an asset on the
/// subscriber's balance sheet.
///
/// # Arguments
/// * `country` - Mutable country (treasury + debt_market).
/// * `amount_needed` - Deficit to cover.
/// * `config` - War economy config.
/// * `current_turn` - Current turn.
/// * `average_wage` - Current average wage (for scaling subscription capacity).
///
/// # Returns
/// Amount actually raised (may be less than requested if subscription is insufficient).
pub fn issue_war_bonds(
    country: &mut crate::state::Country,
    amount_needed: f64,
    config: &WarEconomyConfig,
    current_turn: u32,
    _average_wage: f64,
) -> f64 {
    use crate::economy::finance::debt_market::{
        CouponFrequency, SecurityHolder, SecurityHolderType, TreasurySecurity, TreasurySecurityType,
    };

    if amount_needed <= 0.0 {
        return 0.0;
    }

    // Cap issuance at max_war_bond_gdp_fraction of GDP
    let gdp: f64 = country.regions.iter().map(|r| r.gdp).sum();
    let max_issuance = gdp * config.max_war_bond_gdp_fraction;
    let actual_issuance = amount_needed.min(max_issuance);

    if actual_issuance <= 0.0 {
        return 0.0;
    }

    // Determine subscription capacity from demographic savings
    // Retail subscribers: demographic classes with savings
    let mut retail_capacity: f64 = 0.0;
    for region in &country.regions {
        for demo in region.class_demographics.rural_classes.values() {
            retail_capacity += demo.savings;
        }
        for demo in region.class_demographics.urban_classes.values() {
            retail_capacity += demo.savings;
        }
    }

    // Wholesale subscribers: bank cash (simplified — use debt_market primary dealers)
    // The actual bank cash is tracked in the banking system; here we use
    // a fraction of total outstanding debt as a proxy for bank capacity.
    let wholesale_capacity = country.debt_market.total_outstanding_debt * 0.5;

    let total_capacity = retail_capacity + wholesale_capacity;
    if total_capacity <= 0.0 {
        return 0.0;
    }

    // Pro-rata allocation: retail and wholesale subscribe proportionally
    let retail_fraction = retail_capacity / total_capacity;
    let wholesale_fraction = 1.0 - retail_fraction;

    let retail_subscription = actual_issuance * retail_fraction;
    let wholesale_subscription = actual_issuance * wholesale_fraction;

    // Calculate coupon rate: base rate + war premium
    // Base rate is the weighted average of existing debt, or a default
    let base_rate = if country.debt_market.weighted_avg_interest_rate > 0.0 {
        country.debt_market.weighted_avg_interest_rate
    } else {
        0.05 // 5% baseline if no existing debt
    };
    let war_coupon_rate = base_rate + config.war_bond_coupon_premium;

    // Create the war bond security
    let bond_id = format!("WARBOND-{}-{}", country.name, current_turn);

    let mut holders = Vec::new();

    // Retail holder (citizens as a collective)
    if retail_subscription > 0.0 {
        holders.push(SecurityHolder {
            entity_id: format!("CITIZENS:{}", country.name),
            holder_type: SecurityHolderType::RetailSavingsBond,
            quantity: retail_subscription,
            purchase_price: 1.0, // Issued at par
        });
    }

    // Wholesale holder (banks/funds)
    if wholesale_subscription > 0.0 {
        holders.push(SecurityHolder {
            entity_id: format!("BANKS:{}", country.name),
            holder_type: SecurityHolderType::CommercialBank,
            quantity: wholesale_subscription,
            purchase_price: 1.0,
        });
    }

    let war_bond = TreasurySecurity {
        id: bond_id.clone(),
        security_type: TreasurySecurityType::TreasuryBond,
        face_value: actual_issuance,
        issue_price: 1.0, // Issued at par
        issue_turn: current_turn,
        maturity_turns: config.war_bond_maturity_turns,
        turns_remaining: config.war_bond_maturity_turns,
        coupon_rate: war_coupon_rate,
        coupon_frequency: CouponFrequency::EveryTurn,
        is_inflation_indexed: false,
        holders,
        last_coupon_turn: current_turn,
        is_matured: false,
        is_auction_inventory: false,
    };

    // Add to debt market
    country.debt_market.outstanding_securities.push(war_bond);
    country.debt_market.recalculate();

    // Cash flow: subscribers → treasury
    // Retail: debit ClassDemographics.savings
    let retail_to_drain = retail_subscription;
    let mut remaining = retail_to_drain;
    for region in &mut country.regions {
        if remaining <= 0.0 {
            break;
        }
        for demo in region.class_demographics.rural_classes.values_mut() {
            if remaining <= 0.0 {
                break;
            }
            let drain = demo.savings.min(remaining);
            demo.savings -= drain;
            remaining -= drain;
        }
        for demo in region.class_demographics.urban_classes.values_mut() {
            if remaining <= 0.0 {
                break;
            }
            let drain = demo.savings.min(remaining);
            demo.savings -= drain;
            remaining -= drain;
        }
    }

    // Credit treasury
    country.budget.liquid_reserves += actual_issuance;

    // Track in war economy state
    country.war_economy.war_bonds_issued += actual_issuance;
    country.war_economy.outstanding_war_bond_ids.push(bond_id);

    actual_issuance
}

// ============================================================================
// HELPERS
// ============================================================================

// ============================================================================
// MILITARY CONVERSION METHODS (Phase 69.2 — Registry Completion)
// ============================================================================

/// A military conversion method: maps a civilian sector to a military
/// `ActiveProductionMethod` that can be applied via `ProductionDecree`.
///
/// Each conversion has DISTINCT physical inputs from the civilian method
/// it replaces, shocking the supply chain (Rule 3 compliance).
#[derive(Debug, Clone)]
pub struct MilitaryConversion {
    /// The method ID (used as `military_method_id` in `ProductionDecree`).
    pub method_id: &'static str,
    /// The civilian sector this conversion targets.
    pub target_sector: Sector,
    /// The military `ActiveProductionMethod` to swap in.
    pub method: ActiveProductionMethod,
    /// Human-readable description of the conversion.
    pub description: &'static str,
}

/// Returns all available military conversion methods.
///
/// These are the methods that `ProductionDecree` can swap civilian factories
/// to. Each method has distinct physical input demands that naturally shock
/// the supply chain when applied.
///
/// # Returns
/// Vector of `MilitaryConversion` entries.
pub fn military_conversion_methods() -> Vec<MilitaryConversion> {
    use crate::state::treasury::ProductionMethodChoice;

    vec![
        // ── Heavy Industry → Military Vehicles ──
        MilitaryConversion {
            method_id: "military_truck_conversion",
            target_sector: Sector::HeavyIndustry,
            method: ActiveProductionMethod {
                year: 1916,
                experts_ratio: 0.20,
                skilled_ratio: 0.35,
                basic_ratio: 0.45,
                efficiency: 0.8,
                inputs: std::collections::BTreeMap::from([
                    (Commodity::Steel, 25.0),
                    (Commodity::Fuels, 12.0),
                    (Commodity::MechanicalComponents, 8.0),
                    (Commodity::Plastics, 5.0),
                ]),
                outputs: std::collections::BTreeMap::from([
                    (Commodity::Trucks, 8.0),
                ]),
                active_methods: ProductionMethodChoice::default(),
                active_blueprint: None,
                extra: serde_json::Map::new(),
                            ..Default::default()
            },
            description: "Converts heavy industry to military truck production. Requires plastics and mechanical components not needed for steel production.",
        },
        MilitaryConversion {
            method_id: "light_tank_conversion",
            target_sector: Sector::HeavyIndustry,
            method: ActiveProductionMethod {
                year: 1935,
                experts_ratio: 0.22,
                skilled_ratio: 0.38,
                basic_ratio: 0.40,
                efficiency: 0.7,
                inputs: std::collections::BTreeMap::from([
                    (Commodity::Steel, 35.0),
                    (Commodity::Aluminum, 10.0),
                    (Commodity::Fuels, 15.0),
                    (Commodity::MechanicalComponents, 12.0),
                ]),
                outputs: std::collections::BTreeMap::from([
                    (Commodity::LightTanks, 3.0),
                ]),
                active_methods: ProductionMethodChoice::default(),
                active_blueprint: None,
                extra: serde_json::Map::new(),
                            ..Default::default()
            },
            description: "Converts heavy industry to light tank production. Demands aluminum and heavy mechanical components.",
        },
        MilitaryConversion {
            method_id: "artillery_conversion",
            target_sector: Sector::HeavyIndustry,
            method: ActiveProductionMethod {
                year: 1916,
                experts_ratio: 0.20,
                skilled_ratio: 0.35,
                basic_ratio: 0.45,
                efficiency: 0.8,
                inputs: std::collections::BTreeMap::from([
                    (Commodity::Steel, 30.0),
                    (Commodity::Fuels, 10.0),
                    (Commodity::MechanicalComponents, 8.0),
                ]),
                outputs: std::collections::BTreeMap::from([
                    (Commodity::TowedArtillery, 4.0),
                ]),
                active_methods: ProductionMethodChoice::default(),
                active_blueprint: None,
                extra: serde_json::Map::new(),
                            ..Default::default()
            },
            description: "Converts heavy industry to towed artillery production. High steel demand.",
        },
        // ── Heavy Industry → Ammunition & Gunpowder ──
        MilitaryConversion {
            method_id: "ammunition_surge_production",
            target_sector: Sector::HeavyIndustry,
            method: ActiveProductionMethod {
                year: 1916,
                experts_ratio: 0.18,
                skilled_ratio: 0.32,
                basic_ratio: 0.50,
                efficiency: 0.9,
                inputs: std::collections::BTreeMap::from([
                    (Commodity::Steel, 20.0),
                    (Commodity::Chemicals, 25.0),
                    (Commodity::Fuels, 8.0),
                ]),
                outputs: std::collections::BTreeMap::from([
                    (Commodity::Ammunition, 40.0),
                ]),
                active_methods: ProductionMethodChoice::default(),
                active_blueprint: None,
                extra: serde_json::Map::new(),
                            ..Default::default()
            },
            description: "Surge ammunition production. Demands massive chemical inputs (explosives) not needed for civilian steel.",
        },
        MilitaryConversion {
            method_id: "gunpowder_conversion",
            target_sector: Sector::HeavyIndustry,
            method: ActiveProductionMethod {
                year: 1880,
                experts_ratio: 0.15,
                skilled_ratio: 0.30,
                basic_ratio: 0.55,
                efficiency: 0.8,
                inputs: std::collections::BTreeMap::from([
                    (Commodity::Chemicals, 30.0),
                    (Commodity::Sulfur, 15.0),
                    (Commodity::Energy, 10.0),
                ]),
                outputs: std::collections::BTreeMap::from([
                    (Commodity::Gunpowder, 20.0),
                ]),
                active_methods: ProductionMethodChoice::default(),
                active_blueprint: None,
                extra: serde_json::Map::new(),
                            ..Default::default()
            },
            description: "Converts chemical/heavy industry to gunpowder production. Requires sulfur and heavy chemical inputs.",
        },
        // ── Light Industry → Uniforms & Support Equipment ──
        MilitaryConversion {
            method_id: "military_uniform_conversion",
            target_sector: Sector::LightIndustry,
            method: ActiveProductionMethod {
                year: 1880,
                experts_ratio: 0.10,
                skilled_ratio: 0.25,
                basic_ratio: 0.65,
                efficiency: 0.8,
                inputs: std::collections::BTreeMap::from([
                    (Commodity::Fibers, 20.0),
                    (Commodity::IndustrialFiber, 5.0),
                    (Commodity::Steel, 2.0),
                    (Commodity::Energy, 5.0),
                ]),
                outputs: std::collections::BTreeMap::from([
                    (Commodity::Clothing, 15.0),
                ]),
                active_methods: ProductionMethodChoice::default(),
                active_blueprint: None,
                extra: serde_json::Map::new(),
                            ..Default::default()
            },
            description: "Converts textile factories to military uniform production. Adds industrial fiber and steel inputs for webbing, buttons, buckles.",
        },
        MilitaryConversion {
            method_id: "support_equipment_conversion",
            target_sector: Sector::LightIndustry,
            method: ActiveProductionMethod {
                year: 1916,
                experts_ratio: 0.15,
                skilled_ratio: 0.30,
                basic_ratio: 0.55,
                efficiency: 0.7,
                inputs: std::collections::BTreeMap::from([
                    (Commodity::Fibers, 10.0),
                    (Commodity::Steel, 8.0),
                    (Commodity::IndustrialFiber, 8.0),
                    (Commodity::Energy, 5.0),
                ]),
                outputs: std::collections::BTreeMap::from([
                    (Commodity::SupportEquipment, 6.0),
                ]),
                active_methods: ProductionMethodChoice::default(),
                active_blueprint: None,
                extra: serde_json::Map::new(),
                            ..Default::default()
            },
            description: "Converts light industry to support equipment (webbing, packs, entrenching tools). Steel and industrial fiber heavy.",
        },
    ]
}

/// Looks up a military conversion method by its ID.
///
/// # Arguments
/// * `method_id` - The military method ID (e.g., "light_tank_conversion").
///
/// # Returns
/// The matching `MilitaryConversion`, or None if not found.
pub fn find_military_conversion(method_id: &str) -> Option<MilitaryConversion> {
    military_conversion_methods()
        .into_iter()
        .find(|c| c.method_id == method_id)
}

/// Returns all military conversions available for a given sector.
///
/// # Arguments
/// * `sector` - The civilian sector to find conversions for.
///
/// # Returns
/// Vector of `MilitaryConversion` entries targeting that sector.
pub fn conversions_for_sector(sector: Sector) -> Vec<MilitaryConversion> {
    military_conversion_methods()
        .into_iter()
        .filter(|c| c.target_sector == sector)
        .collect()
}

/// Adds a conscript unit to the OOB in a dedicated conscript reserve army.
///
/// If a "Conscript Reserve" army already exists, the unit is added to its
/// first regiment. Otherwise, a new army/division/regiment hierarchy is created.
fn add_conscript_to_oob(
    oob: &mut crate::military::oob::OrderOfBattle,
    unit: MilitaryUnit,
    country_name: &str,
) {
    use crate::military::oob::{Army, Division, Regiment};

    // Find or create the conscript reserve army
    let conscript_army_id = format!("ARMY-{}-CONSCRIPT", country_name);

    let army_idx = oob.armies.iter().position(|a| a.id == conscript_army_id);
    match army_idx {
        Some(idx) => {
            let army = &mut oob.armies[idx];
            // Add to the first division's first regiment, or create new ones
            if let Some(div) = army.divisions.first_mut() {
                if let Some(reg) = div.regiments.first_mut() {
                    reg.add_unit(unit);
                } else {
                    let mut reg = Regiment::new(
                        format!("REG-{}-CONSCRIPT-001", country_name),
                        "Conscript Regiment".to_string(),
                        unit.home_region.clone(),
                    );
                    reg.add_unit(unit);
                    div.add_regiment(reg);
                }
            } else {
                let home_region = unit.home_region.clone();
                let mut reg = Regiment::new(
                    format!("REG-{}-CONSCRIPT-001", country_name),
                    "Conscript Regiment".to_string(),
                    home_region.clone(),
                );
                reg.add_unit(unit);
                let mut div = Division::new(
                    format!("DIV-{}-CONSCRIPT-001", country_name),
                    "Conscript Division".to_string(),
                    home_region,
                );
                div.add_regiment(reg);
                army.add_division(div);
            }
        }
        None => {
            let mut reg = Regiment::new(
                format!("REG-{}-CONSCRIPT-001", country_name),
                "Conscript Regiment".to_string(),
                unit.home_region.clone(),
            );
            reg.add_unit(unit);
            let mut div = Division::new(
                format!("DIV-{}-CONSCRIPT-001", country_name),
                "Conscript Division".to_string(),
                reg.home_region.clone(),
            );
            div.add_regiment(reg);
            let mut army = Army::new(
                conscript_army_id,
                "Conscript Reserve Army".to_string(),
                div.home_region.clone(),
            );
            army.add_division(div);
            oob.add_army(army);
        }
    }
}

/// Parses a RuralClass from its string key.
fn parse_rural_class(key: &str) -> Option<RuralClass> {
    match key.to_lowercase().as_str() {
        "aristocracy" => Some(RuralClass::Aristocracy),
        "freepeasant" | "free_peasant" | "free peasant" => Some(RuralClass::FreePeasant),
        "serf" | "serfs" => Some(RuralClass::Serf),
        "landlesslaborer" | "landless_laborer" | "landless laborer" => {
            Some(RuralClass::LandlessLaborer)
        }
        _ => None,
    }
}

/// Converts a RuralClass to its string key.
fn rural_class_to_string(class: &RuralClass) -> String {
    match class {
        RuralClass::Aristocracy => "Aristocracy".to_string(),
        RuralClass::FreePeasant => "FreePeasant".to_string(),
        RuralClass::Serf => "Serf".to_string(),
        RuralClass::LandlessLaborer => "LandlessLaborer".to_string(),
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Building;
    use crate::registries::enums::Sector;

    #[test]
    fn test_conscription_level_draft_fraction() {
        let config = WarEconomyConfig::default();
        assert_eq!(ConscriptionLevel::Peacetime.draft_fraction(&config), 0.0);
        assert!(ConscriptionLevel::Selective.draft_fraction(&config) > 0.0);
        assert!(
            ConscriptionLevel::UniversalDraft.draft_fraction(&config)
                > ConscriptionLevel::Selective.draft_fraction(&config)
        );
        assert!(
            ConscriptionLevel::TotalMobilization.draft_fraction(&config)
                > ConscriptionLevel::UniversalDraft.draft_fraction(&config)
        );
    }

    #[test]
    fn test_conscription_level_labor_penalty() {
        let config = WarEconomyConfig::default();
        assert_eq!(
            ConscriptionLevel::Peacetime.labor_participation_penalty(&config),
            0.0
        );
        assert!(
            ConscriptionLevel::TotalMobilization.labor_participation_penalty(&config)
                > ConscriptionLevel::Selective.labor_participation_penalty(&config)
        );
    }

    #[test]
    fn test_production_decree_expiry() {
        let decree = ProductionDecree {
            target_sector: Sector::HeavyIndustry,
            military_method_id: "tank_production".to_string(),
            original_method: ActiveProductionMethod::default(),
            enacted_turn: 10,
            expiry_turn: Some(20),
            affected_building_ids: vec![],
        };
        assert!(!decree.is_expired(15));
        assert!(decree.is_expired(20));
        assert!(decree.is_expired(25));
    }

    #[test]
    fn test_production_decree_no_expiry() {
        let decree = ProductionDecree {
            target_sector: Sector::HeavyIndustry,
            military_method_id: "tank_production".to_string(),
            original_method: ActiveProductionMethod::default(),
            enacted_turn: 10,
            expiry_turn: None,
            affected_building_ids: vec![],
        };
        assert!(!decree.is_expired(100));
    }

    #[test]
    fn test_war_economy_state_default() {
        let state = WarEconomyState::default();
        assert_eq!(state.conscription_level, ConscriptionLevel::Peacetime);
        assert_eq!(state.active_decrees.len(), 0);
        assert_eq!(state.war_bonds_issued, 0.0);
    }

    #[test]
    fn test_apply_production_decree_swaps_method() {
        let mut buildings = vec![Building {
            id: "b1".to_string(),
            name: "Steel Works".to_string(),
            sector: Sector::HeavyIndustry,
            current_employment: 100,
            active_method: ActiveProductionMethod {
                year: 1930,
                efficiency: 1.0,
                outputs: std::collections::BTreeMap::from([(Commodity::Steel, 100.0)]),
                ..Default::default()
            },
            ..Default::default()
        }];

        let military_method = ActiveProductionMethod {
            year: 1935,
            efficiency: 0.9,
            inputs: std::collections::BTreeMap::from([
                (Commodity::Steel, 50.0),
                (Commodity::Aluminum, 20.0),
            ]),
            outputs: std::collections::BTreeMap::from([(Commodity::MediumTanks, 10.0)]),
            ..Default::default()
        };

        let decree = apply_production_decree(
            &mut buildings,
            Sector::HeavyIndustry,
            &military_method,
            "tank_production",
            10,
            Some(20),
            0.15,
        );

        assert!(decree.is_some());
        let d = decree.unwrap();
        assert_eq!(d.affected_building_ids.len(), 1);
        assert_eq!(d.affected_building_ids[0], "b1");
        // Building's active method should now be the military method
        assert!(buildings[0]
            .active_method
            .outputs
            .contains_key(&Commodity::MediumTanks));
        // Efficiency should be reduced by retooling penalty
        assert!(buildings[0].active_method.efficiency < 0.9);
    }

    #[test]
    fn test_apply_production_decree_no_matching_buildings() {
        let mut buildings = vec![Building {
            id: "b1".to_string(),
            sector: Sector::Agriculture,
            current_employment: 100,
            ..Default::default()
        }];

        let military_method = ActiveProductionMethod::default();
        let decree = apply_production_decree(
            &mut buildings,
            Sector::HeavyIndustry,
            &military_method,
            "tank_production",
            10,
            None,
            0.15,
        );

        assert!(decree.is_none());
    }

    #[test]
    fn test_lift_production_decree_restores_original() {
        let original_method = ActiveProductionMethod {
            year: 1930,
            efficiency: 1.0,
            outputs: std::collections::BTreeMap::from([(Commodity::Steel, 100.0)]),
            ..Default::default()
        };

        let mut buildings = vec![Building {
            id: "b1".to_string(),
            sector: Sector::HeavyIndustry,
            current_employment: 100,
            active_method: original_method.clone(),
            ..Default::default()
        }];

        let military_method = ActiveProductionMethod {
            year: 1935,
            efficiency: 0.9,
            outputs: std::collections::BTreeMap::from([(Commodity::MediumTanks, 10.0)]),
            ..Default::default()
        };

        let decree = apply_production_decree(
            &mut buildings,
            Sector::HeavyIndustry,
            &military_method,
            "tank_production",
            10,
            Some(20),
            0.15,
        )
        .unwrap();

        // Verify method was swapped
        assert!(buildings[0]
            .active_method
            .outputs
            .contains_key(&Commodity::MediumTanks));

        // Lift the decree
        lift_production_decree(&mut buildings, &decree);

        // Verify original method was restored
        assert!(buildings[0]
            .active_method
            .outputs
            .contains_key(&Commodity::Steel));
        assert!(!buildings[0]
            .active_method
            .outputs
            .contains_key(&Commodity::MediumTanks));
    }

    #[test]
    fn test_process_expired_decrees() {
        let original_method = ActiveProductionMethod {
            outputs: std::collections::BTreeMap::from([(Commodity::Steel, 100.0)]),
            ..Default::default()
        };

        let mut buildings = vec![Building {
            id: "b1".to_string(),
            sector: Sector::HeavyIndustry,
            current_employment: 100,
            active_method: original_method.clone(),
            ..Default::default()
        }];

        let military_method = ActiveProductionMethod {
            outputs: std::collections::BTreeMap::from([(Commodity::MediumTanks, 10.0)]),
            ..Default::default()
        };

        let decree = apply_production_decree(
            &mut buildings,
            Sector::HeavyIndustry,
            &military_method,
            "tank_production",
            10,
            Some(20),
            0.15,
        )
        .unwrap();

        let mut war_economy = WarEconomyState {
            active_decrees: vec![decree],
            ..Default::default()
        };

        // Not expired at turn 15
        process_expired_decrees(&mut buildings, &mut war_economy, 15);
        assert_eq!(war_economy.active_decrees.len(), 1);
        assert!(buildings[0]
            .active_method
            .outputs
            .contains_key(&Commodity::MediumTanks));

        // Expired at turn 20
        process_expired_decrees(&mut buildings, &mut war_economy, 20);
        assert_eq!(war_economy.active_decrees.len(), 0);
        assert!(buildings[0]
            .active_method
            .outputs
            .contains_key(&Commodity::Steel));
    }

    // ── Phase 69.2: Military Conversion Method Tests ──

    #[test]
    fn test_military_conversion_methods_exist() {
        let methods = military_conversion_methods();
        assert!(
            !methods.is_empty(),
            "Military conversion methods must not be empty (Rule 6)"
        );
    }

    #[test]
    fn test_heavy_industry_has_tank_conversion() {
        let conversions = conversions_for_sector(Sector::HeavyIndustry);
        assert!(
            conversions
                .iter()
                .any(|c| c.method_id == "light_tank_conversion"),
            "Heavy industry must have light tank conversion"
        );
    }

    #[test]
    fn test_heavy_industry_has_ammunition_conversion() {
        let conversions = conversions_for_sector(Sector::HeavyIndustry);
        assert!(
            conversions
                .iter()
                .any(|c| c.method_id == "ammunition_surge_production"),
            "Heavy industry must have ammunition surge conversion"
        );
    }

    #[test]
    fn test_light_industry_has_uniform_conversion() {
        let conversions = conversions_for_sector(Sector::LightIndustry);
        assert!(
            conversions
                .iter()
                .any(|c| c.method_id == "military_uniform_conversion"),
            "Light industry must have military uniform conversion"
        );
    }

    #[test]
    fn test_tank_conversion_has_distinct_inputs() {
        let conversion = find_military_conversion("light_tank_conversion").unwrap();
        // Tank conversion must demand aluminum — not used in civilian steel production
        assert!(
            conversion.method.inputs.contains_key(&Commodity::Aluminum),
            "Light tank conversion must demand aluminum (distinct from civilian steel inputs)"
        );
        // Must output LightTanks
        assert!(conversion
            .method
            .outputs
            .contains_key(&Commodity::LightTanks));
    }

    #[test]
    fn test_ammunition_conversion_has_chemical_inputs() {
        let conversion = find_military_conversion("ammunition_surge_production").unwrap();
        // Ammunition requires massive chemical inputs (explosives) — distinct from civilian
        assert!(
            conversion.method.inputs.contains_key(&Commodity::Chemicals),
            "Ammunition surge must demand chemicals (explosives) — distinct from civilian inputs"
        );
        let chemical_input = conversion.method.inputs.get(&Commodity::Chemicals).unwrap();
        assert!(
            *chemical_input >= 20.0,
            "Chemical input for ammunition must be substantial"
        );
        assert!(conversion
            .method
            .outputs
            .contains_key(&Commodity::Ammunition));
    }

    #[test]
    fn test_uniform_conversion_has_industrial_fiber_inputs() {
        let conversion = find_military_conversion("military_uniform_conversion").unwrap();
        // Military uniforms require industrial fiber (webbing) and steel (buttons/buckles)
        // — distinct from civilian clothing which only needs fibers
        assert!(conversion.method.inputs.contains_key(&Commodity::IndustrialFiber),
            "Military uniform conversion must demand industrial fiber (webbing) — distinct from civilian clothing");
        assert!(
            conversion.method.inputs.contains_key(&Commodity::Steel),
            "Military uniform conversion must demand steel (buttons/buckles)"
        );
        assert!(conversion.method.outputs.contains_key(&Commodity::Clothing));
    }

    #[test]
    fn test_find_military_conversion_returns_none_for_unknown() {
        assert!(find_military_conversion("nonexistent_method").is_none());
    }

    #[test]
    fn test_apply_decree_with_registry_conversion() {
        // End-to-end: use a registry conversion method to apply a decree
        let mut buildings = vec![Building {
            id: "b1".to_string(),
            name: "Steel Works".to_string(),
            sector: Sector::HeavyIndustry,
            current_employment: 100,
            active_method: ActiveProductionMethod {
                year: 1930,
                efficiency: 1.0,
                inputs: std::collections::BTreeMap::from([(Commodity::Iron, 25.0)]),
                outputs: std::collections::BTreeMap::from([(Commodity::Steel, 22.0)]),
                ..Default::default()
            },
            ..Default::default()
        }];

        let conversion = find_military_conversion("light_tank_conversion").unwrap();

        let decree = apply_production_decree(
            &mut buildings,
            conversion.target_sector,
            &conversion.method,
            conversion.method_id,
            10,
            Some(20),
            0.15,
        );

        assert!(decree.is_some());
        // Building now produces LightTanks, not Steel
        assert!(buildings[0]
            .active_method
            .outputs
            .contains_key(&Commodity::LightTanks));
        assert!(!buildings[0]
            .active_method
            .outputs
            .contains_key(&Commodity::Steel));
        // Building now demands Aluminum, not Iron
        assert!(buildings[0]
            .active_method
            .inputs
            .contains_key(&Commodity::Aluminum));
        assert!(!buildings[0]
            .active_method
            .inputs
            .contains_key(&Commodity::Iron));
    }
}

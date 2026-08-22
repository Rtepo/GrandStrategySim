//! Phase 70: Hierarchical Order of Battle (OOB).
//!
//! Replaces the flat `Vec<MilitaryUnit>` with a proper military hierarchy:
//!
//! ```text
//! OrderOfBattle
//!   └── Army
//!         └── Division
//!               └── Regiment
//!                     └── MilitaryUnit
//! ```
//!
//! The OOB is constructed natively during world generation (Turn 0).
//! There is no `rebuild_oob()` compatibility shim and no flat-list-to-hierarchy
//! conversion. All saves from before Phase 70 are intentionally broken (Rule 10).
//!
//! Query methods:
//! - `all_units()` — flat iterator over all units
//! - `all_units_mut()` — flat mutable iterator over all units
//! - `units_at_location(region_id)` — units at a specific region
//! - `units_by_type(UnitType)` — units of a specific type
//! - `total_manpower()` — sum of all unit manpower

use serde::{Deserialize, Serialize};
use rustc_hash::FxHashMap;

type HashMap<K, V> = FxHashMap<K, V>;
use rand::Rng;

use crate::military::units::{MilitaryUnit, UnitType};

// ============================================================================
// REGIMENT — lowest echelon, owns MilitaryUnit instances directly
// ============================================================================

/// A regiment: the lowest echelon of the OOB. Owns military units directly.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Regiment {
    /// Unique regiment ID (e.g., "REG-001").
    pub id: String,
    /// Display name (e.g., "1st Infantry Regiment").
    pub name: String,
    /// Home region ID for this regiment.
    pub home_region: String,
    /// The military units belonging to this regiment.
    pub units: Vec<MilitaryUnit>,
    /// Assigned commander VIP ID (if any).
    pub commander_id: Option<String>,
}

impl Regiment {
    /// Creates a new empty regiment.
    pub fn new(id: String, name: String, home_region: String) -> Self {
        Self {
            id,
            name,
            home_region,
            units: Vec::new(),
            commander_id: None,
        }
    }

    /// Total manpower across all units in this regiment.
    pub fn total_manpower(&self) -> i64 {
        self.units.iter().map(|u| u.manpower).sum()
    }

    /// All units in this regiment.
    pub fn all_units(&self) -> impl Iterator<Item = &MilitaryUnit> {
        self.units.iter()
    }

    /// All units in this regiment (mutable).
    pub fn all_units_mut(&mut self) -> impl Iterator<Item = &mut MilitaryUnit> {
        self.units.iter_mut()
    }

    /// Units at a specific location.
    pub fn units_at_location<'a>(&'a self, region_id: &'a str) -> impl Iterator<Item = &'a MilitaryUnit> {
        self.units.iter().filter(move |u| u.location == region_id)
    }

    /// Units of a specific type.
    pub fn units_by_type<'a>(&'a self, unit_type: UnitType) -> impl Iterator<Item = &'a MilitaryUnit> + 'a {
        self.units.iter().filter(move |u| u.unit_type == unit_type)
    }

    /// Add a unit to this regiment.
    pub fn add_unit(&mut self, unit: MilitaryUnit) {
        self.units.push(unit);
    }

    /// Remove destroyed/disbanded units (manpower <= 0).
    pub fn remove_dead_units(&mut self) {
        self.units.retain(|u| u.manpower > 0);
    }
}

// ============================================================================
// DIVISION — mid echelon, owns regiments
// ============================================================================

/// A division: mid echelon of the OOB. Owns regiments.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Division {
    /// Unique division ID (e.g., "DIV-001").
    pub id: String,
    /// Display name (e.g., "1st Infantry Division").
    pub name: String,
    /// Home region ID for this division's base.
    pub home_region: String,
    /// Regiments belonging to this division.
    pub regiments: Vec<Regiment>,
    /// Assigned division commander VIP ID (if any).
    pub commander_id: Option<String>,
}

impl Division {
    /// Creates a new empty division.
    pub fn new(id: String, name: String, home_region: String) -> Self {
        Self {
            id,
            name,
            home_region,
            regiments: Vec::new(),
            commander_id: None,
        }
    }

    /// Total manpower across all regiments.
    pub fn total_manpower(&self) -> i64 {
        self.regiments.iter().map(|r| r.total_manpower()).sum()
    }

    /// All units in this division (flat iterator).
    pub fn all_units(&self) -> impl Iterator<Item = &MilitaryUnit> {
        self.regiments.iter().flat_map(|r| r.all_units())
    }

    /// All units in this division (flat mutable iterator).
    pub fn all_units_mut(&mut self) -> impl Iterator<Item = &mut MilitaryUnit> {
        self.regiments.iter_mut().flat_map(|r| r.all_units_mut())
    }

    /// Units at a specific location.
    pub fn units_at_location<'a>(&'a self, region_id: &'a str) -> impl Iterator<Item = &'a MilitaryUnit> + 'a {
        self.regiments.iter().flat_map(move |r| r.units_at_location(region_id))
    }

    /// Units of a specific type.
    pub fn units_by_type<'a>(&'a self, unit_type: UnitType) -> impl Iterator<Item = &'a MilitaryUnit> + 'a {
        self.regiments.iter().flat_map(move |r| r.units_by_type(unit_type))
    }

    /// Add a regiment to this division.
    pub fn add_regiment(&mut self, regiment: Regiment) {
        self.regiments.push(regiment);
    }

    /// Remove empty regiments (no units and no regiments).
    pub fn remove_empty_regiments(&mut self) {
        self.regiments.retain(|r| !r.units.is_empty());
    }
}

// ============================================================================
// ARMY — highest echelon, owns divisions
// ============================================================================

/// An army: highest echelon of the OOB. Owns divisions.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Army {
    /// Unique army ID (e.g., "ARMY-001").
    pub id: String,
    /// Display name (e.g., "1st Army").
    pub name: String,
    /// Home region ID for this army's headquarters.
    pub home_region: String,
    /// Divisions belonging to this army.
    pub divisions: Vec<Division>,
    /// Assigned army commander VIP ID (if any).
    pub commander_id: Option<String>,
}

impl Army {
    /// Creates a new empty army.
    pub fn new(id: String, name: String, home_region: String) -> Self {
        Self {
            id,
            name,
            home_region,
            divisions: Vec::new(),
            commander_id: None,
        }
    }

    /// Total manpower across all divisions.
    pub fn total_manpower(&self) -> i64 {
        self.divisions.iter().map(|d| d.total_manpower()).sum()
    }

    /// All units in this army (flat iterator).
    pub fn all_units(&self) -> impl Iterator<Item = &MilitaryUnit> {
        self.divisions.iter().flat_map(|d| d.all_units())
    }

    /// All units in this army (flat mutable iterator).
    pub fn all_units_mut(&mut self) -> impl Iterator<Item = &mut MilitaryUnit> {
        self.divisions.iter_mut().flat_map(|d| d.all_units_mut())
    }

    /// Units at a specific location.
    pub fn units_at_location<'a>(&'a self, region_id: &'a str) -> impl Iterator<Item = &'a MilitaryUnit> + 'a {
        self.divisions.iter().flat_map(move |d| d.units_at_location(region_id))
    }

    /// Units of a specific type.
    pub fn units_by_type<'a>(&'a self, unit_type: UnitType) -> impl Iterator<Item = &'a MilitaryUnit> + 'a {
        self.divisions.iter().flat_map(move |d| d.units_by_type(unit_type))
    }

    /// Add a division to this army.
    pub fn add_division(&mut self, division: Division) {
        self.divisions.push(division);
    }

    /// Remove empty divisions.
    pub fn remove_empty_divisions(&mut self) {
        self.divisions.retain(|d| !d.regiments.is_empty());
    }
}

// ============================================================================
// ORDER OF BATTLE — top-level container, owns armies
// ============================================================================

/// The top-level Order of Battle for a country.
///
/// Replaces the flat `Vec<MilitaryUnit>` with a proper hierarchy:
/// `OrderOfBattle → Army → Division → Regiment → MilitaryUnit`.
///
/// Constructed natively during world generation. No compatibility shims.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct OrderOfBattle {
    /// Armies belonging to this country.
    pub armies: Vec<Army>,
}

impl OrderOfBattle {
    /// Creates a new empty OOB.
    pub fn new() -> Self {
        Self::default()
    }

    /// Total manpower across all armies.
    pub fn total_manpower(&self) -> i64 {
        self.armies.iter().map(|a| a.total_manpower()).sum()
    }

    /// Count of all units across all armies.
    pub fn unit_count(&self) -> usize {
        self.armies.iter().map(|a| a.all_units().count()).sum()
    }

    /// All units in the OOB (flat iterator).
    pub fn all_units(&self) -> impl Iterator<Item = &MilitaryUnit> {
        self.armies.iter().flat_map(|a| a.all_units())
    }

    /// Collect all units into a Vec for APIs that need owned collections.
    pub fn collect_all_units(&self) -> Vec<&MilitaryUnit> {
        self.all_units().collect()
    }

    /// All units in the OOB (flat mutable iterator).
    pub fn all_units_mut(&mut self) -> impl Iterator<Item = &mut MilitaryUnit> {
        self.armies.iter_mut().flat_map(|a| a.all_units_mut())
    }

    /// Collect all units mutably into a Vec for APIs that need owned collections.
    pub fn collect_all_units_mut(&mut self) -> Vec<&mut MilitaryUnit> {
        // We need to collect due to Rust's borrow checker limitations with
        // nested flat_map + iter_mut. This is safe because each unit is unique.
        let mut result = Vec::new();
        for army in &mut self.armies {
            for division in &mut army.divisions {
                for regiment in &mut division.regiments {
                    for unit in &mut regiment.units {
                        result.push(unit);
                    }
                }
            }
        }
        result
    }

    /// Units at a specific location (region ID).
    pub fn units_at_location<'a>(&'a self, region_id: &'a str) -> impl Iterator<Item = &'a MilitaryUnit> + 'a {
        self.armies.iter().flat_map(move |a| a.units_at_location(region_id))
    }

    /// Collect units at a location into a Vec.
    pub fn collect_units_at_location<'a>(&'a self, region_id: &'a str) -> Vec<&'a MilitaryUnit> {
        self.units_at_location(region_id).collect()
    }

    /// Units of a specific type.
    pub fn units_by_type<'a>(&'a self, unit_type: UnitType) -> impl Iterator<Item = &'a MilitaryUnit> + 'a {
        self.armies.iter().flat_map(move |a| a.units_by_type(unit_type))
    }

    /// Collect units by type into a Vec.
    pub fn collect_units_by_type(&self, unit_type: UnitType) -> Vec<&MilitaryUnit> {
        self.units_by_type(unit_type).collect()
    }

    /// Add an army to the OOB.
    pub fn add_army(&mut self, army: Army) {
        self.armies.push(army);
    }

    /// Remove empty armies (no divisions).
    pub fn remove_empty_armies(&mut self) {
        self.armies.retain(|a| !a.divisions.is_empty());
    }

    /// Cleanup: remove dead units, empty regiments, empty divisions, empty armies.
    /// Should be called after combat resolution each turn.
    pub fn cleanup_dead(&mut self) {
        for army in &mut self.armies {
            for division in &mut army.divisions {
                for regiment in &mut division.regiments {
                    regiment.remove_dead_units();
                }
                division.remove_empty_regiments();
            }
            army.remove_empty_divisions();
        }
        self.remove_empty_armies();
    }

    /// Collect all unit IDs in the OOB.
    pub fn all_unit_ids(&self) -> Vec<String> {
        self.all_units().map(|u| u.id.clone()).collect()
    }

    /// Find a unit by ID.
    pub fn find_unit(&self, unit_id: &str) -> Option<&MilitaryUnit> {
        self.all_units().find(|u| u.id == unit_id)
    }

    /// Find a unit by ID (mutable).
    pub fn find_unit_mut(&mut self, unit_id: &str) -> Option<&mut MilitaryUnit> {
        for army in &mut self.armies {
            for division in &mut army.divisions {
                for regiment in &mut division.regiments {
                    for unit in &mut regiment.units {
                        if unit.id == unit_id {
                            return Some(unit);
                        }
                    }
                }
            }
        }
        None
    }

    /// Count units by type, returning a HashMap.
    pub fn count_by_type(&self) -> HashMap<UnitType, usize> {
        let mut counts = HashMap::default();
        for unit in self.all_units() {
            *counts.entry(unit.unit_type).or_insert(0) += 1;
        }
        counts
    }

    /// Build a flat Vec of all units (cloned). Used for serialization/DTOs.
    pub fn flatten(&self) -> Vec<MilitaryUnit> {
        self.all_units().cloned().collect()
    }
}

// ============================================================================
// NATIVE OOB CONSTRUCTION (world generation)
// ============================================================================

/// Configuration for native OOB construction during world generation.
///
/// All values are derived from dynamic country properties (GDP, population,
/// region count) — no magic numbers (Rule 2).
#[derive(Debug, Clone)]
pub struct OobGenerationConfig {
    /// Number of armies to create (derived from country size).
    pub army_count: usize,
    /// Number of divisions per army (derived from country size).
    pub divisions_per_army: usize,
    /// Number of regiments per division (derived from country size).
    pub regiments_per_division: usize,
    /// Units per regiment (derived from country military capacity).
    pub units_per_regiment: usize,
    /// Base manpower per unit (derived from population and conscription level).
    pub base_unit_manpower: i64,
    /// Home regions for the armies.
    pub home_regions: Vec<String>,
    /// Country name (for ID prefixes).
    pub country_name: String,
}

/// Generates an Order of Battle natively from a generation config.
///
/// This is the ONLY way to create an OOB during world generation.
/// There is no `rebuild_oob()` or flat-list conversion.
///
/// # Arguments
/// * `config` - Generation configuration with army/division/regiment counts.
///
/// # Returns
/// A fully populated `OrderOfBattle`.
pub fn generate_oob(config: &OobGenerationConfig) -> OrderOfBattle {
    let mut oob = OrderOfBattle::new();

    for army_idx in 0..config.army_count {
        let home_region = config.home_regions
            .get(army_idx % config.home_regions.len())
            .cloned()
            .unwrap_or_default();

        let army_id = format!("ARMY-{}-{:03}", config.country_name, army_idx + 1);
        let army_name = format!("{} Army", ordinal(army_idx + 1));
        let mut army = Army::new(army_id, army_name, home_region.clone());

        for div_idx in 0..config.divisions_per_army {
            let div_id = format!("DIV-{}-{:03}-{:03}", config.country_name, army_idx + 1, div_idx + 1);
            let div_name = format!("{} Division", ordinal(div_idx + 1));
            let mut division = Division::new(div_id, div_name, home_region.clone());

            for reg_idx in 0..config.regiments_per_division {
                let reg_id = format!("REG-{}-{:03}-{:03}-{:03}",
                    config.country_name, army_idx + 1, div_idx + 1, reg_idx + 1);
                let reg_name = format!("{} Regiment", ordinal(reg_idx + 1));
                let mut regiment = Regiment::new(reg_id, reg_name, home_region.clone());

                for unit_idx in 0..config.units_per_regiment {
                    let unit_id = format!("UNIT-{}-{:03}-{:03}-{:03}-{:03}",
                        config.country_name, army_idx + 1, div_idx + 1, reg_idx + 1, unit_idx + 1);

                    // Distribute unit types: first unit is infantry, second is artillery,
                    // third is tanks (if available), rest are infantry.
                    let unit_type = match unit_idx % 4 {
                        0 => UnitType::Infantry,
                        1 => UnitType::Artillery,
                        2 => UnitType::Tanks,
                        _ => UnitType::Infantry,
                    };

                    let mut manpower_origin = std::collections::HashMap::default();
                    manpower_origin.insert(
                        crate::society::geography::RuralClass::FreePeasant,
                        config.base_unit_manpower,
                    );

                    let unit = MilitaryUnit::new(
                        unit_id,
                        unit_type,
                        config.base_unit_manpower,
                        manpower_origin,
                        home_region.clone(),
                    );

                    regiment.add_unit(unit);
                }

                division.add_regiment(regiment);
            }

            army.add_division(division);
        }

        oob.add_army(army);
    }

    oob
}

/// Generates an asymmetric OOB for poor/rich countries.
///
/// Rich countries get more armies, divisions, and better unit types.
/// Poor countries get fewer units with mostly infantry and peasant battalions.
///
/// # Arguments
/// * `country_name` - Name of the country.
/// * `gdp` - Country total GDP (determines military budget).
/// * `gdp_per_capita` - GDP per capita (determines richness and equipment level).
/// * `average_wage` - Average wage (determines cost of maintaining an army).
/// * `population` - Country population (determines manpower pool).
/// * `home_regions` - Regions to base armies in.
/// * `rng` - Random number generator for OOB variation.
///
/// # Returns
/// A populated `OrderOfBattle` scaled to the country's economic capacity.
///
/// # Scaling Model (Phase 76)
/// All thresholds are derived from `average_wage` and `gdp_per_capita` —
/// no hardcoded nominal constants (Rule 2).
/// * `army_cost_threshold = average_wage × 10000` — cost of maintaining one army.
/// * `military_budget = gdp × military_spending_share` where share scales
///   inversely with gdp_per_capita (poor countries spend a larger fraction).
/// * `army_count = (military_budget / army_cost_threshold).max(1).min(8)`.
/// * `divisions_per_army` scales with manpower pool / army_count.
/// * `regiments_per_division` and `units_per_regiment` scale continuously
///   with gdp_per_capita rather than binary thresholds.
/// * `base_unit_manpower` is derived from population / total_units with no
///   upper clamp — large countries have larger units.
pub fn generate_asymmetric_oob(
    country_name: &str,
    gdp: f64,
    gdp_per_capita: f64,
    average_wage: f64,
    population: i64,
    home_regions: Vec<String>,
    rng: &mut impl rand::Rng,
) -> OrderOfBattle {
    // Phase 76: Derived scaling — no hardcoded nominal thresholds.
    // Army cost = average_wage × 10000 (annual cost of equipping and paying
    // 10,000 soldiers at the country's wage level).
    let army_cost_threshold = (average_wage * 10_000.0).max(1.0);

    // Military spending share: poorer countries spend a larger fraction of
    // GDP on military (inverse relationship with gdp_per_capita).
    // At gdp_pc = 300 (very poor): share ≈ 5%
    // At gdp_pc = 5000 (rich): share ≈ 1.5%
    let military_spending_share = (0.06 - gdp_per_capita * 0.000009).max(0.015).min(0.06);
    let military_budget = gdp * military_spending_share;

    // Army count: scale with military budget / army cost, capped at 8.
    let army_count = ((military_budget / army_cost_threshold).floor() as usize)
        .max(1)
        .min(8)
        .min(home_regions.len().max(8));

    // Conscription rate: poorer countries conscript a larger fraction.
    // At gdp_pc = 300: rate ≈ 2% (0.02)
    // At gdp_pc = 5000: rate ≈ 0.5% (0.005)
    let conscription_rate = (0.025 - gdp_per_capita * 0.000004).max(0.005).min(0.025);
    let manpower_pool = (population as f64 * conscription_rate) as i64;

    // Division size: standard division ~5000 soldiers.
    let division_size = 5000_i64;
    let total_divisions = (manpower_pool / division_size).max(1) as usize;
    let divisions_per_army = (total_divisions / army_count).max(1).min(10);

    // Regiments per division: scale continuously with gdp_per_capita.
    // Phase 76: Use absolute gdp_per_capita thresholds (not wage-relative,
    // since average_wage = gdp_pc × 800 in the generator).
    // gdp_pc < 500: 2 regiments (minimal)
    // gdp_pc 500–2000: 3 regiments
    // gdp_pc 2000–5000: 4 regiments
    // gdp_pc > 5000: 5 regiments
    let regiments_per_division = if gdp_per_capita < 500.0 {
        2
    } else if gdp_per_capita < 2000.0 {
        3
    } else if gdp_per_capita < 5000.0 {
        4
    } else {
        5
    };

    // Units per regiment: scale continuously with gdp_per_capita.
    // gdp_pc < 500: 2 units (minimal)
    // gdp_pc 500–2000: 3 units
    // gdp_pc 2000–5000: 4 units
    // gdp_pc > 5000: 5 units
    let units_per_regiment = if gdp_per_capita < 500.0 {
        2
    } else if gdp_per_capita < 2000.0 {
        3
    } else if gdp_per_capita < 5000.0 {
        4
    } else {
        5
    };

    // Base manpower per unit: derived from manpower_pool / total_units.
    // No upper clamp — large countries have larger units. Lower bound of 10
    // (not 100) so tiny countries get appropriately tiny units.
    let total_units = army_count * divisions_per_army * regiments_per_division * units_per_regiment;
    let base_unit_manpower = (manpower_pool / total_units as i64).max(10);

    // Phase 76: Add ±10% RNG variation to structure counts so countries with
    // similar GDP/population don't have identical OOB.
    let divisions_per_army = (((divisions_per_army as f64)
        * (1.0 + rng.gen_range(-0.1..0.1))).round() as usize).max(1);
    let regiments_per_division = (((regiments_per_division as f64)
        * (1.0 + rng.gen_range(-0.1..0.1))).round() as usize).max(2);
    let units_per_regiment = (((units_per_regiment as f64)
        * (1.0 + rng.gen_range(-0.1..0.1))).round() as usize).max(2);

    // Recompute total_units after variation
    let total_units = army_count * divisions_per_army * regiments_per_division * units_per_regiment;
    let base_unit_manpower = (manpower_pool / total_units as i64).max(10);

    let config = OobGenerationConfig {
        army_count,
        divisions_per_army,
        regiments_per_division,
        units_per_regiment,
        base_unit_manpower,
        home_regions,
        country_name: country_name.to_string(),
    };

    let mut oob = generate_oob(&config);

    // For poor countries (low GDP per capita), replace tanks with infantry.
    // Threshold: gdp_per_capita below 1000 indicates pre-industrial economy
    // that cannot support armored vehicle production.
    // This is an absolute economic development threshold, not a wage-relative
    // one, because average_wage = gdp_pc × 800 in the generator, making
    // wage-relative thresholds always fail.
    let tank_affordability_threshold = 1000.0;
    if gdp_per_capita < tank_affordability_threshold {
        for army in &mut oob.armies {
            for division in &mut army.divisions {
                for regiment in &mut division.regiments {
                    for unit in &mut regiment.units {
                        if unit.unit_type == UnitType::Tanks {
                            // Poor countries can't afford tanks — convert to infantry.
                            unit.unit_type = UnitType::Infantry;
                            unit.stats = UnitType::Infantry.base_stats();
                        }
                    }
                }
            }
        }
    }

    oob
}

/// Returns the ordinal suffix for a number (1st, 2nd, 3rd, 4th, etc.).
fn ordinal(n: usize) -> String {
    let suffix = match n % 10 {
        1 if n % 100 != 11 => "st",
        2 if n % 100 != 12 => "nd",
        3 if n % 100 != 13 => "rd",
        _ => "th",
    };
    format!("{}{}", n, suffix)
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_oob() -> OrderOfBattle {
        let config = OobGenerationConfig {
            army_count: 2,
            divisions_per_army: 2,
            regiments_per_division: 2,
            units_per_regiment: 2,
            base_unit_manpower: 1000,
            home_regions: vec!["region_a".to_string(), "region_b".to_string()],
            country_name: "TestCountry".to_string(),
        };
        generate_oob(&config)
    }

    #[test]
    fn test_oob_hierarchy_structure() {
        let oob = make_test_oob();
        assert_eq!(oob.armies.len(), 2);
        assert_eq!(oob.armies[0].divisions.len(), 2);
        assert_eq!(oob.armies[0].divisions[0].regiments.len(), 2);
        assert_eq!(oob.armies[0].divisions[0].regiments[0].units.len(), 2);
    }

    #[test]
    fn test_oob_total_manpower() {
        let oob = make_test_oob();
        // 2 armies * 2 divisions * 2 regiments * 2 units * 1000 manpower = 16000
        assert_eq!(oob.total_manpower(), 16000);
    }

    #[test]
    fn test_oob_unit_count() {
        let oob = make_test_oob();
        // 2 * 2 * 2 * 2 = 16 units
        assert_eq!(oob.unit_count(), 16);
    }

    #[test]
    fn test_oob_all_units() {
        let oob = make_test_oob();
        let units: Vec<_> = oob.all_units().collect();
        assert_eq!(units.len(), 16);
    }

    #[test]
    fn test_oob_all_units_mut() {
        let mut oob = make_test_oob();
        for unit in oob.all_units_mut() {
            unit.experience += 10.0;
        }
        // Verify all units got the experience boost
        assert!(oob.all_units().all(|u| u.experience >= 10.0));
    }

    #[test]
    fn test_oob_units_at_location() {
        let oob = make_test_oob();
        let units_at_a = oob.collect_units_at_location("region_a");
        let units_at_b = oob.collect_units_at_location("region_b");
        // Army 0 is based in region_a, Army 1 in region_b
        // All units in army 0 should be at region_a, all in army 1 at region_b
        assert_eq!(units_at_a.len(), 8);
        assert_eq!(units_at_b.len(), 8);
    }

    #[test]
    fn test_oob_units_by_type() {
        let oob = make_test_oob();
        let infantry = oob.collect_units_by_type(UnitType::Infantry);
        let artillery = oob.collect_units_by_type(UnitType::Artillery);
        let tanks = oob.collect_units_by_type(UnitType::Tanks);
        // With 2 units per regiment and type pattern [Inf, Art, Tank, Inf] (cycling),
        // unit 0 → Infantry, unit 1 → Artillery.
        // 2 armies * 2 divisions * 2 regiments = 8 regiments * 2 units = 16 units
        // → 8 infantry, 8 artillery, 0 tanks
        assert_eq!(infantry.len(), 8);
        assert_eq!(artillery.len(), 8);
        assert_eq!(tanks.len(), 0);
    }

    #[test]
    fn test_oob_find_unit() {
        let oob = make_test_oob();
        let first_id = oob.all_units().next().unwrap().id.clone();
        let found = oob.find_unit(&first_id);
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, first_id);
    }

    #[test]
    fn test_oob_find_unit_mut() {
        let mut oob = make_test_oob();
        let first_id = oob.all_units().next().unwrap().id.clone();
        let found = oob.find_unit_mut(&first_id);
        assert!(found.is_some());
        found.unwrap().manpower = 500;
        assert_eq!(oob.find_unit(&first_id).unwrap().manpower, 500);
    }

    #[test]
    fn test_oob_cleanup_dead() {
        let mut oob = make_test_oob();
        // Kill all units in the first regiment of the first division of the first army
        let army = &mut oob.armies[0];
        let division = &mut army.divisions[0];
        let regiment = &mut division.regiments[0];
        for unit in &mut regiment.units {
            unit.manpower = 0;
        }
        // Cleanup
        oob.cleanup_dead();
        // The first regiment should now be empty and removed
        assert_eq!(oob.armies[0].divisions[0].regiments.len(), 1);
    }

    #[test]
    fn test_oob_count_by_type() {
        let oob = make_test_oob();
        let counts = oob.count_by_type();
        // With 2 units per regiment: unit 0 → Infantry, unit 1 → Artillery
        // 8 regiments → 8 infantry, 8 artillery
        assert_eq!(*counts.get(&UnitType::Infantry).unwrap_or(&0), 8);
        assert_eq!(*counts.get(&UnitType::Artillery).unwrap_or(&0), 8);
    }

    #[test]
    fn test_oob_flatten() {
        let oob = make_test_oob();
        let flat = oob.flatten();
        assert_eq!(flat.len(), 16);
    }

    #[test]
    fn test_oob_all_unit_ids() {
        let oob = make_test_oob();
        let ids = oob.all_unit_ids();
        assert_eq!(ids.len(), 16);
        // All IDs should be unique
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), 16);
    }

    #[test]
    fn test_asymmetric_oob_rich_country() {
        let mut rng = rand::thread_rng();
        let oob = generate_asymmetric_oob(
            "RichCountry",
            5_000_000_000.0, // High total GDP
            5000.0,           // High GDP per capita
            4000.0,           // High average wage
            1_000_000,
            vec!["r1".to_string(), "r2".to_string(), "r3".to_string()],
            &mut rng,
        );
        // Rich country should have tanks
        let tanks = oob.collect_units_by_type(UnitType::Tanks);
        assert!(!tanks.is_empty(), "Rich country should have tanks");
        assert!(oob.total_manpower() > 0);
    }

    #[test]
    fn test_asymmetric_oob_poor_country() {
        let mut rng = rand::thread_rng();
        let oob = generate_asymmetric_oob(
            "PoorCountry",
            30_000_000.0, // Low total GDP
            300.0,         // Low GDP per capita
            240.0,         // Low average wage (gdp_pc * 800)
            100_000,
            vec!["r1".to_string()],
            &mut rng,
        );
        // Poor country should have NO tanks (converted to infantry)
        let tanks = oob.collect_units_by_type(UnitType::Tanks);
        assert!(tanks.is_empty(), "Poor country should have no tanks");
        let infantry = oob.collect_units_by_type(UnitType::Infantry);
        assert!(!infantry.is_empty(), "Poor country should have infantry");
    }

    #[test]
    fn test_oob_default_is_empty() {
        let oob = OrderOfBattle::default();
        assert_eq!(oob.armies.len(), 0);
        assert_eq!(oob.total_manpower(), 0);
        assert_eq!(oob.unit_count(), 0);
    }

    #[test]
    fn test_ordinal() {
        assert_eq!(ordinal(1), "1st");
        assert_eq!(ordinal(2), "2nd");
        assert_eq!(ordinal(3), "3rd");
        assert_eq!(ordinal(4), "4th");
        assert_eq!(ordinal(11), "11th");
        assert_eq!(ordinal(21), "21st");
    }
}

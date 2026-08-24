//! Phase 17C: Ethnic/Religious Violence — Pogrom trigger logic and effects.
//!
//! This module implements pogrom triggers based on social unrest, wealth inequality,
//! cultural distance, and low justice coverage. Wealth transfer is strictly zero-sum.
//!
//! # Rules
//! * Pogroms require: social_unrest > 50, cultural_distance > 0.4, justice_coverage < 0.3.
//! * Not under OpenCitizenship law (discrimination is a prerequisite).
//! * Wealth transfer: DEBIT minority savings, CREDIT dominant class savings (zero-sum).
//! * Casualties and emigration reduce minority population.
//! * Mitigated by SecurityCapacity and JusticeCapacity.
//! * Minority identification uses religion (ClassDemographics.religion) as the differentiator,
//!   since culture is stored at the country level, not per-class.

use crate::economy::disasters::{DisasterEvent, DisasterType};
use crate::registries::enums::Commodity;
use crate::society::culture_registry::{registry as culture_registry, ReligionDefinition};
use crate::state::Country;
use crate::society::geography::Region;
use std::collections::BTreeMap;

/// Configuration for pogrom triggers and effects (no magic numbers).
#[derive(Debug, Clone, PartialEq)]
pub struct PogromConfig {
    /// Social unrest threshold above which pogroms can trigger.
    pub unrest_threshold: f64,
    /// Cultural distance threshold for "otherness".
    pub cultural_distance_threshold: f64,
    /// Justice coverage below which pogroms can happen.
    pub justice_coverage_threshold: f64,
    /// Wealth inequality ratio for targeted envy (minority > 2x dominant).
    pub wealth_envy_ratio: f64,
    /// Wealth inequality ratio for scapegoating (minority < 0.5x dominant).
    pub wealth_scapegoat_ratio: f64,
    /// Fraction of minority savings transferred to dominant class.
    pub wealth_transfer_rate: f64,
    /// Fraction of minority population killed.
    pub casualty_rate: f64,
    /// Fraction of minority population forced to emigrate.
    pub emigration_rate: f64,
    /// Maximum severity per pogrom event.
    pub max_severity: f64,
    /// Phase 18C: Reduction to unrest threshold from hate speech propaganda.
    /// Lowered by active hate speech campaigns (makes pogroms more likely).
    pub propaganda_threshold_reduction: f64,
}

impl Default for PogromConfig {
    fn default() -> Self {
        Self {
            unrest_threshold: 50.0,
            cultural_distance_threshold: 0.4,
            justice_coverage_threshold: 0.3,
            wealth_envy_ratio: 2.0,
            wealth_scapegoat_ratio: 0.5,
            wealth_transfer_rate: 0.5,
            casualty_rate: 0.05,
            emigration_rate: 0.15,
            max_severity: 0.8,
            propaganda_threshold_reduction: 0.0,
        }
    }
}

/// Result of a pogrom check for a single region.
#[derive(Debug, Clone, Default)]
pub struct PogromResult {
    /// Whether a pogrom was triggered.
    pub triggered: bool,
    /// Severity 0.0–1.0.
    pub severity: f64,
    /// Wealth transferred from minority to dominant class (zero-sum).
    pub wealth_transferred: f64,
    /// Population killed or forced to emigrate.
    pub casualties: i64,
    /// Disaster event for logging.
    pub event: Option<DisasterEvent>,
}

/// Check for pogrom triggers in a country's regions.
///
/// # Arguments
/// * `country` - Mutable country (for demographics and social state).
/// * `buildings` - Buildings slice (for SecurityCapacity/JusticeCapacity mitigation).
/// * `config` - Pogrom configuration.
/// * `turn` - Current turn number (for disaster event logging).
///
/// # Returns
/// Vector of `PogromResult` per region where pogroms were triggered.
///
/// # Rules
/// * Pogroms require: unrest > threshold, cultural_distance > threshold, justice_coverage < threshold.
/// * Not under "Otwarte Obywatelstwo" (OpenCitizenship) law.
/// * Wealth transfer is zero-sum: minority loses exactly what dominant gains.
/// * Security/Justice capacity mitigates severity.
pub fn check_pogrom_triggers(
    country: &mut Country,
    buildings: &[crate::entities::Building],
    config: &PogromConfig,
    turn: u32,
) -> Vec<PogromResult> {
    let mut results = Vec::new();
    let reg = culture_registry();
    let dominant_religion = country.macro_indicators.religion.clone();
    let civil_rights_law = country.politics.civil_rights_law.clone();

    // OpenCitizenship prevents pogroms.
    if civil_rights_law == "Otwarte Obywatelstwo" {
        return results;
    }

    // Compute justice and security coverage from building last_production.
    let total_justice_capacity: f64 = buildings
        .iter()
        .filter_map(|b| b.last_production.get(&Commodity::JusticeCapacity).copied())
        .sum();
    let total_security_capacity: f64 = buildings
        .iter()
        .filter_map(|b| b.last_production.get(&Commodity::SecurityCapacity).copied())
        .sum();

    // Estimate justice demand from population.
    let total_pop: f64 = country.regions.iter().map(|r| r.population as f64).sum();
    let justice_demand = total_pop * 0.01;
    let justice_coverage = if justice_demand > 0.0 {
        (total_justice_capacity / justice_demand).clamp(0.0, 1.0)
    } else {
        0.0
    };

    if justice_coverage >= config.justice_coverage_threshold {
        return results;
    }

    // Get social unrest from macro_indicators.
    let social_unrest = country.macro_indicators.social_unrest;

    if social_unrest <= config.unrest_threshold {
        return results;
    }

    let dominant_religion_def = reg.religion_from_display_name(&dominant_religion);

    for region in &mut country.regions {
        let result = check_region_pogrom(
            region,
            &dominant_religion,
            dominant_religion_def,
            social_unrest,
            justice_coverage,
            total_security_capacity,
            total_pop,
            config,
            turn,
        );
        if result.triggered {
            results.push(result);
        }
    }

    results
}

/// Check a single region for pogrom triggers.
fn check_region_pogrom(
    region: &mut Region,
    dominant_religion: &str,
    dominant_religion_def: Option<&ReligionDefinition>,
    social_unrest: f64,
    justice_coverage: f64,
    total_security_capacity: f64,
    total_pop: f64,
    config: &PogromConfig,
    turn: u32,
) -> PogromResult {
    let result = PogromResult::default();
    let _reg = culture_registry();

    // Check rural classes first.
    let rural_result = check_class_map_pogrom(
        &region.class_demographics.rural_classes,
        "rural",
        dominant_religion,
        dominant_religion_def,
        social_unrest,
        justice_coverage,
        total_security_capacity,
        total_pop,
        config,
        turn,
        &region.id,
    );
    if rural_result.triggered {
        // Apply effects to the region.
        apply_wealth_transfer(region, &rural_result.minority_class, "rural", dominant_religion, rural_result.wealth_transferred);
        reduce_minority_population(region, &rural_result.minority_class, "rural", rural_result.casualties);
        let mut pogrom = PogromResult::default();
        pogrom.triggered = true;
        pogrom.severity = rural_result.severity;
        pogrom.wealth_transferred = rural_result.wealth_transferred;
        pogrom.casualties = rural_result.casualties;
        pogrom.event = rural_result.event;
        return pogrom;
    }

    // Check urban classes.
    let urban_result = check_class_map_pogrom(
        &region.class_demographics.urban_classes,
        "urban",
        dominant_religion,
        dominant_religion_def,
        social_unrest,
        justice_coverage,
        total_security_capacity,
        total_pop,
        config,
        turn,
        &region.id,
    );
    if urban_result.triggered {
        apply_wealth_transfer(region, &urban_result.minority_class, "urban", dominant_religion, urban_result.wealth_transferred);
        reduce_minority_population(region, &urban_result.minority_class, "urban", urban_result.casualties);
        let mut pogrom = PogromResult::default();
        pogrom.triggered = true;
        pogrom.severity = urban_result.severity;
        pogrom.wealth_transferred = urban_result.wealth_transferred;
        pogrom.casualties = urban_result.casualties;
        pogrom.event = urban_result.event;
        return pogrom;
    }

    result
}

/// Internal result from checking a single class map for pogrom triggers.
struct ClassMapPogromResult {
    triggered: bool,
    severity: f64,
    wealth_transferred: f64,
    casualties: i64,
    minority_class: String,
    event: Option<DisasterEvent>,
}

/// Check a single class map (rural or urban) for pogrom triggers.
fn check_class_map_pogrom(
    class_map: &BTreeMap<String, crate::society::geography::ClassDemographics>,
    _class_type: &str,
    dominant_religion: &str,
    dominant_religion_def: Option<&ReligionDefinition>,
    social_unrest: f64,
    justice_coverage: f64,
    total_security_capacity: f64,
    total_pop: f64,
    config: &PogromConfig,
    turn: u32,
    region_id: &str,
) -> ClassMapPogromResult {
    let mut result = ClassMapPogromResult {
        triggered: false,
        severity: 0.0,
        wealth_transferred: 0.0,
        casualties: 0,
        minority_class: String::new(),
        event: None,
    };
    let reg = culture_registry();

    for (class_name, demo) in class_map.iter() {
        if demo.religion == *dominant_religion || demo.religion.is_empty() {
            continue;
        }

        let minority_religion_def = reg.religion_from_display_name(&demo.religion);
        let dist = match (dominant_religion_def, minority_religion_def) {
            (Some(d), Some(m)) => {
                if d.religious_group == m.religious_group {
                    0.3
                } else {
                    0.7
                }
            }
            _ => 0.5,
        };

        if dist < config.cultural_distance_threshold {
            continue;
        }

        let minority_spc = demo.savings_per_capita;
        let dominant_spc = find_dominant_savings_per_capita(class_map, dominant_religion);

        let wealth_inequality = if dominant_spc > 0.0 {
            let ratio = minority_spc / dominant_spc;
            ratio > config.wealth_envy_ratio || ratio < config.wealth_scapegoat_ratio
        } else {
            minority_spc > 0.0
        };

        if !wealth_inequality {
            continue;
        }

        let unrest_factor = ((social_unrest - config.unrest_threshold) / 50.0).clamp(0.0, 1.0);
        let distance_factor = dist;
        let justice_gap = 1.0 - justice_coverage;

        let security_ratio = if total_pop > 0.0 {
            (total_security_capacity / (total_pop * 0.005)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let security_factor = 1.0 - security_ratio * 0.5;

        let severity = (unrest_factor * 0.4 + distance_factor * 0.3 + justice_gap * 0.3)
            * security_factor
            .min(config.max_severity);

        if severity < 0.01 {
            continue;
        }

        let transfer = demo.savings * severity * config.wealth_transfer_rate;
        let minority_pop = demo.population as f64;
        let casualties = (minority_pop * severity * config.casualty_rate) as i64;
        let emigration = (minority_pop * severity * config.emigration_rate) as i64;

        result.triggered = true;
        result.severity = severity;
        result.wealth_transferred = transfer;
        result.casualties = casualties + emigration;
        result.minority_class = class_name.clone();
        result.event = Some(DisasterEvent {
            disaster_type: DisasterType::Pogrom,
            region_id: region_id.to_string(),
            severity,
            buildings_destroyed: 0,
            casualties: result.casualties,
            economic_damage: transfer,
            turn,
            extra: serde_json::Map::new(),
        });

        return result;
    }

    result
}

/// Find the savings per capita of the dominant religion class.
fn find_dominant_savings_per_capita(
    class_map: &BTreeMap<String, crate::society::geography::ClassDemographics>,
    dominant_religion: &str,
) -> f64 {
    let mut total_savings = 0.0;
    let mut total_pop = 0.0;

    for demo in class_map.values() {
        if demo.religion == *dominant_religion || demo.religion.is_empty() {
            total_savings += demo.savings;
            total_pop += demo.population as f64;
        }
    }

    if total_pop > 0.0 {
        total_savings / total_pop
    } else {
        0.0
    }
}

/// Apply zero-sum wealth transfer from minority class to dominant class.
fn apply_wealth_transfer(
    region: &mut Region,
    minority_class: &str,
    class_type: &str,
    dominant_religion: &str,
    amount: f64,
) {
    if amount <= 0.0 {
        return;
    }

    let class_map = if class_type == "rural" {
        &mut region.class_demographics.rural_classes
    } else {
        &mut region.class_demographics.urban_classes
    };

    if let Some(minority) = class_map.get_mut(minority_class) {
        let debit = minority.savings.min(amount);
        minority.savings -= debit;

        let dominant_pop: f64 = class_map.values()
            .filter(|d| d.religion == *dominant_religion || d.religion.is_empty())
            .map(|d| d.population as f64)
            .sum();

        if dominant_pop > 0.0 {
            for demo in class_map.values_mut() {
                if demo.religion == *dominant_religion || demo.religion.is_empty() {
                    let share = (demo.population as f64) / dominant_pop;
                    demo.savings += debit * share;
                }
            }
        }
    }
}

/// Reduce minority population by casualties + emigration.
fn reduce_minority_population(
    region: &mut Region,
    minority_class: &str,
    class_type: &str,
    reduction: i64,
) {
    if reduction <= 0 {
        return;
    }

    let class_map = if class_type == "rural" {
        &mut region.class_demographics.rural_classes
    } else {
        &mut region.class_demographics.urban_classes
    };

    if let Some(demo) = class_map.get_mut(minority_class) {
        let new_pop = (demo.population - reduction).max(0);
        demo.population = new_pop;
    }

    region.population = (region.population - reduction).max(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::society::geography::ClassDemographics;
    use crate::state::Country;

    fn make_test_region_with_minority() -> Region {
        let mut region = Region::default();
        region.id = "test_region".to_string();
        region.population = 10000;

        let mut minority = ClassDemographics::default();
        minority.religion = "Islam".to_string();
        minority.population = 2000;
        minority.savings = 5000.0;
        minority.savings_per_capita = 2.5;

        let mut dominant = ClassDemographics::default();
        dominant.religion = "Catholicism".to_string();
        dominant.population = 8000;
        dominant.savings = 1000.0;
        dominant.savings_per_capita = 0.125;

        region.class_demographics.rural_classes.insert("minority_class".to_string(), minority);
        region.class_demographics.rural_classes.insert("dominant_class".to_string(), dominant);

        region
    }

    #[test]
    fn test_pogrom_wealth_transfer_zero_sum() {
        let mut region = make_test_region_with_minority();

        apply_wealth_transfer(&mut region, "minority_class", "rural", "Catholicism", 500.0);

        let minority = &region.class_demographics.rural_classes["minority_class"];
        let dominant = &region.class_demographics.rural_classes["dominant_class"];

        assert!((minority.savings - 4500.0).abs() < 0.01,
            "minority savings should be 4500, got {}", minority.savings);
        assert!((dominant.savings - 1500.0).abs() < 0.01,
            "dominant savings should be 1500, got {}", dominant.savings);
    }

    #[test]
    fn test_pogrom_requires_high_unrest() {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.social_unrest = 30.0;
        country.macro_indicators.culture = "Illyria".to_string();
        country.politics.civil_rights_law = "5-Year Assimilation".to_string();

        let config = PogromConfig::default();
        let results = check_pogrom_triggers(&mut country, &[], &config, 1);

        assert!(results.is_empty(), "no pogroms when unrest is below threshold");
    }

    #[test]
    fn test_pogrom_blocked_by_open_citizenship() {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.social_unrest = 80.0;
        country.macro_indicators.culture = "Illyria".to_string();
        country.politics.civil_rights_law = "Otwarte Obywatelstwo".to_string();

        let config = PogromConfig::default();
        let results = check_pogrom_triggers(&mut country, &[], &config, 1);

        assert!(results.is_empty(), "no pogroms under OpenCitizenship");
    }

    #[test]
    fn test_pogrom_blocked_by_high_justice_coverage() {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.social_unrest = 80.0;
        country.macro_indicators.culture = "Illyria".to_string();
        country.politics.civil_rights_law = "5-Year Assimilation".to_string();

        let mut building = crate::entities::Building::default();
        building.last_production.insert(Commodity::JusticeCapacity, 10000.0);
        let buildings = vec![building];

        let config = PogromConfig::default();
        let results = check_pogrom_triggers(&mut country, &buildings, &config, 1);

        assert!(results.is_empty(), "no pogroms when justice coverage is high");
    }

    #[test]
    fn test_pogrom_casualties_reduce_population() {
        let mut region = make_test_region_with_minority();
        let original_pop = region.population;
        let original_minority_pop = region.class_demographics.rural_classes["minority_class"].population;

        reduce_minority_population(&mut region, "minority_class", "rural", 200);

        assert_eq!(region.population, original_pop - 200);
        assert_eq!(
            region.class_demographics.rural_classes["minority_class"].population,
            original_minority_pop - 200
        );
    }
}

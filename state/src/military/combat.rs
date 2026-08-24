//! Combat resolution and battle mechanics — deterministic, no RNG.

use std::collections::{HashMap, BTreeMap};

use crate::military::config::MilitaryCombatConfig;
use crate::military::fronts::{Battle, BattleResult, Casualties};
use crate::military::units::MilitaryUnit;
use crate::society::geography::{RuralClass, ClassDemographics};

/// Resolve a battle between attacking and defending units.
///
/// Fully deterministic — no RNG.  Combat power is derived from unit stats,
/// supply level, organization, and terrain.  Ammunition and fuel are burned
/// from unit stockpiles before power is calculated, so supply shortages
/// directly weaken the side.
///
/// # Arguments
/// * `attacker_units` - Attacking military units (will be mutated to burn supplies)
/// * `defender_units` - Defending military units (will be mutated to burn supplies)
/// * `location` - Battle location (region ID)
/// * `attacker_country` - Attacking country name
/// * `defender_country` - Defending country name
/// * `turn` - Current game turn
/// * `battle_id` - Unique battle identifier
/// * `config` - Military combat configuration
/// * `terrain` - Terrain type ("mountain", "forest", "plains", or other)
///
/// # Returns
/// Resolved battle with casualties and result
pub fn resolve_battle(
    attacker_units: &mut [MilitaryUnit],
    defender_units: &mut [MilitaryUnit],
    location: String,
    attacker_country: String,
    defender_country: String,
    turn: u32,
    battle_id: String,
    config: &MilitaryCombatConfig,
    terrain: &str,
) -> Battle {
    // ── Phase 1: Pre-combat commodity burning ──
    // We don't know yet if this will be decisive or stalemate, so we burn
    // at decisive intensity.  The actual intensity is applied retrospectively
    // for casualty calculations.  Burning happens BEFORE power calculation
    // so ammo shortage weakens the side.

    for unit in attacker_units.iter_mut() {
        let manpower_ratio = unit.manpower as f64 / 1000.0;
        let ammo_req = config.base_ammo_burn_per_battle * manpower_ratio * config.decisive_combat_intensity;
        let fuel_req = config.base_fuel_burn_per_battle * manpower_ratio * config.decisive_combat_intensity;
        unit.burn_combat_supplies(ammo_req, fuel_req);
    }

    for unit in defender_units.iter_mut() {
        let manpower_ratio = unit.manpower as f64 / 1000.0;
        let ammo_req = config.base_ammo_burn_per_battle * manpower_ratio * config.decisive_combat_intensity;
        let fuel_req = config.base_fuel_burn_per_battle * manpower_ratio * config.decisive_combat_intensity;
        unit.burn_combat_supplies(ammo_req, fuel_req);
    }

    // ── Phase 2: Calculate combat power (deterministic) ──
    let attacker_power: f64 = attacker_units.iter()
        .map(|u| u.combat_power(config, false, terrain))
        .sum();

    let defender_power: f64 = defender_units.iter()
        .map(|u| u.combat_power(config, true, terrain))
        .sum();

    // ── Phase 3: Determine battle result ──
    let (result, attacker_casualty_ratio, defender_casualty_ratio) = if attacker_power > defender_power * config.decisive_victory_ratio {
        // Decisive attacker victory
        let power_diff = (attacker_power - defender_power) / attacker_power.max(1.0);
        let loser_ratio = power_diff * config.max_loser_casualty_ratio;
        (BattleResult::AttackerVictory, loser_ratio * config.winner_casualty_multiplier, loser_ratio)
    } else if attacker_power > defender_power * config.pyrrhic_victory_ratio {
        // Pyrrhic attacker victory
        let power_diff = (attacker_power - defender_power) / attacker_power.max(1.0);
        let loser_ratio = power_diff * config.max_loser_casualty_ratio;
        (BattleResult::PyrrhicVictory, loser_ratio * config.winner_casualty_multiplier, loser_ratio)
    } else if defender_power > attacker_power * config.decisive_victory_ratio {
        // Decisive defender victory
        let power_diff = (defender_power - attacker_power) / defender_power.max(1.0);
        let loser_ratio = power_diff * config.max_loser_casualty_ratio;
        (BattleResult::DefenderVictory, loser_ratio, loser_ratio * config.winner_casualty_multiplier)
    } else if defender_power > attacker_power {
        // Marginal defender victory
        let power_diff = (defender_power - attacker_power) / defender_power.max(1.0);
        let loser_ratio = power_diff * config.max_loser_casualty_ratio;
        (BattleResult::DefenderVictory, loser_ratio, loser_ratio * config.winner_casualty_multiplier)
    } else {
        // Stalemate
        (BattleResult::Stalemate, config.stalemate_casualty_ratio, config.stalemate_casualty_ratio)
    };

    // ── Phase 4: Calculate casualties (deterministic) ──
    let attacker_casualties = calculate_casualties(attacker_units, attacker_casualty_ratio, config);
    let defender_casualties = calculate_casualties(defender_units, defender_casualty_ratio, config);

    Battle {
        id: battle_id,
        location,
        attacker: attacker_country,
        defender: defender_country,
        turn,
        attacker_units: attacker_units.iter().map(|u| u.id.clone()).collect(),
        defender_units: defender_units.iter().map(|u| u.id.clone()).collect(),
        attacker_casualties,
        defender_casualties,
        result,
    }
}

/// Calculate casualties for a set of units (deterministic, no RNG).
///
/// # Arguments
/// * `units` - Units to calculate casualties for
/// * `casualty_ratio` - Base casualty ratio (0-1)
/// * `config` - Military combat configuration
///
/// # Returns
/// Casualties breakdown with demographic routing
fn calculate_casualties(
    units: &[MilitaryUnit],
    casualty_ratio: f64,
    config: &MilitaryCombatConfig,
) -> Casualties {
    let total_manpower: i64 = units.iter().map(|u| u.manpower).sum();
    let base_casualties = (total_manpower as f64 * casualty_ratio) as i64;

    let dead = (base_casualties as f64 * config.dead_ratio) as i64;
    let wounded = (base_casualties as f64 * config.wounded_ratio) as i64;
    let deserters = base_casualties - dead - wounded;

    // Aggregate demographic breakdown from all units
    let mut demographic_breakdown: HashMap<RuralClass, i64> = HashMap::new();
    for unit in units {
        for (rural_class, &count) in &unit.manpower_origin {
            *demographic_breakdown.entry(*rural_class).or_insert(0) += count;
        }
    }

    // Scale demographic breakdown to match total casualties
    let total_origin: i64 = demographic_breakdown.values().sum();
    if total_origin > 0 {
        let scale = base_casualties as f64 / total_origin as f64;
        for count in demographic_breakdown.values_mut() {
            *count = (*count as f64 * scale) as i64;
        }
    }

    Casualties {
        dead,
        wounded,
        deserters,
        demographic_breakdown,
    }
}

/// Process wounded soldiers with hospital capacity checking.
///
/// # Arguments
/// * `wounded` - Number of wounded soldiers
/// * `hospital_capacity` - Available hospital beds
/// * `region_name` - Region name for messaging
///
/// # Returns
/// (treated, untreated_dead, messages)
///
/// # Rules
/// * If hospital capacity >= wounded, all treated and recover
/// * If capacity exceeded, untreated become dead, spike social_unrest
pub fn process_wounded(
    wounded: i64,
    hospital_capacity: f64,
    region_name: &str,
) -> (i64, i64, Vec<String>) {
    let mut messages = Vec::new();

    if wounded <= 0 {
        return (0, 0, messages);
    }

    let capacity_beds = hospital_capacity as i64;

    if capacity_beds >= wounded {
        messages.push(format!(
            "[HOSPITAL] In region {} healed {} wounded (capacity: {})",
            region_name, wounded, capacity_beds
        ));
        (wounded, 0, messages)
    } else {
        let untreated = wounded - capacity_beds;
        messages.push(format!(
            "[HOSPITAL] CRITICAL in region {}! No capacity: {} wounded without treatment (capacity: {})",
            region_name, untreated, capacity_beds
        ));
        messages.push(format!(
            "[UNREST] Untreated wounded in {} increase social unrest",
            region_name
        ));
        (capacity_beds, untreated, messages)
    }
}

/// Process dead soldiers with demographic routing.
///
/// # Arguments
/// * `casualties` - Casualties with demographic breakdown
/// * `region_demographics` - Mutable reference to region demographics
/// * `region_name` - Region name for messaging
///
/// # Returns
/// messages
///
/// # Rules
/// * Use manpower_origin HashMap to deduct dead proportionally from actual classes
/// * Dead are permanently removed from demographics
pub fn process_dead(
    casualties: &Casualties,
    region_demographics: &mut BTreeMap<String, ClassDemographics>,
    region_name: &str,
) -> Vec<String> {
    let mut messages = Vec::new();

    if casualties.dead <= 0 {
        return messages;
    }

    // Deduct dead from demographic classes
    // Fix: serde_json::to_string produces quoted JSON like "\"free_peasant\""
    // We must trim the quotes to match BTreeMap keys.
    for (rural_class, &dead_count) in &casualties.demographic_breakdown {
        let class_key = serde_json::to_string(rural_class)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        if let Some(class_demographics) = region_demographics.get_mut(&class_key) {
            class_demographics.population = (class_demographics.population - dead_count).max(0);
        }
    }

    messages.push(format!(
        "[CASUALTIES] In region {} {} soldiers killed",
        region_name, casualties.dead
    ));

    messages
}

/// Process deserters with demographic routing.
///
/// # Arguments
/// * `casualties` - Casualties with demographic breakdown
/// * `region_demographics` - Mutable reference to region demographics
/// * `region_name` - Region name for messaging
///
/// # Returns
/// messages
///
/// # Rules
/// * Deserters return to their original demographic classes (population added back)
/// * Desertion spikes social_unrest
pub fn process_deserters(
    casualties: &Casualties,
    region_demographics: &mut BTreeMap<String, ClassDemographics>,
    region_name: &str,
) -> Vec<String> {
    let mut messages = Vec::new();

    if casualties.deserters <= 0 {
        return messages;
    }

    // Deserters return to their original classes — add them back to population
    for (rural_class, &deserter_count) in &casualties.demographic_breakdown {
        let class_key = serde_json::to_string(rural_class)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        if let Some(class_demographics) = region_demographics.get_mut(&class_key) {
            class_demographics.population += deserter_count;
        }
    }

    messages.push(format!(
        "[DESERTION] In region {} {} soldiers deserted (return to society)",
        region_name, casualties.deserters
    ));
    messages.push(format!(
        "[UNREST] Desertion in {} increases social unrest",
        region_name
    ));

    messages
}

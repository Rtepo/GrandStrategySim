//! Military turn processing and war management

use rustc_hash::FxHashMap;

type HashMap<K, V> = FxHashMap<K, V>;

use crate::infrastructure::CapacityType;
use crate::military::combat::{resolve_battle, process_wounded, process_dead, process_deserters};
use crate::military::config::MilitaryCombatConfig;
use crate::military::fronts::Front;
use crate::military::units::MilitaryUnit;
use crate::military::oob::OrderOfBattle;
use crate::registries::enums::Commodity;
use crate::society::geography::{Region, Climate, RuralClass};

/// Process military turn for a country.
///
/// Executes the full military sequence: upkeep, supply delivery, combat,
/// casualty demographics, peasant devastation, and war exhaustion decay.
///
/// # Arguments
/// * `units` - Military units to process (will be mutated)
/// * `fronts` - Active military fronts (will be mutated)
/// * `regions` - Regions (for capacity checking and devastation)
/// * `liquid_reserves` - Country's liquid reserves (will be modified)
/// * `military_stockpile` - Country military depot (will be mutated for resupply)
/// * `trades` - B2B trades from this turn's order book clearing
/// * `config` - Military combat configuration
/// * `turn` - Current game turn
/// * `country_name` - Country name for messaging
///
/// # Returns
/// (updated_fronts, all_messages)
pub fn process_military_turn(
    oob: &mut OrderOfBattle,
    fronts: &mut Vec<Front>,
    regions: &mut Vec<Region>,
    liquid_reserves: &mut f64,
    military_stockpile: &mut HashMap<Commodity, f64>,
    trades: &[crate::economy::order_book::Trade],
    config: &MilitaryCombatConfig,
    turn: u32,
    country_name: &str,
) -> (Vec<Front>, Vec<String>) {
    let mut all_messages = Vec::new();

    // Flatten the OOB into a temporary Vec for processing.
    // This is safe because unit IDs are unique and we write back by ID.
    let mut units: Vec<MilitaryUnit> = oob.flatten();

    // Phase 45: Degrade military equipment reserves by one turn BEFORE upkeep.
    // This ensures ToE deficits grow naturally, driving recurring procurement.
    crate::military::degrade_military_equipment(&mut units);

    // MIL-1: Process unit upkeep (burn stockpiles, pay wages)
    let (_wage_cost, upkeep_messages) = crate::military::process_military_upkeep(
        &mut units,
        liquid_reserves,
        config,
    );
    all_messages.extend(upkeep_messages);

    // MIL-2: Supply delivery from B2B trades (Phase 45: includes equipment delivery)
    let delivered = crate::military::deliver_military_supplies_and_equipment(
        trades,
        &mut units,
        military_stockpile,
    );
    if !delivered.is_empty() {
        for (commodity, qty) in &delivered {
            all_messages.push(format!(
                "[SUPPLY] {} units of {:?} delivered to military depot",
                qty, commodity
            ));
        }
    }

    // Resupply units from depot
    for unit in units.iter_mut() {
        if unit.is_peasant_battalion() {
            continue;
        }
        let drawn = unit.resupply(military_stockpile, config);
        if !drawn.is_empty() {
            let total: f64 = drawn.values().sum();
            if total > 0.0 {
                unit.stats.supply = 100.0;
            }
        }
    }

    // MIL-3: Resolve battles on active fronts
    for front in fronts.iter_mut() {
        if front.is_active(turn, 5) {
            let battle_messages = resolve_front_battles(front, &mut units, regions, turn, country_name, config);
            all_messages.extend(battle_messages);
        }

        // MIL-6: Decay war exhaustion
        front.decay_war_exhaustion(config.war_exhaustion_decay_rate);
    }

    // MIL-4: Disband broken units and return survivors to demographics
    let disbanded_survivors = disband_broken_units(&mut units, regions);
    for msg in &disbanded_survivors {
        all_messages.push(msg.clone());
    }

    // MIL-5: Process peasant battalion devastation
    let devastation_messages = process_peasant_devastation(&units, regions, config);
    all_messages.extend(devastation_messages);

    // Write back processed units to the OOB by ID.
    writeback_units_to_oob(oob, &units);

    // Cleanup dead units from the OOB hierarchy
    oob.cleanup_dead();

    (fronts.clone(), all_messages)
}

/// Writes back processed units from a flat Vec to the OOB by matching unit IDs.
///
/// This is the safe writeback path after flatten-process-writeback.
/// Each unit in the OOB is found by ID and replaced with the processed version.
fn writeback_units_to_oob(oob: &mut OrderOfBattle, units: &[MilitaryUnit]) {
    let unit_map: HashMap<String, &MilitaryUnit> = units.iter()
        .map(|u| (u.id.clone(), u))
        .collect();

    for army in &mut oob.armies {
        for division in &mut army.divisions {
            for regiment in &mut division.regiments {
                for unit in &mut regiment.units {
                    if let Some(processed) = unit_map.get(&unit.id) {
                        *unit = (*processed).clone();
                    }
                }
            }
        }
    }
}

/// Disband units with manpower <= 0 or organization <= 0.
/// Returns ALL surviving manpower to demographics in their home region.
///
/// # Arguments
/// * `units` - Military units (destroyed units will be removed)
/// * `regions` - Regions for demographic routing
///
/// # Returns
/// Vec of log messages
fn disband_broken_units(
    units: &mut Vec<MilitaryUnit>,
    regions: &mut Vec<Region>,
) -> Vec<String> {
    let mut messages = Vec::new();

    let mut survivors_to_return: Vec<(String, HashMap<RuralClass, i64>)> = Vec::new();

    let mut to_disband: Vec<usize> = Vec::new();
    for (idx, unit) in units.iter_mut().enumerate() {
        if unit.manpower <= 0 || unit.stats.organization <= 0.0 {
            let survivors = unit.disband();
            if !survivors.is_empty() {
                survivors_to_return.push((unit.home_region.clone(), survivors));
            }
            to_disband.push(idx);
        }
    }

    // Return survivors to demographics
    for (home_region, survivors) in &survivors_to_return {
        if let Some(region) = regions.iter_mut().find(|r| r.id == *home_region) {
            for (rural_class, &count) in survivors {
                let class_key = serde_json::to_string(rural_class)
                    .unwrap_or_default()
                    .trim_matches('"')
                    .to_string();
                if let Some(class_demographics) = region.class_demographics.rural_classes.get_mut(&class_key) {
                    class_demographics.population += count;
                }
            }
        }
        let total_survivors: i64 = survivors.values().sum();
        messages.push(format!(
            "[DISBAND] Unit from region {} disbanded. {} soldiers return to population.",
            home_region, total_survivors
        ));
    }

    // Remove disbanded units (in reverse order to preserve indices)
    for &idx in to_disband.iter().rev() {
        units.remove(idx);
    }

    messages
}

/// Process economic devastation from peasant battalions.
///
/// # Arguments
/// * `units` - Military units to check
/// * `regions` - Regions to affect
/// * `config` - Military combat configuration
///
/// # Returns
/// messages
fn process_peasant_devastation(
    units: &[MilitaryUnit],
    regions: &mut Vec<Region>,
    config: &MilitaryCombatConfig,
) -> Vec<String> {
    let mut messages = Vec::new();

    let mut peasant_regions: HashMap<String, f64> = HashMap::default();
    for unit in units {
        if unit.is_peasant_battalion() {
            let foraging_intensity = unit.stats.supply / 100.0;
            let devastation = foraging_intensity * config.peasant_devastation_multiplier;
            *peasant_regions.entry(unit.location.clone()).or_insert(0.0) += devastation;
        }
    }

    for region in regions {
        if let Some(&devastation) = peasant_regions.get(&region.id) {
            messages.push(format!(
                "[FORAGING] Region {} suffers from peasant battalion foraging (economic damage: {}%)",
                region.id, devastation * 100.0
            ));
        }
    }

    messages
}

/// Derive terrain type string from a region's climate and land use.
fn derive_terrain(region: &Region) -> &'static str {
    match region.climate {
        Climate::Mountainous => "mountain",
        _ => {
            if let Some(forest_data) = region.land_use_inventory.categories.get("forests") {
                let total = region.land_use_inventory.total_area.max(1.0);
                let forest_fraction = forest_data.area_hectares / total;
                if forest_fraction > 0.25 {
                    "forest"
                } else {
                    "plains"
                }
            } else {
                "plains"
            }
        }
    }
}

/// Resolve battles on a front.
///
/// # Arguments
/// * `front` - Front to process
/// * `units` - All military units (will be mutated for supply burning and casualties)
/// * `regions` - Regions for capacity checking
/// * `turn` - Current game turn
/// * `country_name` - Country name
/// * `config` - Military combat configuration
///
/// # Returns
/// messages
fn resolve_front_battles(
    front: &mut Front,
    units: &mut Vec<MilitaryUnit>,
    regions: &mut Vec<Region>,
    turn: u32,
    _country_name: &str,
    config: &MilitaryCombatConfig,
) -> Vec<String> {
    let mut messages = Vec::new();

    // Find the battle region for terrain
    let battle_region_id = front.regions.first().cloned().unwrap_or_default();
    let terrain = regions.iter()
        .find(|r| r.id == battle_region_id)
        .map(|r| derive_terrain(r))
        .unwrap_or("plains");

    // Split units by location (first region = attacker, others = defender)
    let first_region = front.regions.first().cloned().unwrap_or_default();

    let mut attacker_indices: Vec<usize> = Vec::new();
    let mut defender_indices: Vec<usize> = Vec::new();

    for (idx, unit) in units.iter().enumerate() {
        if front.regions.contains(&unit.location) {
            if unit.location == first_region {
                attacker_indices.push(idx);
            } else {
                defender_indices.push(idx);
            }
        }
    }

    if attacker_indices.is_empty() || defender_indices.is_empty() {
        return messages;
    }

    // Clone attacker and defender units for battle resolution
    // (resolve_battle needs mutable slices to burn supplies)
    let mut attacker_units: Vec<MilitaryUnit> = attacker_indices.iter()
        .map(|&i| units[i].clone())
        .collect();
    let mut defender_units: Vec<MilitaryUnit> = defender_indices.iter()
        .map(|&i| units[i].clone())
        .collect();

    let battle = resolve_battle(
        &mut attacker_units,
        &mut defender_units,
        battle_region_id.clone(),
        front.involved_countries.first().cloned().unwrap_or_default(),
        front.involved_countries.get(1).cloned().unwrap_or_default(),
        turn,
        format!("BATTLE-{}-{}", turn, front.id),
        config,
        terrain,
    );

    messages.push(format!(
        "[BITWA] Bitwa w {}: {} przeciwko {} — {:?}",
        battle.location, battle.attacker, battle.defender, battle.result
    ));

    // Apply supply changes back to original units (ammunition/fuel burned)
    for (orig_idx, battle_unit) in attacker_indices.iter().zip(attacker_units.iter()) {
        units[*orig_idx].stockpile = battle_unit.stockpile.clone();
    }
    for (orig_idx, battle_unit) in defender_indices.iter().zip(defender_units.iter()) {
        units[*orig_idx].stockpile = battle_unit.stockpile.clone();
    }

    // Apply casualties to original units
    let attacker_total_casualties = battle.attacker_casualties.total();
    let defender_total_casualties = battle.defender_casualties.total();
    let attacker_per_unit = if !attacker_indices.is_empty() { attacker_total_casualties / attacker_indices.len() as i64 } else { 0 };
    let defender_per_unit = if !defender_indices.is_empty() { defender_total_casualties / defender_indices.len() as i64 } else { 0 };

    for &idx in &attacker_indices {
        let _ = units[idx].apply_casualties(attacker_per_unit);
    }
    for &idx in &defender_indices {
        let _ = units[idx].apply_casualties(defender_per_unit);
    }

    // Process casualties with healthcare and demographic routing
    let region = regions.iter_mut()
        .find(|r| r.id == battle.location);

    if let Some(region) = region {
        let hospital_capacity = region.capacity_pool
            .get(&CapacityType::HospitalBeds)
            .copied()
            .unwrap_or(0.0);

        let (_treated, untreated_dead, wounded_messages) = process_wounded(
            battle.attacker_casualties.wounded + battle.defender_casualties.wounded,
            hospital_capacity,
            &battle.location,
        );
        messages.extend(wounded_messages);

        let region_demographics = &mut region.class_demographics.rural_classes;
        let dead_messages = process_dead(&battle.attacker_casualties, region_demographics, &battle.location);
        messages.extend(dead_messages);
        let dead_messages = process_dead(&battle.defender_casualties, region_demographics, &battle.location);
        messages.extend(dead_messages);

        let deserter_messages = process_deserters(&battle.attacker_casualties, region_demographics, &battle.location);
        messages.extend(deserter_messages);
        let deserter_messages = process_deserters(&battle.defender_casualties, region_demographics, &battle.location);
        messages.extend(deserter_messages);

        if untreated_dead > 0 {
            region.class_demographics.rural_classes.values_mut()
                .for_each(|d| d.economic_status = crate::society::geography::EconomicStatus::Destitute);
        }
    }

    // Update war exhaustion
    front.increase_war_exhaustion(
        battle.attacker.clone(),
        (battle.attacker_casualties.total() as f64 / 1000.0) * config.war_exhaustion_per_casualty
    );
    front.increase_war_exhaustion(
        battle.defender.clone(),
        (battle.defender_casualties.total() as f64 / 1000.0) * config.war_exhaustion_per_casualty
    );

    front.add_battle(battle);

    messages
}

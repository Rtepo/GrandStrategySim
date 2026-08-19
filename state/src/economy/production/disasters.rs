//! Phase 15A: Disaster triggers, effects, and mitigation.
//!
//! Disasters are triggered by weather events (floods, storms, droughts),
//! poor building condition, and industrial accidents. They are mitigated
//! by `FireProtectionCapacity` and `ShelterCapacity` produced by fire
//! brigades (professional and volunteer) and flood shelters.

#![allow(missing_docs)]

use crate::entities::Building;
use crate::registries::enums::Commodity;
use crate::state::Country;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Type of disaster event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DisasterType {
    #[default]
    IndustrialFire,
    BuildingCollapse,
    Flood,
    Earthquake,
    Epidemic,
    /// Phase 17C: Ethnic/religious violence (pogrom).
    Pogrom,
    /// Phase 18B: Vigilante mob (summary justice in low-capacity regions).
    VigilanteMob,
    /// Phase 18C: Terrorist attack (asymmetric warfare by radicalized minorities).
    TerroristAttack,
}

/// A single disaster event affecting a region.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DisasterEvent {
    /// Type of disaster.
    #[serde(default)]
    pub disaster_type: DisasterType,
    /// Region affected.
    #[serde(default)]
    pub region_id: String,
    /// Severity 0.0–1.0.
    #[serde(default)]
    pub severity: f64,
    /// Buildings destroyed (count).
    #[serde(default)]
    pub buildings_destroyed: u32,
    /// Population killed.
    #[serde(default)]
    pub casualties: i64,
    /// Economic damage (currency units).
    #[serde(default)]
    pub economic_damage: f64,
    /// Turn the disaster occurred.
    #[serde(default)]
    pub turn: u32,
    /// Extra fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Result of disaster processing for one turn.
#[derive(Debug, Clone, Default)]
pub struct DisasterTurnResult {
    /// Disasters that occurred this turn.
    pub disasters: Vec<DisasterEvent>,
    /// Total fire protection capacity available (from all sources).
    pub total_fire_capacity: f64,
    /// Total shelter capacity available.
    pub total_shelter_capacity: f64,
}

/// Sum fire protection capacity from all buildings' last production.
pub fn sum_fire_protection_capacity(buildings: &[Building]) -> f64 {
    buildings
        .iter()
        .map(|b| {
            *b.last_production
                .get(&Commodity::FireProtectionCapacity)
                .unwrap_or(&0.0)
        })
        .sum()
}

/// Sum shelter capacity from all buildings' last production.
pub fn sum_shelter_capacity(buildings: &[Building]) -> f64 {
    buildings
        .iter()
        .map(|b| {
            *b.last_production
                .get(&Commodity::ShelterCapacity)
                .unwrap_or(&0.0)
        })
        .sum()
}

/// Check for and trigger disasters.
///
/// # Arguments
/// * `country` - Mutable country (reads weather state, regions; writes population/economic damage).
/// * `buildings` - Buildings (read for condition-based triggers, fire/shelter capacity).
/// * `turn` - Current global turn.
/// * `rng_seed` - Seed for deterministic disaster generation.
///
/// # Rules
/// * Floods triggered by active Flood/Storm weather events.
/// * Industrial fires triggered by poor building condition in industrial sectors.
/// * Building collapses triggered by very poor condition (< 0.2).
/// * Mitigation: fire capacity reduces fire severity, shelter capacity reduces flood casualties.
/// * Disasters reduce building condition, destroy inventory, and reduce regional population.
pub fn check_disaster_triggers(
    country: &mut Country,
    buildings: &[Building],
    turn: u32,
    rng_seed: u64,
) -> DisasterTurnResult {
    let mut rng: rand::rngs::StdRng = if rng_seed > 0 {
        rand::SeedableRng::seed_from_u64(rng_seed.wrapping_add(turn as u64))
    } else {
        rand::SeedableRng::seed_from_u64(turn as u64)
    };

    let fire_capacity = sum_fire_protection_capacity(buildings);
    let shelter_capacity = sum_shelter_capacity(buildings);
    let mut disasters = Vec::new();

    // Group buildings by region for region-level disaster checks.
    let mut buildings_by_region: BTreeMap<String, Vec<&Building>> = BTreeMap::new();
    for b in buildings {
        if b.region_id.is_empty() {
            continue;
        }
        buildings_by_region
            .entry(b.region_id.clone())
            .or_default()
            .push(b);
    }

    // Check weather-triggered disasters (floods, storms).
    for event in &country.weather_state.active_events {
        for region_id in &event.affected_regions {
            let region = match country.regions.iter_mut().find(|r| &r.id == region_id) {
                Some(r) => r,
                None => continue,
            };

            match event.event_type {
                crate::economy::weather::WeatherEventType::Flood => {
                    let base_severity = event.severity;
                    // Shelter capacity mitigates flood severity.
                    let mitigation = (shelter_capacity / 100.0).min(0.5);
                    let effective_severity = (base_severity - mitigation).max(0.0);
                    if effective_severity < 0.1 {
                        continue;
                    }
                    let casualties = ((region.population as f64 * effective_severity * 0.001) as i64).min(region.population);
                    let economic_damage = region.gdp * effective_severity * 0.05;
                    region.population -= casualties;
                    region.gdp -= economic_damage;
                    disasters.push(DisasterEvent {
                        disaster_type: DisasterType::Flood,
                        region_id: region_id.clone(),
                        severity: effective_severity,
                        buildings_destroyed: 0,
                        casualties,
                        economic_damage,
                        turn,
                        extra: Map::new(),
                    });
                }
                crate::economy::weather::WeatherEventType::Storm => {
                    let base_severity = event.severity * 0.6;
                    let mitigation = (shelter_capacity / 100.0).min(0.3);
                    let effective_severity = (base_severity - mitigation).max(0.0);
                    if effective_severity < 0.1 {
                        continue;
                    }
                    let casualties = ((region.population as f64 * effective_severity * 0.0005) as i64).min(region.population);
                    let economic_damage = region.gdp * effective_severity * 0.03;
                    region.population -= casualties;
                    region.gdp -= economic_damage;
                    disasters.push(DisasterEvent {
                        disaster_type: DisasterType::Flood,
                        region_id: region_id.clone(),
                        severity: effective_severity,
                        buildings_destroyed: 0,
                        casualties,
                        economic_damage,
                        turn,
                        extra: Map::new(),
                    });
                }
                _ => {}
            }
        }
    }

    // Check condition-triggered disasters (industrial fires, building collapses).
    for (region_id, region_buildings) in &buildings_by_region {
        for b in region_buildings {
            // Industrial fire: poor condition + industrial sector + random chance.
            if b.condition < 0.4 {
                let fire_chance = (0.4 - b.condition) * 0.05;
                let roll: f64 = rng.gen_range(0.0..1.0);
                if roll < fire_chance {
                    // Fire capacity mitigates fire severity.
                    let mitigation = (fire_capacity / 50.0).min(0.7);
                    let severity = (0.5 - mitigation).max(0.1);
                    let region = match country.regions.iter_mut().find(|r| &r.id == region_id) {
                        Some(r) => r,
                        None => continue,
                    };
                    let damage = b.reserve * severity;
                    let casualties = ((b.current_employment as f64 * severity * 0.1) as i64).min(b.current_employment as i64);
                    region.population -= casualties;
                    disasters.push(DisasterEvent {
                        disaster_type: DisasterType::IndustrialFire,
                        region_id: region_id.clone(),
                        severity,
                        buildings_destroyed: if severity > 0.8 { 1 } else { 0 },
                        casualties,
                        economic_damage: damage,
                        turn,
                        extra: Map::new(),
                    });
                }
            }

            // Building collapse: very poor condition OR structural defect (Phase 22B).
            let condition_collapse_chance = if b.condition < 0.15 {
                (0.15 - b.condition) * 0.1
            } else {
                0.0
            };
            // Phase 22B: structural defect adds collapse risk even at good condition.
            // defect = 0.0 → 0% extra risk; defect = 1.0 → 5% extra risk per turn.
            let defect_collapse_chance = b.structural_defect * 0.05;
            let total_collapse_chance = condition_collapse_chance + defect_collapse_chance;
            if total_collapse_chance > 0.0 {
                let roll: f64 = rng.gen_range(0.0..1.0);
                if roll < total_collapse_chance {
                    let severity = 0.9;
                    let region = match country.regions.iter_mut().find(|r| &r.id == region_id) {
                        Some(r) => r,
                        None => continue,
                    };
                    let casualties = ((b.current_employment as f64 * severity * 0.2) as i64).min(b.current_employment as i64);
                    region.population -= casualties;
                    let mut extra = Map::new();
                    // Phase 22B: record defect attribution for civil lawsuit evidence.
                    if b.structural_defect > 0.0 {
                        extra.insert("structural_defect".to_string(), serde_json::Value::from(b.structural_defect));
                    }
                    disasters.push(DisasterEvent {
                        disaster_type: DisasterType::BuildingCollapse,
                        region_id: region_id.clone(),
                        severity,
                        buildings_destroyed: 1,
                        casualties,
                        economic_damage: b.reserve * severity,
                        turn,
                        extra,
                    });
                }
            }
        }
    }

    DisasterTurnResult {
        disasters,
        total_fire_capacity: fire_capacity,
        total_shelter_capacity: shelter_capacity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sum_fire_capacity_empty() {
        assert_eq!(sum_fire_protection_capacity(&[]), 0.0);
    }

    #[test]
    fn test_sum_fire_capacity() {
        let mut b1 = Building::default();
        b1.last_production
            .insert(Commodity::FireProtectionCapacity, 10.0);
        let mut b2 = Building::default();
        b2.last_production
            .insert(Commodity::FireProtectionCapacity, 5.0);
        assert_eq!(sum_fire_protection_capacity(&[b1, b2]), 15.0);
    }
}

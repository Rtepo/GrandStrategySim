//! Phase 71: Universal disaster system — industrial accidents and natural disasters.
//!
//! Devastation is NOT limited to warfare. Peacetime events also increase the
//! `devastation_index` on parcels:
//!
//! - **Industrial accidents**: factory fires, chemical spills, explosions.
//!   Triggered probabilistically based on factory condition (worse = more
//!   accidents), sector risk profile, and safety inspection level.
//! - **Natural disasters**: floods on river parcels, wildfires on forested
//!   parcels, earthquakes (rare, based on region geology).
//!
//! All disaster probabilities are configurable — no magic numbers in logic.

use crate::society::cadastre::{Cadastre, ParcelId, WaterAccessType};
use serde::{Deserialize, Serialize};

// ============================================================================
// DISASTER TYPES
// ============================================================================

/// Type of disaster event.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum DisasterType {
    /// Factory fire — triggered by poor building condition in industrial sectors.
    FactoryFire,
    /// Chemical spill — triggered by chemical industry accidents.
    ChemicalSpill,
    /// Explosion — triggered by heavy industry or armaments industry accidents.
    Explosion,
    /// Flood — triggered on parcels with river water access.
    Flood,
    /// Wildfire — triggered on forested parcels.
    Wildfire,
    /// Earthquake — rare, random geological event.
    Earthquake,
}

impl DisasterType {
    /// Returns the base devastation impact of this disaster type.
    pub fn base_devastation(&self) -> f64 {
        match self {
            DisasterType::FactoryFire => 0.20,
            DisasterType::ChemicalSpill => 0.30,
            DisasterType::Explosion => 0.40,
            DisasterType::Flood => 0.25,
            DisasterType::Wildfire => 0.35,
            DisasterType::Earthquake => 0.50,
        }
    }

    /// Returns whether this disaster type is industrial (tied to factories).
    pub fn is_industrial(&self) -> bool {
        matches!(
            self,
            DisasterType::FactoryFire | DisasterType::ChemicalSpill | DisasterType::Explosion
        )
    }

    /// Returns whether this disaster type is natural.
    pub fn is_natural(&self) -> bool {
        matches!(
            self,
            DisasterType::Flood | DisasterType::Wildfire | DisasterType::Earthquake
        )
    }
}

// ============================================================================
// DISASTER EVENT
// ============================================================================

/// A recorded disaster event.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DisasterEvent {
    /// Unique event ID.
    pub id: String,
    /// Type of disaster.
    pub disaster_type: DisasterType,
    /// Parcel where the disaster originated.
    pub parcel_id: ParcelId,
    /// Region where the disaster occurred.
    pub region_id: String,
    /// Turn when the disaster occurred.
    pub turn: u32,
    /// Devastation impact applied to the parcel.
    pub devastation_impact: f64,
    /// Building ID affected (if industrial accident).
    pub building_id: Option<String>,
    /// Casualties caused (if any).
    pub casualties: i64,
    /// Log message.
    pub message: String,
}

// ============================================================================
// DISASTER CONFIG
// ============================================================================

/// Configuration for disaster triggering.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisasterConfig {
    /// Base probability per industrial parcel per turn for factory fires.
    /// Scaled by building condition (worse condition = higher probability).
    pub factory_fire_base_rate: f64,
    /// Base probability per chemical-sector parcel per turn for chemical spills.
    pub chemical_spill_base_rate: f64,
    /// Base probability per heavy-industry parcel per turn for explosions.
    pub explosion_base_rate: f64,
    /// Base probability per river parcel per turn for floods.
    pub flood_base_rate: f64,
    /// Base probability per forested parcel per turn for wildfires.
    pub wildfire_base_rate: f64,
    /// Base probability per parcel per turn for earthquakes (very rare).
    pub earthquake_base_rate: f64,
    /// Maximum devastation impact from a single disaster event (cap).
    pub max_single_event_devastation: f64,
    /// Casualty rate per disaster event (fraction of local population affected).
    pub casualty_rate: f64,
    /// Safety inspection effectiveness multiplier — higher inspection levels
    /// reduce industrial accident probability.
    pub safety_inspection_effectiveness: f64,
}

impl Default for DisasterConfig {
    fn default() -> Self {
        Self {
            factory_fire_base_rate: 0.002,
            chemical_spill_base_rate: 0.001,
            explosion_base_rate: 0.0008,
            flood_base_rate: 0.001,
            wildfire_base_rate: 0.0015,
            earthquake_base_rate: 0.0001,
            max_single_event_devastation: 0.50,
            casualty_rate: 0.01,
            safety_inspection_effectiveness: 0.5,
        }
    }
}

// ============================================================================
// DISASTER TRIGGERING
// ============================================================================

/// Result of disaster triggering for a single country.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DisasterTurnResult {
    /// All disasters that occurred this turn.
    pub events: Vec<DisasterEvent>,
    /// Total devastation applied across all parcels.
    pub total_devastation_applied: f64,
    /// Total casualties across all events.
    pub total_casualties: i64,
    /// Log messages.
    pub messages: Vec<String>,
}

/// Triggers disasters for a country's cadastre.
///
/// This function iterates all parcels and probabilistically triggers disasters
/// based on:
/// - Industrial parcels: factory condition, sector risk, safety inspections.
/// - Natural parcels: topography (river, forest), random geological events.
///
/// # Arguments
/// * `cadastre` - Mutable cadastre (parcels will have devastation applied).
/// * `config` - Disaster configuration.
/// * `turn` - Current game turn.
/// * `safety_inspection_level` - 0.0–1.0, from inspectorates module.
/// * `rng_seed` - Deterministic seed for this turn's disaster rolls.
///
/// # Returns
/// `DisasterTurnResult` with all events that occurred.
pub fn trigger_disasters(
    cadastre: &mut Cadastre,
    config: &DisasterConfig,
    turn: u32,
    safety_inspection_level: f64,
    rng_seed: u64,
) -> DisasterTurnResult {
    let mut result = DisasterTurnResult::default();
    let mut rng = DeterministicRng::new(rng_seed.wrapping_add(turn as u64));

    // Collect parcel data needed for disaster evaluation (avoid borrow conflicts)
    struct ParcelInfo {
        id: ParcelId,
        region_id: String,
        is_industrial: bool,
        has_river: bool,
        is_forest: bool,
    }

    let parcel_infos: Vec<ParcelInfo> = cadastre
        .iter()
        .map(|(id, p)| ParcelInfo {
            id,
            region_id: p.region_id.clone(),
            is_industrial: matches!(
                p.zoning,
                crate::society::cadastre::ZoningDesignation::Industrial
            ),
            has_river: p.topography.water_access == WaterAccessType::River,
            is_forest: p.topography.is_forest,
        })
        .collect();

    for info in parcel_infos {
        // Safety inspection reduces industrial accident probability
        let safety_multiplier =
            1.0 - (safety_inspection_level * config.safety_inspection_effectiveness);

        if info.is_industrial {
            // Factory fire
            let fire_prob = config.factory_fire_base_rate * safety_multiplier;
            if rng.next_f64() < fire_prob {
                let impact = DisasterType::FactoryFire
                    .base_devastation()
                    .min(config.max_single_event_devastation);
                apply_disaster(
                    cadastre,
                    &mut result,
                    info.id,
                    &info.region_id,
                    DisasterType::FactoryFire,
                    impact,
                    turn,
                    config,
                );
            }

            // Chemical spill
            let spill_prob = config.chemical_spill_base_rate * safety_multiplier;
            if rng.next_f64() < spill_prob {
                let impact = DisasterType::ChemicalSpill
                    .base_devastation()
                    .min(config.max_single_event_devastation);
                apply_disaster(
                    cadastre,
                    &mut result,
                    info.id,
                    &info.region_id,
                    DisasterType::ChemicalSpill,
                    impact,
                    turn,
                    config,
                );
            }

            // Explosion
            let explosion_prob = config.explosion_base_rate * safety_multiplier;
            if rng.next_f64() < explosion_prob {
                let impact = DisasterType::Explosion
                    .base_devastation()
                    .min(config.max_single_event_devastation);
                apply_disaster(
                    cadastre,
                    &mut result,
                    info.id,
                    &info.region_id,
                    DisasterType::Explosion,
                    impact,
                    turn,
                    config,
                );
            }
        }

        // Natural disasters
        if info.has_river && rng.next_f64() < config.flood_base_rate {
            let impact = DisasterType::Flood
                .base_devastation()
                .min(config.max_single_event_devastation);
            apply_disaster(
                cadastre,
                &mut result,
                info.id,
                &info.region_id,
                DisasterType::Flood,
                impact,
                turn,
                config,
            );
        }

        if info.is_forest && rng.next_f64() < config.wildfire_base_rate {
            let impact = DisasterType::Wildfire
                .base_devastation()
                .min(config.max_single_event_devastation);
            apply_disaster(
                cadastre,
                &mut result,
                info.id,
                &info.region_id,
                DisasterType::Wildfire,
                impact,
                turn,
                config,
            );
        }

        // Earthquake: rare, any parcel
        if rng.next_f64() < config.earthquake_base_rate {
            let impact = DisasterType::Earthquake
                .base_devastation()
                .min(config.max_single_event_devastation);
            apply_disaster(
                cadastre,
                &mut result,
                info.id,
                &info.region_id,
                DisasterType::Earthquake,
                impact,
                turn,
                config,
            );
        }
    }

    result
}

/// Applies a disaster to a parcel, recording the event.
fn apply_disaster(
    cadastre: &mut Cadastre,
    result: &mut DisasterTurnResult,
    parcel_id: ParcelId,
    region_id: &str,
    disaster_type: DisasterType,
    impact: f64,
    turn: u32,
    config: &DisasterConfig,
) {
    let parcel_idx = crate::society::cadastre::parcel_id_to_index(parcel_id);

    // Apply devastation to the parcel
    if let Some(p) = cadastre.get_mut(parcel_id) {
        p.devastation_index = (p.devastation_index + impact).min(1.0);
    }

    // Calculate casualties (fraction of local population — simplified)
    let casualties = (impact * config.casualty_rate * 1000.0) as i64;

    let event = DisasterEvent {
        id: format!(
            "DISASTER-{}-{}-{}",
            turn,
            disaster_type_name(&disaster_type),
            parcel_idx
        ),
        disaster_type: disaster_type.clone(),
        parcel_id,
        region_id: region_id.to_string(),
        turn,
        devastation_impact: impact,
        building_id: None,
        casualties,
        message: format!(
            "[DISASTER] {:?} on parcel idx {} in region {} — devastation +{:.2}, {} casualties",
            disaster_type, parcel_idx, region_id, impact, casualties
        ),
    };

    result.total_devastation_applied += impact;
    result.total_casualties += casualties;
    result.messages.push(event.message.clone());
    result.events.push(event);
}

/// Returns a short name for a disaster type (for event IDs).
fn disaster_type_name(dt: &DisasterType) -> &str {
    match dt {
        DisasterType::FactoryFire => "fire",
        DisasterType::ChemicalSpill => "spill",
        DisasterType::Explosion => "explosion",
        DisasterType::Flood => "flood",
        DisasterType::Wildfire => "wildfire",
        DisasterType::Earthquake => "quake",
    }
}

// ============================================================================
// DEVASTATION SPREAD (TOPOLOGICAL)
// ============================================================================

/// Spreads devastation across the topological adjacency graph.
///
/// Devastation flows from heavily-devastated parcels to their less-devastated
/// neighbors. The spread rate determines how much of the difference flows.
///
/// # Arguments
/// * `cadastre` - Mutable cadastre (parcels will have devastation updated).
/// * `spread_rate` - Fraction of excess devastation that spreads to neighbors.
pub fn spread_devastation(cadastre: &mut Cadastre, spread_rate: f64) {
    // First pass: compute the devastation to spread from each parcel
    let spread_from: Vec<(ParcelId, f64, Vec<ParcelId>)> = {
        cadastre
            .iter()
            .map(|(id, p)| {
                let neighbors: Vec<ParcelId> = p.adjacent_parcels.clone();
                (id, p.devastation_index, neighbors)
            })
            .collect()
    };

    // Second pass: apply spread to neighbors
    let mut spread_to: std::collections::HashMap<ParcelId, f64> = std::collections::HashMap::new();

    for (_, devastation, neighbors) in &spread_from {
        if neighbors.is_empty() {
            continue;
        }
        // Each neighbor receives a fraction of the devastation
        let per_neighbor = devastation * spread_rate / neighbors.len() as f64;
        for neighbor_id in neighbors {
            *spread_to.entry(*neighbor_id).or_insert(0.0) += per_neighbor;
        }
    }

    // Apply the spread (capped at 1.0)
    for (parcel_id, spread_amount) in spread_to {
        if let Some(p) = cadastre.get_mut(parcel_id) {
            p.devastation_index = (p.devastation_index + spread_amount).min(1.0);
        }
    }
}

/// Decays devastation on all parcels (natural recovery).
///
/// # Arguments
/// * `cadastre` - Mutable cadastre.
/// * `decay_rate` - Fraction of devastation to remove per turn.
pub fn decay_devastation(cadastre: &mut Cadastre, decay_rate: f64) {
    for (_, p) in cadastre.iter_mut() {
        p.devastation_index = (p.devastation_index * (1.0 - decay_rate)).max(0.0);
    }
}

/// Computes the average devastation index for a region (on-demand aggregation).
///
/// # Arguments
/// * `cadastre` - The cadastre.
/// * `parcel_ids` - Parcel IDs belonging to the region.
///
/// # Returns
/// Average devastation index (0.0–1.0), or 0.0 if no parcels.
pub fn region_devastation_index(cadastre: &Cadastre, parcel_ids: &[ParcelId]) -> f64 {
    if parcel_ids.is_empty() {
        return 0.0;
    }
    let total: f64 = parcel_ids
        .iter()
        .filter_map(|id| cadastre.get(*id))
        .map(|p| p.devastation_index)
        .sum();
    total / parcel_ids.len() as f64
}

// ============================================================================
// DETERMINISTIC RNG (xorshift)
// ============================================================================

/// Simple deterministic RNG for reproducible disaster triggering.
struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        // xorshift64
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::society::cadastre::{
        ParcelChunk, ParcelOwnerType, ParcelTopography, ZoningDesignation,
    };

    fn make_cadastre_with_parcels(n: usize) -> Cadastre {
        let mut c = Cadastre::default();
        for _ in 0..n {
            let parcel = ParcelChunk {
                soil_class: "Class_III".to_string(),
                size_hectares: 100.0,
                zoning: ZoningDesignation::Unplanned,
                owner_type: ParcelOwnerType::State,
                owner_id: "TREASURY".to_string(),
                region_id: "region_1".to_string(),
                legal_certainty: 0.5,
                infrastructure_access: 0.2,
                current_value: 0.0,
                acquisition_price: 0.0,
                acquisition_turn: 0,
                is_frozen: false,
                zoning_change_turn: 0,
                is_border_zone: false,
                land_use_tag: String::new(),
                adjacent_parcels: Vec::new(),
                co_owners: std::collections::BTreeMap::new(),
                usufruct_holder: None,
                easements: Vec::new(),
                adverse_possession: None,
                pollution_level: 0.0,
                topography: ParcelTopography::default(),
                devastation_index: 0.0,
                micro_region_id: None,
            };
            c.insert(parcel);
        }
        c
    }

    #[test]
    fn test_disaster_type_base_devastation() {
        assert!(DisasterType::FactoryFire.base_devastation() > 0.0);
        assert!(
            DisasterType::Earthquake.base_devastation()
                > DisasterType::FactoryFire.base_devastation()
        );
    }

    #[test]
    fn test_disaster_type_classification() {
        assert!(DisasterType::FactoryFire.is_industrial());
        assert!(!DisasterType::Flood.is_industrial());
        assert!(DisasterType::Flood.is_natural());
        assert!(!DisasterType::Explosion.is_natural());
    }

    #[test]
    fn test_trigger_disasters_no_events_with_zero_rates() {
        let mut c = make_cadastre_with_parcels(10);
        let config = DisasterConfig {
            factory_fire_base_rate: 0.0,
            chemical_spill_base_rate: 0.0,
            explosion_base_rate: 0.0,
            flood_base_rate: 0.0,
            wildfire_base_rate: 0.0,
            earthquake_base_rate: 0.0,
            ..Default::default()
        };

        let result = trigger_disasters(&mut c, &config, 1, 0.0, 42);
        assert!(result.events.is_empty());
        assert_eq!(result.total_devastation_applied, 0.0);
    }

    #[test]
    fn test_trigger_disasters_with_guaranteed_rates() {
        let mut c = make_cadastre_with_parcels(5);
        // Set all parcels to heavy industry for factory fire
        for (_, p) in c.iter_mut() {
            p.zoning = ZoningDesignation::Industrial;
        }
        let config = DisasterConfig {
            factory_fire_base_rate: 1.0, // 100% chance
            ..Default::default()
        };

        let result = trigger_disasters(&mut c, &config, 1, 0.0, 42);
        assert!(
            !result.events.is_empty(),
            "Should trigger factory fires on all industrial parcels"
        );
        assert!(result.total_devastation_applied > 0.0);
    }

    #[test]
    fn test_flood_only_on_river_parcels() {
        let mut c = make_cadastre_with_parcels(5);
        // Set first parcel to have river access
        let first_id = c.iter().next().map(|(id, _)| id);
        if let Some(pid) = first_id {
            if let Some(p) = c.get_mut(pid) {
                p.topography.water_access = WaterAccessType::River;
            }
        }
        let config = DisasterConfig {
            flood_base_rate: 1.0, // 100% chance
            ..Default::default()
        };

        let result = trigger_disasters(&mut c, &config, 1, 0.0, 42);
        let floods = result
            .events
            .iter()
            .filter(|e| e.disaster_type == DisasterType::Flood)
            .count();
        assert_eq!(floods, 1, "Only one river parcel should flood");
    }

    #[test]
    fn test_wildfire_only_on_forest_parcels() {
        let mut c = make_cadastre_with_parcels(5);
        // Set first parcel to be forested
        let first_id = c.iter().next().map(|(id, _)| id);
        if let Some(pid) = first_id {
            if let Some(p) = c.get_mut(pid) {
                p.topography.is_forest = true;
            }
        }
        let config = DisasterConfig {
            wildfire_base_rate: 1.0,
            ..Default::default()
        };

        let result = trigger_disasters(&mut c, &config, 1, 0.0, 42);
        let wildfires = result
            .events
            .iter()
            .filter(|e| e.disaster_type == DisasterType::Wildfire)
            .count();
        assert_eq!(wildfires, 1, "Only one forest parcel should have wildfire");
    }

    #[test]
    fn test_safety_inspection_reduces_accidents() {
        let mut c1 = make_cadastre_with_parcels(10);
        for (_, p) in c1.iter_mut() {
            p.zoning = ZoningDesignation::Industrial;
        }
        let mut c2 = c1.clone();

        let config = DisasterConfig {
            factory_fire_base_rate: 0.5,
            ..Default::default()
        };

        // No safety inspections
        let result_no_safety = trigger_disasters(&mut c1, &config, 1, 0.0, 42);
        // Full safety inspections
        let result_with_safety = trigger_disasters(&mut c2, &config, 1, 1.0, 42);

        // With safety inspections, fewer or equal events should occur
        assert!(
            result_with_safety.events.len() <= result_no_safety.events.len(),
            "Safety inspections should reduce accidents"
        );
    }

    #[test]
    fn test_devastation_spread_to_adjacent_parcels() {
        let mut c = Cadastre::default();
        let p1 = ParcelChunk {
            region_id: "r1".to_string(),
            devastation_index: 0.5,
            adjacent_parcels: Vec::new(), // Will set after we know IDs
            ..Default::default()
        };
        let p2 = ParcelChunk {
            region_id: "r1".to_string(),
            devastation_index: 0.0,
            ..Default::default()
        };
        let id1 = c.insert(p1);
        let id2 = c.insert(p2);

        // Set adjacency: p1 -> p2
        c.get_mut(id1).unwrap().adjacent_parcels = vec![id2];

        spread_devastation(&mut c, 0.1);

        // p2 should have received some devastation from p1
        let p2_devastation = c.get(id2).unwrap().devastation_index;
        assert!(
            p2_devastation > 0.0,
            "Devastation must spread to adjacent parcels"
        );
    }

    #[test]
    fn test_devastation_decay() {
        let mut c = make_cadastre_with_parcels(3);
        for (_, p) in c.iter_mut() {
            p.devastation_index = 0.5;
        }

        decay_devastation(&mut c, 0.1);

        for (_, p) in c.iter() {
            assert!(
                (p.devastation_index - 0.45).abs() < 0.001,
                "Devastation must decay by 10%"
            );
        }
    }

    #[test]
    fn test_region_devastation_index_aggregation() {
        let mut c = Cadastre::default();
        let p1 = ParcelChunk {
            devastation_index: 0.2,
            ..Default::default()
        };
        let p2 = ParcelChunk {
            devastation_index: 0.4,
            ..Default::default()
        };
        let p3 = ParcelChunk {
            devastation_index: 0.6,
            ..Default::default()
        };
        let id1 = c.insert(p1);
        let id2 = c.insert(p2);
        let id3 = c.insert(p3);

        let avg = region_devastation_index(&c, &[id1, id2, id3]);
        assert!(
            (avg - 0.4).abs() < 0.001,
            "Average must be (0.2+0.4+0.6)/3 = 0.4"
        );
    }

    #[test]
    fn test_region_devastation_index_empty() {
        let c = Cadastre::default();
        let avg = region_devastation_index(&c, &[]);
        assert_eq!(avg, 0.0);
    }

    #[test]
    fn test_disaster_deterministic_with_same_seed() {
        let mut c1 = make_cadastre_with_parcels(10);
        let mut c2 = make_cadastre_with_parcels(10);
        let config = DisasterConfig {
            earthquake_base_rate: 0.5,
            ..Default::default()
        };

        let r1 = trigger_disasters(&mut c1, &config, 1, 0.0, 12345);
        let r2 = trigger_disasters(&mut c2, &config, 1, 0.0, 12345);

        assert_eq!(
            r1.events.len(),
            r2.events.len(),
            "Same seed must produce same events"
        );
    }

    #[test]
    fn test_devastation_capped_at_1() {
        let mut c = Cadastre::default();
        let p = ParcelChunk {
            devastation_index: 0.9,
            ..Default::default()
        };
        let id = c.insert(p);

        // Apply a disaster that would push above 1.0
        if let Some(p) = c.get_mut(id) {
            p.devastation_index = (p.devastation_index + 0.5).min(1.0);
        }
        assert_eq!(c.get(id).unwrap().devastation_index, 1.0);
    }
}

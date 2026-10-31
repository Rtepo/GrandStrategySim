//! v4.23.0 WS5: War Bridge — materializes diplomatic wars into real military
//! fronts, battles, casualties, devastation, and POW captures.
//!
//! v4.22.1 introduced `GameState::declared_wars` / `at_war_with` /
//! `BilateralTension` — wars existed on the *diplomatic* ledger but never
//! reached the *military* ledger, so `diplomatic_active_wars` never matched
//! `military_active_wars` (fronts). This module closes that gap once per
//! turn, sequentially at GameState scope (cross-country mutation cannot run
//! inside the per-country `par_iter` military pass):
//!
//! 1. **Front materialization** — every active war pair gets exactly one
//!    `Front` (`WAR-{a}-{b}`) in *each* belligerent's `military_fronts`, so
//!    the Military dashboard reports the same war count as the diplomacy
//!    ledger. Bounded: one front per war pair per country.
//! 2. **Battle resolution** — one deterministic battle per war pair per
//!    turn via the existing `resolve_battle` engine (bounded: at most
//!    `MAX_WAR_UNITS_PER_SIDE` units per belligerent). Casualties are
//!    written back to the real OOB, routed to class demographics, and
//!    hospitals are consulted for the wounded.
//! 3. **Devastation** — battle impacts land on the defender's combat-zone
//!    parcels via the same `devastation_index` the heatmap reads.
//! 4. **POWs** — the loser of each battle yields prisoners to the victor's
//!    `PowCamp`; on peace, fronts are torn down and POWs repatriated.
//! 5. **Peacetime→wartime disaster hooks** — the Phase-71 disaster engine
//!    (`trigger_disasters` / `decay_devastation` / `spread_devastation`)
//!    runs for every country: industrial/natural events in peacetime, and
//!    battle devastation rides the same `devastation_index` channel in war.
//!
//! Determinism: sorted iteration everywhere, no RNG in battle resolution
//! (the combat engine is deterministic by construction), disaster rolls use
//! a per-country FNV seed + turn, identical across runs.

use std::collections::{BTreeSet, HashMap};

use crate::infrastructure::CapacityType;
use crate::military::combat::{
    process_dead, process_deserters, process_wounded, resolve_battle,
};
use crate::military::fronts::{BattleResult, Front, RegionControl};
use crate::military::morale::apply_casualty_morale_impact;
use crate::military::oob::OrderOfBattle;
use crate::international::DiplomaticRelation;
use crate::military::pows::{
    capture_pows_from_casualties, repatriate_pows_from_country, PowCaptureConfig,
};
use crate::military::units::MilitaryUnit;
use crate::military::war_declarations::{
    settle_peace, tension_key, PeaceTerms, WarOutcome,
};
use crate::society::cadastre::{parcel_id_to_index, ParcelId};
use crate::society::disasters::{
    decay_devastation, spread_devastation, trigger_disasters, DisasterConfig,
};
use crate::society::geography::{Climate, Region};
use crate::state::{Country, GameState};

/// Maximum units per side committed to a single war-front battle per turn.
/// Keeps battles bounded regardless of OOB size.
const MAX_WAR_UNITS_PER_SIDE: usize = 8;
/// Maximum parcels tracked as a front's combat zone (devastation target).
const MAX_COMBAT_ZONE_PARCELS: usize = 64;
/// Prefix identifying fronts created by the war bridge.
pub const WAR_FRONT_PREFIX: &str = "WAR-";

/// Canonical front id for a war pair (sorted countries — one front per pair).
pub fn war_front_id(country_a: &str, country_b: &str) -> String {
    let (a, b) = canonical_pair(country_a, country_b);
    format!("{}{}-{}", WAR_FRONT_PREFIX, a, b)
}

/// Canonical (sorted) representation of a bilateral pair.
fn canonical_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

// ============================================================================
// v4.24.0: Synthetic world adjacency (casus belli border gate)
// ============================================================================
//
// The generated world has no explicit inter-country border graph — each
// country's regions form a country-local ring. For the war-declaration
// gate we derive a deterministic synthetic adjacency from the sorted
// country roster:
//
//  1. **Continental ring** — every country borders its lexicographic
//     neighbours (i ↔ i+1, plus wraparound), like provinces around a
//     continent's rim. With N countries this yields exactly N borders.
//  2. **Cultural bridge** — countries sharing `macro_indicators
//     .cultural_group` form chains in sorted order, so culturally related
//     neighbours border even when the ring separates them.
//
// Everything is derived from sorted names + stored culture — no RNG, no
// HashMap iteration order, identical across runs and platforms.
/// Deterministic synthetic border set over all country pairs — see the
/// module comment above for the construction (continental ring +
/// cultural bridge chains).
pub fn compute_country_adjacency(state: &GameState) -> BTreeSet<(String, String)> {
    let mut names: Vec<String> = state.countries.keys().cloned().collect();
    names.sort();
    let n = names.len();
    let mut adjacency: BTreeSet<(String, String)> = BTreeSet::new();

    // 1) Continental ring.
    for i in 0..n {
        let a = &names[i];
        let b = &names[(i + 1) % n];
        if a != b {
            adjacency.insert(canonical_pair(a, b));
        }
    }

    // 2) Cultural bridge — chain consecutive members of each culture group.
    let mut by_culture: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for name in &names {
        let group = state.countries[name]
            .macro_indicators
            .cultural_group
            .trim()
            .to_string();
        if !group.is_empty() {
            by_culture.entry(group).or_default().push(name.clone());
        }
    }
    for (_group, members) in by_culture {
        for w in members.windows(2) {
            adjacency.insert(canonical_pair(&w[0], &w[1]));
        }
    }

    adjacency
}

/// Whether two countries share a synthetic land border.
pub fn countries_share_border(state: &GameState, a: &str, b: &str) -> bool {
    compute_country_adjacency(state).contains(&canonical_pair(a, b))
}

/// Turn-level entry point. Called sequentially from the engine after
/// `process_diplomatic_friction` so wars declared *this* turn materialize
/// immediately. `diplomacy` is the per-direction relations map already
/// built for the friction pass — the bridge reads it to detect wars whose
/// belligerents have normalized, and settles them.
pub fn process_war_bridge(
    state: &mut GameState,
    diplomacy: &HashMap<String, HashMap<String, DiplomaticRelation>>,
    current_turn: u32,
) {
    // ── 1. Collect active war pairs (canonical sorted order). ──
    // `at_war_with` is the authoritative "war is live" flag; `declared_wars`
    // supplies aggressor/defender roles.
    let mut pairs: BTreeSet<(String, String)> = BTreeSet::new();
    for (name, country) in &state.countries {
        for enemy in &country.at_war_with {
            pairs.insert(canonical_pair(name, enemy));
        }
    }

    // ── 1b. Settle wars whose belligerents have normalized. ──
    // Wars ignite when bilateral tension crosses `war_tension_threshold`
    // (the escalation checker then resets tension to zero). While average
    // relations stay below the friction hostility floor (-60), fresh
    // provocations keep re-escalating — the war stays hot. Once relations
    // normalize AND residual tension sits below the war threshold, the war
    // settles Status Quo Ante: `at_war_with` clears, fronts tear down, and
    // POWs repatriate in step 2. Deterministic — driven by the same
    // relation drift that caused the war.
    const PEACE_RELATIONS_FLOOR: i64 = -60;
    let peace_tension_cap = state.war_declaration_config.war_tension_threshold;
    let rel_avg = |a: &str, b: &str| -> i64 {
        let fwd = diplomacy
            .get(a)
            .and_then(|m| m.get(b))
            .map(|r| r.relations)
            .unwrap_or(0);
        let bwd = diplomacy
            .get(b)
            .and_then(|m| m.get(a))
            .map(|r| r.relations)
            .unwrap_or(0);
        (fwd + bwd) / 2
    };
    let settlements: Vec<(String, String)> = pairs
        .iter()
        .filter(|(a, b)| {
            let tension = state
                .bilateral_tensions
                .get(&tension_key(a, b))
                .map(|t| t.level)
                .unwrap_or(0.0);
            rel_avg(a, b) >= PEACE_RELATIONS_FLOOR && tension < peace_tension_cap
        })
        .cloned()
        .collect();
    if !settlements.is_empty() {
        // Materialize war lists into the HashMap form `settle_peace` expects.
        let mut war_map: HashMap<String, Vec<String>> = HashMap::new();
        for (name, country) in &state.countries {
            if !country.at_war_with.is_empty() {
                war_map.insert(name.clone(), country.at_war_with.clone());
            }
        }
        for (a, b) in &settlements {
            let result =
                settle_peace(a, b, &PeaceTerms::StatusQuoAnte, &mut war_map);
            for msg in &result.messages {
                eprintln!("{}", msg);
            }
            // Mark the live WarState ended (history rows stay, flagged)
            // and record the outcome for the Historical Wars ledger —
            // relations-normalized settlements are white peaces; casualty
            // totals come from the shared front ledger before teardown.
            let front_id = war_front_id(a, b);
            // `roles` is built below — resolve aggressor/defender from the
            // live WarState directly.
            let (agg, ddf) = state
                .declared_wars
                .iter()
                .rev()
                .find(|ws| {
                    ws.end_turn.is_none()
                        && canonical_pair(&ws.aggressor, &ws.defender)
                            == canonical_pair(a, b)
                })
                .map(|ws| (ws.aggressor.clone(), ws.defender.clone()))
                .unwrap_or_else(|| (a.clone(), b.clone()));
            let (agg_cas, def_cas, score) = state
                .countries
                .get(&ddf)
                .and_then(|c| {
                    c.military_fronts
                        .iter()
                        .find(|f| f.id == front_id)
                        .map(|f| {
                            (
                                f.get_country_casualties(&agg),
                                f.get_country_casualties(&ddf),
                                front_war_score(f, &agg),
                            )
                        })
                })
                .unwrap_or((0, 0, 0.0));
            let outcome = WarOutcome {
                victor: if score > 0.0 {
                    Some(agg.clone())
                } else if score < 0.0 {
                    Some(ddf.clone())
                } else {
                    None
                },
                terms_label: "White Peace — normalized relations".to_string(),
                regions_ceded: Vec::new(),
                reparations_paid: 0.0,
                aggressor_casualties: agg_cas,
                defender_casualties: def_cas,
                pows_repatriated: true,
            };
            for ws in state.declared_wars.iter_mut().rev() {
                if ws.end_turn.is_none()
                    && canonical_pair(&ws.aggressor, &ws.defender)
                        == canonical_pair(a, b)
                {
                    ws.end_turn = Some(current_turn);
                    ws.outcome = Some(outcome.clone());
                    break;
                }
            }
            pairs.remove(&canonical_pair(a, b));
        }
        // Write the settled lists back onto the countries.
        for (name, wars) in war_map {
            if let Some(country) = state.countries.get_mut(&name) {
                country.at_war_with = wars;
            }
        }
    }

    // Resolve aggressor/defender roles from the most recent live WarState.
    let mut roles: HashMap<(String, String), (String, String)> = HashMap::new();
    for ws in &state.declared_wars {
        if ws.end_turn.is_some() {
            continue; // Settled — a redeclared pair uses its newest WarState.
        }
        roles.insert(
            canonical_pair(&ws.aggressor, &ws.defender),
            (ws.aggressor.clone(), ws.defender.clone()),
        );
    }

    // ── 2. Tear down stale war fronts + repatriate POWs on peace. ──
    for country in state.countries.values_mut() {
        let mut repatriate_from: Vec<String> = Vec::new();
        country.military_fronts.retain(|f| {
            if !f.id.starts_with(WAR_FRONT_PREFIX) {
                return true; // Not a war-bridge front (e.g., domestic fronts).
            }
            // A war front stays iff its pair is still active.
            let still_at_war = f.involved_countries.len() == 2
                && pairs.contains(&canonical_pair(
                    &f.involved_countries[0],
                    &f.involved_countries[1],
                ));
            if !still_at_war {
                for other in &f.involved_countries {
                    if *other != country.name {
                        repatriate_from.push(other.clone());
                    }
                }
            }
            still_at_war
        });
        for origin in repatriate_from {
            let repatriated =
                repatriate_pows_from_country(&mut country.pow_camp, &origin);
            if !repatriated.is_empty() {
                eprintln!(
                    "[WAR] Peace: {} repatriates {} POWs to {}",
                    country.name,
                    repatriated.len(),
                    origin
                );
            }
        }
    }

    // ── 3. Disaster lifecycle for every country (peacetime→wartime hook). ──
    // `trigger_disasters` / `decay_devastation` / `spread_devastation` were
    // never called from the turn loop before this bridge. Industrial
    // accidents + natural disasters run in peace AND war; battle devastation
    // rides the same `devastation_index` channel the cadastre heatmap reads.
    let disaster_config = DisasterConfig::default();
    let mut country_names: Vec<String> = state.countries.keys().cloned().collect();
    country_names.sort();
    for name in &country_names {
        let Some(country) = state.countries.get_mut(name) else {
            continue;
        };
        let mil = country.military_config.clone();
        // Natural recovery + spread before new events land.
        decay_devastation(&mut country.cadastre, mil.devastation_decay_rate);
        spread_devastation(&mut country.cadastre, mil.devastation_spread_rate);
        // Safety-inspection effectiveness: use the real health-system service
        // quality as the institutional-capacity proxy (0.0–1.0). There is no
        // separate inspectorates field in state.
        let safety = country
            .macro_indicators
            .health_statistics
            .service_quality
            .clamp(0.0, 1.0);
        let result = trigger_disasters(
            &mut country.cadastre,
            &disaster_config,
            current_turn,
            safety,
            fnv1a(name),
        );
        for msg in &result.messages {
            eprintln!("{}", msg);
        }

        // ── v4.24.0: POW camp lifecycle (per-country, once per turn) ──
        // Fresh captures are processed into internment (labor-eligible),
        // interned POWs attempt deterministic escapes at a rate set by
        // camp conditions (worse rations/health ⇒ more pressure), and
        // attrition removes a fraction of the held population. Rates are
        // derived from average prisoner productivity (the health/ration
        // proxy), never magic constants.
        let held = country.pow_camp.prisoners.len();
        if held > 0 || country.pow_camp.captured_last_turn > 0 {
            let avg_prod = if held > 0 {
                country
                    .pow_camp
                    .prisoners
                    .iter()
                    .map(|p| p.productivity_factor)
                    .sum::<f64>()
                    / held as f64
            } else {
                0.6
            };
            let intake = country.pow_camp.process_intake(current_turn);
            if intake > 0 {
                eprintln!("[POW] {}: {} POWs processed into internment", name, intake);
            }
            let escape_rate = (1.0 - avg_prod.clamp(0.0, 1.0)) * 0.01;
            let escapes = country.pow_camp.process_escapes(escape_rate);
            if escapes > 0 {
                eprintln!("[POW] {}: {} POWs escaped internment", name, escapes);
            }
            let attrition_rate = (1.0 - avg_prod.clamp(0.0, 1.0)) * 0.005;
            let deaths = country.pow_camp.process_attrition(attrition_rate);
            if deaths > 0 {
                eprintln!("[POW] {}: {} POWs died in captivity", name, deaths);
            }
            country.pow_camp.cleanup();
        }
    }

    // ── 4. Materialize fronts + resolve one battle per war pair. ──
    for (a, b) in &pairs {
        let (aggressor, defender) = roles
            .get(&(a.clone(), b.clone()))
            .cloned()
            .unwrap_or_else(|| (a.clone(), b.clone()));
        process_war_pair(state, &aggressor, &defender, current_turn);
    }
}

/// FNV-1a hash — stable per-country disaster seed (no HashMap iteration
/// order dependence, identical across runs).
fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        h ^= byte as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// v4.24.0: Ordered invasion corridor on a country's land ring.
///
/// Regions form a country-local ring (see `build_structured_edges`):
/// index 0 is the capital, `i` borders `i±1 mod n`. A war front enters at
/// a deterministic border province — `fnv1a(front_id) % n_provinces` picks
/// the entry edge — then the front line marches inward along contiguous
/// ring neighbours toward the capital, which is ALWAYS last:
///
///   `[border_entry, …, Region3, Region2, Capital]`
///
/// Both belligerents' copies of a `WAR-*` front share this corridor, so
/// the two ledgers stay byte-identical. Determinism: region Vec order is
/// generation order (capital first, then ring order); only LandRegion
/// nodes participate (maritime nodes excluded).
fn invasion_corridor(country: &Country, front_id: &str) -> Vec<String> {
    let land: Vec<&Region> = country
        .regions
        .iter()
        .filter(|r| r.node_type == crate::society::geography::NodeType::LandRegion)
        .collect();
    let capital_id = land
        .iter()
        .find(|r| r.is_capital)
        .map(|r| r.id.clone());
    let provinces: Vec<String> = land
        .iter()
        .filter(|r| !r.is_capital)
        .map(|r| r.id.clone())
        .collect();
    let m = provinces.len();
    if m == 0 {
        // Single-region country (capital only) — the corridor IS the capital.
        return capital_id.into_iter().collect();
    }
    // Entry edge rotates per war pair so different wars breach different
    // borders. The corridor then walks contiguous ring neighbours inward
    // (provinces are stored in ring order, so index-1 is the neighbour
    // one step closer to the capital).
    let entry = (fnv1a(front_id) as usize) % m;
    let mut corridor: Vec<String> = provinces[..=entry].iter().rev().cloned().collect();
    if let Some(cap) = capital_id {
        corridor.push(cap);
    } else {
        // Legacy save without a capital flag — append the rest of the ring.
        corridor.extend(provinces[entry + 1..].iter().rev().cloned());
    }
    corridor
}

/// The region where a war front's battle line currently sits — the first
/// corridor region not under occupation. `Occupied` markers accumulate in
/// `region_control` as the attacker advances; everything ahead of the
/// first unoccupied cell is taken ground.
pub(crate) fn frontier_region(front: &Front, corridor: &[String]) -> Option<String> {
    for rid in corridor {
        match front.region_control.get(rid) {
            Some(RegionControl::Occupied(_)) => continue,
            _ => return Some(rid.clone()),
        }
    }
    corridor.last().cloned()
}

/// The last corridor cell the attacker holds — counter-attacks roll the
/// front line back here first.
fn last_occupied_region(front: &Front, corridor: &[String]) -> Option<String> {
    corridor
        .iter()
        .rev()
        .find(|rid| matches!(front.region_control.get(*rid), Some(RegionControl::Occupied(_))))
        .cloned()
}

/// Deterministic mustering region — a country's own corridor entry for
/// this war pair (its border province facing the enemy).
fn muster_region(country: &Country, front_id: &str) -> Option<String> {
    invasion_corridor(country, front_id).into_iter().next()
}

/// Ensures the war front exists in `country.military_fronts`, creating it
/// with the DEFENDER's invasion corridor and combat-zone parcels. Both
/// belligerents' copies share the corridor so the two ledgers stay
/// byte-identical. `involved` must be the canonical [aggressor, defender]
/// ordering. Legacy fronts (single mustering region) are migrated: their
/// `regions` list is replaced by the corridor, preserving any existing
/// control marks.
fn ensure_war_front(
    country: &mut Country,
    front_id: &str,
    front_name: &str,
    involved: &[String],
    corridor: &[String],
    zone_parcels: &[ParcelId],
) {
    if corridor.is_empty() {
        return; // Landless defender — no front possible.
    }
    if let Some(front) = country
        .military_fronts
        .iter_mut()
        .find(|f| f.id == front_id)
    {
        // Legacy-front migration: rebuild the corridor, preserving
        // occupation marks for regions still on the line.
        if front.regions != corridor {
            front.regions = corridor.to_vec();
        }
        for rid in corridor {
            front
                .region_control
                .entry(rid.clone())
                .or_insert(RegionControl::Owner);
        }
        return;
    }
    let mut front = Front::new(
        front_id.to_string(),
        front_name.to_string(),
        corridor.to_vec(),
        involved.to_vec(),
    );
    front.combat_zones = zone_parcels.to_vec();
    country.military_fronts.push(front);
    eprintln!(
        "[WAR] Front '{}' materializes in {} (invasion corridor: {} → {} … → {})",
        front_id,
        country.name,
        corridor.first().cloned().unwrap_or_default(),
        corridor.get(1).cloned().unwrap_or_default(),
        corridor.last().cloned().unwrap_or_default()
    );
}

/// Selects up to `MAX_WAR_UNITS_PER_SIDE` combat units for a war battle.
/// Units garrisoned in the front region fight first; the remainder are
/// drawn deterministically by unit id (no RNG).
fn select_war_units(oob: &OrderOfBattle, front_region: &str) -> Vec<MilitaryUnit> {
    let mut all: Vec<&MilitaryUnit> = oob
        .all_units()
        .filter(|u| !u.is_peasant_battalion() && u.manpower > 0)
        .collect();
    all.sort_by(|a, b| a.id.cmp(&b.id));
    let mut engaged: Vec<&MilitaryUnit> = Vec::new();
    let mut reserves: Vec<&MilitaryUnit> = Vec::new();
    for u in all {
        if u.location == front_region {
            engaged.push(u);
        } else {
            reserves.push(u);
        }
    }
    engaged.extend(
        reserves
            .into_iter()
            .take(MAX_WAR_UNITS_PER_SIDE.saturating_sub(engaged.len())),
    );
    engaged
        .into_iter()
        .take(MAX_WAR_UNITS_PER_SIDE)
        .cloned()
        .collect()
}

/// Derive terrain type string from a region's climate and land use.
/// Mirrors `derive_terrain` in `military::turn` (private there).
fn derive_terrain(region: &Region) -> &'static str {
    match region.climate {
        Climate::Mountainous => "mountain",
        _ => {
            if let Some(forest_data) =
                region.land_use_inventory.categories.get("forests")
            {
                let total = region.land_use_inventory.total_area.max(1.0);
                if forest_data.area_hectares / total > 0.25 {
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

/// Resolves one war pair: front materialization on both sides plus a
/// single bounded battle between the two belligerents' real OOBs.
fn process_war_pair(
    state: &mut GameState,
    aggressor: &str,
    defender: &str,
    current_turn: u32,
) {
    let front_id = war_front_id(aggressor, defender);
    let front_name = format!("{}–{} War", aggressor, defender);
    let involved = vec![aggressor.to_string(), defender.to_string()];

    // v4.24.0: the theater is the DEFENDER's invasion corridor — border
    // provinces first, capital last. Both front copies share it so the
    // ledgers stay byte-identical.
    let corridor = state
        .countries
        .get(defender)
        .map(|dc| invasion_corridor(dc, &front_id))
        .unwrap_or_default();
    // Combat zone = the current frontier region's parcels (recomputed each
    // battle; at materialization it's the border entry region).
    let zone_parcels: Vec<ParcelId> = {
        let dc = state.countries.get(defender);
        let front = dc.and_then(|c| {
            c.military_fronts.iter().find(|f| f.id == front_id)
        });
        let battle_region = front
            .and_then(|f| frontier_region(f, &corridor))
            .or_else(|| corridor.first().cloned());
        dc.and_then(|c| {
            c.regions
                .iter()
                .find(|r| Some(&r.id) == battle_region.as_ref())
                .map(|r| {
                    let mut p = r.parcel_ids.clone();
                    p.sort_by_key(|pid| parcel_id_to_index(*pid));
                    p.truncate(MAX_COMBAT_ZONE_PARCELS);
                    p
                })
        })
        .unwrap_or_default()
    };

    // Materialize the front in BOTH countries — 1 front per war pair per
    // belligerent, so military_active_wars == diplomatic_active_wars.
    for name in [aggressor, defender] {
        if let Some(country) = state.countries.get_mut(name) {
            ensure_war_front(
                country,
                &front_id,
                &front_name,
                &involved,
                &corridor,
                &zone_parcels,
            );
        }
    }

    // Split-borrow both belligerents for the battle itself.
    let mut agg = state.countries.remove(aggressor);
    let mut def = state.countries.remove(defender);
    let mut concluded: Option<WarOutcome> = None;
    if let (Some(ac), Some(dc)) = (agg.as_mut(), def.as_mut()) {
        concluded = resolve_war_battle(ac, dc, &front_id, &corridor, current_turn);
    }
    if let Some(c) = agg {
        state.countries.insert(aggressor.to_string(), c);
    }
    if let Some(c) = def {
        state.countries.insert(defender.to_string(), c);
    }

    // Post-battle: a concluded war (exhaustion/capitulation) marks the
    // live WarState ended — the recidivism cooldown and the Historical
    // Wars ledger both read `end_turn`/`outcome`.
    if let Some(outcome) = concluded {
        for ws in state.declared_wars.iter_mut().rev() {
            if ws.end_turn.is_none()
                && canonical_pair(&ws.aggressor, &ws.defender)
                    == canonical_pair(aggressor, defender)
            {
                ws.end_turn = Some(current_turn);
                ws.outcome = Some(outcome);
                break;
            }
        }
    }
}

/// Executes the bounded per-turn battle between aggressor (attacker) and
/// defender at the front's current FRONTIER region (v4.24.0 — the border
/// provinces fight first; the line advances contiguously toward the
/// capital instead of teleporting to `*-Region1`). Then writes everything
/// back: unit stockpiles/casualties, demographic routing, POW capture,
/// devastation on the battle region's parcels, region control
/// (occupation advance / counter-attack rollback), war exhaustion
/// (+2%–8% per decisive battle, stalemates grind slower), and the battle
/// record on both countries' byte-identical war fronts.
///
/// Returns `Some(WarOutcome)` when the battle concluded the war —
/// exhaustion collapse, score capitulation, or capital fall — after
/// clearing `at_war_with`, applying treaty terms (incl. real reparations
/// transfers), and recording the outcome on both fronts.
fn resolve_war_battle(
    ac: &mut Country,
    dc: &mut Country,
    front_id: &str,
    corridor: &[String],
    current_turn: u32,
) -> Option<WarOutcome> {
    let aggressor = ac.name.clone();
    let defender = dc.name.clone();

    // Front-line positions: the battle happens at the defender's current
    // frontier region (first unoccupied corridor cell). The aggressor
    // musters from its own border province for the same war pair.
    let def_front = dc
        .military_fronts
        .iter()
        .find(|f| f.id == front_id);
    let def_corridor: Vec<String> = if corridor.is_empty() {
        def_front.map(|f| f.regions.clone()).unwrap_or_default()
    } else {
        corridor.to_vec()
    };
    let battle_region = def_front
        .and_then(|f| frontier_region(f, &def_corridor))
        .or_else(|| def_corridor.first().cloned());
    let agg_region = muster_region(ac, front_id);
    let (Some(agg_region), Some(battle_region)) = (agg_region, battle_region) else {
        return None;
    };
    // Legacy alias: demographic routing for the defender happens on the
    // battle region (their soil); the attacker routes on its muster region.
    let def_region = battle_region.clone();

    // Draw bounded combat forces (clones — resolve_battle mutates stockpiles).
    // v4.24.0: the defender's units AT the frontier fight first (defense
    // concentrates at the front line).
    let mut attacker_units = select_war_units(&ac.order_of_battle, &agg_region);
    let mut defender_units = select_war_units(&dc.order_of_battle, &battle_region);
    if attacker_units.is_empty() || defender_units.is_empty() {
        return None; // No forces committed — cold war, no battle this turn.
    }
    let attacker_engaged: i64 = attacker_units.iter().map(|u| u.manpower).sum();
    let defender_engaged: i64 = defender_units.iter().map(|u| u.manpower).sum();

    // Battle happens on defender soil — defender's config + terrain apply.
    let config = dc.military_config.clone();
    let terrain = dc
        .regions
        .iter()
        .find(|r| r.id == battle_region)
        .map(derive_terrain)
        .unwrap_or("plains");
    let battle_id = format!("WARBATTLE-{}-{}", current_turn, front_id);

    let mut battle = resolve_battle(
        &mut attacker_units,
        &mut defender_units,
        battle_region.clone(),
        aggressor.clone(),
        defender.clone(),
        current_turn,
        battle_id,
        &config,
        terrain,
    );
    let attacker_casualties = battle.attacker_casualties.clone();
    let defender_casualties = battle.defender_casualties.clone();

    eprintln!(
        "[WAR] {} — {} (att) vs {} (def) in {}: {:?} ({} vs {} casualties)",
        battle.id,
        aggressor,
        defender,
        battle_region,
        battle.result,
        attacker_casualties.total(),
        defender_casualties.total()
    );

    // v4.25.0: Snapshot the chain of command BEFORE writeback — engaged
    // units' regiment/division/army commanders become the battle's
    // participating commanders (deduped, in OOB order).
    let agg_cmd_map = ac.order_of_battle.commanders_by_unit();
    let def_cmd_map = dc.order_of_battle.commanders_by_unit();
    let cmd_for = |units: &[MilitaryUnit],
                   map: &rustc_hash::FxHashMap<String, Vec<(String, String)>>| {
        let mut seen = BTreeSet::new();
        let mut out: Vec<(String, String)> = Vec::new();
        for u in units {
            if let Some(chain) = map.get(&u.id) {
                for (vip_id, echelon) in chain {
                    if seen.insert((vip_id.clone(), echelon.clone())) {
                        out.push((vip_id.clone(), echelon.clone()));
                    }
                }
            }
        }
        out
    };
    battle.attacker_commanders = cmd_for(&attacker_units, &agg_cmd_map);
    battle.defender_commanders = cmd_for(&defender_units, &def_cmd_map);

    // Write supply/equipment deltas + casualties back into the real OOBs.
    writeback_battle_units(
        &mut ac.order_of_battle,
        &attacker_units,
        attacker_casualties.total(),
    );
    writeback_battle_units(
        &mut dc.order_of_battle,
        &defender_units,
        defender_casualties.total(),
    );
    ac.order_of_battle.cleanup_dead();
    dc.order_of_battle.cleanup_dead();

    // v4.25.0: Committed-vs-surviving force lines by branch — initial
    // strength from the engaged clones, survivors from the real OOB
    // (destroyed units resolve to 0).
    battle.attacker_forces = crate::military::combat::battle_force_lines(
        &attacker_units,
        &|uid| {
            ac.order_of_battle
                .find_unit(uid)
                .map(|u| u.manpower)
                .unwrap_or(0)
        },
    );
    battle.defender_forces = crate::military::combat::battle_force_lines(
        &defender_units,
        &|uid| {
            dc.order_of_battle
                .find_unit(uid)
                .map(|u| u.manpower)
                .unwrap_or(0)
        },
    );

    // Demographic routing — each side's casualties land on its own front
    // region (dead removed, deserters return, wounded triaged by hospitals),
    // then casualty morale impact on every class of that region.
    {
        let morale_cfg = ac.morale_config.clone();
        if let Some(region) =
            ac.regions.iter_mut().find(|r| r.id == agg_region)
        {
            let hospital_capacity = region
                .capacity_pool
                .get(&CapacityType::HospitalBeds)
                .copied()
                .unwrap_or(0.0);
            let (_treated, untreated_dead, msgs) = process_wounded(
                attacker_casualties.wounded,
                hospital_capacity,
                &agg_region,
            );
            for m in &msgs {
                eprintln!("{}", m);
            }
            let demos = &mut region.class_demographics.rural_classes;
            for m in process_dead(&attacker_casualties, demos, &agg_region) {
                eprintln!("{}", m);
            }
            for m in process_deserters(&attacker_casualties, demos, &agg_region)
            {
                eprintln!("{}", m);
            }
            for demo in demos.values_mut() {
                for m in
                    apply_casualty_morale_impact(demo, &attacker_casualties, &morale_cfg)
                        .messages
                {
                    eprintln!("{}", m);
                }
            }
            if untreated_dead > 0 {
                demos.values_mut().for_each(|d| {
                    d.economic_status =
                        crate::society::geography::EconomicStatus::Destitute;
                });
            }
        }
    }
    {
        let morale_cfg = dc.morale_config.clone();
        if let Some(region) =
            dc.regions.iter_mut().find(|r| r.id == def_region)
        {
            let hospital_capacity = region
                .capacity_pool
                .get(&CapacityType::HospitalBeds)
                .copied()
                .unwrap_or(0.0);
            let (_treated, untreated_dead, msgs) = process_wounded(
                defender_casualties.wounded,
                hospital_capacity,
                &def_region,
            );
            for m in &msgs {
                eprintln!("{}", m);
            }
            let demos = &mut region.class_demographics.rural_classes;
            for m in process_dead(&defender_casualties, demos, &def_region) {
                eprintln!("{}", m);
            }
            for m in process_deserters(&defender_casualties, demos, &def_region)
            {
                eprintln!("{}", m);
            }
            for demo in demos.values_mut() {
                for m in
                    apply_casualty_morale_impact(demo, &defender_casualties, &morale_cfg)
                        .messages
                {
                    eprintln!("{}", m);
                }
            }
            if untreated_dead > 0 {
                demos.values_mut().for_each(|d| {
                    d.economic_status =
                        crate::society::geography::EconomicStatus::Destitute;
                });
            }
        }
    }

    // POW capture — the heavier-losing side yields prisoners to the victor
    // (mirrors `resolve_front_battles`: loser = side with more casualties,
    // captor = the other belligerent, internment in the captor's region).
    let pow_config = PowCaptureConfig::default();
    let (loser_casualties, captor, origin, internment) =
        if attacker_casualties.total() > defender_casualties.total() {
            (&attacker_casualties, &mut *dc, &aggressor, &def_region)
        } else {
            (&defender_casualties, &mut *ac, &defender, &agg_region)
        };
    let captor_is_attacker = captor.name == aggressor;
    let mut pow_counter = captor.pow_camp.total_ever_captured as u64;
    let captor_name = captor.name.clone();
    let captured = capture_pows_from_casualties(
        loser_casualties,
        &captor_name,
        origin,
        current_turn,
        internment,
        &pow_config,
        &mut pow_counter,
    );
    if !captured.is_empty() {
        eprintln!(
            "[WAR] {} captures {} POWs from {}",
            captor.name,
            captured.len(),
            origin
        );
        if captor_is_attacker {
            battle.captured_by_attacker = captured.len() as i64;
        } else {
            battle.captured_by_defender = captured.len() as i64;
        }
        captor.pow_camp.add_prisoners(captured);
    }

    // Devastation — decisive results crater the BATTLE region's parcels
    // (the frontier where the fight happened — not a fixed capital
    // district); stalemates grind more slowly. Same `devastation_index`
    // the cadastre heatmap reads.
    let impact = match battle.result {
        BattleResult::AttackerVictory
        | BattleResult::PyrrhicVictory
        | BattleResult::DefenderVictory => config.devastation_per_decisive_battle,
        _ => config.devastation_per_stalemate_turn,
    };
    let mut zone_parcels: Vec<ParcelId> = dc
        .regions
        .iter()
        .find(|r| r.id == battle_region)
        .map(|r| r.parcel_ids.clone())
        .unwrap_or_default();
    zone_parcels.sort_by_key(|p| parcel_id_to_index(*p));
    zone_parcels.truncate(MAX_COMBAT_ZONE_PARCELS);
    for pid in &zone_parcels {
        if let Some(p) = dc.cadastre.get_mut(*pid) {
            p.devastation_index = (p.devastation_index + impact).min(1.0);
        }
    }

    // Front-line movement on the defender's corridor — applied to BOTH
    // front copies below so the ledgers stay byte-identical:
    //   * attacker decisive/pyrrhic win → battle region OCCUPIED, the
    //     corridor advances one contiguous step toward the capital;
    //   * defender win → the attacker's deepest occupation rolls back to
    //     Contested (counter-attack), or the assault is repelled in place;
    //   * stalemate → the battle region is Contested ground.
    let decisive_attacker = matches!(
        battle.result,
        BattleResult::AttackerVictory | BattleResult::PyrrhicVictory
    );
    let defender_win = matches!(battle.result, BattleResult::DefenderVictory);
    let capital_id = def_corridor.last().cloned();
    let capital_fell = decisive_attacker
        && capital_id.as_deref() == Some(battle_region.as_str());
    let rollback_region = if defender_win {
        dc.military_fronts
            .iter()
            .find(|f| f.id == front_id)
            .and_then(|f| last_occupied_region(f, &def_corridor))
    } else {
        None
    };

    // v4.24.0 war exhaustion — +2%–8% per DECISIVE battle scaled by each
    // side's casualty share of its engaged force; stalemates grind at
    // +0.5%–1.5%. Region loss adds +3% defender shock (+15% if the capital
    // itself falls). Scale is 0–100 points to match the dashboard.
    const EXH_DECISIVE_BASE: f64 = 2.0;
    const EXH_DECISIVE_SCALE: f64 = 6.0;
    const EXH_STALEMATE_BASE: f64 = 0.5;
    const EXH_STALEMATE_SCALE: f64 = 1.0;
    const EXH_REGION_FALLEN: f64 = 3.0;
    const EXH_CAPITAL_FALLEN: f64 = 15.0;
    /// War-exhaustion level (0–100) at which a belligerent collapses to
    /// the negotiating table.
    const SURRENDER_EXHAUSTION: f64 = 80.0;
    /// Net war score (decisive wins + held occupations, attacker-positive)
    /// that forces capitulation regardless of exhaustion.
    const CAPITULATION_SCORE: f64 = 6.0;

    let attacker_share = (attacker_casualties.total() as f64
        / (attacker_engaged.max(1) as f64))
        .clamp(0.0, 1.0);
    let defender_share = (defender_casualties.total() as f64
        / (defender_engaged.max(1) as f64))
        .clamp(0.0, 1.0);
    let decisive = decisive_attacker || defender_win;
    let (mut agg_delta, mut def_delta) = if decisive {
        (
            EXH_DECISIVE_BASE + EXH_DECISIVE_SCALE * attacker_share,
            EXH_DECISIVE_BASE + EXH_DECISIVE_SCALE * defender_share,
        )
    } else {
        (
            EXH_STALEMATE_BASE + EXH_STALEMATE_SCALE * attacker_share,
            EXH_STALEMATE_BASE + EXH_STALEMATE_SCALE * defender_share,
        )
    };
    if decisive_attacker {
        def_delta += EXH_REGION_FALLEN;
        if capital_fell {
            def_delta += EXH_CAPITAL_FALLEN;
        }
    }

    // Write combat zones, region control, exhaustion, and the battle
    // record to BOTH countries' front copies — identical values so the two
    // ledgers stay byte-identical (the dashboard dedups war fronts by id).
    for country in [&mut *ac, &mut *dc] {
        if let Some(front) =
            country.military_fronts.iter_mut().find(|f| f.id == front_id)
        {
            front.combat_zones = zone_parcels.clone();
            if decisive_attacker {
                front.update_region_control(
                    battle_region.clone(),
                    RegionControl::Occupied(aggressor.clone()),
                );
            } else if let Some(rb) = &rollback_region {
                front.update_region_control(rb.clone(), RegionControl::Contested);
            } else if !decisive_attacker {
                front.update_region_control(
                    battle_region.clone(),
                    RegionControl::Contested,
                );
            }
            front.increase_war_exhaustion(aggressor.clone(), agg_delta);
            front.increase_war_exhaustion(defender.clone(), def_delta);
            for v in front.war_exhaustion.values_mut() {
                *v = v.clamp(0.0, 100.0);
            }
            front.add_battle(battle.clone());
        }
    }

    // ── v4.24.0: War termination check ──
    // A war concludes when either belligerent's exhaustion crosses the
    // surrender threshold, the war score is a runaway, or the capital has
    // fallen. The treaty is written into `declared_wars.outcome` by the
    // caller; fronts tear down + POWs repatriate on the next bridge pass.
    let (agg_exh, def_exh, score, occupied) = dc
        .military_fronts
        .iter()
        .find(|f| f.id == front_id)
        .map(|f| {
            (
                f.war_exhaustion.get(&aggressor).copied().unwrap_or(0.0),
                f.war_exhaustion.get(&defender).copied().unwrap_or(0.0),
                front_war_score(f, &aggressor),
                occupied_corridor_regions(f, &def_corridor),
            )
        })
        .unwrap_or((0.0, 0.0, 0.0, Vec::new()));

    let collapse = agg_exh >= SURRENDER_EXHAUSTION
        || def_exh >= SURRENDER_EXHAUSTION;
    let capitulation = score.abs() >= CAPITULATION_SCORE;
    if !collapse && !capitulation && !capital_fell {
        return None;
    }

    Some(conclude_war(
        ac,
        dc,
        front_id,
        &def_corridor,
        score,
        occupied,
        capital_fell,
        current_turn,
    ))
}

/// Net war score from the ATTACKER's perspective on the shared front:
/// +1.0 per decisive attacker victory, +0.5 per pyrrhic/forced-retreat
/// win, −1.0 per defender victory, +1.0 per corridor region currently
/// occupied. Deterministic — derived purely from the battle ledger.
pub(crate) fn front_war_score(front: &Front, aggressor: &str) -> f64 {
    let mut score = 0.0;
    for b in &front.battles {
        match &b.result {
            BattleResult::AttackerVictory => score += 1.0,
            BattleResult::PyrrhicVictory => score += 0.5,
            BattleResult::DefenderVictory => score -= 1.0,
            BattleResult::Stalemate => {}
            BattleResult::Retreat { retreating_side } => {
                if retreating_side == aggressor {
                    score -= 0.5;
                } else {
                    score += 0.5;
                }
            }
        }
    }
    score += front
        .region_control
        .values()
        .filter(|c| matches!(c, RegionControl::Occupied(_)))
        .count() as f64;
    score
}

/// Corridor regions currently under occupation, in corridor order.
pub(crate) fn occupied_corridor_regions(front: &Front, corridor: &[String]) -> Vec<String> {
    corridor
        .iter()
        .filter(|rid| {
            matches!(front.region_control.get(*rid), Some(RegionControl::Occupied(_)))
        })
        .cloned()
        .collect()
}

/// Concludes a war between two in-hand belligerents: clears both
/// `at_war_with` lists, applies treaty terms (real double-entry
/// reparations), and builds the `WarOutcome` the caller records on
/// `declared_wars`. `score`/`occupied` come from the shared front;
/// `capital_fell` forces an unconditional surrender of the defender.
fn conclude_war(
    ac: &mut Country,
    dc: &mut Country,
    front_id: &str,
    corridor: &[String],
    score: f64,
    occupied: Vec<String>,
    capital_fell: bool,
    current_turn: u32,
) -> WarOutcome {
    let aggressor = ac.name.clone();
    let defender = dc.name.clone();

    // Victor by war score; a dead-even score is a negotiated white peace
    // unless the capital fell (then the defender must surrender).
    let victor: Option<String> = if capital_fell || score > 0.0 {
        Some(aggressor.clone())
    } else if score < 0.0 {
        Some(defender.clone())
    } else {
        None
    };

    // Casualty totals from the shared front ledger (both copies agree).
    let (agg_cas, def_cas) = dc
        .military_fronts
        .iter()
        .find(|f| f.id == front_id)
        .map(|f| {
            (
                f.get_country_casualties(&aggressor),
                f.get_country_casualties(&defender),
            )
        })
        .unwrap_or((0, 0));

    // Treaty terms by victor. `settle_peace`'s messaging always frames
    // defender→aggressor, so treasury motion + the outcome ledger are
    // written here with explicit direction instead.
    let reparations_for = |payer: &Country| -> f64 {
        (payer.budget.liquid_reserves * 0.05).max(0.0)
    };
    let (terms, terms_label, mut regions_ceded, mut reparations_paid) =
        match victor.as_deref() {
            Some(v) if v == aggressor => {
                let amount = reparations_for(dc);
                let regions = if capital_fell || !occupied.is_empty() {
                    occupied.clone()
                } else {
                    Vec::new()
                };
                let label = if capital_fell {
                    format!("Capitulation of {} — {} regions ceded", defender, regions.len())
                } else if !regions.is_empty() {
                    format!("Treaty of Cession — {} regions to {}", regions.len(), aggressor)
                } else {
                    format!("Indemnity Peace — {} pays reparations", defender)
                };
                (
                    if regions.is_empty() {
                        PeaceTerms::Reparations {
                            amount,
                            pow_repatriation: true,
                        }
                    } else {
                        PeaceTerms::UnconditionalSurrender {
                            regions: regions.clone(),
                            reparations: amount,
                        }
                    },
                    label,
                    regions,
                    amount,
                )
            }
            Some(v) if v == defender => {
                // Victorious defender — the AGGRESSOR pays the indemnity.
                let amount = reparations_for(ac);
                (
                    PeaceTerms::Reparations {
                        amount,
                        pow_repatriation: true,
                    },
                    format!("Defender's Peace — {} pays reparations", aggressor),
                    Vec::new(),
                    amount,
                )
            }
            _ => (
                PeaceTerms::StatusQuoAnte,
                "White Peace — status quo ante bellum".to_string(),
                Vec::new(),
                0.0,
            ),
        };

    // Real double-entry reparations transfer: loser → victor treasury.
    if reparations_paid > 0.0 {
        let (payer, payee) = if victor.as_deref() == Some(aggressor.as_str()) {
            (&mut *dc, &mut *ac)
        } else {
            (&mut *ac, &mut *dc)
        };
        let amount = reparations_paid.min(payer.budget.liquid_reserves.max(0.0));
        if amount > 0.0 {
            payer.budget.liquid_reserves -= amount;
            payee.budget.liquid_reserves += amount;
            reparations_paid = amount;
        } else {
            reparations_paid = 0.0;
        }
    }
    if regions_ceded.is_empty() {
        regions_ceded = Vec::new();
    }

    // Clear the diplomatic war flags — the war is over.
    ac.at_war_with.retain(|e| *e != defender);
    dc.at_war_with.retain(|e| *e != aggressor);

    let outcome = WarOutcome {
        victor: victor.clone(),
        terms_label: terms_label.clone(),
        regions_ceded: regions_ceded.clone(),
        reparations_paid,
        aggressor_casualties: agg_cas,
        defender_casualties: def_cas,
        // POWs repatriate when the front tears down on the next bridge
        // pass (`repatriate_pows_from_country`) — always true at peace.
        pows_repatriated: true,
    };

    let _ = terms; // Terms are summarized by `terms_label`; the enum stays
                   // available for future diplomatic-action paths.
    eprintln!(
        "[WAR] PEACE after {} battles — {} vs {} ends t{}: {} (victor: {}, score {:+.1}, rep ${:.0})",
        dc.military_fronts
            .iter()
            .find(|f| f.id == front_id)
            .map(|f| f.battles.len())
            .unwrap_or(0),
        aggressor,
        defender,
        current_turn,
        terms_label,
        victor.unwrap_or_else(|| "none".to_string()),
        score,
        reparations_paid
    );
    outcome
}

/// Writes post-battle supply/equipment state back to real units by id, then
/// distributes that side's total casualties evenly across engaged units
/// (same convention as `resolve_front_battles`).
fn writeback_battle_units(
    oob: &mut OrderOfBattle,
    battle_units: &[MilitaryUnit],
    total_casualties: i64,
) {
    let per_unit = if battle_units.is_empty() {
        0
    } else {
        total_casualties / battle_units.len() as i64
    };
    let mut by_id: HashMap<&str, &MilitaryUnit> = HashMap::new();
    for u in battle_units {
        by_id.insert(u.id.as_str(), u);
    }
    for unit in oob.all_units_mut() {
        if let Some(processed) = by_id.get(unit.id.as_str()) {
            unit.stockpile = processed.stockpile.clone();
            unit.equipment_reserves = processed.equipment_reserves.clone();
            let _ = unit.apply_casualties(per_unit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_war_front_id_canonical() {
        assert_eq!(war_front_id("B", "A"), war_front_id("A", "B"));
        assert_eq!(war_front_id("A", "B"), "WAR-A-B");
    }

    #[test]
    fn test_fnv1a_stable() {
        assert_eq!(fnv1a("Test"), fnv1a("Test"));
        assert_ne!(fnv1a("A"), fnv1a("B"));
    }
}

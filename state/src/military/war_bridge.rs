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
    settle_peace, tension_key, PeaceTerms,
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
const WAR_FRONT_PREFIX: &str = "WAR-";

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
            // Mark the live WarState ended (history rows stay, flagged).
            for ws in state.declared_wars.iter_mut().rev() {
                if ws.end_turn.is_none()
                    && canonical_pair(&ws.aggressor, &ws.defender)
                        == canonical_pair(a, b)
                {
                    ws.end_turn = Some(current_turn);
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

/// Ensures the war front exists in `country.military_fronts`, creating it
/// with the country's own mustering region and combat-zone parcels.
/// `involved` must be the canonical [aggressor, defender] ordering.
fn ensure_war_front(
    country: &mut Country,
    front_id: &str,
    front_name: &str,
    involved: &[String],
) {
    if country.military_fronts.iter().any(|f| f.id == front_id) {
        return;
    }
    // Copy what we need out of the region before mutating the front list.
    let (region_id, parcels) = {
        let Some(region) = pick_front_region(country) else {
            return; // Landless country — no front possible.
        };
        let mut parcels: Vec<ParcelId> = region.parcel_ids.clone();
        parcels.sort_by_key(|p| parcel_id_to_index(*p));
        parcels.truncate(MAX_COMBAT_ZONE_PARCELS);
        (region.id.clone(), parcels)
    };
    let mut front = Front::new(
        front_id.to_string(),
        front_name.to_string(),
        vec![region_id.clone()],
        involved.to_vec(),
    );
    front.combat_zones = parcels;
    country.military_fronts.push(front);
    eprintln!(
        "[WAR] Front '{}' materializes in {} (region {})",
        front_id, country.name, region_id
    );
}

/// The region a country fights its war front from — the most populous
/// region (largest mustering ground), ties broken by region id for
/// determinism.
fn pick_front_region(country: &Country) -> Option<&Region> {
    country.regions.iter().max_by(|a, b| {
        a.population
            .cmp(&b.population)
            .then_with(|| b.id.cmp(&a.id))
    })
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

    // Materialize the front in BOTH countries — 1 front per war pair per
    // belligerent, so military_active_wars == diplomatic_active_wars.
    for name in [aggressor, defender] {
        if let Some(country) = state.countries.get_mut(name) {
            ensure_war_front(country, &front_id, &front_name, &involved);
        }
    }

    // Split-borrow both belligerents for the battle itself.
    let mut agg = state.countries.remove(aggressor);
    let mut def = state.countries.remove(defender);
    if let (Some(ac), Some(dc)) = (agg.as_mut(), def.as_mut()) {
        resolve_war_battle(ac, dc, &front_id, current_turn);
    }
    if let Some(c) = agg {
        state.countries.insert(aggressor.to_string(), c);
    }
    if let Some(c) = def {
        state.countries.insert(defender.to_string(), c);
    }
}

/// Executes the bounded per-turn battle between aggressor (attacker) and
/// defender on the defender's front region, then writes everything back:
/// unit stockpiles/casualties, demographic routing, POW capture,
/// devastation, region control, war exhaustion, and the battle record on
/// both countries' war fronts.
fn resolve_war_battle(
    ac: &mut Country,
    dc: &mut Country,
    front_id: &str,
    current_turn: u32,
) {
    let aggressor = ac.name.clone();
    let defender = dc.name.clone();

    // Front regions: each country's own mustering region (set at front
    // creation; fall back to the most populous region for legacy saves).
    let agg_region = ac
        .military_fronts
        .iter()
        .find(|f| f.id == front_id)
        .and_then(|f| f.regions.first().cloned())
        .or_else(|| pick_front_region(ac).map(|r| r.id.clone()));
    let def_region = dc
        .military_fronts
        .iter()
        .find(|f| f.id == front_id)
        .and_then(|f| f.regions.first().cloned())
        .or_else(|| pick_front_region(dc).map(|r| r.id.clone()));
    let (Some(agg_region), Some(def_region)) = (agg_region, def_region) else {
        return;
    };

    // Draw bounded combat forces (clones — resolve_battle mutates stockpiles).
    let mut attacker_units = select_war_units(&ac.order_of_battle, &agg_region);
    let mut defender_units = select_war_units(&dc.order_of_battle, &def_region);
    if attacker_units.is_empty() || defender_units.is_empty() {
        return; // No forces committed — cold war, no battle this turn.
    }

    // Battle happens on defender soil — defender's config + terrain apply.
    let config = dc.military_config.clone();
    let terrain = dc
        .regions
        .iter()
        .find(|r| r.id == def_region)
        .map(derive_terrain)
        .unwrap_or("plains");
    let battle_id = format!("WARBATTLE-{}-{}", current_turn, front_id);

    let battle = resolve_battle(
        &mut attacker_units,
        &mut defender_units,
        def_region.clone(),
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
        def_region,
        battle.result,
        attacker_casualties.total(),
        defender_casualties.total()
    );

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
        captor.pow_camp.add_prisoners(captured);
    }

    // Devastation — decisive results crater the defender's combat-zone
    // parcels; stalemates grind more slowly. Same `devastation_index` the
    // cadastre heatmap reads.
    let impact = match battle.result {
        BattleResult::AttackerVictory
        | BattleResult::PyrrhicVictory
        | BattleResult::DefenderVictory => config.devastation_per_decisive_battle,
        _ => config.devastation_per_stalemate_turn,
    };
    if let Some(front) =
        dc.military_fronts.iter_mut().find(|f| f.id == front_id)
    {
        for pid in front.combat_zones.clone() {
            if let Some(p) = dc.cadastre.get_mut(pid) {
                p.devastation_index = (p.devastation_index + impact).min(1.0);
            }
        }
        // Attacker success contests the region outright.
        if matches!(
            battle.result,
            BattleResult::AttackerVictory | BattleResult::PyrrhicVictory
        ) {
            front.update_region_control(
                def_region.clone(),
                RegionControl::Contested,
            );
        }
    }

    // War exhaustion + battle record on BOTH countries' front copies —
    // BOTH exhaustion entries land on each copy so the two ledgers stay
    // byte-identical (the military dashboard dedups war fronts by id).
    let agg_delta = (attacker_casualties.total() as f64 / 1000.0)
        * config.war_exhaustion_per_casualty;
    let def_delta = (defender_casualties.total() as f64 / 1000.0)
        * config.war_exhaustion_per_casualty;
    for country in [&mut *ac, &mut *dc] {
        if let Some(front) =
            country.military_fronts.iter_mut().find(|f| f.id == front_id)
        {
            front.increase_war_exhaustion(aggressor.clone(), agg_delta);
            front.increase_war_exhaustion(defender.clone(), def_delta);
            front.add_battle(battle.clone());
        }
    }
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

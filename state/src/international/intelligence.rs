//! v4.25.0 WS7: Intelligence & Espionage — per-country covert operations,
//! counter-intelligence bookkeeping, and the agency lifecycle that feeds
//! the Diplomacy → Intelligence & Espionage sub-tab.
//!
//! The model rides on REAL sim state rather than a parallel fiction:
//!
//! * **Operatives** are the posted `DiplomaticPostType::Spy` VIPs recruited
//!   by [`crate::international::diplomacy::staff_diplomatic_posts`]. Every
//!   host with a posted ring gets a persistent [`CovertOperation`] whose
//!   `ring_size` tracks live postings — expelled spies shrink the ring and
//!   can compromise the operation.
//! * **Budgets** are the Phase-10 `intelligence_budget` pool funded each
//!   turn by `process_black_ops_funding` (regime-driven allocation).
//!   Sustaining a ring costs `op_cost_per_agent` per operative per turn;
//!   the fiat is double-entry conserved — the domestic pool is debited and
//!   `foreign_sector_balance` is credited (covert payments to foreign
//!   assets leave the country). Underfunded agencies run slower ops.
//! * **Counter-intelligence** is scored from the host's real
//!   `justice_state` coverage and `intelligence_state` surveillance
//!   coverage. Captures credited to the host come from the same
//!   `expel_actions` stream `process_diplomacy_turn` produces when a spy
//!   VIP is caught; hostile operations the host compromises are logged as
//!   foiled plots.
//! * **Mission outcomes** wire back into existing mechanics: Military
//!   Recon upgrades `foreign_intelligence` estimates, Industrial
//!   Espionage moves real fiat (target treasury → home treasury),
//!   Political Subversion raises `bilateral_tensions`.
//!
//! Determinism: sorted country iteration, `BTreeMap` host ordering, seeded
//! RNG only (`restore_main_rng` before any draw).

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use crate::politics::vip_registry::DiplomaticPostType;
use crate::state::GameState;

/// Mission type for a foreign covert operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum CovertMissionType {
    /// Reconnoiter the target's military — success upgrades the intel
    /// estimate on the target (fog-of-war asset path).
    #[default]
    MilitaryRecon,
    /// Steal industrial secrets — success siphons fiat from the target's
    /// treasury into the home treasury (M0-conserving transfer).
    IndustrialEspionage,
    /// Fund opposition networks — success raises bilateral friction on the
    /// pair as covert agitation surfaces.
    PoliticalSubversion,
}

impl CovertMissionType {
    /// Human-readable label for UI display.
    pub fn as_str(&self) -> &'static str {
        match self {
            CovertMissionType::MilitaryRecon => "Military Recon",
            CovertMissionType::IndustrialEspionage => "Industrial Espionage",
            CovertMissionType::PoliticalSubversion => "Political Subversion",
        }
    }
}

/// Lifecycle status of a covert operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum CovertOpStatus {
    /// Operation is running — progress accumulates each funded turn.
    #[default]
    Active,
    /// Operation completed its objective.
    Succeeded,
    /// Operation failed to achieve its objective.
    Failed,
    /// Operation was rolled up by hostile counter-intelligence.
    Compromised,
}

impl CovertOpStatus {
    /// Human-readable label for UI display.
    pub fn as_str(&self) -> &'static str {
        match self {
            CovertOpStatus::Active => "Active",
            CovertOpStatus::Succeeded => "Succeeded",
            CovertOpStatus::Failed => "Failed",
            CovertOpStatus::Compromised => "Compromised",
        }
    }
}

/// A single foreign covert operation (spy ring) run by a country's agency.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CovertOperation {
    /// Unique operation ID (e.g., "OP-000042").
    #[serde(default)]
    pub id: String,
    /// Host country the ring operates against.
    #[serde(default)]
    pub target_country: String,
    /// Mission type (recon, industrial espionage, subversion).
    #[serde(default)]
    pub mission_type: CovertMissionType,
    /// Turn the operation was launched.
    #[serde(default)]
    pub launched_turn: u32,
    /// Operatives currently assigned (live posted spies on the target).
    #[serde(default)]
    pub ring_size: u32,
    /// Mission progress (0.0–1.0); resolves at 1.0.
    #[serde(default)]
    pub progress: f64,
    /// Lifecycle status.
    #[serde(default)]
    pub status: CovertOpStatus,
    /// Turn the operation resolved (None while active).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_turn: Option<u32>,
    /// Human-readable resolution summary / latest outcome note.
    #[serde(default)]
    pub outcome: String,
}

/// Per-country intelligence agency state — covert operations abroad and
/// counter-intelligence bookkeeping at home. Stored on `GameState` keyed
/// by country name (mirrors `foreign_intelligence`'s observer→target map).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct IntelligenceAgency {
    /// Covert operations owned by this agency (active + resolved history).
    #[serde(default)]
    pub operations: Vec<CovertOperation>,
    /// Next auto-increment operation ID counter.
    #[serde(default)]
    pub next_op_id: u64,
    /// Foreign spies captured on home soil (cumulative, this run).
    #[serde(default)]
    pub spies_captured: u32,
    /// Hostile covert operations foiled on home soil (cumulative).
    #[serde(default)]
    pub plots_foiled: u32,
    /// Counter-intelligence defensive posture (0.0–1.0), refreshed each
    /// turn from justice coverage + surveillance coverage + CI funding.
    #[serde(default)]
    pub security_level: f64,
    /// Agency efficiency (0.0–1.0) — funding adequacy + physical
    /// intelligence capacity; scales mission progress and success.
    #[serde(default)]
    pub efficiency: f64,
    /// Intelligence budget actually spent on operations this turn.
    #[serde(default)]
    pub budget_spent_this_turn: f64,
    /// Bounded agency event log (newest last) — captures, compromises,
    /// successes. Powers the counter-intel "recent events" feed.
    #[serde(default)]
    pub events: Vec<String>,
}

impl IntelligenceAgency {
    /// Push an entry onto the bounded event log (oldest dropped past cap).
    pub fn log_event(&mut self, cap: usize, entry: String) {
        self.events.push(entry);
        if self.events.len() > cap {
            let excess = self.events.len() - cap;
            self.events.drain(0..excess);
        }
    }
}

/// Configuration for the intelligence agency system. No magic numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntelligenceConfig {
    /// Base mission progress per funded turn.
    pub op_base_progress: f64,
    /// Additional progress per turn per posted operative in the ring.
    pub op_progress_per_agent: f64,
    /// Fiat cost per turn per operative sustaining a ring.
    pub op_cost_per_agent: f64,
    /// Base per-turn risk a ring is compromised, scaled by host
    /// counter-intelligence posture.
    pub compromise_base_risk: f64,
    /// Fraction of target liquid reserves an industrial-espionage success
    /// siphons (capped by `industrial_espionage_cap`).
    pub industrial_espionage_share: f64,
    /// Maximum fiat an industrial-espionage success can move.
    pub industrial_espionage_cap: f64,
    /// Bilateral tension added when political subversion succeeds.
    pub subversion_tension_gain: f64,
    /// Resolved operations retained per agency (older entries pruned).
    pub resolved_history_cap: usize,
    /// Agency event-log entries retained.
    pub event_log_cap: usize,
}

impl Default for IntelligenceConfig {
    fn default() -> Self {
        Self {
            op_base_progress: 0.05,
            op_progress_per_agent: 0.06,
            op_cost_per_agent: 25_000.0,
            compromise_base_risk: 0.04,
            industrial_espionage_share: 0.0005,
            industrial_espionage_cap: 5_000_000.0,
            subversion_tension_gain: 8.0,
            resolved_history_cap: 24,
            event_log_cap: 16,
        }
    }
}

/// Per-turn counter-intelligence + progress processing for every country's
/// intelligence agency.
///
/// Runs at GameState scope after the diplomatic-action drain so this
/// turn's staffing and expulsions are already materialized.
///
/// # Arguments
/// * `state` — mutable game state (agencies, budgets, tensions, intel).
/// * `diplomacy` — bilateral relations matrix (immutable).
/// * `expel_actions` — `(spy_home, host)` pairs caught this turn by
///   `process_diplomacy_turn`; each is a real capture credited to the
///   host's counter-intelligence ledger.
/// * `market` — global market; funded operation budgets are credited to
///   `foreign_sector_balance` (covert payments abroad) so the M0 walk
///   stays conserved.
/// * `current_turn` — global turn for timestamps.
pub fn process_intelligence_agencies(
    state: &mut GameState,
    diplomacy: &HashMap<String, HashMap<String, crate::international::DiplomaticRelation>>,
    expel_actions: &[(String, String)],
    market: &mut crate::economy::market::GlobalMarket,
    current_turn: u32,
) {
    // Restore the main-thread RNG before drawing — same discipline as
    // process_diplomacy_turn (rayon scheduling leaves the stream
    // position-dependent otherwise).
    crate::engine::seed_propagation::restore_main_rng();
    let mut rng = crate::engine::seeded_rng::thread_rng();

    let config = state.intelligence_config.clone();
    let mut names: Vec<String> = state.countries.keys().cloned().collect();
    names.sort();

    // Pre-compute every country's defensive posture once — justice /
    // security coverage plus surveillance, plus a counter-intel funding
    // floor (a funded domestic agency runs a baseline watch even with
    // weak coverage). Computed upfront so the per-country mutable pass
    // never re-borrows `intelligence_agencies` or `countries`.
    let mut defense_posture: BTreeMap<String, f64> = BTreeMap::new();
    for name in &names {
        let Some(c) = state.countries.get(name) else {
            continue;
        };
        let justice = c
            .politics
            .justice_state
            .as_ref()
            .map(|js| js.justice_coverage.max(js.security_coverage))
            .unwrap_or_else(|| {
                c.budget
                    .extra
                    .get("justice_coverage")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(50.0)
                    / 100.0
            });
        let surveillance = c
            .politics
            .intelligence_state
            .as_ref()
            .map(|s| s.surveillance_coverage)
            .unwrap_or(0.0);
        let ci_floor = if c.intelligence_budget.current_budget > 0.0 {
            0.1
        } else {
            0.0
        };
        defense_posture.insert(
            name.clone(),
            (0.6 * justice + 0.3 * surveillance + ci_floor).clamp(0.0, 1.0),
        );
    }

    // ── A) Counter-intelligence captures — credit the HOST agency for
    // each foreign spy expelled this turn, and roll up the spy ring's
    // operation (a captured network compromises its mission).
    for (spy_home, host) in expel_actions {
        {
            let host_agency = state.intelligence_agencies.entry(host.clone()).or_default();
            host_agency.spies_captured += 1;
            host_agency.plots_foiled += 1;
            host_agency.log_event(
                config.event_log_cap,
                format!("T{current_turn}: {spy_home} agent captured — network rolled up"),
            );
        }
        if let Some(home_agency) = state.intelligence_agencies.get_mut(spy_home) {
            if let Some(op) = home_agency.operations.iter_mut().find(|o| {
                o.status == CovertOpStatus::Active && o.target_country == *host
            }) {
                op.status = CovertOpStatus::Compromised;
                op.resolved_turn = Some(current_turn);
                op.outcome = format!("Ring rolled up — agents captured (T{current_turn})");
            }
            home_agency.log_event(
                config.event_log_cap,
                format!("T{current_turn}: operation in {host} compromised — agents captured"),
            );
        }
    }

    // Deferred cross-country effects — collected while each home agency is
    // mutably borrowed, applied after the borrow is released.
    let mut intel_bumps: Vec<(String, String, crate::international::fog_of_war::IntelLevel)> =
        Vec::new();
    let mut treasury_moves: Vec<(String, String, f64)> = Vec::new(); // (from, to, amount)
    let mut tension_gains: Vec<(String, String, f64)> = Vec::new(); // (a, b, gain)
    let mut foil_credits: Vec<(String, String)> = Vec::new(); // (host, event text)

    // ── B) Per-country agency pass ──
    for home_name in &names {
        // Immutable snapshot of the home country's field network and
        // intelligence infrastructure.
        let (spies_by_host, intel_capacity, budget_available) = {
            let Some(home) = state.countries.get(home_name) else {
                continue;
            };
            let mut hosts: BTreeMap<String, u32> = BTreeMap::new();
            if let Some(reg) = &home.politics.vip_registry {
                for vip in reg.vips.values() {
                    if vip.is_dead {
                        continue;
                    }
                    if let Some(post) = &vip.diplomatic_post {
                        if post.post_type == DiplomaticPostType::Spy {
                            *hosts.entry(post.host_country.clone()).or_insert(0) += 1;
                        }
                    }
                }
            }
            let capacity = home
                .politics
                .intelligence_state
                .as_ref()
                .map(|s| s.total_capacity)
                .unwrap_or(0.0);
            (
                hosts,
                capacity,
                home.intelligence_budget.current_budget.max(0.0),
            )
        };

        let active_agents: u32 = spies_by_host.values().sum();
        let agency = state
            .intelligence_agencies
            .entry(home_name.clone())
            .or_default();

        // Defensive posture from the shared pre-pass.
        agency.security_level = defense_posture.get(home_name).copied().unwrap_or(0.0);

        // ── Ring bookkeeping: one active operation per host with posted
        // spies; mission type rotates deterministically across resolved
        // operations on that target (recon first — reconnaissance is the
        // standing tasking of a resident ring).
        let host_list: Vec<(String, u32)> =
            spies_by_host.iter().map(|(h, n)| (h.clone(), *n)).collect();
        for (target, n) in &host_list {
            let has_active = agency
                .operations
                .iter()
                .any(|o| o.status == CovertOpStatus::Active && &o.target_country == target);
            if !has_active {
                const ROTATION: [CovertMissionType; 3] = [
                    CovertMissionType::MilitaryRecon,
                    CovertMissionType::IndustrialEspionage,
                    CovertMissionType::PoliticalSubversion,
                ];
                let resolved_vs_target = agency
                    .operations
                    .iter()
                    .filter(|o| {
                        &o.target_country == target && o.status != CovertOpStatus::Active
                    })
                    .count();
                agency.next_op_id += 1;
                agency.operations.push(CovertOperation {
                    id: format!("OP-{:06}", agency.next_op_id),
                    target_country: target.clone(),
                    mission_type: ROTATION[resolved_vs_target % ROTATION.len()].clone(),
                    launched_turn: current_turn,
                    ring_size: *n,
                    progress: 0.0,
                    status: CovertOpStatus::Active,
                    resolved_turn: None,
                    outcome: String::new(),
                });
            }
        }

        // Ops whose rings vanished without a capture report (recalls,
        // deaths) go dark rather than being marked compromised.
        for op in agency.operations.iter_mut() {
            if op.status == CovertOpStatus::Active {
                match spies_by_host.get(&op.target_country) {
                    Some(&n) => op.ring_size = n,
                    None => {
                        op.status = CovertOpStatus::Failed;
                        op.resolved_turn = Some(current_turn);
                        op.outcome =
                            "Ring went dark — operatives lost contact".to_string();
                    }
                }
            }
        }

        // ── Funding: sustain every active ring from this turn's
        // intelligence budget. The spend leaves the domestic pool for
        // foreign handlers (foreign_sector_balance) — strict double-entry.
        let ops_cost: f64 = agency
            .operations
            .iter()
            .filter(|o| o.status == CovertOpStatus::Active)
            .map(|o| o.ring_size as f64 * config.op_cost_per_agent)
            .sum();
        let funded = ops_cost.min(budget_available);
        let funding_ratio = if ops_cost > 0.0 { funded / ops_cost } else { 1.0 };
        if funded > 0.0 {
            if let Some(home) = state.countries.get_mut(home_name) {
                home.intelligence_budget.current_budget -= funded;
                home.intelligence_budget.spent += funded;
            }
            market.foreign_sector_balance += funded;
        }
        agency.budget_spent_this_turn = funded;

        // Agency efficiency: funding adequacy (can the service pay its
        // rings?) plus physical intelligence capacity scaled to the field
        // corps (HQ capacity per operative).
        let funding_score = if ops_cost > 0.0 { funding_ratio } else { 0.5 };
        let capacity_score = if active_agents > 0 {
            (intel_capacity / (active_agents as f64 * 10.0)).min(1.0)
        } else {
            0.0
        };
        agency.efficiency = (0.3 + 0.4 * funding_score + 0.3 * capacity_score).clamp(0.0, 1.0);
        let efficiency = agency.efficiency;

        // ── Advance active operations ──
        for op in agency.operations.iter_mut() {
            if op.status != CovertOpStatus::Active {
                continue;
            }

            // Hostile counter-intel may roll up the ring at any time —
            // risk scales with the target's defensive posture.
            let host_defense = defense_posture
                .get(&op.target_country)
                .copied()
                .unwrap_or(0.5);
            let compromise_risk =
                config.compromise_base_risk * (0.5 + host_defense) * (1.2 - efficiency);
            if rng.gen::<f64>() < compromise_risk {
                op.status = CovertOpStatus::Compromised;
                op.resolved_turn = Some(current_turn);
                op.outcome = format!(
                    "Compromised by {} counter-intelligence (T{current_turn})",
                    op.target_country
                );
                foil_credits.push((
                    op.target_country.clone(),
                    format!(
                        "T{current_turn}: {home_name} {} operation foiled",
                        op.mission_type.as_str()
                    ),
                ));
                continue;
            }

            op.progress = (op.progress
                + (config.op_base_progress
                    + config.op_progress_per_agent * op.ring_size as f64)
                    * funding_ratio)
                .min(1.0);
            if op.progress < 1.0 {
                continue;
            }

            // Resolve: success scales with ring strength, agency
            // efficiency, and the target's defensive posture.
            let success_chance =
                (0.45 + 0.10 * op.ring_size as f64 + 0.20 * efficiency
                    - 0.35 * host_defense)
                    .clamp(0.05, 0.95);
            op.resolved_turn = Some(current_turn);
            if rng.gen::<f64>() < success_chance {
                op.status = CovertOpStatus::Succeeded;
                match op.mission_type {
                    CovertMissionType::MilitaryRecon => {
                        let cur = state
                            .foreign_intelligence
                            .get(home_name)
                            .and_then(|m| m.get(&op.target_country))
                            .map(|i| i.intel_level)
                            .unwrap_or_default();
                        let new_level = cur.upgrade().unwrap_or(
                            crate::international::fog_of_war::IntelLevel::Exact,
                        );
                        intel_bumps.push((
                            home_name.clone(),
                            op.target_country.clone(),
                            new_level,
                        ));
                        op.outcome = format!(
                            "Reconnaissance complete — {} intel upgraded to {}",
                            op.target_country,
                            new_level.as_str()
                        );
                    }
                    CovertMissionType::IndustrialEspionage => {
                        let loot = (state
                            .countries
                            .get(&op.target_country)
                            .map(|c| {
                                c.budget.liquid_reserves
                                    * config.industrial_espionage_share
                            })
                            .unwrap_or(0.0))
                        .min(config.industrial_espionage_cap)
                        .max(0.0);
                        if loot > 0.0 {
                            treasury_moves.push((
                                op.target_country.clone(),
                                home_name.clone(),
                                loot,
                            ));
                        }
                        op.outcome =
                            format!("Industrial secrets exfiltrated (≈{loot:.0} value)");
                    }
                    CovertMissionType::PoliticalSubversion => {
                        tension_gains.push((
                            home_name.clone(),
                            op.target_country.clone(),
                            config.subversion_tension_gain,
                        ));
                        op.outcome =
                            "Subversion network activated — unrest agitation funded"
                                .to_string();
                    }
                }
            } else {
                op.status = CovertOpStatus::Failed;
                op.outcome = "Mission failed — objective not achieved".to_string();
                // A blown mission is often detected even when it isn't
                // fully rolled up — the host's counter-intel logs a
                // foiled plot.
                foil_credits.push((
                    op.target_country.clone(),
                    format!(
                        "T{current_turn}: {home_name} {} operation foiled",
                        op.mission_type.as_str()
                    ),
                ));
            }
        }

        // Prune resolved history past the cap (oldest first; Vec order is
        // append order).
        let resolved_count = agency
            .operations
            .iter()
            .filter(|o| o.status != CovertOpStatus::Active)
            .count();
        if resolved_count > config.resolved_history_cap {
            let mut to_drop = resolved_count - config.resolved_history_cap;
            agency.operations.retain(|o| {
                if to_drop > 0 && o.status != CovertOpStatus::Active {
                    to_drop -= 1;
                    false
                } else {
                    true
                }
            });
        }
    }

    // ── C) Apply deferred cross-country effects ──

    // Successful recon upgrades the observer's intel estimate through the
    // same asset path spy activity uses (real fog-of-war writeback).
    for (observer, target, new_level) in intel_bumps {
        let Some(target_country) = state.countries.get(&target) else {
            continue;
        };
        let true_gdp = target_country.budget.gdp;
        let true_military = target_country.order_of_battle.unit_count() as u32;
        let true_treasury = target_country.budget.liquid_reserves;
        let true_population = target_country.budget.population as f64;
        let intel = state
            .foreign_intelligence
            .entry(observer)
            .or_default()
            .entry(target)
            .or_insert_with(crate::international::fog_of_war::ForeignIntelligence::unknown);
        crate::engine::seed_propagation::restore_main_rng();
        let mut rng = crate::engine::seeded_rng::thread_rng();
        intel.update_from_true_values(
            true_gdp,
            true_military,
            true_treasury,
            true_population,
            new_level,
            new_level,
            current_turn,
            &mut rng,
        );
    }

    // Industrial espionage: real fiat moves target → home (M0-conserving).
    for (from, to, amount) in treasury_moves {
        let mut moved = 0.0;
        if let Some(src) = state.countries.get_mut(&from) {
            moved = amount.min(src.budget.liquid_reserves.max(0.0));
            src.budget.liquid_reserves -= moved;
        }
        if moved > 0.0 {
            if let Some(dst) = state.countries.get_mut(&to) {
                dst.budget.liquid_reserves += moved;
            }
        }
    }

    // Subversion: covert agitation surfaces as bilateral friction on the
    // real tension ledger (feeds the existing war-escalation pipeline).
    for (a, b, gain) in tension_gains {
        let key = crate::military::war_declarations::tension_key(&a, &b);
        let tension = state.bilateral_tensions.entry(key).or_default();
        tension.level = (tension.level + gain).min(100.0);
        tension.total_ever_accumulated += gain;
        tension.provocation_count += 1;
    }

    // Foiled hostile operations credit the defender's counter-intel ledger.
    for (host, event) in foil_credits {
        let host_agency = state.intelligence_agencies.entry(host).or_default();
        host_agency.plots_foiled += 1;
        host_agency.log_event(config.event_log_cap, event);
    }
}

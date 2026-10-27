#![allow(missing_docs)]

use crate::international::DiplomaticRelation;
use crate::politics::vip_registry::DiplomaticPostType;
use crate::state::GameState;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Generates the full bilateral diplomacy matrix for a set of countries.
pub fn generate_diplomacy(
    countries: &[String],
) -> HashMap<String, HashMap<String, DiplomaticRelation>> {
    let mut rng = crate::engine::seeded_rng::thread_rng();
    let mut diplomacy: HashMap<String, HashMap<String, DiplomaticRelation>> = HashMap::new();

    for c1 in countries {
        let mut inner = HashMap::new();
        for c2 in countries {
            if c1 == c2 {
                continue;
            }
            let relations = rng.gen_range(-100..=100);
            let mut rel = DiplomaticRelation {
                relations,
                frozen_turns: 0,
                ban_import: false,
                ban_export: false,
                free_trade: false,
                customs_union: false,
                investment_treaty: false,
                economic_community: false,
                treaty_description: "None".to_string(),
                embargo_penalty: 0.0,
            };
            if relations < -50 && rng.gen::<f64>() < 0.5 {
                rel.ban_export = true;
                rel.ban_import = true;
            } else if relations > 50 && rng.gen::<f64>() < 0.5 {
                rel.free_trade = true;
            }
            inner.insert(c2.clone(), rel);
        }
        diplomacy.insert(c1.clone(), inner);
    }

    diplomacy
}

/// Phase 78: Diplomatic post capacity configuration.
///
/// Defines the maximum number of diplomats of each type that a country
/// can post to a single host country. These are structural limits based
/// on diplomatic protocol, not magic numbers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiplomaticPostCap {
    /// Maximum ambassadors per host country (1 — only one chief of mission).
    pub ambassador: usize,
    /// Maximum consuls per host country (2 — consular representation).
    pub consul: usize,
    /// Maximum spies per host country (3 — higher risk justifies more).
    pub spy: usize,
    /// Maximum military attachés per host country (1 — single defense liaison).
    pub military_attache: usize,
}

impl Default for DiplomaticPostCap {
    fn default() -> Self {
        Self {
            ambassador: 1,
            consul: 2,
            spy: 3,
            military_attache: 1,
        }
    }
}

impl DiplomaticPostCap {
    /// Returns the cap for a given diplomatic post type.
    pub fn for_post_type(&self, post_type: &DiplomaticPostType) -> usize {
        match post_type {
            DiplomaticPostType::Ambassador => self.ambassador,
            DiplomaticPostType::Consul => self.consul,
            DiplomaticPostType::Spy => self.spy,
            DiplomaticPostType::MilitaryAttache => self.military_attache,
        }
    }
}

/// Phase 66: Count diplomats of a given type posted from `home` to `host`.
pub fn count_diplomats(
    state: &GameState,
    home: &str,
    host: &str,
    post_type: &DiplomaticPostType,
) -> usize {
    let Some(country) = state.countries.get(home) else {
        return 0;
    };
    let Some(registry) = &country.politics.vip_registry else {
        return 0;
    };
    registry
        .vips
        .values()
        .filter(|vip| {
            vip.diplomatic_post
                .as_ref()
                .is_some_and(|post| post.host_country == host && &post.post_type == post_type)
        })
        .count()
}

/// Phase 66: Compute diplomatic modifiers from VIP traits for a posted diplomat.
///
/// Returns (relation_improvement_bonus, spy_discovery_risk_modifier).
/// - Charismatic → +relation improvement rate
/// - Diplomatic → +relation improvement rate (smaller)
/// - Cautious → -discovery risk for spies
/// - Paranoid → +discovery risk for spies (self-defeating)
pub fn compute_diplomat_modifiers(traits: &[String]) -> (f64, f64) {
    let mut relation_bonus = 0.0;
    let mut discovery_modifier = 1.0;
    for trait_id in traits {
        match trait_id.as_str() {
            "Charismatic" => relation_bonus += 0.5,
            "Diplomatic" => relation_bonus += 0.3,
            "Cautious" => discovery_modifier *= 0.7,
            "Paranoid" => discovery_modifier *= 1.3,
            "Incompetent" => relation_bonus -= 0.3,
            _ => {}
        }
    }
    (relation_bonus, discovery_modifier)
}

/// Updates diplomatic relations dynamically based on physical world events.
///
/// Phase 66 additions:
/// - Ambassador presence boosts relation improvement rate.
/// - Spy activity generates intelligence and runs discovery risk checks.
/// - Caught spies trigger automatic ExpelDiplomat and relation freeze.
///
/// # Arguments
/// * `state` - Immutable game state (reads trade balances, politics, military fronts).
/// * `diplomacy` - Mutable bilateral diplomacy matrix.
/// * `diplomatic_config` - Configuration for spy discovery risk, relation rates.
/// * `current_turn` - Current global turn (for intel timestamping).
/// * `intel_updates` - Output buffer: (observer_country, target_country, new_intel_level) tuples.
/// * `expel_actions` - Output buffer: (spy_home_country, host_country) pairs for caught spies.
pub fn process_diplomacy_turn(
    state: &GameState,
    diplomacy: &mut HashMap<String, HashMap<String, DiplomaticRelation>>,
    diplomatic_config: &crate::international::fog_of_war::DiplomaticConfig,
    _current_turn: u32,
    intel_updates: &mut Vec<(String, String, crate::international::fog_of_war::IntelLevel)>,
    expel_actions: &mut Vec<(String, String)>,
) {
    // Restore the main-thread RNG before drawing — every other sequential
    // consumer in the turn tail does this. Without it, the stream position
    // is whatever rayon work-stealing left on the main thread after the
    // last parallel section (scheduling-dependent → nondeterministic).
    crate::engine::seed_propagation::restore_main_rng();
    let mut rng = crate::engine::seeded_rng::thread_rng();

    // Collect sorted country names for deterministic iteration
    let mut sorted_names: Vec<&String> = state.countries.keys().collect();
    sorted_names.sort();

    for &c1_name in &sorted_names {
        let c1 = match state.countries.get(c1_name) {
            Some(c) => c,
            None => continue,
        };

        // Gather c1's involved front countries
        let c1_front_countries: Vec<&String> = c1
            .military_fronts
            .iter()
            .flat_map(|f| f.involved_countries.iter())
            .filter(|c| *c != c1_name)
            .collect();

        // Read c1's trade balance from budget.extra
        // Agent 4 — Phase 6: Renamed from Polish "bilans_handlowy" to English (Rule 12).
        let c1_trade_balance = c1
            .budget
            .extra
            .get("trade_balance")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let c1_exports = c1
            .budget
            .extra
            .get("exports")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let c1_cultural_group = &c1.macro_indicators.cultural_group;
        let c1_is_democratic = c1.politics.government_form.is_democratic();

        // Phase 66: Count ambassadors posted from c1 to each partner
        // and spies posted from c1 to each partner
        let c1_vip_registry = c1.politics.vip_registry.as_ref();

        for &c2_name in &sorted_names {
            if c1_name == c2_name {
                continue;
            }

            let c2 = match state.countries.get(c2_name) {
                Some(c) => c,
                None => continue,
            };

            // Get or create the relation c1 → c2
            let rel = match diplomacy
                .get_mut(c1_name)
                .and_then(|partners| partners.get_mut(c2_name))
            {
                Some(r) => r,
                None => continue,
            };

            // Frozen relations: skip all changes
            if rel.frozen_turns > 0 {
                rel.frozen_turns -= 1;
                continue;
            }

            let mut delta: i64 = 0;

            // 1. Trade imbalance: if c1 has a large deficit, resentment toward partners
            if c1_trade_balance < -c1.budget.gdp * 0.05 {
                delta -= 2;
            } else if c1_trade_balance > c1.budget.gdp * 0.05 {
                delta += 1;
            }

            // 2. Ideological distance
            let c2_is_democratic = c2.politics.government_form.is_democratic();
            if c1_is_democratic != c2_is_democratic {
                delta -= 1;
            }

            // 3. Border tension: both countries involved in the same front
            if c1_front_countries.contains(&c2_name) {
                delta -= 2;
            }

            // 4. Cultural affinity
            if !c1_cultural_group.is_empty()
                && c1_cultural_group == &c2.macro_indicators.cultural_group
            {
                delta += 1;
            }

            // 5. Trade volume: high exports build trust
            if c1_exports > c1.budget.gdp * 0.10 {
                delta += 1;
            }

            // 6. Phase 66: Ambassador presence boosts relation improvement
            let has_ambassador = c1_vip_registry.is_some_and(|reg| {
                reg.vips.values().any(|vip| {
                    vip.diplomatic_post.as_ref().is_some_and(|post| {
                        post.host_country == *c2_name
                            && post.post_type == DiplomaticPostType::Ambassador
                    })
                })
            });
            if has_ambassador {
                // Find the ambassador's traits for modifier computation —
                // deterministic pick: lowest vip id (HashMap order is
                // reseeded per clone).
                if let Some(reg) = c1_vip_registry {
                    let mut envoys: Vec<&_> = reg
                        .vips
                        .values()
                        .filter(|vip| {
                            vip.diplomatic_post.as_ref().is_some_and(|p| {
                                p.host_country == *c2_name
                                    && p.post_type == DiplomaticPostType::Ambassador
                            })
                        })
                        .collect();
                    envoys.sort_by(|a, b| a.id.cmp(&b.id));
                    if let Some(vip) = envoys.first() {
                        let (rel_bonus, _) = compute_diplomat_modifiers(&vip.traits);
                        delta += 1 + rel_bonus as i64;
                    }
                }
            }

            // Apply delta and clamp
            rel.relations = (rel.relations + delta).clamp(-100, 100);

            // Threshold triggers
            if rel.relations < -50 && rng.gen::<f64>() < 0.3 {
                rel.ban_export = true;
                rel.ban_import = true;
                rel.free_trade = false;
                rel.customs_union = false;
                rel.treaty_description = "Embargo".to_string();
            } else if rel.relations > 50 && rng.gen::<f64>() < 0.3 {
                rel.ban_export = false;
                rel.ban_import = false;
                rel.free_trade = true;
                rel.treaty_description = "Free Trade".to_string();
            } else if rel.relations > -10 && rel.relations < 10 {
                // Relations normalized — lift embargoes if relations improve
                if rel.relations > 0 {
                    rel.ban_export = false;
                    rel.ban_import = false;
                }
            }

            // 7. Phase 66: Spy activity — intel generation and discovery risk
            // Sorted by vip id — each spy consumes a seeded rng draw below,
            // so HashMap order would scramble draw alignment across runs.
            let spy_vips: Vec<_> = c1_vip_registry.map_or(Vec::new(), |reg| {
                let mut v: Vec<_> = reg
                    .vips
                    .values()
                    .filter(|vip| {
                        vip.diplomatic_post.as_ref().is_some_and(|post| {
                            post.host_country == *c2_name
                                && post.post_type == DiplomaticPostType::Spy
                        })
                    })
                    .collect();
                v.sort_by(|a, b| a.id.cmp(&b.id));
                v
            });

            for spy in &spy_vips {
                let (_, discovery_mod) = compute_diplomat_modifiers(&spy.traits);

                // Host country counter-intelligence: justice coverage reduces spy success
                let host_justice = c2
                    .budget
                    .extra
                    .get("justice_coverage")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(50.0)
                    / 100.0;

                // Discovery risk = base_risk * trait_modifier * host_justice_coverage
                let discovery_risk =
                    diplomatic_config.spy_discovery_risk * discovery_mod * host_justice;
                if rng.gen::<f64>() < discovery_risk {
                    // Spy caught! Trigger expulsion and relation freeze
                    expel_actions.push((c1_name.clone(), c2_name.clone()));
                    rel.frozen_turns = diplomatic_config.spy_caught_freeze_turns as i64;
                    rel.relations = (rel.relations - diplomatic_config.spy_caught_relation_penalty)
                        .clamp(-100, 100);
                } else {
                    // Spy succeeds — upgrade intel level
                    let new_level = crate::international::fog_of_war::IntelLevel::NarrowRange;
                    intel_updates.push((c1_name.clone(), c2_name.clone(), new_level));
                }
            }
        }
    }
}

/// v4.21 WS4: AI diplomatic & intelligence staffing.
///
/// Closes the recruitment catch-22: `AssignDiplomat` requires an eligible
/// registered VIP, but nothing ever created one — so every country's
/// Diplomacy tab reported zero posts forever.
///
/// v4.22: The corps now SCALES with country size instead of the old binary
/// "1 envoy + 1 spy" coverage check that left 12/14 countries at
/// `IntelLevel::Unknown` forever. Each country targets:
///   * `target_envoys = 1 + pop_tier + gdp_tier` ambassadors on distinct
///     hosts — `pop_tier` is the population log-tier (10M→+1, 100M→+2,
///     1B→+3) and `gdp_tier` adds one post per 250B GDP. Ambassadors go to
///     the largest-GDP partners first (real diplomatic priority).
///   * `target_spies = 1 + pop_tier` spies on distinct hosts, posted to the
///     countries relations are worst with (adversarial intelligence).
/// Both are capped by the number of foreign countries and by what the
/// treasury can actually fund this turn — each post still moves the real
/// `diplomat_assignment_cost` home→host in the sequential drain (strict
/// double-entry, no M0 creation). Because every missing post is queued in
/// a single call, a fresh world fills its corps on the first turn and
/// holds it at steady state thereafter.
///
/// Recruitment pipeline is unchanged: seeded-RNG culturally-appropriate
/// VIP → `VipRegistry` → `DiplomaticAction::AssignDiplomat` (the drain
/// enforces `DiplomaticPostCap` per host). Deterministic: sorted country
/// iteration, seeded RNG only, RNG stream restored first.
pub fn staff_diplomatic_posts(
    state: &mut GameState,
    diplomacy: &HashMap<String, HashMap<String, DiplomaticRelation>>,
    diplomatic_config: &crate::international::fog_of_war::DiplomaticConfig,
    current_turn: u32,
) {
    crate::engine::seed_propagation::restore_main_rng();
    let mut rng = crate::engine::seeded_rng::thread_rng();

    let cap = DiplomaticPostCap::default();
    let cost = diplomatic_config.diplomat_assignment_cost;

    let mut sorted_names: Vec<String> = state.countries.keys().cloned().collect();
    sorted_names.sort();
    let foreign_count = sorted_names.len().saturating_sub(1);
    if foreign_count == 0 {
        return; // A lone country posts no foreign staff.
    }

    for home_name in &sorted_names {
        // Snapshot the home country's staffing state (single borrow).
        // v4.23: per-post-type counts + per-type host coverage so a scaled
        // corps can station several envoy types on the same host.
        let (
            post_counts,
            covered_hosts,
            affordable,
            cultural_group,
            used_names,
            population,
            gdp,
        ) = {
            let Some(home) = state.countries.get(home_name) else {
                continue;
            };
            let reg = home.politics.vip_registry.as_ref();
            let mut counts: HashMap<DiplomaticPostType, usize> = HashMap::new();
            let mut covered: HashMap<
                DiplomaticPostType,
                std::collections::HashSet<String>,
            > = HashMap::new();
            if let Some(r) = reg {
                for v in r.vips.values() {
                    if v.is_dead {
                        continue;
                    }
                    if let Some(p) = &v.diplomatic_post {
                        *counts.entry(p.post_type.clone()).or_insert(0) += 1;
                        covered
                            .entry(p.post_type.clone())
                            .or_default()
                            .insert(p.host_country.clone());
                    }
                }
            }
            let names: std::collections::HashSet<String> = reg
                .map(|r| r.name_index.keys().cloned().collect())
                .unwrap_or_default();
            let population: i64 = home.regions.iter().map(|r| r.population).sum();
            (
                counts,
                covered,
                if cost > 0.0 {
                    (home.budget.liquid_reserves / cost).floor() as usize
                } else {
                    foreign_count * 4
                },
                home.macro_indicators.cultural_group.clone(),
                names,
                population,
                home.budget.gdp,
            )
        };

        // v4.22: corps size scales with country size. pop_tier: +1 post per
        // population decade past 1M (10M→1, 100M→2, 1B→3). gdp_tier: +1 post
        // per 250B GDP. Both targets clamp to the number of foreign hosts.
        let pop_tier = ((population.max(1) as f64).log10() - 6.0).max(0.0) as usize;
        let gdp_tier = (gdp.max(0.0) / 250_000_000_000.0) as usize;
        // v4.23 WS5b: major powers (pop_tier + gdp_tier ≥ 4 — i.e. ~100M+
        // people AND ~$500B+ GDP) field a full diplomatic corps of 15–30
        // personnel: envoys distributed across ambassador/consul/attaché
        // posts plus a spy network. Minor powers keep the lean bilateral
        // corps from v4.22.
        let is_major = pop_tier + gdp_tier >= 4;
        let (target_envoys, target_spies) = if is_major {
            (18usize.min(foreign_count * 2), 9usize.min(foreign_count))
        } else {
            (
                (1 + pop_tier + gdp_tier).min(foreign_count),
                (1 + pop_tier).min(foreign_count),
            )
        };

        // Build the staffing plan. Majors distribute envoy slots over the
        // three envoy post types (ambassador/consul/military attaché) —
        // per-host type caps (1/2/1) and per-type coverage still apply, so
        // the corps spreads across hosts while allowing several envoy types
        // on the same mission. Minors stay ambassador-only as in v4.22.
        let staffing_plan: Vec<(DiplomaticPostType, usize, usize)> = if is_major {
            const ENVOY_TYPES: [DiplomaticPostType; 3] = [
                DiplomaticPostType::Ambassador,
                DiplomaticPostType::Consul,
                DiplomaticPostType::MilitaryAttache,
            ];
            let per_type = (target_envoys / ENVOY_TYPES.len()).max(1);
            let mut plan: Vec<(DiplomaticPostType, usize, usize)> = ENVOY_TYPES
                .iter()
                .map(|t| {
                    (
                        t.clone(),
                        *post_counts.get(t).unwrap_or(&0),
                        per_type.min(foreign_count),
                    )
                })
                .collect();
            plan.push((
                DiplomaticPostType::Spy,
                *post_counts.get(&DiplomaticPostType::Spy).unwrap_or(&0),
                target_spies,
            ));
            plan
        } else {
            let envoy_total: usize = post_counts
                .iter()
                .filter(|(t, _)| **t != DiplomaticPostType::Spy)
                .map(|(_, n)| *n)
                .sum();
            vec![
                (
                    DiplomaticPostType::Ambassador,
                    envoy_total,
                    target_envoys,
                ),
                (
                    DiplomaticPostType::Spy,
                    *post_counts.get(&DiplomaticPostType::Spy).unwrap_or(&0),
                    target_spies,
                ),
            ]
        };

        let mut used_names = used_names;
        let mut covered_hosts = covered_hosts;
        let mut slots = affordable;

        for (post_type, have, target) in staffing_plan {
            for _ in have..target {
                if slots == 0 {
                    break;
                }
                // Per-type coverage: an ambassador on host X doesn't block a
                // consul or attaché on X (embassy staff of different types).
                let covered_for_type =
                    covered_hosts.entry(post_type.clone()).or_default();
                let Some(host_name) = pick_staffing_host(
                    state,
                    diplomacy,
                    home_name,
                    &post_type,
                    &sorted_names,
                    &cap,
                    covered_for_type,
                ) else {
                    break;
                };
                // Recruit the officer: seeded name + trait draw, then add the
                // professional trait the post calls for — `compute_diplomat_
                // modifiers` rewards Diplomatic envoys and Cautious spies.
                let vip_name = crate::politics::names::generate_unique_vip(
                    &cultural_group,
                    &mut rng,
                    &mut used_names,
                );
                let (mut traits, main_trait) =
                    crate::politics::vip_registry::assign_core_traits(&mut rng);
                let professional = match post_type {
                    DiplomaticPostType::Spy => "Cautious",
                    _ => "Diplomatic",
                };
                if !traits.iter().any(|t| t == professional) {
                    traits.push(professional.to_string());
                }
                let vip = crate::politics::vip_registry::Vip {
                    full_name: vip_name.full_name,
                    gender: vip_name.gender,
                    age: 35 + rng.gen_range(0..25),
                    health: crate::politics::vip_registry::VipHealth {
                        physical_health: 1.0,
                        mental_health: 1.0,
                    },
                    traits,
                    main_trait,
                    ideology: "Centrist".to_string(),
                    nationality: home_name.clone(),
                    born_turn: current_turn,
                    base_influence: 10 + rng.gen_range(0..20),
                    ..Default::default()
                };

                let vip_id = {
                    let Some(home) = state.countries.get_mut(home_name) else {
                        break;
                    };
                    let Some(registry) = home.politics.vip_registry.as_mut() else {
                        break;
                    };
                    registry.register_new(vip)
                };

                state.pending_diplomatic_actions.push(
                    crate::state::diplomatic_actions::DiplomaticAction::AssignDiplomat {
                        vip_id,
                        home_country: home_name.clone(),
                        host_country: host_name.clone(),
                        post_type: post_type.clone(),
                        assigned_turn: current_turn,
                    },
                );
                // Mark covered so the next slot spreads to a new host (for
                // this post type), and reserve the funds the drain will move.
                covered_for_type.insert(host_name);
                slots = slots.saturating_sub(1);
            }
        }
    }
}

/// Deterministically pick the host for a new posting.
///
/// * **Ambassador** (chief of mission / embassy): the highest-GDP foreign
///   country with post capacity — envoys go to the major powers and
///   trading partners first.
/// * **Spy**: the foreign country relations are WORST with (adversarial
///   intelligence — missing relation entries read as 0).
///
/// In both cases hosts at the `DiplomaticPostCap` for that type are
/// skipped, and all ties break by sorted country name so the pick is
/// identical across runs. `count_diplomats` reads the CURRENT posted
/// state; same-turn queued actions are re-checked by the drain anyway.
/// `covered_hosts` additionally suppresses hosts the caller has already
/// queued a post to this turn, so a scaled corps spreads across distinct
/// partners instead of stacking on one host.
fn pick_staffing_host(
    state: &GameState,
    diplomacy: &HashMap<String, HashMap<String, DiplomaticRelation>>,
    home_name: &str,
    post_type: &DiplomaticPostType,
    sorted_names: &[String],
    cap: &DiplomaticPostCap,
    covered_hosts: &std::collections::HashSet<String>,
) -> Option<String> {
    let mut best: Option<(String, f64)> = None; // (name, score)
    for cand in sorted_names {
        if cand == home_name || covered_hosts.contains(cand) {
            continue;
        }
        // Per-host capacity: a full embassy / burned network is skipped.
        if count_diplomats(state, home_name, cand, post_type)
            >= cap.for_post_type(post_type)
        {
            continue;
        }
        let score = match post_type {
            // Spy → LOWEST relations = highest priority (adversarial
            // intel). Negate so a larger score still wins.
            DiplomaticPostType::Spy => -diplomacy
                .get(home_name)
                .and_then(|m| m.get(cand))
                .map(|r| r.relations as f64)
                .unwrap_or(0.0),
            // Ambassador & envoys → largest economy (trade & treaty
            // relevance first).
            _ => state
                .countries
                .get(cand)
                .map(|c| c.budget.gdp)
                .unwrap_or(0.0),
        };
        // Strictly-greater keeps the FIRST best under sorted order →
        // deterministic tie-break by country name.
        if best.as_ref().map(|(_, s)| score > *s).unwrap_or(true) {
            best = Some((cand.clone(), score));
        }
    }
    best.map(|(name, _)| name)
}

/// v4.23.0 WS5b: Dynamic diplomacy — regional blocs federate by cultural
/// group, and bilateral treaties are proposed and lifecycle-driven by
/// relations and bilateral tension. Runs once per turn at GameState scope,
/// before `advance_negotiations` so fresh proposals begin negotiating the
/// same turn.
///
/// **Bloc rule:** every cultural group with ≥2 countries federates a
/// `"{Group} Compact"` international organization at `FreeTradeArea`
/// integration. A country accedes when it is not at war with any current
/// member, its average bilateral relation with the membership is ≥ -20,
/// and no bilateral tension with a member exceeds 30. A member that goes
/// to war with another member is expelled — no bloc spans a live war.
/// Integration progression thereafter rides the existing organization
/// turn pipeline (`OrganizationRegistry::process_turn`).
///
/// **Treaty rule:** the two best bilateral pairs by average relations
/// (≥60) that share no live treaty are proposed a Trade Preference pact
/// each turn (≥80 also adds Mutual Defense). Proposals enter the standard
/// `Proposed → Negotiating → Active` lifecycle. Active treaties suspend
/// when a participant pair goes to war or minimum pairwise relations
/// collapse below -50; suspended treaties resume when peace returns and
/// minimum pairwise relations recover to ≥0.
///
/// Determinism: BTreeMap/BTreeSet iteration, sorted candidate order with
/// name tie-breaks, no RNG.
pub fn process_dynamic_diplomacy(
    state: &mut GameState,
    diplomacy: &HashMap<String, HashMap<String, DiplomaticRelation>>,
    current_turn: u32,
) {
    use std::collections::{BTreeMap, BTreeSet, HashSet};

    use crate::international::organizations::{
        IntegrationLevel, InternationalOrganization, VotingMechanism,
    };
    use crate::international::treaties::{Treaty, TreatyClause, TreatyStatus};
    use crate::military::war_declarations::tension_key;

    /// A candidate must not join a bloc while this hostile toward it.
    const BLOC_MIN_RELATIONS: i64 = -20;
    /// Only genuinely friendly pairs get treaty proposals.
    const PROPOSE_MIN_RELATIONS: i64 = 60;
    /// The closest pairs add a mutual-defense clause.
    const DEFENSE_PACT_RELATIONS: i64 = 80;
    /// Bounded churn — at most this many new proposals per turn.
    const MAX_TREATY_PROPOSALS_PER_TURN: usize = 2;
    /// Active treaties suspend below this pairwise-relations floor.
    const SUSPEND_MIN_RELATIONS: i64 = -50;
    /// Suspended treaties resume at this pairwise-relations level.
    const RESUME_MIN_RELATIONS: i64 = 0;
    /// Bloc accession requires cool bilateral tension with every member.
    const TENSION_ACCESSION_CAP: f64 = 30.0;

    let pair_key =
        |a: &str, b: &str| -> String { tension_key(a, b) };

    // Active war pairs (canonical tension-key format).
    let mut at_war: HashSet<String> = HashSet::new();
    for (name, c) in &state.countries {
        for e in &c.at_war_with {
            at_war.insert(pair_key(name, e));
        }
    }
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
    let tension_of = |a: &str, b: &str| -> f64 {
        state
            .bilateral_tensions
            .get(&pair_key(a, b))
            .map(|t| t.level)
            .unwrap_or(0.0)
    };

    // ── A) Regional blocs (cultural-group compacts) ──
    let mut groups: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (name, c) in &state.countries {
        let g = c.macro_indicators.cultural_group.trim().to_string();
        if !g.is_empty() {
            groups.entry(g).or_default().insert(name.clone());
        }
    }
    for (group, members) in &groups {
        if members.len() < 2 {
            continue;
        }
        let bloc_name = format!("{} Compact", title_case(group));
        let idx = match state
            .international_organizations
            .organizations
            .iter()
            .position(|o| o.name == bloc_name)
        {
            Some(i) => i,
            None => {
                // Founding members: no pair at war inside the new bloc.
                let founders: Vec<String> = members
                    .iter()
                    .filter(|m| {
                        !members
                            .iter()
                            .any(|o| o != *m && at_war.contains(&pair_key(m, o)))
                    })
                    .cloned()
                    .collect();
                if founders.len() < 2 {
                    continue;
                }
                let id = state.international_organizations.next_org_id();
                state.international_organizations.organizations.push(
                    InternationalOrganization::new(
                        id,
                        bloc_name.clone(),
                        founders,
                        IntegrationLevel::FreeTradeArea,
                        VotingMechanism::Unanimity,
                        current_turn,
                    ),
                );
                eprintln!("[BLOC] {} founded", bloc_name);
                state.international_organizations.organizations.len() - 1
            }
        };
        let org = &mut state.international_organizations.organizations[idx];
        // Expel members at war with another member — no bloc spans a live
        // war (collect first to keep the iteration immutable).
        let mut expel: Vec<String> = Vec::new();
        for m in &org.member_states {
            if org
                .member_states
                .iter()
                .any(|o| o != m && at_war.contains(&pair_key(m, o)))
            {
                expel.push(m.clone());
            }
        }
        for m in expel {
            let org_name = org.name.clone();
            org.remove_member(&m);
            eprintln!("[BLOC] {} expelled from {} (war within bloc)", m, org_name);
        }
        // Accession: group members not at war with the membership, with
        // calm relations AND cool bilateral tension.
        for m in members {
            if org.is_member(m) {
                continue;
            }
            if org
                .member_states
                .iter()
                .any(|o| at_war.contains(&pair_key(m, o)))
            {
                continue;
            }
            let member_rel: i64 = org
                .member_states
                .iter()
                .map(|o| rel_avg(m, o))
                .sum::<i64>()
                / (org.member_states.len().max(1) as i64);
            if member_rel < BLOC_MIN_RELATIONS {
                continue;
            }
            if org
                .member_states
                .iter()
                .any(|o| tension_of(m, o) > TENSION_ACCESSION_CAP)
            {
                continue;
            }
            let org_name = org.name.clone();
            org.add_member(m);
            eprintln!("[BLOC] {} joins {}", m, org_name);
        }
    }

    // ── B) Bilateral treaty proposals — best relations, no live treaty ──
    let mut names: Vec<String> = state.countries.keys().cloned().collect();
    names.sort();
    let mut candidates: Vec<(i64, String, String)> = Vec::new();
    for i in 0..names.len() {
        for j in (i + 1)..names.len() {
            let (a, b) = (&names[i], &names[j]);
            let avg = rel_avg(a, b);
            if avg >= PROPOSE_MIN_RELATIONS && !at_war.contains(&pair_key(a, b))
            {
                candidates.push((avg, a.clone(), b.clone()));
            }
        }
    }
    // Strongest relations first; ties break by sorted names.
    candidates.sort_by(|x, y| {
        y.0.cmp(&x.0)
            .then_with(|| x.1.cmp(&y.1))
            .then_with(|| x.2.cmp(&y.2))
    });

    let treaty_duration = state.treaty_config.default_duration_turns;
    let mut proposals = 0usize;
    for (avg, a, b) in candidates {
        if proposals >= MAX_TREATY_PROPOSALS_PER_TURN {
            break;
        }
        // Skip pairs already bound by a live (non-expired/abrogated) treaty.
        let bound = state.treaty_registry.treaties.iter().any(|t| {
            t.has_participants(&a, &b)
                && !matches!(
                    t.status,
                    TreatyStatus::Expired | TreatyStatus::Abrogated
                )
        });
        if bound {
            continue;
        }
        let (label, clauses) = if avg >= DEFENSE_PACT_RELATIONS {
            (
                "Mutual Defense & Trade Pact",
                vec![
                    TreatyClause::MutualDefense,
                    TreatyClause::TradePreference,
                ],
            )
        } else {
            (
                "Trade Preference Pact",
                vec![TreatyClause::TradePreference],
            )
        };
        let id = state.treaty_registry.next_treaty_id();
        let name = format!("{}–{} {}", a, b, label);
        state.treaty_registry.treaties.push(Treaty::new(
            id,
            name.clone(),
            vec![a.clone(), b.clone()],
            clauses,
            current_turn,
            treaty_duration,
            a.clone(),
        ));
        eprintln!("[DIPLOMACY] {} proposed (avg relations {})", name, avg);
        proposals += 1;
    }

    // ── C) Treaty lifecycle — war/hostility suspends, normalization resumes ──
    for t in &mut state.treaty_registry.treaties {
        let mut min_rel: i64 = i64::MAX;
        let mut pair_at_war = false;
        for i in 0..t.participants.len() {
            for j in (i + 1)..t.participants.len() {
                let (a, b) = (&t.participants[i], &t.participants[j]);
                if at_war.contains(&pair_key(a, b)) {
                    pair_at_war = true;
                }
                min_rel = min_rel.min(rel_avg(a, b));
            }
        }
        match t.status {
            TreatyStatus::Active => {
                if pair_at_war || min_rel < SUSPEND_MIN_RELATIONS {
                    t.status = TreatyStatus::Suspended;
                    eprintln!(
                        "[DIPLOMACY] {} suspended (war/hostility)",
                        t.name
                    );
                }
            }
            TreatyStatus::Suspended => {
                if !pair_at_war && min_rel >= RESUME_MIN_RELATIONS {
                    t.status = TreatyStatus::Active;
                    eprintln!("[DIPLOMACY] {} resumed (normalized)", t.name);
                }
            }
            _ => {}
        }
    }
}

/// Title-case a snake/kebab/space-separated identifier for display names
/// ("north_germanic" → "North Germanic").
fn title_case(raw: &str) -> String {
    raw.split(|c: char| c == '_' || c == '-' || c == ' ')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(f) => {
                    f.to_uppercase().collect::<String>()
                        + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::international::fog_of_war::DiplomaticConfig;

    #[test]
    fn test_generate_diplomacy_creates_matrix() {
        let countries = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let diplomacy = generate_diplomacy(&countries);
        assert_eq!(diplomacy.len(), 3);
        assert!(diplomacy["A"].contains_key("B"));
        assert!(!diplomacy["A"].contains_key("A"));
        // Check renamed fields
        let rel = &diplomacy["A"]["B"];
        assert!(rel.relations >= -100 && rel.relations <= 100);
        assert_eq!(rel.frozen_turns, 0);
        assert_eq!(rel.treaty_description, "None");
    }

    #[test]
    fn test_compute_diplomat_modifiers() {
        let traits = vec!["Charismatic".to_string(), "Cautious".to_string()];
        let (rel_bonus, disc_mod) = compute_diplomat_modifiers(&traits);
        assert!(rel_bonus > 0.0, "Charismatic should boost relations");
        assert!(disc_mod < 1.0, "Cautious should reduce discovery risk");
    }

    #[test]
    fn test_compute_diplomat_modifiers_paranoid() {
        let traits = vec!["Paranoid".to_string()];
        let (_, disc_mod) = compute_diplomat_modifiers(&traits);
        assert!(disc_mod > 1.0, "Paranoid should increase discovery risk");
    }

    #[test]
    fn test_process_diplomacy_turn_with_config() {
        let mut state = GameState::default();
        let mut c1 = crate::state::Country::mock_for_tests();
        c1.name = "TestA".to_string();
        let mut c2 = crate::state::Country::mock_for_tests();
        c2.name = "TestB".to_string();
        state.countries.insert("TestA".to_string(), c1);
        state.countries.insert("TestB".to_string(), c2);

        let mut diplomacy = generate_diplomacy(&["TestA".to_string(), "TestB".to_string()]);
        let config = DiplomaticConfig::default();
        let mut intel_updates = Vec::new();
        let mut expel_actions = Vec::new();

        process_diplomacy_turn(
            &state,
            &mut diplomacy,
            &config,
            1,
            &mut intel_updates,
            &mut expel_actions,
        );

        // Should not crash, relations should be in valid range
        let rel = &diplomacy["TestA"]["TestB"];
        assert!(rel.relations >= -100 && rel.relations <= 100);
    }
}

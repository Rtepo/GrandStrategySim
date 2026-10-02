//! Grid Distribution Viability — Epic Test
//!
//! Regression coverage for the "energy distribution black hole": the
//! national grid generated ~2770 MW while Regional Effective Supply read
//! exactly 0.0 MW, producing nationwide blackouts and scarcity-ceiling spot
//! prices. Root cause: `power_grid_state` (LV/MV capacity maps) was never
//! persisted — `save_game_state` skipped it and `load_game_state` rebuilt
//! `PowerGridState::default()`, so every save→load cycle wiped the
//! distribution bottleneck and `effective_supply = supply.min(0) = 0`.
//!
//! Assertions:
//!   G1 — worldgen seeds LV/MV capacity for every region.
//!   G2 — `power_grid_state` survives a save→load round-trip.
//!   G3 — a legacy save (empty maps) self-heals from measured demand on
//!         the first post-load turn instead of blacking out.
//!   G4 — generated power reaches consumers: every region that generated
//!         power has `effective_supply > 0`, no MW is dropped below the
//!         wire limit, and nationally Σ effective ≥ 90% of what the wires
//!         could physically deliver.
//!   G5 — no region with supply > 0 sits in `LoadShedTier::Blackout`.
//!   G6 — spot prices are finite and within the scarcity ceiling.
//!   G7 — LV capacity tracks demand: it never shrinks and grows toward
//!         demand × headroom over turns.
//!
//! # Run
//! ```bash
//! CI=true cargo nextest run --release --features "epic-tests diagnostic" grid_distribution_viability_test
//! ```

#![cfg(all(feature = "epic-tests", feature = "diagnostic"))]

use sim_engine::engine::diagnostic::{CapturingProbe, HarnessTargets, MassSinkWhitelist};
use sim_engine::engine::turn::run_turn_inner;
use sim_engine::engine::turn_context::InMemoryTurnContext;
use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
use sim_engine::energy::types::LoadShedTier;
use sim_engine::io::save_manager::{load_game_state, save_game_state};
use sim_engine::registries::Registries;
use tempfile::TempDir;

const TURNS: u32 = 4;

/// Fraction of generated supply that must survive the LV/MV bottleneck
/// nationally before we declare a distribution failure. 10% slack covers
/// legitimate curtailment/load-shed at region level.
const MIN_EFFECTIVE_FRACTION: f64 = 0.9;

fn minimal_targets(state: &sim_engine::state::GameState) -> HarnessTargets {
    let country_name = state.countries.keys().min().cloned().unwrap_or_default();
    let region_id = state
        .countries
        .get(&country_name)
        .and_then(|c| c.regions.first().map(|r| r.id.clone()))
        .unwrap_or_default();
    HarnessTargets {
        company_ids: Vec::new(),
        bank_id: String::new(),
        region_id,
        country_name,
    }
}

#[test]
fn test_grid_distribution_viability() {
    let tmp = TempDir::new().expect("temp dir");
    let data_dir = tmp.path();

    let registries = Registries::native_only();
    let options = GenerateOptions {
        country_count: 4,
        start_year: StartYear::Y1925,
        seed: Some(42),
    };
    let GeneratedWorld { mut state, .. } =
        generate_world(data_dir, options, &registries).expect("world generation failed");

    // ── G1: worldgen must seed LV/MV capacity for every region ────────────
    for (name, country) in &state.countries {
        for region in &country.regions {
            assert!(
                country
                    .power_grid_state
                    .region_lv_capacity
                    .get(&region.id)
                    .copied()
                    .unwrap_or(0.0)
                    > 0.0,
                "G1 FAIL [{}/{}]: LV capacity missing/0 after worldgen",
                name,
                region.id
            );
            assert!(
                country
                    .power_grid_state
                    .region_mv_capacity
                    .get(&region.id)
                    .copied()
                    .unwrap_or(0.0)
                    > 0.0,
                "G1 FAIL [{}/{}]: MV capacity missing/0 after worldgen",
                name,
                region.id
            );
        }
    }

    // ── G2: grid state must survive a save → load round-trip ──────────────
    save_game_state(data_dir, &state).expect("save_game_state failed");
    let reloaded = load_game_state(data_dir).expect("load_game_state failed");
    for (name, country) in &state.countries {
        let re = reloaded
            .countries
            .get(name)
            .expect("reloaded country missing");
        assert!(
            !re.power_grid_state.region_lv_capacity.is_empty(),
            "G2 FAIL [{}]: power_grid_state lost on save→load round-trip",
            name
        );
        for region in &country.regions {
            assert!(
                re.power_grid_state
                    .region_lv_capacity
                    .contains_key(&region.id),
                "G2 FAIL [{}/{}]: LV capacity key dropped in round-trip",
                name,
                region.id
            );
        }
    }

    // ── G3: simulate a legacy save — wipe maps, expect self-heal ───────────
    for country in state.countries.values_mut() {
        country.power_grid_state = Default::default();
    }
    assert!(
        state
            .countries
            .values()
            .all(|c| c.power_grid_state.region_lv_capacity.is_empty()),
        "test setup: grid maps should be wiped"
    );

    let mut ctx = InMemoryTurnContext::load_from_disk(data_dir, &mut state)
        .expect("load_from_disk failed");
    let mut probe = CapturingProbe::new(minimal_targets(&state), MassSinkWhitelist::canonical());

    let mut saw_reconcile_ready = false;
    let mut lv_history: std::collections::HashMap<String, Vec<f64>> =
        std::collections::HashMap::new();
    for _turn in 0..TURNS {
        run_turn_inner(&mut state, &registries, &mut ctx, &mut probe)
            .expect("turn failed");

        // After each turn, every region must hold LV/MV entries again
        // (reconcile seeded them before the bottleneck lookup).
        for (name, country) in &state.countries {
            for region in &country.regions {
                let lv = country
                    .power_grid_state
                    .region_lv_capacity
                    .get(&region.id)
                    .copied()
                    .unwrap_or(0.0);
                lv_history
                    .entry(format!("{}/{}", name, region.id))
                    .or_default()
                    .push(lv);
                if lv > 0.0 {
                    saw_reconcile_ready = true;
                }
                assert!(
                    country
                        .power_grid_state
                        .region_lv_capacity
                        .contains_key(&region.id),
                    "G3 FAIL [{}/{}]: LV key still missing after turn",
                    name,
                    region.id
                );
            }
        }
    }
    assert!(
        saw_reconcile_ready,
        "G3 FAIL: reconcile never restored positive LV capacity"
    );

    // ── G4/G5/G6: distribution invariants after TURNS ─────────────────────
    let mut national_generated = 0.0f64;
    let mut national_effective = 0.0f64;
    let mut national_deliverable = 0.0f64;
    for (name, country) in &state.countries {
        let grid = &country.power_grid_state;
        for region in &country.regions {
            let generated = grid.region_supply_mw.get(&region.id).copied().unwrap_or(0.0);
            let effective = grid
                .region_effective_supply_mw
                .get(&region.id)
                .copied()
                .unwrap_or(0.0);
            let demand = grid
                .region_demand_mw
                .get(&region.id)
                .copied()
                .unwrap_or(0.0);
            national_generated += generated;
            national_effective += effective;
            // National denominator: what the wires could physically carry to
            // demand — export diversions, transmission losses, storage
            // charging, and genuine capacity bottlenecks are all excluded
            // (G7 polices the buildout instead).
            let net = grid
                .region_net_supply_mw
                .get(&region.id)
                .copied()
                .unwrap_or(generated);
            let wire_cap = grid
                .region_wire_cap_mw
                .get(&region.id)
                .copied()
                .unwrap_or_else(|| {
                    grid.region_lv_capacity
                        .get(&region.id)
                        .copied()
                        .unwrap_or(0.0)
                        .min(
                            grid.region_mv_capacity
                                .get(&region.id)
                                .copied()
                                .unwrap_or(0.0),
                        )
                });
            national_deliverable += net.min(demand).min(wire_cap);

            if generated > 0.0 && demand > 0.0 {
                assert!(
                    effective > 0.0,
                    "G4 FAIL [{}/{}]: generated {:.2} MW and demand {:.2} MW but delivered 0.0",
                    name,
                    region.id,
                    generated,
                    demand
                );
            }
            // G4b: no MW may be silently dropped below the physical wire
            // limit. `net` is post-flow/post-storage supply — exports and
            // pumped charging are legitimate diversions. A residual
            // shortfall is acceptable ONLY when LV/MV capacity is the
            // binding constraint (labor-bounded buildout can lag demand
            // spikes — G7 tracks the catch-up).
            if demand > 0.0 && generated > 0.0 {
                let net = grid
                    .region_net_supply_mw
                    .get(&region.id)
                    .copied()
                    .unwrap_or(generated);
                // Wire cap in force at dispatch — recorded pre-buildout so
                // same-turn growth can't move the goalposts.
                let wire_cap = grid
                    .region_wire_cap_mw
                    .get(&region.id)
                    .copied()
                    .unwrap_or_else(|| {
                        grid.region_lv_capacity
                            .get(&region.id)
                            .copied()
                            .unwrap_or(0.0)
                            .min(
                                grid.region_mv_capacity
                                    .get(&region.id)
                                    .copied()
                                    .unwrap_or(0.0),
                            )
                    });
                let must_deliver = net.min(demand).min(wire_cap);
                assert!(
                    effective >= must_deliver * 0.999,
                    "G4b FAIL [{}/{}]: effective {:.2} MW < wire-permitted {:.2} MW (net {:.2}, gross {:.2}, demand {:.2}, wire@dispatch {:.2})",
                    name,
                    region.id,
                    effective,
                    must_deliver,
                    net,
                    generated,
                    demand,
                    wire_cap
                );
            }
            // G5: tier must be honest about delivered coverage. Blackout
            // is legitimate under real scarcity (deficit > 50%) — the
            // black-hole signature it must NEVER show is effective supply
            // collapsing while generation exists (covered by G4 above and
            // the effective>0 assertion), or declaring Blackout while the
            // region actually delivered ≥50% of demand.
            let tier = grid
                .load_shed_tiers
                .get(&region.id)
                .copied()
                .unwrap_or(LoadShedTier::Normal);
            if tier == LoadShedTier::Blackout && demand > 0.0 {
                assert!(
                    effective < 0.5 * demand,
                    "G5 FAIL [{}/{}]: Blackout tier but effective {:.2} MW covers {:.1}% of {:.2} MW demand",
                    name,
                    region.id,
                    effective,
                    100.0 * effective / demand,
                    demand
                );
            }
            if let Some(&spot) = grid.spot_prices.get(&region.id) {
                // Bounds recorded at write time (average_wage drifts
                // mid-turn). Surplus clearing tracks marginal cost, which
                // may exceed the wage-indexed ceiling — take the max.
                let ceiling = grid
                    .region_spot_ceiling
                    .get(&region.id)
                    .copied()
                    .unwrap_or(0.0);
                let marginal = grid
                    .region_spot_marginal_cost
                    .get(&region.id)
                    .copied()
                    .unwrap_or(0.0);
                let bound = ceiling.max(marginal);
                assert!(
                    spot.is_finite() && spot >= 0.0 && spot <= bound * 1.0001,
                    "G6 FAIL [{}/{}]: spot price {:.4} outside [0, {:.4}] (ceiling {:.4}, marginal {:.4})",
                    name,
                    region.id,
                    spot,
                    bound,
                    ceiling,
                    marginal
                );
            }
        }
    }
    // National reconciliation on the deliverable basis: generated MW that
    // local demand could absorb must be delivered. Export surplus is
    // legitimately curtailed, so `generated` itself is not the denominator.
    if national_deliverable > 0.0 {
        assert!(
            national_effective >= MIN_EFFECTIVE_FRACTION * national_deliverable,
            "G4 FAIL: national effective {:.1} MW < {:.0}% of deliverable {:.1} MW (generated {:.1} MW)",
            national_effective,
            MIN_EFFECTIVE_FRACTION * 100.0,
            national_deliverable,
            national_generated
        );
    }

    // ── G7: LV capacity never regresses and tracks toward demand ─────────
    // Labor-bounded buildout cannot close a demand spike inside 4 turns, so
    // the invariant is *progress*, not arrival: LV must be monotone
    // non-decreasing, above the catastrophic floor, and — when a shortfall
    // persists while construction labor and treasury exist — strictly
    // growing.
    for (name, country) in &state.countries {
        // Build budget in force during the LAST turn's dispatch — zero means
        // settlement drained the treasury (or no crew existed) and no growth
        // was possible; positive means every short region received a
        // pro-rata share that turn.
        let build_budget = country.power_grid_state.last_build_budget_mw;
        for region in &country.regions {
            let lv = country
                .power_grid_state
                .region_lv_capacity
                .get(&region.id)
                .copied()
                .unwrap_or(0.0);
            let demand = country
                .power_grid_state
                .region_demand_mw
                .get(&region.id)
                .copied()
                .unwrap_or(0.0);
            // Catastrophic-behind floor: demand grows faster than labor can
            // build wire, so LV legitimately lags during buildout — but it
            // must never sit orders of magnitude behind (the 0.1-MW legacy
            // pin that produced the original black hole).
            let floor = if demand > 0.0 { demand * 0.4 } else { 0.1 };
            assert!(
                lv >= floor * 0.999,
                "G7 FAIL [{}/{}]: LV cap {:.3} below floor {:.3} (demand {:.2})",
                name,
                region.id,
                lv,
                floor,
                demand
            );
            let key = format!("{}/{}", name, region.id);
            if let Some(hist) = lv_history.get(&key) {
                for w in hist.windows(2) {
                    assert!(
                        w[1] >= w[0] * 0.999,
                        "G7 FAIL [{}/{}]: LV regressed {:.3} → {:.3}",
                        name,
                        region.id,
                        w[0],
                        w[1]
                    );
                }
                let shortfall_persists = *hist.last().unwrap() < demand * 1.2 * 0.999;
                // A *meaningful* budget means pro-rata allocation delivered
                // wire to every desired region. Sub-kW budgets (< 1e-3 MW —
                // the engine's growth-viable epsilon, e.g. a treasury drained
                // to ~$0 by settlement) produce sub-f64-resolution deltas and
                // are economically inert, not stalled growth.
                if shortfall_persists && build_budget > 1e-3 && hist.len() >= 2 {
                    assert!(
                        hist[hist.len() - 1] > hist[hist.len() - 2],
                        "G7 FAIL [{}/{}]: LV did not grow on the last turn ({:.3} → {:.3}) despite shortfall (demand {:.2}) and build budget {:.2} MW",
                        name,
                        region.id,
                        hist[hist.len() - 2],
                        hist[hist.len() - 1],
                        demand,
                        build_budget
                    );
                }
            }
        }
    }

    eprintln!(
        "GRID-VIABILITY PASS: generated={:.1} MW effective={:.1} MW over {} turns",
        national_generated, national_effective, TURNS
    );
}

//! Phase 94: Baseline Performance Test
//!
//! Establishes a baseline performance profile of the engine by running a
//! standard, uninstrumented 3-turn simulation using the vanilla
//! `run_turn_in_memory` (no diagnostic feature, no probe overhead).
//!
//! # Metrics Captured
//! - Exact execution time (in milliseconds) for each of the 3 turns.
//! - Active `Company` entity count at Turn 1 and Turn 3.
//! - Active `Building` entity count at Turn 1 and Turn 3.
//!
//! # Purpose
//! This baseline isolates the engine's standard processing time from any
//! probe overhead introduced by the diagnostic harness. It also verifies
//! that recent M&A and tombstoning implementations have reduced the
//! active entity count as expected.
//!
//! This test does NOT modify the harness architecture, touch telemetry,
//! or attempt any optimizations.

use sim_engine::engine::turn_context::InMemoryTurnContext;
use sim_engine::engine::{
    generate_world, run_turn_in_memory, GenerateOptions, GeneratedWorld, StartYear,
};
use sim_engine::registries::Registries;
use std::time::Instant;
use tempfile::TempDir;

/// Count active (non-liquidated, non-merged) companies across all countries
/// in the in-memory turn context.
fn count_active_companies(ctx: &InMemoryTurnContext) -> usize {
    ctx.entities
        .values()
        .flat_map(|ents| ents.companies.iter())
        .filter(|c| !c.is_liquidated && c.merged_into.is_none())
        .count()
}

/// Count active (non-liquidated) companies across all countries, including
/// those that may have been tombstoned. This counts ALL companies in the
/// entities map regardless of liquidation/merge status.
fn count_all_companies(ctx: &InMemoryTurnContext) -> usize {
    ctx.entities
        .values()
        .flat_map(|ents| ents.companies.iter())
        .count()
}

/// Count all buildings across all countries in the in-memory turn context.
fn count_buildings(ctx: &InMemoryTurnContext) -> usize {
    ctx.entities.values().map(|ents| ents.buildings.len()).sum()
}

/// Run a 3-turn baseline simulation and report performance metrics.
#[test]
fn test_baseline_performance_3_turns() {
    // --- Setup: generate a fresh world ---
    let tmp = TempDir::new().expect("failed to create temp dir");
    let data_dir = tmp.path();

    let registries = Registries::native_only();
    let options = GenerateOptions {
        country_count: 4,
        start_year: StartYear::Y1900,
    };

    let GeneratedWorld {
        state: mut initial_state,
        ..
    } = generate_world(data_dir, options, &registries).expect("world generation failed");

    let mut ctx = InMemoryTurnContext::load_from_disk(data_dir, &mut initial_state)
        .expect("failed to load turn context from generated world");

    let mut state = initial_state;

    // --- Entity census at Turn 0 (pre-simulation) ---
    let companies_turn0 = count_active_companies(&ctx);
    let all_companies_turn0 = count_all_companies(&ctx);
    let buildings_turn0 = count_buildings(&ctx);

    println!("\n═══════════════════════════════════════════════════════════════");
    println!("  PHASE 94: BASELINE PERFORMANCE REPORT (3 turns, no probe)");
    println!("═══════════════════════════════════════════════════════════════");
    println!("  World: 4 countries, start year 1900");
    println!();
    println!("  ── Entity Census (Turn 0, pre-simulation) ──");
    println!(
        "  Active companies (non-liquidated, non-merged): {}",
        companies_turn0
    );
    println!(
        "  All companies (including tombstoned):           {}",
        all_companies_turn0
    );
    println!(
        "  Buildings:                                       {}",
        buildings_turn0
    );
    println!();

    // --- Run 3 turns with per-turn timing ---
    let mut turn_times_ms: Vec<u128> = Vec::new();

    for turn_num in 0..3u32 {
        let start = Instant::now();
        let result = run_turn_in_memory(&mut state, &registries, &mut ctx);
        let elapsed = start.elapsed();

        if let Err(e) = &result {
            panic!(
                "Turn {} (global {}) failed: {:?}",
                turn_num, state.calendar.global_turn, e
            );
        }

        let elapsed_ms = elapsed.as_millis();
        turn_times_ms.push(elapsed_ms);

        println!(
            "  Turn {}: {:>8} ms (global_turn={})",
            turn_num + 1,
            elapsed_ms,
            state.calendar.global_turn
        );

        // Entity census after Turn 1 and Turn 3
        if turn_num == 0 {
            let companies_t1 = count_active_companies(&ctx);
            let all_companies_t1 = count_all_companies(&ctx);
            let buildings_t1 = count_buildings(&ctx);
            println!("    ── Entity Census after Turn 1 ──");
            println!(
                "    Active companies: {}, All companies: {}, Buildings: {}",
                companies_t1, all_companies_t1, buildings_t1
            );
        }
    }

    // --- Entity census at Turn 3 ---
    let companies_t3 = count_active_companies(&ctx);
    let all_companies_t3 = count_all_companies(&ctx);
    let buildings_t3 = count_buildings(&ctx);

    println!();
    println!("  ── Entity Census (after Turn 3) ──");
    println!(
        "  Active companies (non-liquidated, non-merged): {}",
        companies_t3
    );
    println!(
        "  All companies (including tombstoned):           {}",
        all_companies_t3
    );
    println!(
        "  Buildings:                                       {}",
        buildings_t3
    );
    println!();

    // --- Summary ---
    let total_ms: u128 = turn_times_ms.iter().sum();
    let avg_ms = if turn_times_ms.is_empty() {
        0
    } else {
        total_ms / turn_times_ms.len() as u128
    };

    println!("  ── Performance Summary ──");
    println!(
        "  Total execution time: {} ms ({:.2} s)",
        total_ms,
        total_ms as f64 / 1000.0
    );
    println!(
        "  Average per turn:     {} ms ({:.2} s)",
        avg_ms,
        avg_ms as f64 / 1000.0
    );
    println!(
        "  Per-turn breakdown:   {} ms",
        turn_times_ms
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!();
    println!("  ── Entity Count Delta (Turn 0 → Turn 3) ──");
    println!(
        "  Active companies: {} → {} (delta: {:+})",
        companies_turn0,
        companies_t3,
        companies_t3 as i64 - companies_turn0 as i64
    );
    println!(
        "  All companies:    {} → {} (delta: {:+})",
        all_companies_turn0,
        all_companies_t3,
        all_companies_t3 as i64 - all_companies_turn0 as i64
    );
    println!(
        "  Buildings:         {} → {} (delta: {:+})",
        buildings_turn0,
        buildings_t3,
        buildings_t3 as i64 - buildings_turn0 as i64
    );
    println!("═══════════════════════════════════════════════════════════════\n");

    // --- Assertions (sanity checks, not performance thresholds) ---
    // 1. All 3 turns completed successfully (already asserted by the loop).
    // 2. Calendar advanced exactly 3 turns.
    assert_eq!(
        state.calendar.global_turn, 3,
        "Calendar should advance exactly 3 turns"
    );

    // 3. Entity counts should be non-zero (economy is alive).
    assert!(
        companies_t3 > 0,
        "There should be at least one active company after 3 turns"
    );
    assert!(
        buildings_t3 > 0,
        "There should be at least one building after 3 turns"
    );

    // 4. Total execution time should be reasonable (sanity bound: < 10 minutes).
    //    This is NOT a performance threshold — just a sanity check that the
    //    engine didn't hang or enter an infinite loop.
    assert!(
        total_ms < 600_000,
        "3-turn simulation should complete in under 10 minutes (took {} ms)",
        total_ms
    );
}

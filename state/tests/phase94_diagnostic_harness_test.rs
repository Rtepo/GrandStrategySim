//! Phase 94: 6-Turn Diagnostic Harness Test
//!
//! This test runs exactly 6 turns (one fiscal quarter) with the diagnostic
//! probe enabled, capturing per-phase state deltas and asserting strict
//! conservation of:
//! - M0 base money (fiat currency) — no creation/destruction except via CB
//! - Physical mass — no creation/destruction except via whitelisted sinks/sources
//! - Bank balance-sheet identity — assets == liabilities + equity
//!
//! # Output Artifacts
//! - `state/tests/diagnostic_output/turn_trace_q1.json` — structured trace for AI analysis
//! - `state/tests/diagnostic_output/turn_summary_q1.csv` — flat summary for spreadsheet analysis
//!
//! # Test Rules
//! 1. Exactly 6 turns are run (not 24).
//! 2. Every checkpoint must have fiat_conserved == true.
//! 3. Every checkpoint must have mass_conserved == true.
//! 4. Every checkpoint must have no_negative_inventories == true.
//! 5. Any violation hard-fails the test with a detailed explanation.
//! 6. The JSON trace must serialize and deserialize correctly.
//! 7. The CSV must be generated and non-empty.

#![cfg(feature = "diagnostic")]

use sim_engine::engine::diagnostic::{
    select_targets, write_turn_summary_csv, write_turn_trace_json, CapturingProbe,
    MassSinkWhitelist, TurnProbe, TurnTrace,
};
use sim_engine::engine::turn::run_turn_inner;
use sim_engine::engine::turn_context::InMemoryTurnContext;
use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
use sim_engine::registries::Registries;
use std::path::PathBuf;
use tempfile::TempDir;

/// Output directory for diagnostic artifacts.
const OUTPUT_DIR: &str = "tests/diagnostic_output";

/// Number of turns to run (one fiscal quarter = 6 half-monthly turns).
const TURNS: u32 = 6;

/// Run a 6-turn diagnostic simulation and verify all conservation laws.
#[test]
fn test_6_turn_diagnostic_harness() {
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

    // --- Select diagnostic targets ---
    // We need to run one turn first to get tasks into the right format,
    // but actually select_targets needs the tasks from inside the turn.
    // Instead, we'll select targets from the initial state's entities.
    // The probe will be configured after the first checkpoint.

    // Build initial targets from the entity context.
    // We need to convert CountryEntities to a format select_targets can use.
    // Since select_targets takes &[CountryTask], and we don't have tasks yet,
    // we'll use a simpler approach: select from the initial state directly.

    let mut state = initial_state;
    let initial_turn = state.calendar.global_turn;

    // Create a placeholder probe — we'll configure targets after the first
    // checkpoint gives us access to the tasks.
    let whitelist = MassSinkWhitelist::canonical();

    // We need targets before creating the probe. Let's extract them from
    // the initial ctx entities.
    let targets = select_targets_from_ctx(&state, &ctx);

    let mut probe = CapturingProbe::new(targets.clone(), whitelist);

    // --- Run 6 turns with probe instrumentation ---
    let mut years: Vec<u32> = Vec::new();
    for turn_num in 0..TURNS {
        years.push(state.calendar.current_year);
        let result = run_turn_inner(&mut state, &registries, &mut ctx, &mut probe);
        if let Err(e) = &result {
            panic!(
                "Turn {} (global {}) failed: {:?}",
                turn_num, state.calendar.global_turn, e
            );
        }
    }
    years.push(state.calendar.current_year);

    // --- Finalize the trace ---
    let trace = probe.finalize(&years);

    // --- Assertion 1: Exactly 6 turns were captured ---
    assert_eq!(
        trace.turns.len(),
        TURNS as usize,
        "Trace should contain exactly {} turn records, got {}",
        TURNS,
        trace.turns.len()
    );

    // --- Assertion 2: Calendar advanced exactly 6 turns ---
    assert_eq!(
        state.calendar.global_turn,
        initial_turn + TURNS,
        "Calendar should advance exactly {} turns",
        TURNS
    );

    // --- Assertion 3: Checkpoints were captured ---
    let total_checkpoints = trace.summary.total_checkpoints;
    assert!(
        total_checkpoints > 0,
        "At least one checkpoint should have been captured"
    );
    // Each turn should have at least 5 checkpoints (turn_start, building_cycle_post,
    // banking_turn_post, b2b_orders_post, b2b_settlement_post, etc.)
    let min_expected_checkpoints = TURNS * 5;
    assert!(
        total_checkpoints >= min_expected_checkpoints,
        "Expected at least {} checkpoints ({} turns × 5 minimum), got {}",
        min_expected_checkpoints,
        TURNS,
        total_checkpoints
    );

    // --- Assertion 4: No conservation violations ---
    let total_violations = trace.summary.total_violations;
    if total_violations > 0 {
        // Phase 94: Write trace artifacts BEFORE panicking so we can analyze.
        let output_dir = PathBuf::from(OUTPUT_DIR);
        std::fs::create_dir_all(&output_dir).ok();
        let json_path = output_dir.join("turn_trace_q1.json");
        let csv_path = output_dir.join("turn_summary_q1.csv");
        write_turn_trace_json(&trace, &json_path).ok();
        write_turn_summary_csv(&trace, &csv_path).ok();

        // Print ALL violations for diagnostic analysis.
        eprintln!("\n═══════════════════════════════════════════════════════════════");
        eprintln!("  PHASE 94 DIAGNOSTIC: {} CONSERVATION VIOLATIONS DETECTED", total_violations);
        eprintln!("═══════════════════════════════════════════════════════════════");
        let mut shown = 0;
        for turn_record in &trace.turns {
            for cp in &turn_record.checkpoints {
                for v in &cp.conservation.violations {
                    eprintln!(
                        "  [Turn {} Phase {} ({})] {:?} — magnitude={:.2} — {}",
                        cp.turn, cp.phase_index, cp.phase_name, v.kind, v.magnitude, v.explanation
                    );
                    shown += 1;
                    if shown >= 50 {
                        eprintln!("  ... (truncated, see turn_trace_q1.json for full list)");
                        break;
                    }
                }
                if shown >= 50 { break; }
            }
            if shown >= 50 { break; }
        }
        eprintln!("═══════════════════════════════════════════════════════════════\n");

        // Panic with the first violation for the test framework.
        for turn_record in &trace.turns {
            for cp in &turn_record.checkpoints {
                if !cp.conservation.violations.is_empty() {
                    let v = &cp.conservation.violations[0];
                    panic!(
                        "Conservation violation at turn {} phase {} ({}): {:?} — magnitude={:.2} — {}",
                        cp.turn, cp.phase_index, cp.phase_name, v.kind, v.magnitude, v.explanation
                    );
                }
            }
        }
    }
    assert_eq!(
        total_violations, 0,
        "All checkpoints must pass conservation checks (fiat + mass + bank balance sheet)"
    );

    // --- Assertion 5: Fiat conservation pass rate is 100% ---
    assert_eq!(
        trace.summary.fiat_conservation_pass_rate, 1.0,
        "Fiat conservation must pass at every checkpoint"
    );

    // --- Assertion 6: Mass conservation pass rate is 100% ---
    assert_eq!(
        trace.summary.mass_conservation_pass_rate, 1.0,
        "Mass conservation must pass at every checkpoint"
    );

    // --- Assertion 7: Loan events were tracked (if any loans exist) ---
    // We don't assert that loans MUST exist, but if they do, events should be tracked.
    // This is informational — the test passes even if no loans are issued in 6 turns.

    // --- Write output artifacts ---
    let output_dir = PathBuf::from(OUTPUT_DIR);
    std::fs::create_dir_all(&output_dir).expect("failed to create output directory");

    let json_path = output_dir.join("turn_trace_q1.json");
    write_turn_trace_json(&trace, &json_path).expect("failed to write JSON trace");
    assert!(
        json_path.exists(),
        "JSON trace file should exist at {:?}",
        json_path
    );

    let csv_path = output_dir.join("turn_summary_q1.csv");
    write_turn_summary_csv(&trace, &csv_path).expect("failed to write CSV summary");
    assert!(
        csv_path.exists(),
        "CSV summary file should exist at {:?}",
        csv_path
    );

    // --- Assertion 8: JSON round-trips correctly ---
    let json_content = std::fs::read_to_string(&json_path).expect("failed to read JSON trace");
    let _deserialized: TurnTrace =
        serde_json::from_str(&json_content).expect("JSON trace should deserialize correctly");

    // --- Assertion 9: CSV is non-empty and has a header ---
    let csv_content = std::fs::read_to_string(&csv_path).expect("failed to read CSV summary");
    assert!(!csv_content.is_empty(), "CSV summary should not be empty");
    assert!(
        csv_content.contains("turn,phase_index,phase_name"),
        "CSV should have the expected header"
    );

    // --- Print summary for diagnostic purposes ---
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("  PHASE 94: 6-TURN DIAGNOSTIC HARNESS — SUMMARY");
    println!("═══════════════════════════════════════════════════════════════");
    println!("  Turns run:           {}", trace.turns.len());
    println!("  Total checkpoints:   {}", trace.summary.total_checkpoints);
    println!("  Total violations:    {}", trace.summary.total_violations);
    println!("  Total loan events:   {}", trace.summary.total_loan_events);
    println!(
        "  Fiat pass rate:      {:.1}%",
        trace.summary.fiat_conservation_pass_rate * 100.0
    );
    println!(
        "  Mass pass rate:      {:.1}%",
        trace.summary.mass_conservation_pass_rate * 100.0
    );
    if let Some(ref first) = trace.summary.first_violation_checkpoint {
        println!("  First violation:     {}", first);
    }
    println!("  JSON trace:          {:?}", json_path);
    println!("  CSV summary:         {:?}", csv_path);
    println!("═══════════════════════════════════════════════════════════════\n");
}

/// Select diagnostic targets from the initial state and context.
///
/// This is a simplified version that picks:
/// - The alphabetically-first country
/// - Its capital region
/// - 5 companies by sector (largest fixed_capital)
/// - 1 bank (largest total_assets)
fn select_targets_from_ctx(
    state: &sim_engine::state::GameState,
    ctx: &InMemoryTurnContext,
) -> sim_engine::engine::diagnostic::HarnessTargets {
    use sim_engine::engine::diagnostic::HarnessTargets;
    use sim_engine::registries::enums::Sector;

    // Pick the alphabetically-first country that has entities.
    let country_name = ctx.entities.keys().min().cloned().unwrap_or_default();

    // Find the capital region of that country.
    let region_id = state
        .countries
        .get(&country_name)
        .and_then(|c| {
            c.regions
                .iter()
                .find(|r| r.is_capital)
                .or_else(|| c.regions.first())
                .map(|r| r.id.clone())
        })
        .unwrap_or_default();

    let ents = ctx.entities.get(&country_name);

    // Select 5 companies by sector, largest fixed_capital within each sector.
    let mut company_ids = Vec::new();
    if let Some(ents) = ents {
        let target_sectors = [
            Sector::Agriculture,
            Sector::HeavyIndustry,
            Sector::Construction,
            Sector::Mining,
            Sector::LocalServices,
        ];
        for target_sector in &target_sectors {
            let best = ents
                .companies
                .iter()
                .filter(|c| c.sector == *target_sector && c.merged_into.is_none())
                .max_by(|a, b| {
                    a.fixed_capital
                        .partial_cmp(&b.fixed_capital)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            if let Some(c) = best {
                company_ids.push(c.id.clone());
            }
        }
    }

    // Select the bank with the largest total_assets.
    let bank_id = ents
        .and_then(|ents| {
            ents.companies
                .iter()
                .filter(|c| c.sector == Sector::Banking && c.balance_sheet.is_some())
                .max_by(|a, b| {
                    let ta_a = a
                        .balance_sheet
                        .as_ref()
                        .map(|bs| bs.total_assets())
                        .unwrap_or(0.0);
                    let ta_b = b
                        .balance_sheet
                        .as_ref()
                        .map(|bs| bs.total_assets())
                        .unwrap_or(0.0);
                    ta_a.partial_cmp(&ta_b).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|c| c.id.clone())
        })
        .unwrap_or_default();

    HarnessTargets {
        company_ids,
        bank_id,
        region_id,
        country_name,
    }
}

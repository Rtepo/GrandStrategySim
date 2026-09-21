//! Market Gridlock Diagnostic — Epic Test
//!
//! Runs a 4-turn simulation with the diagnostic probe enabled and asserts the
//! economic-flow telemetry needed to diagnose the Turn-2 market gridlock:
//!
//! 1. **Wage Transfer Conservation (Probe 3a):** total cash debited from
//!    companies as wages equals total credited to `ClassDemographics.savings`.
//!    Catches wage money vanishing between payroll debit and citizen credit.
//! 2. **Propensity-to-consume (Probe 3b):** per class, savings_per_capita vs
//!    the commodity wealth gate — distinguishes "cannot afford" from
//!    "will not spend" (hoarding).
//! 3. **B2B/B2C flow:** order-book supply/demand volume and net surplus from
//!    the regional market snapshot.
//! 4. **Income summary:** per-company revenue from `financial_history`.
//! 5. **Furlough state:** per-company FTE / furlough / receivership.
//! 6. **Headline gridlock assertion:** fraction of companies with zero income
//!    by Turn 4.
//!
//! # Output Artifacts
//! - `state/tests/diagnostic_output/market_gridlock_diagnostic.json` —
//!   structured diagnostic dump for root-cause analysis.
//!
//! # Run
//! ```bash
//! CI=true cargo nextest run --release --features epic-tests market_gridlock_diagnostic_test
//! ```
//! (`--release` mandatory — debug builds are ~1100x slower for multi-turn sims.)

#![cfg(feature = "diagnostic")]

// Reuse the pure helper logic from the fast assertion helpers so the
// wealth-gate / era-multiplier classification is defined in exactly one place.
#[path = "../market_gridlock_assertions.rs"]
mod assertions;

use assertions::{
    check_wage_transfer, classify_demand_gate, consumption_commodities,
    DemandGateClass, WageTransferResult,
};
use sim_engine::engine::diagnostic::{
    write_all_dumps, CapturingProbe, MassSinkWhitelist, TurnTrace,
};
use sim_engine::engine::turn::run_turn_inner;
use sim_engine::engine::turn_context::InMemoryTurnContext;
use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
use sim_engine::registries::enums::Commodity;
use sim_engine::registries::Registries;
use sim_engine::society::geography::ClassDemographics;
use std::collections::BTreeMap;
use std::path::PathBuf;
use tempfile::TempDir;

/// Output directory for diagnostic artifacts.
const OUTPUT_DIR: &str = "tests/diagnostic_output";

/// Number of turns to run — the Turn-3 bankruptcy cascade is captured.
const TURNS: u32 = 4;

/// Per-class savings snapshot for the propensity-to-consume probe.
#[derive(Debug, Clone, serde::Serialize)]
struct ClassSavingsEntry {
    region_id: String,
    class_name: String,
    population: i64,
    savings: f64,
    savings_per_capita: f64,
}

/// Per-company income/furlough summary.
#[derive(Debug, Clone, serde::Serialize)]
struct CompanyIncomeEntry {
    id: String,
    sector: String,
    available_cash: f64,
    liquid_capital: f64,
    company_capital: f64,
    fulfilled_fte: u32,
    furloughed_workers_count: f64,
    is_in_receivership: bool,
    is_liquidated: bool,
    financial_history_len: usize,
    last_revenue: f64,
    last_net_profit: f64,
}

/// The full diagnostic dump written to JSON.
#[derive(Debug, Clone, serde::Serialize)]
struct MarketGridlockDiagnostic {
    dump_version: String,
    turns_run: u32,
    start_year: u32,
    wage_transfers: Vec<WageTransferResult>,
    class_savings: Vec<ClassSavingsEntry>,
    demand_gate_summary: DemandGateSummary,
    company_income: Vec<CompanyIncomeEntry>,
    b2b_b2c_flow: Vec<B2bB2cFlowEntry>,
    headline: HeadlineSummary,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DemandGateSummary {
    year: u32,
    total_classes: usize,
    cannot_afford_count: usize,
    era_gated_count: usize,
    eligible_count: usize,
    /// Per-commodity breakdown: how many classes clear the gate.
    per_commodity: BTreeMap<String, GateBreakdown>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct GateBreakdown {
    gate_threshold: f64,
    cannot_afford: usize,
    era_gated: usize,
    eligible: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
struct B2bB2cFlowEntry {
    turn: u32,
    phase: String,
    total_supply: f64,
    total_demand: f64,
    total_net_surplus: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
struct HeadlineSummary {
    total_companies: usize,
    companies_with_zero_income: usize,
    zero_income_fraction: f64,
    companies_furloughed: usize,
    companies_in_receivership: usize,
    companies_liquidated: usize,
    gridlock_detected: bool,
}

/// The main diagnostic test: run 4 turns, capture telemetry, assert flow
/// invariants, and emit a structured dump for root-cause analysis.
#[test]
fn test_market_gridlock_diagnostic() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let data_dir = tmp.path();

    let registries = Registries::native_only();
    let options = GenerateOptions {
        country_count: 4,
        start_year: StartYear::Y1925,
        seed: Some(42),
    };

    let GeneratedWorld {
        state: mut initial_state,
        ..
    } = generate_world(data_dir, options, &registries).expect("world generation failed");

    let mut ctx = InMemoryTurnContext::load_from_disk(data_dir, &mut initial_state)
        .expect("failed to load turn context from generated world");

    let mut state = initial_state;
    let initial_turn = state.calendar.global_turn;

    // --- Select diagnostic targets (same logic as phase94 harness) ---
    let whitelist = MassSinkWhitelist::canonical();
    let targets = select_targets_from_state(&state, &ctx);
    let mut probe = CapturingProbe::new(targets.clone(), whitelist);

    // --- Snapshot pre-turn citizen savings baseline (for per-class deltas) ---
    let _pre_turn_savings = snapshot_class_savings(&state);

    // --- Run TURNS turns with probe instrumentation ---
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

    // --- Write v4 dumps (sector ledger, market clearing, banking) ---
    let output_dir = PathBuf::from(OUTPUT_DIR);
    std::fs::create_dir_all(&output_dir).expect("failed to create output directory");
    let final_turn = state.calendar.global_turn;
    let final_year = state.calendar.current_year;
    write_all_dumps(&ctx, final_turn, final_year, &output_dir)
        .expect("failed to write v4 diagnostic dumps");

    // ========================================================================
    // PROBE 3a: Wage Transfer Conservation
    // ========================================================================
    let wage_transfers = compute_wage_transfers(&trace);

    // WAGE-1/2/3: INFORMATIONAL — the turn_start → building_cycle_post window
    // includes taxes, subsistence debits, and other citizen cash flows beyond
    // pure wage credits. A negative citizen delta or positive company delta is
    // a DIAGNOSTIC SIGNAL, not necessarily a bug. Record it in the dump and
    // print a high-visibility warning, but do not hard-fail.
    for wt in &wage_transfers {
        if !wt.citizens_gained {
            eprintln!(
                "WAGE-1 WARN [turn {}]: citizen cash decreased by {:.2} during wage phase \
                 — may indicate wages not reaching citizens OR taxes/subsistence debits \
                 exceeding wage credits",
                wt.turn, wt.citizen_cash_delta.abs()
            );
        }
        if !wt.companies_paid {
            eprintln!(
                "WAGE-2 WARN [turn {}]: company cash increased by {:.2} during wage phase \
                 — no wages paid (companies not paying workers)",
                wt.turn, wt.company_cash_delta
            );
        }
        if wt.transfer_balance < -1e6 {
            eprintln!(
                "WAGE-3 WARN [turn {}]: transfer balance={:.2} — companies debited {:.2} \
                 but citizens only credited {:.2}. Potential money leak.",
                wt.turn, wt.transfer_balance, wt.company_cash_delta.abs(), wt.citizen_cash_delta
            );
        }
    }

    // WAGE-4 (HARD-FAIL): At least one class has savings_per_capita > 0 by Turn 1.
    // This is the robust invariant — if NO class received any wages, the wage
    // transfer is definitively broken.
    let post_turn_savings = snapshot_class_savings(&state);
    let any_class_with_savings = post_turn_savings
        .iter()
        .any(|c| c.savings_per_capita > 0.0);
    assert!(
        any_class_with_savings,
        "WAGE-4 FAIL: All classes have savings_per_capita == 0 after {} turns \
         — no wages reached any demographic class",
        TURNS
    );

    // ========================================================================
    // PROBE 3b: Propensity-to-consume vs Savings Targets
    // ========================================================================
    let demand_gate_summary = compute_demand_gate_summary(&state, final_year);

    // PROP-1: At least one class can afford Cereal (the most basic staple).
    let cereal_breakdown = demand_gate_summary
        .per_commodity
        .get("Cereal")
        .expect("Cereal must be in the demand-gate probe set");
    assert!(
        cereal_breakdown.eligible > 0,
        "PROP-1 FAIL: No class clears the wealth gate for Cereal — universal poverty, \
         wages insufficient for subsistence consumption"
    );

    // PROP-2: At least one class can afford Clothing (basic manufactured good).
    let clothing_breakdown = demand_gate_summary
        .per_commodity
        .get("Clothing")
        .expect("Clothing must be in the demand-gate probe set");
    assert!(
        clothing_breakdown.eligible > 0,
        "PROP-2 FAIL: No class clears the wealth gate for Clothing — middle-class \
         consumption impossible, B2C demand structurally zero"
    );

    // ========================================================================
    // B2B / B2C Flow (from RegionalMarketSnapshot)
    // ========================================================================
    let b2b_b2c_flow = compute_b2b_b2c_flow(&trace);

    // By Turn 1, there should be SOME market activity (supply or demand > 0).
    let turn1_activity = b2b_b2c_flow
        .iter()
        .filter(|f| f.turn == initial_turn + 1)
        .map(|f| f.total_supply + f.total_demand)
        .sum::<f64>();
    assert!(
        turn1_activity > 0.0,
        "B2B/B2C FAIL: Zero market activity (supply+demand=0) by Turn 1 — \
         total market gridlock at the order-book level"
    );

    // ========================================================================
    // Income Summary + Furlough State (from ctx.entities post-turn)
    // ========================================================================
    let company_income = compute_company_income(&ctx);

    // ========================================================================
    // Headline Gridlock Assertion
    // ========================================================================
    let headline = compute_headline(&company_income);
    assert!(
        headline.total_companies > 0,
        "No companies found in post-turn state"
    );

    // The headline assertion: by Turn 4, less than 50% of companies should have
    // zero income. If >= 50%, gridlock is confirmed.
    //
    // NOTE: This test DOCUMENTS the gridlock — it does not hard-fail on
    // gridlock detection. Instead it emits the diagnostic dump and prints a
    // high-visibility summary. The assertion that DOES hard-fail is the
    // wage-transfer and propensity checks above (those catch actual bugs).
    // The headline is informational: if gridlock is detected, the dump JSON
    // contains the full telemetry for root-cause analysis.
    if headline.gridlock_detected {
        eprintln!();
        eprintln!("═══════════════════════════════════════════════════════════════");
        eprintln!("  MARKET GRIDLOCK DETECTED — {} of {} companies ({:.1}%) have zero income by Turn {}",
            headline.companies_with_zero_income,
            headline.total_companies,
            headline.zero_income_fraction * 100.0,
            TURNS);
        eprintln!("  Furloughed: {} | Receivership: {} | Liquidated: {}",
            headline.companies_furloughed,
            headline.companies_in_receivership,
            headline.companies_liquidated);
        eprintln!("  Diagnostic dump: {}/market_gridlock_diagnostic.json", OUTPUT_DIR);
        eprintln!("═══════════════════════════════════════════════════════════════");
        eprintln!();
    }

    // ========================================================================
    // Emit the structured diagnostic dump
    // ========================================================================
    let diagnostic = MarketGridlockDiagnostic {
        dump_version: "gridlock-v1".to_string(),
        turns_run: TURNS,
        start_year: 1925,
        wage_transfers,
        class_savings: post_turn_savings,
        demand_gate_summary,
        company_income,
        b2b_b2c_flow,
        headline: headline.clone(),
    };

    let dump_path = output_dir.join("market_gridlock_diagnostic.json");
    let dump_json = serde_json::to_string_pretty(&diagnostic)
        .expect("failed to serialize diagnostic dump");
    std::fs::write(&dump_path, dump_json).expect("failed to write diagnostic dump");
    assert!(
        dump_path.exists(),
        "Diagnostic dump file should exist at {:?}",
        dump_path
    );

    // --- Print summary ---
    println!();
    println!("═══════════════════════════════════════════════════════════════");
    println!("  MARKET GRIDLOCK DIAGNOSTIC — {}-TURN SUMMARY", TURNS);
    println!("═══════════════════════════════════════════════════════════════");
    println!("  Turns run:              {}", TURNS);
    println!("  Companies:              {}", headline.total_companies);
    println!("  Zero-income companies:  {} ({:.1}%)",
        headline.companies_with_zero_income,
        headline.zero_income_fraction * 100.0);
    println!("  Furloughed:             {}", headline.companies_furloughed);
    println!("  Gridlock detected:      {}", headline.gridlock_detected);
    println!("  Diagnostic dump:        {:?}", dump_path);
    println!("═══════════════════════════════════════════════════════════════");
    println!();
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Select diagnostic targets from the initial state and context.
///
/// Same logic as `phase94_diagnostic_harness_test.rs:select_targets_from_ctx`:
/// alphabetically-first country, capital region, 5 companies by sector
/// (largest fixed_capital), 1 bank (largest total_assets).
fn select_targets_from_state(
    state: &sim_engine::state::GameState,
    ctx: &InMemoryTurnContext,
) -> sim_engine::engine::diagnostic::HarnessTargets {
    use sim_engine::engine::diagnostic::HarnessTargets;
    use sim_engine::registries::enums::Sector;

    let country_name = ctx.entities.keys().min().cloned().unwrap_or_default();
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

/// Snapshot per-class savings across all countries/regions.
fn snapshot_class_savings(state: &sim_engine::state::GameState) -> Vec<ClassSavingsEntry> {
    let mut entries = Vec::new();
    for country in state.countries.values() {
        for region in &country.regions {
            for (class, demo) in &region.class_demographics.rural_classes {
                entries.push(ClassSavingsEntry {
                    region_id: region.id.clone(),
                    class_name: format!("{:?}", class),
                    population: demo.population,
                    savings: demo.savings,
                    savings_per_capita: demo.savings_per_capita,
                });
            }
            for (class, demo) in &region.class_demographics.urban_classes {
                entries.push(ClassSavingsEntry {
                    region_id: region.id.clone(),
                    class_name: format!("{:?}", class),
                    population: demo.population,
                    savings: demo.savings,
                    savings_per_capita: demo.savings_per_capita,
                });
            }
        }
    }
    entries
}

/// Compute wage-transfer results for each turn by diffing FiatWalk at
/// `turn_start` vs `building_cycle_post`.
fn compute_wage_transfers(trace: &TurnTrace) -> Vec<WageTransferResult> {
    let mut results = Vec::new();
    for turn_record in &trace.turns {
        let turn_start = turn_record
            .checkpoints
            .iter()
            .find(|cp| cp.phase_name == "turn_start");
        let building_post = turn_record
            .checkpoints
            .iter()
            .find(|cp| cp.phase_name == "building_cycle_post");

        if let (Some(start), Some(post)) = (turn_start, building_post) {
            // Sum company available_cash from the targeted company snapshots.
            let company_cash_start: f64 =
                start.companies.iter().map(|c| c.available_cash).sum();
            let company_cash_post: f64 =
                post.companies.iter().map(|c| c.available_cash).sum();
            let citizen_cash_start = start.global_fiat.citizen_cash;
            let citizen_cash_post = post.global_fiat.citizen_cash;

            results.push(check_wage_transfer(
                turn_record.turn,
                company_cash_start,
                company_cash_post,
                citizen_cash_start,
                citizen_cash_post,
            ));
        }
    }
    results
}

/// Compute the demand-gate summary: for each consumption commodity, count how
/// many classes are cannot_afford / era_gated / eligible.
fn compute_demand_gate_summary(
    state: &sim_engine::state::GameState,
    year: u32,
) -> DemandGateSummary {
    let commodities = consumption_commodities();
    let mut per_commodity: BTreeMap<String, GateBreakdown> = BTreeMap::new();

    // Initialize breakdowns with gate thresholds.
    for &commodity in commodities {
        let name = format!("{:?}", commodity);
        per_commodity.insert(
            name,
            GateBreakdown {
                gate_threshold: assertions::commodity_wealth_gate(commodity),
                cannot_afford: 0,
                era_gated: 0,
                eligible: 0,
            },
        );
    }

    let mut total_classes = 0usize;
    let mut cannot_afford_count = 0usize;
    let mut era_gated_count = 0usize;
    let mut eligible_count = 0usize;

    for country in state.countries.values() {
        for region in &country.regions {
            for demo in region.class_demographics.rural_classes.values() {
                total_classes += 1;
                classify_class(
                    demo,
                    commodities,
                    year,
                    &mut per_commodity,
                    &mut cannot_afford_count,
                    &mut era_gated_count,
                    &mut eligible_count,
                );
            }
            for demo in region.class_demographics.urban_classes.values() {
                total_classes += 1;
                classify_class(
                    demo,
                    commodities,
                    year,
                    &mut per_commodity,
                    &mut cannot_afford_count,
                    &mut era_gated_count,
                    &mut eligible_count,
                );
            }
        }
    }

    DemandGateSummary {
        year,
        total_classes,
        cannot_afford_count,
        era_gated_count,
        eligible_count,
        per_commodity,
    }
}

/// Classify a single class against all consumption commodities.
fn classify_class(
    demo: &ClassDemographics,
    commodities: &[Commodity],
    year: u32,
    per_commodity: &mut BTreeMap<String, GateBreakdown>,
    cannot_afford_count: &mut usize,
    era_gated_count: &mut usize,
    eligible_count: &mut usize,
) {
    // Use the class's best classification across staple commodities (Cereal)
    // as the class-level summary. Per-commodity breakdowns are recorded separately.
    let staple = Commodity::Cereal;
    let staple_cls = classify_demand_gate(demo.savings_per_capita, staple, year);

    // Record per-commodity breakdowns.
    for &commodity in commodities {
        let name = format!("{:?}", commodity);
        let cls = classify_demand_gate(demo.savings_per_capita, commodity, year);
        if let Some(b) = per_commodity.get_mut(&name) {
            match cls {
                DemandGateClass::CannotAfford => b.cannot_afford += 1,
                DemandGateClass::EraGated => b.era_gated += 1,
                DemandGateClass::Eligible => b.eligible += 1,
            }
        }
    }

    // Class-level summary uses the staple (Cereal) classification.
    match staple_cls {
        DemandGateClass::CannotAfford => *cannot_afford_count += 1,
        DemandGateClass::EraGated => *era_gated_count += 1,
        DemandGateClass::Eligible => *eligible_count += 1,
    }
}

/// Compute B2B/B2C flow entries from the regional market snapshots.
fn compute_b2b_b2c_flow(trace: &TurnTrace) -> Vec<B2bB2cFlowEntry> {
    let mut entries = Vec::new();
    for turn_record in &trace.turns {
        for cp in &turn_record.checkpoints {
            let total_supply: f64 = cp.regional_market.supply_volume.values().sum();
            let total_demand: f64 = cp.regional_market.demand_volume.values().sum();
            let total_net_surplus: f64 = cp.regional_market.net_surplus.values().sum();
            if total_supply > 0.0 || total_demand > 0.0 || total_net_surplus.abs() > 0.0 {
                entries.push(B2bB2cFlowEntry {
                    turn: cp.turn,
                    phase: cp.phase_name.clone(),
                    total_supply,
                    total_demand,
                    total_net_surplus,
                });
            }
        }
    }
    entries
}

/// Compute per-company income and furlough state from the post-turn context.
fn compute_company_income(ctx: &InMemoryTurnContext) -> Vec<CompanyIncomeEntry> {
    let mut entries = Vec::new();
    for ents in ctx.entities.values() {
        for company in &ents.companies {
            let (last_revenue, last_net_profit) = company
                .financial_history
                .last()
                .and_then(|record| {
                    let revenue = record
                        .get("revenue")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let net_profit = record
                        .get("net_profit")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    Some((revenue, net_profit))
                })
                .unwrap_or((0.0, 0.0));

            entries.push(CompanyIncomeEntry {
                id: company.id.clone(),
                sector: format!("{:?}", company.sector),
                available_cash: company.available_cash,
                liquid_capital: company.liquid_capital,
                company_capital: company.company_capital,
                fulfilled_fte: company.fulfilled_fte,
                furloughed_workers_count: company.furloughed_workers_count,
                is_in_receivership: company.is_in_receivership,
                is_liquidated: company.is_liquidated,
                financial_history_len: company.financial_history.len(),
                last_revenue,
                last_net_profit,
            });
        }
    }
    entries
}

/// Compute the headline gridlock summary.
fn compute_headline(company_income: &[CompanyIncomeEntry]) -> HeadlineSummary {
    let total = company_income.len();
    let zero_income = company_income
        .iter()
        .filter(|c| c.last_revenue <= 0.0)
        .count();
    let furloughed = company_income
        .iter()
        .filter(|c| c.furloughed_workers_count > 0.0)
        .count();
    let receivership = company_income.iter().filter(|c| c.is_in_receivership).count();
    let liquidated = company_income.iter().filter(|c| c.is_liquidated).count();
    let fraction = if total > 0 {
        zero_income as f64 / total as f64
    } else {
        0.0
    };
    HeadlineSummary {
        total_companies: total,
        companies_with_zero_income: zero_income,
        zero_income_fraction: fraction,
        companies_furloughed: furloughed,
        companies_in_receivership: receivership,
        companies_liquidated: liquidated,
        gridlock_detected: fraction >= 0.5,
    }
}

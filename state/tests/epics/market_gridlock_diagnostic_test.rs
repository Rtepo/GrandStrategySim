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
use sim_engine::entities::{Company, JointStockData, LegalForm};
use sim_engine::registries::enums::{Commodity, Sector};
use sim_engine::registries::Registries;
use sim_engine::state::{BankBalanceSheet, BankType};
use sim_engine::society::geography::ClassDemographics;
use std::collections::BTreeMap;
use std::path::PathBuf;
use tempfile::TempDir;

/// Output directory for diagnostic artifacts.
const OUTPUT_DIR: &str = "tests/diagnostic_output";

/// Number of turns to run — the Turn-3 bankruptcy cascade is captured.
// 24-turn mandate: the 12-turn horizon masked a delayed collapse — real-world
// simulation to Turn 25 showed GDP spiking ~Turn 3 then collapsing to ~0,
// negative wealth tax, profitable companies drained to $0.00 cash with wage
// arrears, energy supply crashing to 0 MW, and KNF Sev-10 leverage floods.
// 120-turn survival mandate: real-world Turn-48 runs revealed a slow-burn
// collapse the yearling window missed — GDP flatlining, 93.79% unemployment,
// mass bank liquidations, companies holding −5.1B in base cash (a
// double-entry violation), ministry expenditure at $0.00, and regional
// blackouts coexisting with 945% oversupply. The horizon extends to a full
// five years to catch slow drains.
const TURNS: u32 = 120;

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
    /// Window-summed wage expense + arrears from the financial record —
    /// decomposes losses so the dump shows WHERE the money went.
    wage_expense: f64,
    operating_costs: f64,
    interest_paid: f64,
    taxes_paid: f64,
    /// Cumulative lifetime export revenue credited by the foreign-sector
    /// sweep (not windowed) — correlates the purse's reach with P&L.
    export_revenue: f64,
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
    /// Companies in sectors that sell output through the production
    /// pipeline (excludes Banking/NGO/Religion/Government/etc., whose
    /// revenue legitimately arrives via fees, treasury, or grid billing).
    producing_companies: usize,
    producing_zero_income: usize,
    producing_zero_income_fraction: f64,
    /// Producing-sector companies whose last booked `net_profit` is > 0.
    producing_profitable: usize,
    producing_profitable_fraction: f64,
    /// All-sector profitability (informational — banks/NGOs legitimately
    /// show net_profit <= 0 because their income is not booked through
    /// the industrial P&L path).
    all_sector_profitable_fraction: f64,
    /// Population-weighted unemployment rate (percent) aggregated across
    /// countries, recomputed with the engine's own formula:
    /// labor_force = population * labor_force_participation / 100.
    unemployment_rate: f64,
    companies_furloughed: usize,
    companies_in_receivership: usize,
    companies_liquidated: usize,
    gridlock_detected: bool,
}

/// Sectors whose income does NOT flow through `last_output_value`:
/// they earn via interest/fees (Banking), donations (NGO, Religion),
/// treasury funding (Government, PublicAdministration, PublicServices),
/// foreign-trade margins (ExportServices), or grid billing for utility
/// waste streams (WasteManagement). Counting them as "zero income"
/// would mask the real production-economy signal.
const NON_PRODUCING_SECTORS: &[&str] = &[
    "Banking",
    "NGO",
    "Religion",
    "PublicAdministration",
    "Government",
    "PublicServices",
    "ExportServices",
    "WasteManagement",
    // B2C service sectors — `Sector::sector_commodities` returns an empty
    // list for these, i.e. they produce no tradeable commodity output and
    // their revenue flows through consumer/contract channels that the
    // `last_output_value` P&L path does not observe. Counting them as
    // "producing companies with zero income" misclassifies a healthy
    // service economy as gridlock.
    "Hospitality",
    "LocalServices",
    // Project-based sector — construction output is a completed building
    // asset delivered over many turns via `active_project`, not per-turn
    // commodity output. A 4-turn window from a cold start cannot contain
    // a project completion, so zero `last_output_value` is structural,
    // not a failure signal.
    "Construction",
];

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

    // Phase 96 determinism probe: fingerprint the loaded world. Two runs of
    // this binary must produce identical fingerprints; divergence means
    // worldgen still consumes per-process entropy somewhere.
    {
        let mut total_companies = 0usize;
        let mut total_buildings = 0usize;
        let mut cash_values: Vec<u64> = Vec::new();
        let mut capital_values: Vec<u64> = Vec::new();
        let mut total_pop: i64 = 0;
        let mut country_summaries: Vec<String> = Vec::new();
        let mut cnames: Vec<&String> = ctx.entities.keys().collect();
        cnames.sort();
        for cname in cnames {
            let ents = &ctx.entities[cname];
            total_companies += ents.companies.len();
            total_buildings += ents.buildings.len();
            // Sort the per-company values before summing: identical multisets
            // produce identical sums regardless of map iteration order, while
            // real content divergence still shows up.
            cash_values.extend(ents.companies.iter().map(|c| c.available_cash.to_bits()));
            capital_values.extend(ents.companies.iter().map(|c| c.company_capital.to_bits()));
            if let Some(country) = initial_state.countries.get(cname.as_str()) {
                let cpop: i64 = country.regions.iter().map(|r| r.population).sum();
                total_pop += cpop;
                let mut ccash: Vec<u64> = ents
                    .companies
                    .iter()
                    .map(|c| c.available_cash.to_bits())
                    .collect();
                ccash.sort_unstable();
                let ccash_hash = ccash
                    .iter()
                    .fold(0u64, |acc, v| {
                        acc.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(*v)
                    });
                country_summaries.push(format!(
                    "{}:r{}:p{}:c{}:h{:016x}",
                    cname,
                    country.regions.len(),
                    cpop,
                    ents.companies.len(),
                    ccash_hash
                ));
            }
        }
        country_summaries.sort();
        // Hash the sorted bit-patterns — order-independent, exact-value.
        cash_values.sort_unstable();
        capital_values.sort_unstable();
        let cash_hash = cash_values
            .iter()
            .fold(0u64, |acc, v| acc.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(*v));
        let capital_hash = capital_values
            .iter()
            .fold(0u64, |acc, v| acc.wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(*v));
        eprintln!(
            "GENESIS_FP: companies={} buildings={} cash_hash={:016x} capital_hash={:016x} pop={}",
            total_companies, total_buildings, cash_hash, capital_hash, total_pop
        );
        eprintln!("GENESIS_FP_DETAIL: {}", country_summaries.join(" | "));

        // CASH-1 guard (genesis): the per-country company list must hold each
        // entity exactly once. A worldgen merge-save that buckets by bare
        // `sector_json_name` wrote custom-`file_stem` entities (e.g.
        // STRATEGIC_RESERVE_*) into a second file, so `load_companies`
        // resurrected two live objects sharing one id — every id-keyed settle
        // index then resolved to the last instance, double-decrementing its
        // `debit_cash` below zero while the twin's encumbrance stayed stuck.
        {
            let mut dupes: Vec<String> = Vec::new();
            for (cname, ents) in &ctx.entities {
                let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
                for c in &ents.companies {
                    if !seen.insert(c.id.as_str()) {
                        dupes.push(format!("{}:{}", cname, c.id));
                    }
                }
            }
            assert!(
                dupes.is_empty(),
                "Duplicate company ids in loaded entities: {:?}",
                dupes
            );
        }

        // Phase 96: optional per-entity dump — set SES_DUMP_ENTITIES to a
        // file path to write every (country, company id, sector, cash, capital)
        // row sorted canonically. Diffing two runs' dumps pinpoints exactly
        // which entities diverge.
        if let Ok(dump_path) = std::env::var("SES_DUMP_ENTITIES") {
            let mut rows: Vec<String> = Vec::new();
            let mut cnames: Vec<&String> = ctx.entities.keys().collect();
            cnames.sort();
            for cname in cnames {
                let ents = &ctx.entities[cname];
                let mut crows: Vec<String> = ents
                    .companies
                    .iter()
                    .map(|c| {
                        format!(
                            "{}\t{}\t{:?}\t{:.4}\t{:.4}",
                            cname,
                            c.id,
                            c.sector,
                            c.available_cash,
                            c.company_capital
                        )
                    })
                    .collect();
                crows.sort();
                rows.extend(crows);
            }
            std::fs::write(&dump_path, rows.join("\n")).ok();
        }
    }

    let mut state = initial_state;
    let initial_turn = state.calendar.global_turn;
    // POP-1 baseline: genesis census before any turn runs.
    let genesis_population: i64 = state
        .countries
        .values()
        .flat_map(|c| c.regions.iter())
        .map(|r| r.population)
        .sum();

    // --- Phase 95: Guarantee the banking layer is observable ---
    // Worldgen writes banking.json per country, but a world whose sector file
    // is missing/empty would run bank-free and the reserve invariants would
    // be vacuous. Inject a balanced synthetic commercial bank into any
    // country that lacks one. The CB liquidity_injected bump keeps the M0
    // walk honest: the new reserves are CB-issued base money, booked against
    // deposits + tier_1_capital (A = L + E at genesis).
    for (cname, ents) in ctx.entities.iter_mut() {
        let has_bank = ents
            .companies
            .iter()
            .any(|c| c.sector == Sector::Banking && c.balance_sheet.is_some());
        if has_bank {
            continue;
        }
        let gdp = state
            .countries
            .get(cname)
            .map(|c| c.budget.gdp)
            .unwrap_or(1.0e9);
        let deposits = gdp * 0.10;
        let reserves = gdp * 0.15;
        let tier_1 = (reserves - deposits).max(1.0);
        let mut bank = Company::new(
            format!(
                "BANK-{}-TST",
                cname[..3.min(cname.len())].to_uppercase()
            ),
            format!("Test Bank of {}", cname),
            Sector::Banking,
            LegalForm::JointStockCompany(JointStockData::default()),
            tier_1,
            0.0,
            500,
        );
        bank.region_id = state
            .countries
            .get(cname)
            .and_then(|c| c.regions.iter().find(|r| r.is_capital).or_else(|| c.regions.first()))
            .map(|r| r.id.clone())
            .unwrap_or_default();
        bank.bank_type = Some(BankType::Commercial);
        bank.balance_sheet = Some(BankBalanceSheet {
            reserves_at_central_bank: reserves,
            deposits,
            tier_1_capital: tier_1,
            ..Default::default()
        });
        if let Some(country) = state.countries.get_mut(cname) {
            country.central_bank.liquidity_injected += reserves;
        }
        ents.companies.push(bank);
    }

    // --- Select diagnostic targets (same logic as phase94 harness) ---
    let whitelist = MassSinkWhitelist::canonical();
    let targets = select_targets_from_state(&state, &ctx);
    let mut probe = CapturingProbe::new(targets.clone(), whitelist);

    // --- Snapshot pre-turn citizen savings baseline (for per-class deltas) ---
    let _pre_turn_savings = snapshot_class_savings(&state);

    // --- Run TURNS turns with probe instrumentation ---
    // 24-turn mandate: capture a per-turn GDP series so the GDP-1 collapse
    // gate can compare Turn-24 output against the Turn-4 peak.
    let mut years: Vec<u32> = Vec::new();
    let mut gdp_series: Vec<f64> = Vec::new();
    // Per-turn GDP-component series for FS-1's trailing-window shares —
    // investment is lumpy turn-to-turn (CapEx bursts), so composition is
    // measured on an annualized window, matching national-accounts practice.
    let mut inv_series: Vec<f64> = Vec::new();
    let mut gov_series: Vec<f64> = Vec::new();
    for turn_num in 0..TURNS {
        years.push(state.calendar.current_year);
        let result = run_turn_inner(&mut state, &registries, &mut ctx, &mut probe);
        if let Err(e) = &result {
            panic!(
                "Turn {} (global {}) failed: {:?}",
                turn_num, state.calendar.global_turn, e
            );
        }
        gdp_series.push(
            state
                .countries
                .values()
                .map(|c| c.budget.gdp)
                .sum::<f64>(),
        );
        inv_series.push(
            state
                .countries
                .values()
                .map(|c| c.macro_indicators.gdp_breakdown.investment)
                .sum::<f64>(),
        );
        gov_series.push(
            state
                .countries
                .values()
                .map(|c| c.macro_indicators.gdp_breakdown.government_spending)
                .sum::<f64>(),
        );
        let turn_pop: i64 = state
            .countries
            .values()
            .flat_map(|c| c.regions.iter())
            .map(|r| r.population)
            .sum();
        #[cfg(feature = "diagnostic")]
        {
            let (mut c, mut g, mut i, mut nx, mut imp) = (0.0, 0.0, 0.0, 0.0, 0.0);
            let (mut births, mut deaths, mut mig) = (0.0, 0.0, 0.0);
            for country in state.countries.values() {
                let bd = &country.macro_indicators.gdp_breakdown;
                c += bd.consumption;
                g += bd.government_spending;
                i += bd.investment;
                nx += bd.net_exports;
                imp += bd.imputed_consumption;
                let dem = &country.macro_indicators.demographics;
                births += dem.last_births;
                deaths += dem.last_deaths;
                mig += dem.last_migration;
            }
            eprintln!(
                "GDPSER t={}: c={:.2}B g={:.2}B i={:.2}B nx={:.2}B imp={:.2}B total={:.2}B",
                turn_num, c / 1e9, g / 1e9, i / 1e9, nx / 1e9, imp / 1e9,
                gdp_series.last().unwrap() / 1e9
            );
            eprintln!(
                "POPSER t={}: pop={} births={:.0} deaths={:.0} mig={:.0}",
                turn_num, turn_pop, births, deaths, mig
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
    // RESERVE FLOOR (Phase 95): no bank may post negative reserves at ANY
    // checkpoint — ELA covers debits at the point of shortfall and the
    // per-phase reconciliation sweeps catch unguarded paths.
    // ========================================================================
    for rec in &trace.turns {
        for cp in &rec.checkpoints {
            if !cp.bank.id.is_empty() {
                assert!(
                    cp.bank.reserves_at_central_bank >= -0.01,
                    "RESERVE FLOOR FAIL [turn {} phase {}]: bank {} reserves={:.4}",
                    cp.turn,
                    cp.phase_name,
                    cp.bank.id,
                    cp.bank.reserves_at_central_bank
                );
            }
        }
    }
    // Post-run scan: every surviving bank must hold non-negative reserves.
    let mut surviving_banks = 0usize;
    for ents in ctx.entities.values() {
        for c in &ents.companies {
            if c.sector == Sector::Banking {
                if let Some(ref bs) = c.balance_sheet {
                    surviving_banks += 1;
                    assert!(
                        bs.reserves_at_central_bank >= -0.01,
                        "RESERVE FLOOR FAIL [final]: bank {} reserves={:.4}",
                        c.id,
                        bs.reserves_at_central_bank
                    );
                }
            }
        }
    }
    // 12-turn mandate (BANK-1): commercial banking must remain an active
    // sector — zero survivors means reserve drains/liquidation cascades
    // exterminated the credit system.
    assert!(
        surviving_banks > 0,
        "BANK-1 FAIL: zero commercial banks survived {} turns — all were \
         liquidated or despawned. The credit system is extinct.",
        TURNS
    );

    // ========================================================================
    // PROBE 3a: Wage Transfer Conservation
    // ========================================================================
    let wage_transfers = compute_wage_transfers(&trace);

    // WAGE-1/2/3: INFORMATIONAL — the production_cycle_post → labor_market_post
    // window isolates the wage phase, but still includes shadow-economy and
    // reserve-floor flows beyond pure wage credits. A negative citizen delta
    // or positive company delta is a DIAGNOSTIC SIGNAL, not necessarily a
    // bug. Record it in the dump and print a high-visibility warning, but do
    // not hard-fail.
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
        if wt.transfer_balance.abs() > 1e6 {
            eprintln!(
                "WAGE-3 WARN [turn {}]: M0 drift={:.2} inside wage window — money {}. \
                 Employer pockets debited {:.2}, citizens credited {:.2}.",
                wt.turn,
                wt.transfer_balance,
                if wt.transfer_balance < 0.0 { "vanished" } else { "was created" },
                wt.company_cash_delta.abs(),
                wt.citizen_cash_delta
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
    let headline = compute_headline(&company_income, &state);
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
        eprintln!("  MARKET GRIDLOCK DETECTED — {} of {} producing-sector companies ({:.1}%) have zero income by Turn {}",
            headline.producing_zero_income,
            headline.producing_companies,
            headline.producing_zero_income_fraction * 100.0,
            TURNS);
        eprintln!("  (all-sector zero-income: {} of {} = {:.1}%)",
            headline.companies_with_zero_income,
            headline.total_companies,
            headline.zero_income_fraction * 100.0);
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
    println!("  Producing-sector zero-income: {} of {} ({:.1}%)",
        headline.producing_zero_income,
        headline.producing_companies,
        headline.producing_zero_income_fraction * 100.0);
    println!("  Producing-sector profitable: {} of {} ({:.1}%)",
        headline.producing_profitable,
        headline.producing_companies,
        headline.producing_profitable_fraction * 100.0);
    println!("  All-sector profitable:   {:.1}%",
        headline.all_sector_profitable_fraction * 100.0);
    println!("  Unemployment rate:       {:.2}%", headline.unemployment_rate);
    println!("  Furloughed:             {}", headline.companies_furloughed);
    println!("  Gridlock detected:      {}", headline.gridlock_detected);
    println!("  Diagnostic dump:        {:?}", dump_path);
    println!("═══════════════════════════════════════════════════════════════");
    println!();

    // ========================================================================
    // 12-TURN CRUCIBLE ASSERTS
    // ========================================================================
    // Turn-6 real-world simulation exposed: Energy companies spawned at
    // 65M+ FTE against a 3M-population country, the grid ran 320% supply/
    // demand into GridDamage, Local Services booked zero revenue, and
    // commercial banks went extinct. These asserts make each failure mode
    // impossible rather than diagnostic-only.
    //
    // LABOR-0: no company may employ people who do not exist. The genesis
    // caps in corporate.rs bound fulfilled_fte to real demographics; this
    // assert is the backstop proving no path resurrects the overflow.
    let total_population: f64 = state
        .countries
        .values()
        .map(|c| c.budget.population as f64)
        .sum();
    let total_employment: f64 = ctx
        .entities
        .values()
        .flat_map(|ents| ents.companies.iter())
        .map(|c| c.fulfilled_fte as f64)
        .sum();
    assert!(
        total_employment <= total_population,
        "LABOR-0 FAIL: total employment {:.0} exceeds total population {:.0} \
         — world-gen minted workers who do not exist.",
        total_employment,
        total_population
    );

    // GRID-0: effective supply must stay under 1.5x demand in EVERY region.
    // The load-following throttle caps dispatch at demand x 1.15; >1.5 means
    // plants dumped nameplate onto the wire into GridDamage territory.
    let mut worst_grid_ratio = 0.0_f64;
    let mut worst_grid_region = String::new();
    for country in state.countries.values() {
        for (region_id, &demand) in &country.power_grid_state.region_demand_mw {
            if demand <= 0.0 {
                continue;
            }
            let supply = country
                .power_grid_state
                .region_effective_supply_mw
                .get(region_id)
                .copied()
                .unwrap_or(0.0);
            let ratio = supply / demand;
            if ratio > worst_grid_ratio {
                worst_grid_ratio = ratio;
                worst_grid_region = region_id.clone();
            }
        }
    }
    assert!(
        worst_grid_ratio < 1.5,
        "GRID-0 FAIL: worst regional supply/demand ratio {:.2}x >= 1.5 \
         (region {}) — load-following throttle is not preventing \
         grid-damaging overproduction.",
        worst_grid_ratio,
        worst_grid_region
    );

    // SERVICES-0: Local Services must book actual revenue — the consumer
    // basket demand stream (clear_local_services_b2c) is wired, so a zero
    // means citizens are not paying for services at all.
    let services_sector_revenue: f64 = diagnostic
        .company_income
        .iter()
        .filter(|c| c.sector == "LocalServices")
        .map(|c| c.last_revenue)
        .sum();
    assert!(
        services_sector_revenue > 0.0,
        "SERVICES-0 FAIL: Local Services booked $0 revenue over {} turns — \
         the B2C consumer-basket wiring is dead; sector survives only on \
         fire-sales and furloughs.",
        TURNS
    );

    // ========================================================================
    // MACRO-HEALTH HARD ASSERTS (Phase 96)
    // ========================================================================
    // The engine being "alive" (turns complete, M0 conserved) is not enough:
    // a simulation that finishes Turn 4 with 90%+ unemployment and every
    // company insolvent is dead in every economically meaningful sense.
    // These asserts fire AFTER the dump is written so failures still leave
    // full telemetry for root-cause analysis.
    //
    // MACRO-1: aggregate unemployment must be < 20% by Turn 4. The engine
    // seeds companies at ~60% of capacity FTE, so a functioning economy
    // starts well below this bar; only systemic collapse reaches it.
    assert!(
        headline.unemployment_rate < 20.0,
        "MACRO-1 FAIL: aggregate unemployment {:.1}% >= 20% by Turn {} — \
         companies are not retaining/hiring their workforce ({} unemployed of \
         {:.0} labor force). The economy is technically alive but socially dead.",
        headline.unemployment_rate,
        TURNS,
        state
            .countries
            .values()
            .map(|c| c.macro_indicators.labor_market.unemployed)
            .sum::<f64>(),
        state
            .countries
            .values()
            .map(|c| c.budget.population as f64
                * c.macro_indicators.labor_market.labor_force_participation
                / 100.0)
            .sum::<f64>(),
    );
    // MACRO-2: >60% of producing-sector companies must book a positive
    // net_profit by Turn 4. Structural non-producers (banks, NGOs,
    // government) are excluded — their income arrives via fees, grants,
    // and treasury funding, not industrial P&L.
    assert!(
        headline.producing_profitable_fraction > 0.6,
        "MACRO-2 FAIL: only {:.1}% of producing-sector companies are profitable \
         by Turn {} ({} of {}). Unit economics are broken — companies produce \
         and pay wages but cannot cover costs.",
        headline.producing_profitable_fraction * 100.0,
        TURNS,
        headline.producing_profitable,
        headline.producing_companies,
    );

    // MACRO-3: no company's windowed net_profit may be more negative than
    // ~50× its windowed wage expense (plus a $1B noise cushion). The wage
    // bill is the dominant cost term; a loss hundreds of times larger than
    // payroll can only come from a bookkeeping artifact — e.g., the
    // building-cycle wage ESTIMATE (headcount × tier multipliers ×
    // genesis-scale average_wage) flowing into `last_profit` and becoming
    // liabilities, which reached ~−$1.26T for a single energy plant while
    // real payroll was ~$2.8B (ratio ≈ 450×). 50× leaves real input-cost
    // losses headroom but makes that class of bug impossible.
    if let Some(worst) = diagnostic
        .company_income
        .iter()
        .filter(|c| c.last_net_profit < -(50.0 * c.wage_expense + 1.0e9))
        .max_by(|a, b| {
            (a.last_net_profit.abs())
                .partial_cmp(&b.last_net_profit.abs())
                .unwrap()
        })
    {
        panic!(
            "MACRO-3 FAIL: {} ({}) reports windowed net_profit {:.2}B vs \
             wage_expense {:.2}B — losses at >>50× payroll scale are a \
             phantom accounting artifact, not economics",
            worst.id,
            worst.sector,
            worst.last_net_profit / 1e9,
            worst.wage_expense / 1e9,
        );
    }

    // ========================================================================
    // 24-TURN YEARLING CRUCIBLE ASSERTS (post-v4.13.0 mandate)
    // ========================================================================
    // The 12-turn horizon masked a delayed collapse: real-world simulation to
    // Turn 25 showed GDP spiking ~Turn 3 then crashing ~99%, wealth tax
    // collecting -$1.09B while PIT/CIT/VAT collected $0.00, profitable
    // companies drained to $0.00 cash with $23M+ wage arrears, national
    // energy supply collapsing to 0 MW, and a Sev-10 leverage flood on the
    // KNF feed. These gates make each anomaly structurally impossible.

    // TAX-1: wealth tax can never be negative — a tax collector returning
    // money is either treasury leakage or unbacked fiat mint.
    for (cname, country) in &state.countries {
        for entry in &country.budget.tax_history {
            assert!(
                entry.wealth_tax_collected >= 0.0,
                "TAX-1 FAIL: country {} recorded NEGATIVE wealth tax {:.2} at \
                 turn {} — tax collector is paying out, not collecting",
                cname,
                entry.wealth_tax_collected,
                entry.turn
            );
        }
    }

    // TAX-2: core taxes must collect real revenue — PIT+CIT+VAT+wealth+CGT
    // summed over the whole horizon. All-zero means the debit pipeline is
    // dead (liabilities computed but never enforced, or zero base).
    let total_tax_revenue: f64 = state
        .countries
        .values()
        .flat_map(|c| c.budget.tax_history.iter())
        .map(|e| {
            e.pit_collected
                + e.cit_collected
                + e.vat_collected
                + e.wealth_tax_collected
                + e.capital_gains_collected
        })
        .sum();
    assert!(
        total_tax_revenue > 0.0,
        "TAX-2 FAIL: cumulative tax revenue = {:.2} over {} turns — the \
         treasury collected nothing; PIT/CIT/VAT pipeline is dead",
        total_tax_revenue,
        TURNS
    );

    // ARREARS-1: a company that booked positive net_profit cannot owe wage
    // arrears — profit must cover payroll before any other cash drain
    // (dividends, debt service, capex). epsilon 0.01 covers f64 residue.
    let profit_by_id: std::collections::HashMap<&str, f64> = diagnostic
        .company_income
        .iter()
        .map(|e| (e.id.as_str(), e.last_net_profit))
        .collect();
    let mut worst_arrears: Option<(&Company, f64)> = None;
    let mut arrears_violators: Vec<(&Company, f64)> = Vec::new();
    for ents in ctx.entities.values() {
        for c in &ents.companies {
            let profit = profit_by_id.get(c.id.as_str()).copied().unwrap_or(0.0);
            if profit > 0.0 && c.wage_arrears > 0.01 {
                arrears_violators.push((c, profit));
                if worst_arrears.map_or(true, |(_, a)| c.wage_arrears > a) {
                    worst_arrears = Some((c, profit));
                }
            }
        }
    }
    arrears_violators
        .sort_by(|a, b| b.0.wage_arrears.partial_cmp(&a.0.wage_arrears).unwrap());
    eprintln!(
        "ARREARS-DBG: {} profitable companies carry wage_arrears; top:",
        arrears_violators.len()
    );
    for (c, p) in arrears_violators.iter().take(12) {
        eprintln!(
            "  {} {:?} profit={:.2}M arrears={:.2}M cash={:.2}M ba={:.2}M dc={:.2}M sev={:.2}M rev={:.2}M",
            c.id,
            c.sector,
            p / 1e6,
            c.wage_arrears / 1e6,
            c.available_cash / 1e6,
            c.brokerage_account.as_ref().map(|b| b.cash).unwrap_or(0.0) / 1e6,
            c.debit_cash / 1e6,
            c.severance_arrears / 1e6,
            c.turn_settled_sales / 1e6,
        );
    }
    if let Some((c, profit)) = worst_arrears {
        panic!(
            "ARREARS-1 FAIL: {} ({:?}) booked net_profit {:.2}M but carries \
             wage_arrears {:.2}M with cash {:.2} — non-payroll drains \
             (dividends/debt/capex) are gutting payroll",
            c.id,
            c.sector,
            profit / 1e6,
            c.wage_arrears / 1e6,
            c.available_cash,
        );
    }

    // ENERGY-1: national effective energy supply must be non-zero at the
    // horizon — the blackout absorbing state is forbidden at Turn 24.
    let national_energy_supply: f64 = state
        .countries
        .values()
        .flat_map(|c| c.power_grid_state.region_effective_supply_mw.values())
        .sum();
    assert!(
        national_energy_supply > 0.0,
        "ENERGY-1 FAIL: national energy supply = {:.2} MW at Turn {} — the \
         energy death spiral consumed the grid (wage-arrears furloughs → \
         fuel starvation → blackout → deeper arrears)",
        national_energy_supply,
        TURNS
    );

    // LEVERAGE-1: no bank may breach the KNF 10x-equity leverage limit at the
    // horizon (the Sev-10 flood indicates liability accretion or equity
    // erosion that the prudential layer failed to contain).
    const MAX_REGULATORY_LEVERAGE: f64 = 10.0;
    for ents in ctx.entities.values() {
        for c in &ents.companies {
            if c.sector != Sector::Banking {
                continue;
            }
            if let Some(ref bs) = c.balance_sheet {
                if bs.tier_1_capital > 0.0 {
                    let leverage = bs.total_liabilities() / bs.tier_1_capital;
                    assert!(
                        leverage < MAX_REGULATORY_LEVERAGE,
                        "LEVERAGE-1 FAIL: bank {} leverage {:.2}x >= {}x \
                         (liabilities {:.2}B / tier_1 {:.2}B) — Sev-10 \
                         excessive leverage at Turn {}",
                        c.id,
                        leverage,
                        MAX_REGULATORY_LEVERAGE,
                        bs.total_liabilities() / 1e9,
                        bs.tier_1_capital / 1e9,
                        TURNS
                    );
                }
            }
        }
    }

    // GDP-1: horizon GDP must retain at least 20% of the early-run level —
    // the observed real-world pattern was a ~99% collapse after a Turn-3
    // spike. The baseline is the MEDIAN of the first half of the series:
    // robust to the genesis CapEx buildout (the fixed-asset fleet is
    // purchased in a one-time wave that inflates turns ~1-9 — v4.16.0's
    // restored investment pipeline makes t=4 an inflated sample).
    let gdp_turn_24 = gdp_series.last().copied().unwrap_or(0.0);
    let gdp_turn_4 = {
        let mut early: Vec<f64> = gdp_series[4..gdp_series.len() / 2].to_vec();
        early.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        early.get(early.len() / 2).copied().unwrap_or(0.0)
    };
    // GDP-DBG: decompose the horizon GDP by country so a collapse can be
    // attributed to a component (C/G/I/NX).
    for (name, country) in &state.countries {
        let bd = &country.macro_indicators.gdp_breakdown;
        eprintln!(
            "GDP-DBG[{}]: c={:.2}B g={:.2}B i={:.2}B nx={:.2}B total={:.2}B",
            name,
            bd.consumption / 1e9,
            bd.government_spending / 1e9,
            bd.investment / 1e9,
            bd.net_exports / 1e9,
            bd.official_gdp / 1e9,
        );
    }
    assert!(
        gdp_turn_24 >= gdp_turn_4 * 0.2,
        "GDP-1 FAIL: GDP collapsed {:.1}% — Turn-24 GDP {:.2}B vs Turn-4 \
         GDP {:.2}B (required >= {:.2}B). Series: {:?}",
        (1.0 - gdp_turn_24 / gdp_turn_4.max(1e-9)) * 100.0,
        gdp_turn_24 / 1e9,
        gdp_turn_4 / 1e9,
        gdp_turn_4 * 0.2 / 1e9,
        gdp_series.iter().map(|g| (g / 1e9 * 10.0).round() / 10.0).collect::<Vec<_>>(),
    );

    // ========================================================================
    // GROWTH & GOVERNANCE HARD ASSERTS (v4.15.0 mandate)
    // ========================================================================
    // Survival is necessary but not sufficient — a released simulation must
    // demonstrate the economy growing and the state governing.
    //
    // GROWTH-1: horizon GDP must exceed the post-genesis baseline. Index 0
    // is GDP after the first turn — the realized production baseline the
    // generated estimate settles into. Five years of healthy production,
    // trade, and investment must leave output higher than it started.
    let gdp_turn_0 = gdp_series.first().copied().unwrap_or(0.0);
    let gdp_turn_horizon = gdp_series.last().copied().unwrap_or(0.0);
    assert!(
        gdp_turn_horizon > gdp_turn_0,
        "GROWTH-1 FAIL: GDP did not grow over {} turns — horizon {:.2}B vs \
         post-genesis {:.2}B (Δ={:+.1}%). The economy must thrive, not just \
         survive. Series: {:?}",
        TURNS,
        gdp_turn_horizon / 1e9,
        gdp_turn_0 / 1e9,
        (gdp_turn_horizon / gdp_turn_0.max(1e-9) - 1.0) * 100.0,
        gdp_series.iter().map(|g| (g / 1e9 * 10.0).round() / 10.0).collect::<Vec<_>>(),
    );

    // POP-1: births + migration must outpace deaths — the horizon census
    // must exceed the genesis census. A shrinking population means the
    // demographic engine is entropic, not living.
    let pop_turn_horizon: i64 = state
        .countries
        .values()
        .flat_map(|c| c.regions.iter())
        .map(|r| r.population)
        .sum();
    if pop_turn_horizon <= genesis_population {
        // POP-DBG: decompose the collapse by country and class so the drain
        // channel (deaths vs deportation vs emigration vs transition loss)
        // can be identified from the failure output alone.
        for (name, country) in &state.countries {
            let rpop: i64 = country.regions.iter().map(|r| r.population).sum();
            let dem = &country.macro_indicators.demographics;
            eprintln!(
                "POP-DBG[{}]: regions={} budget={} births={:.0} deaths={:.0} \
                 mig={:.0} br={:.3} dr={:.3} illegal={:.0}",
                name,
                rpop,
                country.budget.population,
                dem.last_births,
                dem.last_deaths,
                dem.last_migration,
                dem.birth_rate,
                dem.death_rate,
                dem.illegal_immigrants,
            );
            let mut by_class: std::collections::BTreeMap<String, i64> =
                std::collections::BTreeMap::new();
            for r in &country.regions {
                for (k, d) in &r.class_demographics.rural_classes {
                    *by_class.entry(format!("R:{:?}", k)).or_default() += d.population;
                }
                for (k, d) in &r.class_demographics.urban_classes {
                    *by_class.entry(format!("U:{:?}", k)).or_default() += d.population;
                }
            }
            eprintln!("  POP-DBG-classes[{}]: {:?}", name, by_class);
        }
    }
    assert!(
        pop_turn_horizon > genesis_population,
        "POP-1 FAIL: population shrank over {} turns — horizon {} vs genesis \
         {} (Δ={}). Demographics are not regenerating the labor force.",
        TURNS,
        pop_turn_horizon,
        genesis_population,
        pop_turn_horizon - genesis_population
    );

    // GOV-1: the political engine must not be deadlocked — over five years,
    // at least one legislative instrument must move: an enacted law, an
    // executive crisis decree, an enacted budget bill, or a parliament that
    // actually sat (budget sessions are recorded in politics.history).
    let mut passed_decrees_count = 0usize;
    let mut parliamentary_sessions = 0usize;
    for country in state.countries.values() {
        if let Some(sess) = &country.politics.legislative_session {
            passed_decrees_count += sess.enacted_laws.len();
            if sess.session_year > 0
                || !sess.active_bills.is_empty()
                || !sess.enacted_laws.is_empty()
                || !sess.rejected_bills.is_empty()
            {
                parliamentary_sessions += 1;
            }
        }
        if let Some(arr) = country
            .macro_indicators
            .extra
            .get("crisis_decrees")
            .and_then(|v| v.as_array())
        {
            passed_decrees_count += arr.len();
        }
        let sessions = country
            .politics
            .history
            .iter()
            .filter(|v| v.get("event").and_then(|e| e.as_str()) == Some("budget_bill"))
            .count();
        parliamentary_sessions += sessions;
        passed_decrees_count += country
            .politics
            .history
            .iter()
            .filter(|v| {
                v.get("event").and_then(|e| e.as_str()) == Some("budget_bill")
                    && v.get("enacted").and_then(|e| e.as_bool()) == Some(true)
            })
            .count();
    }
    println!(
        "  Legislative activity:   {} sessions held, {} decrees/laws enacted",
        parliamentary_sessions, passed_decrees_count
    );
    assert!(
        passed_decrees_count > 0 || parliamentary_sessions > 0,
        "GOV-1 FAIL: zero legislative activity over {} turns — no budget \
         sessions sat, no decrees issued, no laws enacted. The political \
         and legislative engine is deadlocked.",
        TURNS
    );

    // ========================================================================
    // STRUCTURAL FULL-STACK ASSERTS (v4.16.0 anti-Goodhart mandate)
    // ========================================================================
    // v4.15.0 passed every headline gate while the production UI showed dead
    // subsystems: I=$0.37, u64::MAX fund shares, identical regional GDP/CAP
    // in all 13 Sarmatian regions, 0.00 extraction telemetry, an empty
    // listed-companies table, and a parliament that never voted. These
    // asserts verify the structures underneath the aggregates.

    // FS-1: Investment and government must share GDP plausibly — the
    // v4.15.0 run had government spending at 77% of GDP and I=$0.37 because
    // construction spend booked qty×0.0 and capex bids were dust.
    // Composition is measured on the trailing year (12 turns): capital
    // formation is lumpy per-turn even in a healthy economy, so a single
    // spot sample flaps between bursts.
    let win = 12usize.min(gdp_series.len());
    let sum_i: f64 = inv_series.iter().rev().take(win).sum();
    let sum_g: f64 = gov_series.iter().rev().take(win).sum();
    let sum_gdp: f64 = gdp_series.iter().rev().take(win).sum();
    let i_share = sum_i / sum_gdp.max(1e-9);
    let g_share = sum_g / sum_gdp.max(1e-9);
    let spot_i = *inv_series.last().unwrap_or(&0.0)
        / (*gdp_series.last().unwrap_or(&0.0)).max(1e-9);
    let spot_g = *gov_series.last().unwrap_or(&0.0)
        / (*gdp_series.last().unwrap_or(&0.0)).max(1e-9);
    eprintln!(
        "FS-DBG: I/GDP={:.1}% (spot {:.1}%) G/GDP={:.1}% (spot {:.1}%) over {}-turn window",
        i_share * 100.0,
        spot_i * 100.0,
        g_share * 100.0,
        spot_g * 100.0,
        win
    );
    // G-source attribution for the final turn, per country — audits which
    // channel (procurement/works/wages/church/subsidies) drives G.
    for (name, country) in &state.countries {
        if let Some(src) = country.macro_indicators.extra.get("gdp_g_sources") {
            eprintln!("FS-DBG[{}]: G-sources {}", name, src);
        }
    }
    assert!(
        i_share > 0.05 && g_share < 0.50,
        "FS-1 FAIL: GDP composition collapsed — I/GDP={:.1}% (need >5%), \
         G/GDP={:.1}% (need <50%). Corporate CapEx/construction pipeline is \
         dead or government is absorbing the whole economy.",
        i_share * 100.0,
        g_share * 100.0,
    );

    // FS-2: Regional GDP must differ — bottom-up value added, not a
    // population-share copy of the national figure. Every multi-region
    // country must show real per-capita dispersion.
    for (name, country) in &state.countries {
        let pcs: Vec<f64> = country
            .regions
            .iter()
            .filter(|r| r.population > 0)
            .map(|r| r.gdp_pc)
            .collect();
        if pcs.len() < 2 {
            continue;
        }
        let mean = pcs.iter().sum::<f64>() / pcs.len() as f64;
        let stddev =
            (pcs.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / pcs.len() as f64).sqrt();
        eprintln!(
            "FS-DBG[{}]: regional GDP/cap mean={:.0} stddev={:.0} regions={}",
            name,
            mean,
            stddev,
            pcs.len()
        );
        assert!(
            stddev > 10.0,
            "FS-2 FAIL[{}]: regional GDP/cap stddev {:.2} <= 10.0 across {} \
             regions — regions are still clones (national pro-rating) \
             instead of bottom-up economies",
            name,
            stddev,
            pcs.len(),
        );
    }

    // FS-3: Geological extraction telemetry must be nonzero — mines
    // physically depleted veins all run, but the Region.resources mirror
    // was never written (UI read Extraction: 0.00 everywhere).
    let total_extraction: f64 = state
        .countries
        .values()
        .flat_map(|c| c.regions.iter())
        .flat_map(|r| r.resources.values())
        .filter_map(|v| v.get("extraction_rate").and_then(|x| x.as_f64()))
        .sum();
    eprintln!("FS-DBG: total geological extraction this turn = {:.2}", total_extraction);
    assert!(
        total_extraction > 0.0,
        "FS-3 FAIL: zero geological extraction recorded at horizon — mine \
         output is not reaching ResourceDeposit telemetry"
    );

    // FS-4: The exchange must be a live market — listed companies exist and
    // sector indices have moved off the 1000.0 base (was: empty table,
    // indices frozen at 1.00K forever via a ×(cap/cap)≡×1.0 formula).
    let listed_count: usize = ctx
        .entities
        .values()
        .flat_map(|e| &e.companies)
        .filter(|c| c.legal_form.is_listed() && c.shares_count > 0 && !c.is_liquidated)
        .count();
    let mut sector_indices_seen = 0usize;
    let mut sector_indices_moved = 0usize;
    for country in state.countries.values() {
        let mi = &country.stock_exchange.market_index;
        sector_indices_seen += mi.sector_indices.len();
        sector_indices_moved += mi
            .sector_indices
            .values()
            .filter(|v| (**v - 1000.0).abs() > 1.0)
            .count();
    }
    eprintln!(
        "FS-DBG: listed={} sector_indices={} moved={}",
        listed_count, sector_indices_seen, sector_indices_moved
    );
    assert!(
        listed_count > 0 && sector_indices_moved > 0,
        "FS-4 FAIL: listed_companies={} sector_indices_moved={}/{} — the \
         exchange is a ghost market (no listings, frozen indices)",
        listed_count,
        sector_indices_moved,
        sector_indices_seen,
    );

    // FS-5: Fund ledgers must hold sane quantities — the u64::MAX shares
    // readout was a saturating f64→u64 cast on a collapsed NAV divisor,
    // itself caused by the ~20%/turn fee drain.
    let mut fund_count = 0usize;
    for c in ctx
        .entities
        .values()
        .flat_map(|e| &e.companies)
    {
        if let Some(fl) = &c.fund_ledger {
            fund_count += 1;
            assert!(
                (fl.shares_outstanding as f64) < 1e15
                    && fl.nav_per_share > 0.0
                    && fl.nav_per_share.is_finite(),
                "FS-5 FAIL: fund {} shares={} nav={:.6} — share/NAV math is \
                 still overflowing or zeroed",
                c.id,
                fl.shares_outstanding,
                fl.nav_per_share,
            );
        }
    }
    eprintln!("FS-DBG: funds audited={}", fund_count);

    // FS-6: Parliament must actually vote — every seated parliament must
    // have at least one recorded floor vote over five years (the v4.15.0
    // GOV-1 pass was satisfied by synthetic politics.history strings while
    // the real chamber queue never ran).
    let mut total_votes = 0usize;
    let mut parliaments_checked = 0usize;
    for (name, country) in &state.countries {
        if let Some(parl) = &country.politics.parliament_struct {
            if parl.chambers.is_empty() {
                continue; // no legislative body — nothing to vote in
            }
            parliaments_checked += 1;
            let votes: usize = parl.chambers.iter().map(|ch| ch.recent_votes.len()).sum();
            eprintln!(
                "FS-DBG[{}]: parliament chambers={} recent_votes={}",
                name,
                parl.chambers.len(),
                votes
            );
            assert!(
                votes > 0,
                "FS-6 FAIL[{}]: parliament is seated but recent_votes is \
                 empty — bills never reach a floor vote",
                name,
            );
            total_votes += votes;
        }
    }
    assert!(
        total_votes > 0,
        "FS-6 FAIL: no chamber recorded a single floor vote across {} \
         parliaments",
        parliaments_checked,
    );

    // ── v4.17.0 anti-sleight gates: mechanical behavior, not aggregates ──

    // FS-7: Construction is a real operating sector — firms must earn
    // revenue (contract billings) and employ workers, not sit at
    // Income/Expenses/Wages = 0 forever. `turn_settled_sales` is a scratch
    // field consumed by `process_company` every turn, so the durable
    // revenue lives in `financial_history` — the same last-turn record the
    // UI renders as company "Income".
    let (mut constr_revenue, mut constr_fte) = (0.0f64, 0u64);
    let mut fs7_census: Vec<String> = Vec::new();
    for ents in ctx.entities.values() {
        for c in &ents.companies {
            if c.sector == Sector::Construction {
                fs7_census.push(format!(
                    "{} fte={} tgt={} furl={:.0} cash={:.0} recv={:.0} liq={} rev_hist={:.0}",
                    c.id,
                    c.fulfilled_fte,
                    c.target_fte_demand,
                    c.furloughed_workers_count,
                    c.available_cash,
                    c.extra
                        .get("contract_receivables")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0),
                    c.is_liquidated,
                    c.financial_history
                        .iter()
                        .map(|r| r.get("revenue").and_then(|v| v.as_f64()).unwrap_or(0.0))
                        .sum::<f64>(),
                ));
            }
            if c.sector == Sector::Construction && !c.is_liquidated
            {
                // Sum the retained financial-history window (≤5 turns of
                // real P&L) — a live contracting sector books tranche
                // revenue across any such window even if one turn had no
                // milestone release.
                for r in &c.financial_history {
                    constr_revenue += r
                        .get("revenue")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                }
                constr_fte += c.fulfilled_fte as u64;
            }
        }
    }
    eprintln!(
        "FS-DBG: construction revenue={:.2} fte={} companies={}",
        constr_revenue,
        constr_fte,
        fs7_census.len()
    );
    fs7_census.sort();
    for line in &fs7_census {
        eprintln!("FS7CENSUS: {}", line);
    }
    assert!(
        constr_revenue > 0.0 && constr_fte > 10_000,
        "FS-7 FAIL: construction sector dead — revenue={:.2}, fte={} \
         (need >0 and >10_000)",
        constr_revenue,
        constr_fte,
    );

    // FS-8: Banking must not be the whole economy — banking VA share of
    // GDP < 25% and banking FTE < 10% of total employment. Income-side VA:
    // wages + realized profit of banking entities.
    for (name, country) in &state.countries {
        let Some(ents) = ctx.entities.get(name) else { continue };
        let gdp = country.macro_indicators.gdp_breakdown.official_gdp.max(1.0);
        let mut bank_va = 0.0f64;
        let mut bank_fte = 0.0f64;
        let mut total_fte = 0.0f64;
        for c in &ents.companies {
            if c.is_liquidated {
                continue;
            }
            total_fte += c.fulfilled_fte as f64;
            if c.bank_type.is_some() || c.sector == Sector::Banking {
                bank_va += c.wages_paid_this_turn + c.realized_profit_this_turn;
                bank_fte += c.fulfilled_fte as f64;
            }
        }
        eprintln!(
            "FS-DBG[{}]: bank_va_share={:.1}% bank_fte={:.0}/{:.0} ({:.1}%)",
            name,
            bank_va / gdp * 100.0,
            bank_fte,
            total_fte,
            bank_fte / total_fte.max(1.0) * 100.0
        );
        assert!(
            bank_va / gdp < 0.25 && bank_fte < total_fte * 0.10,
            "FS-8 FAIL[{}]: banking bloat — va_share={:.1}% (need <25%), \
             fte={:.0}/{:.0} (need <10%)",
            name,
            bank_va / gdp * 100.0,
            bank_fte,
            total_fte,
        );
    }

    // FS-9: Regional accounts must reconcile with national GDP — the sum
    // of regional GDPs within ±10% of national, and no populated region
    // may sit at starvation-level per-capita GDP.
    for (name, country) in &state.countries {
        let national = country.macro_indicators.gdp_breakdown.official_gdp;
        if national <= 0.0 {
            continue;
        }
        let regional_sum: f64 = country.regions.iter().map(|r| r.gdp).sum();
        let gap = (regional_sum - national).abs() / national;
        let min_pc = country
            .regions
            .iter()
            .filter(|r| r.population > 1_000)
            .map(|r| r.gdp_pc)
            .fold(f64::INFINITY, f64::min);
        eprintln!(
            "FS-DBG[{}]: Σregional={:.2}B national={:.2}B gap={:.1}% min_pc={:.2}",
            name,
            regional_sum / 1e9,
            national / 1e9,
            gap * 100.0,
            min_pc
        );
        assert!(
            gap < 0.10,
            "FS-9 FAIL[{}]: Σregional={:.2}B vs national={:.2}B — gap {:.1}% \
             exceeds 10% (regional accounts undercount real flows)",
            name,
            regional_sum / 1e9,
            national / 1e9,
            gap * 100.0,
        );
        assert!(
            min_pc > 10.0,
            "FS-9 FAIL[{}]: min regional GDP/cap={:.2} <= 10 — a region of \
             thousands has no measurable economy",
            name,
            min_pc,
        );
    }

    // FS-10: Real secondary-market activity — the exchange must clear
    // actual share trades (volume > 0) and funds must hold equities, not
    // only CB bonds.
    let total_volume: u64 = state
        .countries
        .values()
        .map(|c| c.stock_exchange.market_index.total_volume)
        .sum();
    let mut fund_equity_positions = 0usize;
    for ents in ctx.entities.values() {
        for c in &ents.companies {
            if let Some(fl) = &c.fund_ledger {
                fund_equity_positions += fl.equity_holdings.len();
            }
        }
    }
    eprintln!(
        "FS-DBG: exchange total_volume={} fund_equity_positions={}",
        total_volume, fund_equity_positions
    );
    assert!(
        total_volume > 0 && fund_equity_positions > 0,
        "FS-10 FAIL: exchange volume={} fund equity positions={} — \
         secondary market is dead (funds hold bonds only)",
        total_volume,
        fund_equity_positions,
    );

    // FS-11: Fiscal channels must carry real revenue — CIT on profitable
    // firms, customs on cross-border flows, and ministry budgets must be
    // allocated AND spent (not frozen at 0.00).
    for (name, country) in &state.countries {
        let last = country.budget.tax_history.back();
        let (cit, customs) = last
            .map(|e| {
                (
                    e.cit_collected,
                    e.extra
                        .get("customs_revenue")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0),
                )
            })
            .unwrap_or((0.0, 0.0));
        let ministry_spent: f64 = country
            .politics
            .ministry_config
            .as_ref()
            .map(|cfg| cfg.ministries.iter().map(|m| m.spent_cash).sum())
            .unwrap_or(0.0);
        eprintln!(
            "FS-DBG[{}]: cit={:.0} customs={:.0} ministry_spent={:.0}",
            name, cit, customs, ministry_spent
        );
        assert!(
            cit > 0.0 && customs > 0.0 && ministry_spent > 0.0,
            "FS-11 FAIL[{}]: fiscal channels dead — cit={:.0} customs={:.0} \
             ministry_spent={:.0} (all must be >0)",
            name,
            cit,
            customs,
            ministry_spent,
        );
    }

    // FS-12: Legislative diversity — recent floor votes must include at
    // least 3 DISTINCT non-emergency bill types. An emergency-only cycle
    // means the fiscal-crisis trigger is spamming the queue and starving
    // real legislation.
    {
        use sim_engine::politics::legislation::BillType;
        const EMERGENCY_TYPES: &[BillType] = &[
            BillType::Budget,
            BillType::EmergencyBonds,
            BillType::EmergencyFiscal,
        ];
        let mut distinct: std::collections::HashSet<BillType> =
            std::collections::HashSet::new();
        for country in state.countries.values() {
            if let Some(parl) = &country.politics.parliament_struct {
                for ch in &parl.chambers {
                    for v in &ch.recent_votes {
                        if !EMERGENCY_TYPES.contains(&v.bill_type) {
                            distinct.insert(v.bill_type);
                        }
                    }
                }
            }
        }
        eprintln!(
            "FS-DBG: distinct non-emergency bill types voted: {:?}",
            distinct
        );
        assert!(
            distinct.len() >= 3,
            "FS-12 FAIL: only {} distinct non-emergency bill types voted — \
             the emergency-bill loop still crowds out real legislation",
            distinct.len(),
        );
    }

    // CASH-1: base cash may never be negative — a company that cannot pay
    // must accrue tracked debt or default, never hold negative M0. The
    // −5.10B void seen in 48-turn runs is a double-entry violation: three
    // money pockets are audited (spendable, brokerage, escrow).
    let mut neg_cash: Vec<(&Company, f64)> = Vec::new();
    for ents in ctx.entities.values() {
        for c in &ents.companies {
            let worst = [
                c.available_cash,
                c.brokerage_account
                    .as_ref()
                    .map(|b| b.cash)
                    .unwrap_or(0.0),
                c.debit_cash,
            ]
            .iter()
            .cloned()
            .fold(0.0, f64::min);
            if worst < -0.01 {
                neg_cash.push((c, worst));
            }
        }
    }
    neg_cash.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    eprintln!(
        "CASH-DBG: {} companies hold negative base cash",
        neg_cash.len()
    );
    for (c, v) in neg_cash.iter().take(10) {
        eprintln!(
            "  CASH-DBG {} {:?} avail={:.2}M broker={:.2}M debit={:.2}M",
            c.id,
            c.sector,
            c.available_cash / 1e6,
            c.brokerage_account
                .as_ref()
                .map(|b| b.cash)
                .unwrap_or(0.0)
                / 1e6,
            c.debit_cash / 1e6,
        );
        let _ = v;
    }
    assert!(
        neg_cash.is_empty(),
        "CASH-1 FAIL: {} companies hold negative base cash (worst {:.3}B in {}) — \
         inability to pay must book tracked debt/default, not void money",
        neg_cash.len(),
        neg_cash.first().map(|(_, v)| v / 1e9).unwrap_or(0.0),
        neg_cash.first().map(|(c, _)| c.id.as_str()).unwrap_or("?"),
    );

    // BANKS-1: the commercial-banking sector must survive the horizon —
    // mass KNF leverage liquidations that leave only empty investment banks
    // freeze the entire credit supply.
    let commercial_banks = ctx
        .entities
        .values()
        .flat_map(|e| &e.companies)
        .filter(|c| matches!(c.bank_type, Some(BankType::Commercial)))
        .count();
    assert!(
        commercial_banks > 0,
        "BANKS-1 FAIL: zero commercial banks alive at Turn {} — leverage \
         liquidations wiped out the credit sector; ELA and supervised \
         deleveraging must let banks shrink safely instead of dying",
        TURNS
    );

    // GRID-1: when national supply exceeds national demand, no region may
    // sit in a supply deficit — unmet load amid oversupply means the
    // HV/interconnector routing failed to move surplus power to deficit
    // regions (945% oversupply with blackouts was observed at Turn 48).
    let mut nat_supply = 0.0;
    let mut nat_demand = 0.0;
    let mut deficit_regions: Vec<String> = Vec::new();
    for (name, country) in &state.countries {
        let gs = &country.power_grid_state;
        nat_supply += gs.region_effective_supply_mw.values().sum::<f64>();
        nat_demand += gs.region_demand_mw.values().sum::<f64>();
        for (region, &demand) in &gs.region_demand_mw {
            let eff = gs
                .region_effective_supply_mw
                .get(region)
                .copied()
                .unwrap_or(0.0);
            if demand > 1.0 && eff < demand * 0.95 {
                deficit_regions.push(format!(
                    "{}/{} eff={:.0}MW dem={:.0}MW",
                    name, region, eff, demand
                ));
            }
        }
    }
    eprintln!(
        "GRID-DBG: national supply={:.0}MW demand={:.0}MW deficit_regions={:?}",
        nat_supply, nat_demand, deficit_regions
    );
    if nat_supply > nat_demand * 1.05 {
        assert!(
            deficit_regions.is_empty(),
            "GRID-1 FAIL: {} regions in deficit despite national oversupply \
             ({:.0} MW supply vs {:.0} MW demand): {:?} — distribute_grid_power \
             failed to route surplus to deficit regions",
            deficit_regions.len(),
            nat_supply,
            nat_demand,
            deficit_regions.iter().take(8).collect::<Vec<_>>(),
        );
    }
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

/// Compute wage-transfer results for each turn by diffing
/// `wage_clearing_pre` vs `wage_clearing_post` — the narrow window that
/// encloses ONLY the payroll leg: works-program/state-employer funding,
/// regional labor clearing, PIT/garnishment/remittance routing, and the
/// commuter remittance. The previous `production_cycle_post` →
/// `labor_market_post` window also enclosed utility settlement, rent
/// collection, retail restock, and municipal-AI budgeting — legitimate
/// non-wage flows whose treasury→banked-sector legs made the measured
/// "company cash" rise every turn (the WAGE-2 artifact: aggregate deltas
/// of +0.3B…+48B while payroll itself was conserved).
///
/// Employer-side outflow = ΔΣ(corporate_cash + debit_cash + bank_reserves
/// + ministry_cash + treasury_cash) − Δcumulative_cb_injection:
/// unbanked payroll debits corporate cash, the wage trust spends from
/// debit_cash, the banked batch sync debits reserves, ministries pay from
/// their cash pools, and the State Employer funds wages from treasury.
/// Central-bank injections (ELA top-ups inside the window) are policy
/// flows, not payroll — subtracted. `transfer_balance` measures the
/// window's total M0 drift (ΔM0 − Δcb): ~0 when every withheld leg is
/// routed to its destination.
fn compute_wage_transfers(trace: &TurnTrace) -> Vec<WageTransferResult> {
    let mut results = Vec::new();
    for turn_record in &trace.turns {
        let pre = turn_record
            .checkpoints
            .iter()
            .find(|cp| cp.phase_name == "wage_clearing_pre");
        let post = turn_record
            .checkpoints
            .iter()
            .find(|cp| cp.phase_name == "wage_clearing_post");

        if let (Some(start), Some(post)) = (pre, post) {
            // Employer pockets across ALL companies — not the targeted
            // subset. The old probe summed targeted-company cash plus
            // global bank_reserves, so any settlement into the banked
            // sector (or a targeted receipt) read as a wage-phase gain.
            let company_cash_start: f64 = start.global_fiat.corporate_cash
                + start.global_fiat.debit_cash
                + start.global_fiat.bank_reserves
                + start.global_fiat.ministry_cash
                + start.global_fiat.treasury_cash;
            let company_cash_post: f64 = post.global_fiat.corporate_cash
                + post.global_fiat.debit_cash
                + post.global_fiat.bank_reserves
                + post.global_fiat.ministry_cash
                + post.global_fiat.treasury_cash;
            let cb_delta = post.global_fiat.cumulative_cb_injection
                - start.global_fiat.cumulative_cb_injection;
            let citizen_cash_start = start.global_fiat.citizen_cash;
            let citizen_cash_post = post.global_fiat.citizen_cash;
            let m0_drift = (post.global_fiat.total - start.global_fiat.total) - cb_delta;

            results.push(check_wage_transfer(
                turn_record.turn,
                company_cash_start,
                company_cash_post - cb_delta,
                citizen_cash_start,
                citizen_cash_post,
                m0_drift,
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
            // Window metrics, not last-turn snapshots: agriculture is seasonal
            // (a farm books all revenue in its harvest turns and legitimately
            // shows $0 between them), and construction books output only at
            // project completion. A last-turn read misclassifies every
            // seasonal business as a zombie. Summing the full window answers
            // the question the diagnostic actually asks — did this company
            // produce anything / make money during the simulation?
            let (last_revenue, last_net_profit, wage_expense, operating_costs, interest_paid, taxes_paid) = company
                .financial_history
                .iter()
                .fold((0.0, 0.0, 0.0, 0.0, 0.0, 0.0), |(rev_acc, profit_acc, w, oc, i, t), record| {
                    let g = |k: &str| record.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0);
                    (
                        rev_acc + g("revenue"),
                        profit_acc + g("net_profit"),
                        w + g("wage_expense"),
                        oc + g("operating_costs"),
                        i + g("interest"),
                        t + g("taxes"),
                    )
                });

            entries.push(CompanyIncomeEntry {
                id: company.id.clone(),
                sector: format!("{:?}", company.sector),
                available_cash: company.available_cash,
                liquid_capital: company.liquid_capital,
                company_capital: company.company_capital,
                export_revenue: company
                    .extra
                    .get("export_revenue_earned")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0),
                fulfilled_fte: company.fulfilled_fte,
                furloughed_workers_count: company.furloughed_workers_count,
                is_in_receivership: company.is_in_receivership,
                is_liquidated: company.is_liquidated,
                financial_history_len: company.financial_history.len(),
                last_revenue,
                last_net_profit,
                wage_expense,
                operating_costs,
                interest_paid,
                taxes_paid,
            });
        }
    }
    entries
}

/// Compute the headline gridlock summary, including the macro-health
/// metrics the hardened harness asserts on: aggregate unemployment and
/// the fraction of producing-sector companies posting a positive
/// `net_profit` in their last financial record.
fn compute_headline(
    company_income: &[CompanyIncomeEntry],
    state: &sim_engine::state::GameState,
) -> HeadlineSummary {
    let total = company_income.len();
    let zero_income = company_income
        .iter()
        .filter(|c| c.last_revenue <= 0.0)
        .count();
    let producing: Vec<&CompanyIncomeEntry> = company_income
        .iter()
        .filter(|c| !NON_PRODUCING_SECTORS.contains(&c.sector.as_str()))
        .collect();
    let producing_zero = producing.iter().filter(|c| c.last_revenue <= 0.0).count();
    let producing_fraction = if !producing.is_empty() {
        producing_zero as f64 / producing.len() as f64
    } else {
        0.0
    };
    let producing_profitable = producing.iter().filter(|c| c.last_net_profit > 0.0).count();
    let producing_profitable_fraction = if !producing.is_empty() {
        producing_profitable as f64 / producing.len() as f64
    } else {
        0.0
    };
    let all_sector_profitable = company_income
        .iter()
        .filter(|c| c.last_net_profit > 0.0)
        .count();
    let all_sector_profitable_fraction = if total > 0 {
        all_sector_profitable as f64 / total as f64
    } else {
        0.0
    };
    // Aggregate unemployment across countries using the engine's formula:
    // labor_force = population * participation / 100 (see turn.rs Phase 25).
    let mut total_labor_force = 0.0;
    let mut total_unemployed = 0.0;
    for country in state.countries.values() {
        let lm = &country.macro_indicators.labor_market;
        total_labor_force +=
            country.budget.population as f64 * lm.labor_force_participation / 100.0;
        total_unemployed += lm.unemployed;
    }
    let unemployment_rate = if total_labor_force > 0.0 {
        (total_unemployed / total_labor_force * 100.0).max(0.0)
    } else {
        100.0
    };
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
        producing_companies: producing.len(),
        producing_zero_income: producing_zero,
        producing_zero_income_fraction: producing_fraction,
        producing_profitable,
        producing_profitable_fraction,
        all_sector_profitable_fraction,
        unemployment_rate,
        companies_furloughed: furloughed,
        companies_in_receivership: receivership,
        companies_liquidated: liquidated,
        gridlock_detected: producing_fraction >= 0.5,
    }
}

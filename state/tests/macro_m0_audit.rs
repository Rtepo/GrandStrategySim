//! Macro M0 Audit — formal balance-sheet conservation test.
//!
//! Verifies that M0 base money is conserved across turn boundaries:
//!   Δfiat == Δcb_injected + Δtreasury_external_financing
//!
//! Also verifies bank balance-sheet identity:
//!   assets == liabilities + equity (for every bank, every checkpoint)
//!
//! This test is the manager's merge gate. It does NOT require the
//! `diagnostic` feature flag — it uses inline computation.
//!
//! # M0 Equation
//!
//! M0 base money = treasury_cash + bank_reserves + offshore_capital
//!                 + see_charity_pool + ministry_cash + bfg_reserves
//!                 + sobk_pool
//!
//! The ONLY valid ways M0 can change:
//! 1. Central Bank injection (Lombard loans, OMO, emergency lending)
//!    → tracked by `central_bank.liquidity_injected`
//! 2. Treasury external financing (foreign bond purchases, direct CB
//!    deficit monetization) → tracked by
//!    `budget.external_financing_injected`
//!
//! Any M0 change beyond these two channels is a conservation violation.

use sim_engine::engine::turn::run_turn_inner;
use sim_engine::engine::turn_context::InMemoryTurnContext;
use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
use sim_engine::entities::Company;
use sim_engine::registries::Registries;
use sim_engine::state::Country;
use tempfile::TempDir;

/// Compute M0 base money from a Country, its companies, and the global market (inline).
///
/// Sums: treasury liquid_reserves + regional/megaregion government reserves
/// + bank reserves_at_central_bank + cb_deposit_facility_balance
/// + offshore_capital + see_charity_pool + ministry_cash
/// + BFG reserves + SOBK pool + pending_defense_orders value.
///
/// NOTE: citizen savings are EXCLUDED (physical cash in circulation,
/// not central bank reserves — see diagnostic.rs:413-431).
fn compute_m0(
    country: &Country,
    companies: &[Company],
    market: &sim_engine::economy::market::GlobalMarket,
) -> f64 {
    let mut total: f64 = 0.0;

    // Treasury liquid reserves
    total += country.budget.liquid_reserves;

    // Regional and megaregion government budgets
    for region in &country.regions {
        if let Some(ref gov) = region.governance {
            total += gov.budget.liquid_reserves;
        }
    }
    for megaregion in &country.megaregions {
        if let Some(ref gov) = megaregion.governance {
            total += gov.budget.liquid_reserves;
        }
    }

    // Bank reserves at central bank + deposit facility balances
    for company in companies {
        if company.bank_type.is_some() {
            if let Some(ref bs) = company.balance_sheet {
                total += bs.reserves_at_central_bank;
                total += bs.cb_deposit_facility_balance;
            }
        }
    }

    // Offshore capital (money fled to tax havens — still M0)
    total += market.offshore_capital;

    // See charity pool
    total += market.apostolic_see_ledger.global_charity_pool;

    // Ministry cash pockets (fiat debited from treasury, held by ministries)
    if let Some(ref config) = country.politics.ministry_config {
        for ministry in &config.ministries {
            total += ministry.ministry_cash;
        }
    }

    // BFG (Bank Guarantee Fund) reserves
    total += country.bfg_fund.reserves;

    // SOBK (Voluntary Savings Scheme) pool
    total += country.sobk_scheme.pool;

    // Pending defense orders (encumbered treasury cash — still M0)
    for bid in &country.pending_defense_orders {
        total += bid.quantity * bid.limit_price;
    }

    total
}

/// Compute total central bank liquidity injected.
fn compute_cb_injected(country: &Country) -> f64 {
    country.central_bank.liquidity_injected
}

/// Compute total treasury external financing injected.
fn compute_treasury_external(country: &Country) -> f64 {
    country.budget.external_financing_injected
}

/// Verify bank balance-sheet identity: assets == liabilities + equity.
fn verify_bank_balance_sheets(companies: &[Company]) -> Result<(), String> {
    for company in companies {
        if company.bank_type.is_some() {
            if let Some(ref bs) = company.balance_sheet {
                let assets = bs.reserves_at_central_bank
                    + bs.loans_issued
                    + bs.securities
                    + bs.cb_lombard_loans
                    + bs.mbs_holdings;
                let liabilities = bs.deposits + bs.interbank_borrowing;
                let equity = bs.equity_capital;
                let drift = assets - liabilities - equity;
                if drift.abs() > 1e-6 {
                    return Err(format!(
                        "BANK BALANCE SHEET IMBALANCE: bank={}, assets={:.2}, liab+equity={:.2}, drift={:.6}",
                        company.id, assets, liabilities + equity, drift
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Extract the first (primary) country from a GameState.
fn get_primary_country(
    state: &sim_engine::state::GameState,
) -> &Country {
    state
        .countries
        .values()
        .next()
        .expect("expected at least one country")
}

/// Extract companies for the primary country from the turn context.
fn get_primary_companies(ctx: &InMemoryTurnContext, state: &sim_engine::state::GameState) -> Vec<Company> {
    let country_name = state.countries.keys().next().expect("expected country");
    ctx.entities
        .get(country_name)
        .map(|e| e.companies.clone())
        .unwrap_or_default()
}

/// Run a 1-turn simulation and verify M0 conservation.
#[test]
fn test_m0_conservation_single_turn() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let data_dir = tmp.path();

    let registries = Registries::native_only();
    let options = GenerateOptions {
        country_count: 1,
        start_year: StartYear::Y1900,
    };

    let GeneratedWorld {
        state: mut state,
        ..
    } = generate_world(data_dir, options, &registries)
        .expect("world generation failed");

    let mut ctx = InMemoryTurnContext::load_from_disk(data_dir, &mut state)
        .expect("failed to load turn context");

    // Compute M0 BEFORE the turn
    let country_before = get_primary_country(&state).clone();
    let companies_before = get_primary_companies(&ctx, &state);
    let fiat_before = compute_m0(&country_before, &companies_before, &ctx.market);
    let cb_injected_before = compute_cb_injected(&country_before);
    let treasury_ext_before = compute_treasury_external(&country_before);

    // Run exactly 1 turn
    let mut probe = sim_engine::engine::diagnostic::NoopProbe;
    let result = run_turn_inner(&mut state, &registries, &mut ctx, &mut probe);
    assert!(result.is_ok(), "Turn 1 failed: {:?}", result.err());

    // Compute M0 AFTER the turn
    let country_after = get_primary_country(&state).clone();
    let companies_after = get_primary_companies(&ctx, &state);
    let fiat_after = compute_m0(&country_after, &companies_after, &ctx.market);
    let cb_injected_after = compute_cb_injected(&country_after);
    let treasury_ext_after = compute_treasury_external(&country_after);

    // CORRECTED conservation: Δfiat == Δcb_injected + Δtreasury_external
    let fiat_delta = fiat_after - fiat_before;
    let cb_delta = cb_injected_after - cb_injected_before;
    let treasury_ext_delta = treasury_ext_after - treasury_ext_before;
    let allowed_expansion = cb_delta + treasury_ext_delta;
    let drift = fiat_delta - allowed_expansion;

    assert!(
        drift.abs() < 1e-6,
        "M0 CONSERVATION VIOLATION: Δfiat={:.6}, Δcb={:.6}, Δtreasury_ext={:.6}, drift={:.6}",
        fiat_delta, cb_delta, treasury_ext_delta, drift
    );

    // Verify bank balance-sheet identity
    if let Err(e) = verify_bank_balance_sheets(&companies_after) {
        panic!("{}", e);
    }
}

/// Run a 6-turn simulation and verify M0 conservation at EVERY turn boundary.
/// Catches leaks that only manifest after multi-turn cascades.
#[test]
fn test_m0_conservation_six_turns() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let data_dir = tmp.path();

    let registries = Registries::native_only();
    let options = GenerateOptions {
        country_count: 1,
        start_year: StartYear::Y1900,
    };

    let GeneratedWorld {
        state: mut state,
        ..
    } = generate_world(data_dir, options, &registries)
        .expect("world generation failed");

    let mut ctx = InMemoryTurnContext::load_from_disk(data_dir, &mut state)
        .expect("failed to load turn context");

    let turns: u32 = 6;
    let mut probe = sim_engine::engine::diagnostic::NoopProbe;

    for turn_num in 0..turns {
        // Compute M0 BEFORE this turn
        let country_before = get_primary_country(&state).clone();
        let companies_before = get_primary_companies(&ctx, &state);
        let fiat_before = compute_m0(&country_before, &companies_before, &ctx.market);
        let cb_before = compute_cb_injected(&country_before);
        let ext_before = compute_treasury_external(&country_before);

        // Run the turn
        let result = run_turn_inner(&mut state, &registries, &mut ctx, &mut probe);
        assert!(
            result.is_ok(),
            "Turn {} failed: {:?}",
            turn_num,
            result.err()
        );

        // Compute M0 AFTER this turn
        let country_after = get_primary_country(&state).clone();
        let companies_after = get_primary_companies(&ctx, &state);
        let fiat_after = compute_m0(&country_after, &companies_after, &ctx.market);
        let cb_after = compute_cb_injected(&country_after);
        let ext_after = compute_treasury_external(&country_after);

        // Check conservation at EVERY turn boundary
        let fiat_delta = fiat_after - fiat_before;
        let cb_delta = cb_after - cb_before;
        let ext_delta = ext_after - ext_before;
        let allowed = cb_delta + ext_delta;
        let drift = fiat_delta - allowed;

        assert!(
            drift.abs() < 1e-6,
            "M0 CONSERVATION VIOLATION at turn {}: Δfiat={:.6}, Δcb={:.6}, Δtreasury_ext={:.6}, drift={:.6}",
            turn_num, fiat_delta, cb_delta, ext_delta, drift
        );

        // Verify bank balance sheets at every turn
        if let Err(e) = verify_bank_balance_sheets(&companies_after) {
            panic!("Turn {}: {}", turn_num, e);
        }
    }
}

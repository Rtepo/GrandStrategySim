//! Phase 25: "Golden Year" 24-turn empirical balance audit.
//!
//! Generates a fresh world, runs exactly 24 turns (one full year for YoY
//! metrics), persists every turn so the Phase 24F CSV telemetry exporter
//! writes `data/telemetry/<country>_macro.csv`, and additionally snapshots
//! treasury / private-capital / citizen-savings / market imbalance data so
//! the balance audit can diagnose black holes, hoarding, and supply-chain
//! freezes without magic money.

use sim_engine::engine::{generate_world, GenerateOptions, StartYear, run_turn_in_memory, InMemoryTurnContext};
use sim_engine::economy::market::GlobalMarket;
use sim_engine::registries::enums::{Commodity, Sector};
use sim_engine::registries::Registries;
use sim_engine::state::Country;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Number of turns to run — one full "golden year".
const NUM_TURNS: u32 = 24;

/// Output directory for the audit artefacts.
const AUDIT_DIR: &str = "C:/Users/netse/Downloads/SillyElaborateState/state/test_simulation_data_phase25";

/// Per-turn per-country snapshot used for the audit report.
#[derive(Debug, Clone)]
struct CountrySnapshot {
    turn: u32,
    year: u32,
    country: String,
    liquid_reserves: f64,
    private_capital: f64,
    citizen_savings: f64,
    nominal_budget: f64,
    gdp: f64,
    shadow_gdp: f64,
    cpi: f64,
    ppi: f64,
    m0: f64,
    m3: f64,
    unemployment_pct: f64,
    true_labor_util_pct: f64,
    average_wage: f64,
    population: u64,
    sovereign_default_turns: u32,
    bank_count: usize,
    bank_deposits: f64,
    bank_reserves: f64,
}

/// Load the persisted market from disk (market is saved separately from state).
fn load_market(data_dir: &PathBuf) -> GlobalMarket {
    use sim_engine::economy::market::ApostolicSeeLedger;
    let path = data_dir.join("market.json");
    if !path.exists() {
        return GlobalMarket {
            base_prices: HashMap::new(),
            net_surplus: HashMap::new(),
            offshore_capital: 0.0,
            apostolic_see_ledger: ApostolicSeeLedger::default(),
            supply_volume: HashMap::new(),
            demand_volume: HashMap::new(),
        };
    }
    let text = fs::read_to_string(&path).unwrap_or_default();
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();

    let mut base_prices = HashMap::new();
    let mut net_surplus = HashMap::new();

    if let Some(prices) = parsed.get("prices").and_then(|v| v.as_object()) {
        for (key, value) in prices {
            if let Ok(commodity) = serde_json::from_str::<Commodity>(&format!("\"{}\"", key)) {
                if let Some(price) = value.as_f64() {
                    base_prices.insert(commodity, price);
                }
            }
        }
    }
    if let Some(orders) = parsed.get("orders").and_then(|v| v.as_object()) {
        for (key, value) in orders {
            if let Ok(commodity) = serde_json::from_str::<Commodity>(&format!("\"{}\"", key)) {
                if let Some(order) = value.as_object() {
                    let buy = order.get("buy").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let sell = order.get("sell").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    net_surplus.insert(commodity, sell - buy);
                }
            }
        }
    }
    GlobalMarket {
        base_prices,
        net_surplus,
        offshore_capital: 0.0,
        apostolic_see_ledger: ApostolicSeeLedger::default(),
        supply_volume: HashMap::new(),
        demand_volume: HashMap::new(),
    }
}

/// Load all companies for a country from the entity store.
fn load_all_companies(data_dir: &PathBuf, country: &str) -> Vec<sim_engine::entities::Company> {
    use sim_engine::io::entity_store::{DiskEntityStore, EntityStore};
    use std::fs as stdfs;
    let companies_dir = data_dir.join("entities").join(country).join("companies");
    if !companies_dir.exists() {
        return Vec::new();
    }
    let store = DiskEntityStore::<sim_engine::entities::Company>::new(data_dir);
    let mut all = Vec::new();
    for entry in stdfs::read_dir(&companies_dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let sector = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        if sector.is_empty() { continue; }
        if let Ok(loaded) = store.load_sector(country, &sector, None) {
            all.extend(loaded);
        }
    }
    all
}

/// Snapshot a single country's macro + banking state.
fn snapshot_country(
    turn: u32,
    year: u32,
    country_name: &str,
    country: &Country,
    data_dir: &PathBuf,
) -> CountrySnapshot {
    let md = &country.macro_indicators;
    let employed = md.labor_market.employed_total;
    let unemployed = md.labor_market.unemployed;
    let mut unable: f64 = 0.0;
    for region in &country.regions {
        for demo in region.class_demographics.rural_classes.values() {
            unable += demo.unable_to_work;
        }
        for demo in region.class_demographics.urban_classes.values() {
            unable += demo.unable_to_work;
        }
    }
    let denom = employed + unemployed + unable;
    let true_labor_util = if denom > 0.0 { (employed / denom) * 100.0 } else { 0.0 };

    let companies = load_all_companies(data_dir, country_name);
    let mut bank_count = 0usize;
    let mut bank_deposits = 0.0_f64;
    let mut bank_reserves = 0.0_f64;
    for c in &companies {
        if c.bank_type.is_some() || c.sector == Sector::Banking {
            bank_count += 1;
            if let Some(ref bs) = c.balance_sheet {
                bank_deposits += bs.deposits;
                bank_reserves += bs.reserves_at_central_bank;
            }
        }
    }

    CountrySnapshot {
        turn,
        year,
        country: country_name.to_string(),
        liquid_reserves: country.budget.liquid_reserves,
        private_capital: country.budget.private_capital,
        citizen_savings: country.budget.citizen_savings,
        nominal_budget: country.budget.nominal_budget,
        gdp: md.gdp_breakdown.official_gdp,
        shadow_gdp: md.gdp_breakdown.shadow_gdp,
        cpi: md.inflation_indices.cpi_index,
        ppi: md.inflation_indices.ppi_index,
        m0: md.money_supply.m0,
        m3: md.money_supply.m3,
        unemployment_pct: md.labor_market.unemployment_rate,
        true_labor_util_pct: true_labor_util,
        average_wage: md.average_wage,
        population: country.budget.population,
        sovereign_default_turns: country.sovereign_default_turns_remaining,
        bank_count,
        bank_deposits,
        bank_reserves,
    }
}

#[test]
fn golden_year_24_turn_audit() {
    let audit_dir = PathBuf::from(AUDIT_DIR);
    if audit_dir.exists() {
        fs::remove_dir_all(&audit_dir).unwrap();
    }
    fs::create_dir_all(&audit_dir).unwrap();

    let reg = Registries::native_only();
    let options = GenerateOptions {
        country_count: 6,
        start_year: StartYear::Y1975,
    };
    let generated = generate_world(&audit_dir, options, &reg)
        .expect("world generation should succeed");
    let mut state = generated.state;

    let initial_market = load_market(&audit_dir);
    let initial_m3: f64 = state.countries.values().map(|c| c.macro_indicators.money_supply.m3).sum();
    let initial_pop: u64 = state.countries.values().map(|c| c.budget.population).sum();
    let initial_country_count = state.countries.len();

    eprintln!("=== PHASE 25: GOLDEN YEAR 24-TURN AUDIT ===");
    eprintln!("Countries: {}", initial_country_count);
    eprintln!("Initial M3: {:.2}", initial_m3);
    eprintln!("Initial population: {}", initial_pop);

    // Snapshot turn 0 (pre-first-turn).
    let mut snapshots: Vec<CountrySnapshot> = Vec::new();
    let start_year = state.calendar.current_year;
    for (name, country) in &state.countries {
        snapshots.push(snapshot_country(0, start_year, name, country, &audit_dir));
    }

    // Snapshot initial market net_surplus for supply-chain freeze analysis.
    {
        let mut surplus_lines = String::from("Commodity,Net_Surplus_Turn0\n");
        let mut items: Vec<(Commodity, f64)> = initial_market.net_surplus.iter().map(|(&k,&v)| (k,v)).collect();
        items.sort_by(|a,b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        for (c, v) in &items {
            surplus_lines.push_str(&format!("{:?},{:.4}\n", c, v));
        }
        fs::write(audit_dir.join("market_surplus_turn0.csv"), surplus_lines).unwrap();
    }

    let mut last_market = initial_market.clone();
    let mut errors: Vec<(u32, String)> = Vec::new();

    // Load in-memory context once (replaces per-turn disk I/O).
    let mut ctx = InMemoryTurnContext::load_from_disk(&audit_dir, &mut state)
        .expect("failed to load in-memory context");

    for turn in 0..NUM_TURNS {
        let result = run_turn_in_memory(&mut state, &reg, &mut ctx);
        if let Err(ref e) = result {
            errors.push((turn, format!("{:?}", e)));
            eprintln!("Turn {} ERROR: {:?}", turn, e);
        }

        // Save to disk for telemetry CSV export.
        let global_orders = sim_engine::economy::market::MarketOrders::default();
        let trade_result = sim_engine::international::TradeBalanceResult::default();
        let _ = ctx.save_to_disk(&audit_dir, &state, &global_orders, &trade_result);

        last_market = ctx.market.clone();

        // Phase 27: Use the actual calendar year from state, not a computed
        // value. The year only increments after 24 turns (1 year = 24 turns).
        let year = state.calendar.current_year;
        for (name, country) in &state.countries {
            snapshots.push(snapshot_country(turn + 1, year, name, country, &audit_dir));
        }

        if (turn + 1) % 4 == 0 || turn == 0 {
            let m3: f64 = state.countries.values().map(|c| c.macro_indicators.money_supply.m3).sum();
            let pop: u64 = state.countries.values().map(|c| c.budget.population).sum();
            eprintln!(
                "  Turn {:>2} | M3: {:>14.2} ({:.3}x) | Pop: {:>10} | Countries: {} | Errors: {}",
                turn + 1, m3, m3 / initial_m3.max(1.0), pop, state.countries.len(), errors.len()
            );
        }
    }

    // === Write extended treasury CSV (richer than the auto-generated macro CSV) ===
    let mut treasury_csv = String::from(
        "Turn,Year,Country,LiquidReserves,PrivateCapital,CitizenSavings,NominalBudget,GDP,ShadowGDP,CPI,PPI,M0,M3,UnemploymentPct,TrueLaborUtilPct,AverageWage,Population,SovereignDefaultTurns,BankCount,BankDeposits,BankReserves\n",
    );
    for s in &snapshots {
        treasury_csv.push_str(&format!(
            "{},{},{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4},{:.6},{:.6},{:.4},{:.4},{:.4},{:.4},{:.4},{},{},{},{:.4},{:.4}\n",
            s.turn, s.year, s.country,
            s.liquid_reserves, s.private_capital, s.citizen_savings, s.nominal_budget,
            s.gdp, s.shadow_gdp, s.cpi, s.ppi, s.m0, s.m3,
            s.unemployment_pct, s.true_labor_util_pct, s.average_wage, s.population,
            s.sovereign_default_turns, s.bank_count, s.bank_deposits, s.bank_reserves,
        ));
    }
    fs::write(audit_dir.join("phase25_treasury_audit.csv"), treasury_csv).unwrap();

    // === Final market surplus ===
    {
        let mut surplus_lines = String::from("Commodity,Net_Surplus_Turn24\n");
        let mut items: Vec<(Commodity, f64)> = last_market.net_surplus.iter().map(|(&k,&v)| (k,v)).collect();
        items.sort_by(|a,b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        for (c, v) in &items {
            surplus_lines.push_str(&format!("{:?},{:.4}\n", c, v));
        }
        fs::write(audit_dir.join("market_surplus_turn24.csv"), surplus_lines).unwrap();
    }

    // === Per-country summary printed to stderr for capture ===
    eprintln!("\n=== PER-COUNTRY FINAL STATE (turn {}) ===", NUM_TURNS);
    for s in snapshots.iter().filter(|s| s.turn == NUM_TURNS) {
        eprintln!(
            "  {:>10} | GDP:{:>12.2} | Shadow:{:>10.2} | CPI:{:.2} PPI:{:.2} | Unemp:{:.2}% | Wage:{:.2} | Reserves:{:.2} | Banks:{}",
            s.country, s.gdp, s.shadow_gdp, s.cpi, s.ppi, s.unemployment_pct, s.average_wage, s.liquid_reserves, s.bank_count
        );
    }

    // Phase 27: Print GDP breakdown (C+I+G+NX) for each country at turn 24.
    eprintln!("\n=== GDP BREAKDOWN (turn {}) ===", NUM_TURNS);
    for (name, country) in &state.countries {
        let g = &country.macro_indicators.gdp_breakdown;
        eprintln!(
            "  {:>10} | C:{:>12.2} | I:{:>10.2} | G:{:>10.2} | NX:{:>10.2} | Total:{:>12.2}",
            name, g.consumption, g.investment, g.government_spending, g.net_exports, g.official_gdp,
        );
    }

    if !errors.is_empty() {
        eprintln!("\n=== TURN ERRORS ({}) ===", errors.len());
        for (t, e) in errors.iter().take(20) {
            eprintln!("  Turn {}: {}", t, e);
        }
    }

    eprintln!("\nAudit artefacts written to: {}", AUDIT_DIR);
    eprintln!("  - phase25_treasury_audit.csv (per-country per-turn extended telemetry)");
    eprintln!("  - telemetry/<country>_macro.csv (Phase 24F auto-generated)");
    eprintln!("  - market_surplus_turn0.csv / market_surplus_turn24.csv");

    // Soft assertion: the run should complete without total collapse.
    let final_pop: u64 = state.countries.values().map(|c| c.budget.population).sum();
    assert!(final_pop > 0, "Global extinction during golden year audit");
    assert!(!state.countries.is_empty(), "All countries vanished during golden year audit");
}

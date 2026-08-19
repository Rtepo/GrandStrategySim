//! 100-turn simulation test for empirical data collection.
//!
//! This test runs the game engine for 100 turns and collects telemetry data
//! to identify potential economic collapse risks before Stage 7.

use sim_engine::engine::generator::{generate_world, GenerateOptions, StartYear};
use sim_engine::engine::turn_context::InMemoryTurnContext;
use sim_engine::engine::run_turn_in_memory;
use sim_engine::economy::market::GlobalMarket;
use sim_engine::registries::enums::Commodity;
use sim_engine::registries::Registries;
use sim_engine::state::Country;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Telemetry data collected during simulation.
#[derive(Debug, Default)]
struct SimulationTelemetry {
    turn: u32,
    year: u32,
    total_global_capital: f64,
    state_liquid_reserves: f64,
    private_capital: f64,
    citizen_savings: f64,
    top_deficit_commodities: Vec<(Commodity, f64)>,
    top_surplus_commodities: Vec<(Commodity, f64)>,
}

/// Load market data from disk.
fn load_market(data_dir: &PathBuf) -> GlobalMarket {
    let path = data_dir.join("market.json");
    if !path.exists() {
        return GlobalMarket { base_prices: HashMap::new(), net_surplus: HashMap::new(), offshore_capital: 0.0, apostolic_see_ledger: sim_engine::economy::market::ApostolicSeeLedger::default(), supply_volume: HashMap::new(), demand_volume: HashMap::new() };
    }
    let text = fs::read_to_string(&path).unwrap_or_default();
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    
    let mut base_prices = HashMap::new();
    let mut net_surplus = HashMap::new();
    
    if let Some(prices) = parsed.get("prices").and_then(|v| v.as_object()) {
        for (key, value) in prices {
            let commodity_str = key.as_str();
            if let Ok(commodity) = serde_json::from_str::<Commodity>(&format!("\"{}\"", commodity_str)) {
                if let Some(price) = value.as_f64() {
                    base_prices.insert(commodity, price);
                }
            }
        }
    }
    
    if let Some(orders) = parsed.get("orders").and_then(|v| v.as_object()) {
        for (key, value) in orders {
            let commodity_str = key.as_str();
            if let Ok(commodity) = serde_json::from_str::<Commodity>(&format!("\"{}\"", commodity_str)) {
                if let Some(order) = value.as_object() {
                    let buy = order.get("buy").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let sell = order.get("sell").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    net_surplus.insert(commodity, sell - buy);
                }
            }
        }
    }
    
    GlobalMarket { base_prices, net_surplus, offshore_capital: 0.0, apostolic_see_ledger: sim_engine::economy::market::ApostolicSeeLedger::default(), supply_volume: HashMap::new(), demand_volume: HashMap::new() }
}

/// Get top N commodities by deficit or surplus.
fn get_top_commodities(net_surplus: &HashMap<Commodity, f64>, top_n: usize, deficit: bool) -> Vec<(Commodity, f64)> {
    let mut items: Vec<(Commodity, f64)> = net_surplus.iter()
        .map(|(&commodity, &surplus)| {
            if deficit {
                (commodity, -surplus) // Negative surplus = deficit
            } else {
                (commodity, surplus)
            }
        })
        .filter(|(_, value)| *value > 0.0)
        .collect();
    
    items.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    items.into_iter().take(top_n).collect()
}

/// Collect telemetry for a single turn.
fn collect_telemetry(
    turn: u32,
    year: u32,
    state: &sim_engine::state::GameState,
    country_name: &str,
    data_dir: &PathBuf,
) -> SimulationTelemetry {
    // Access country state directly from memory
    let country = state.countries.get(country_name).unwrap();
    
    // Load market from disk (market is persisted separately)
    let market = load_market(data_dir);
    
    // Calculate metrics from country state
    let state_liquid_reserves = country.budget.liquid_reserves;
    let private_capital = country.budget.private_capital;
    let citizen_savings = country.budget.citizen_savings;
    let total_global_capital = state_liquid_reserves + private_capital + citizen_savings;
    
    let top_deficit = get_top_commodities(&market.net_surplus, 3, true);
    let top_surplus = get_top_commodities(&market.net_surplus, 3, false);
    
    SimulationTelemetry {
        turn,
        year,
        total_global_capital,
        state_liquid_reserves,
        private_capital,
        citizen_savings,
        top_deficit_commodities: top_deficit,
        top_surplus_commodities: top_surplus,
    }
}

#[test]
fn run_100_turn_simulation() {
    // Setup temporary data directory
    let data_dir = PathBuf::from("C:/Users/netse/Downloads/SillyElaborateState/state/test_simulation_data");
    let _ = fs::remove_dir_all(&data_dir);
    fs::create_dir_all(&data_dir).unwrap();
    
    // Generate initial world
    let registries = Registries::native_only();
    let options = GenerateOptions {
        country_count: 1,
        start_year: StartYear::Y1950,
    };
    
    let generated = generate_world(&data_dir, options, &registries).unwrap();
    let country_name = generated.state.countries.keys().next().unwrap().clone();
    
    println!("Generated world with country: {}", country_name);
    println!("Initial regions: {}", generated.regions.len());
    
    // Run 100 turns
    let mut telemetry_log = Vec::new();
    let mut state = generated.state;
    
    // Load in-memory context once (replaces per-turn disk I/O).
    let mut ctx = InMemoryTurnContext::load_from_disk(&data_dir, &mut state)
        .expect("failed to load in-memory context");

    for turn in 0..100 {
        let result = run_turn_in_memory(
            &mut state,
            &registries,
            &mut ctx,
        );
        
        if let Err(e) = result {
            eprintln!("Turn {} error: {}", turn, e);
            break;
        }
        
        // Collect telemetry every 10 turns
        if turn % 10 == 0 {
            let telemetry = collect_telemetry(turn, 1950 + turn as u32, &state, &country_name, &data_dir);
            println!("Turn {}: Capital={:.2}, StateReserves={:.2}, PrivateCapital={:.2}, CitizenSavings={:.2}", 
                turn, telemetry.total_global_capital, telemetry.state_liquid_reserves, telemetry.private_capital, telemetry.citizen_savings);
            telemetry_log.push(telemetry);
        }
    }
    
    // Generate report
    let report_path = PathBuf::from("C:/Users/netse/Downloads/SillyElaborateState/state/SIMULATION_100_TURNS_RESULTS.md");
    let mut report = String::new();
    
    report.push_str("# SIMULATION 100 TURNS RESULTS\n\n");
    report.push_str("**Date:** 2026-07-17\n");
    report.push_str("**Country:** ");
    report.push_str(&country_name);
    report.push_str("\n**Total Turns:** 100\n\n");
    
    report.push_str("## TELEMETRY SUMMARY\n\n");
    report.push_str("| Turn | Year | Total Capital | State Reserves | Private Capital | Citizen Savings |\n");
    report.push_str("|------|------|---------------|----------------|-----------------|-----------------|\n");
    
    for telemetry in &telemetry_log {
        report.push_str(&format!(
            "| {} | {} | {:.2} | {:.2} | {:.2} | {:.2} |\n",
            telemetry.turn,
            telemetry.year,
            telemetry.total_global_capital,
            telemetry.state_liquid_reserves,
            telemetry.private_capital,
            telemetry.citizen_savings,
        ));
    }
    
    report.push_str("\n## MARKET IMBALANCES\n\n");
    
    for telemetry in &telemetry_log {
        report.push_str(&format!("### Turn {} (Year {})\n\n", telemetry.turn, telemetry.year));
        
        if !telemetry.top_deficit_commodities.is_empty() {
            report.push_str("**Top 3 Deficit Commodities (Input Starvation):**\n\n");
            report.push_str("| Commodity | Deficit Amount |\n");
            report.push_str("|-----------|---------------|\n");
            for (commodity, deficit) in &telemetry.top_deficit_commodities {
                report.push_str(&format!("| {:?} | {:.2} |\n", commodity, deficit));
            }
            report.push_str("\n");
        }
        
        if !telemetry.top_surplus_commodities.is_empty() {
            report.push_str("**Top 3 Surplus Commodities (Output Glut):**\n\n");
            report.push_str("| Commodity | Surplus Amount |\n");
            report.push_str("|-----------|---------------|\n");
            for (commodity, surplus) in &telemetry.top_surplus_commodities {
                report.push_str(&format!("| {:?} | {:.2} |\n", commodity, surplus));
            }
            report.push_str("\n");
        }
    }
    
    report.push_str("\n## ANALYSIS\n\n");
    
    // Calculate capital drift
    if telemetry_log.len() >= 2 {
        let initial_capital = telemetry_log.first().unwrap().total_global_capital;
        let final_capital = telemetry_log.last().unwrap().total_global_capital;
        let capital_drift = final_capital - initial_capital;
        let drift_percent = if initial_capital > 0.0 { (capital_drift / initial_capital) * 100.0 } else { 0.0 };
        
        report.push_str(&format!("**Capital Drift:** {:.2} ({:.2}%)\n\n", capital_drift, drift_percent));
        
        if drift_percent.abs() > 10.0 {
            report.push_str("⚠️ **WARNING:** Significant capital drift detected - money may be created or destroyed.\n\n");
        } else {
            report.push_str("✅ Capital conservation appears stable.\n\n");
        }
    }
    
    // Analyze citizen savings trend
    let savings_trend: Vec<f64> = telemetry_log.iter().map(|t| t.citizen_savings).collect();
    if savings_trend.len() >= 2 {
        let initial_savings = *savings_trend.first().unwrap();
        let final_savings = *savings_trend.last().unwrap();
        let savings_growth = final_savings - initial_savings;
        let savings_growth_percent = if initial_savings > 0.0 { (savings_growth / initial_savings) * 100.0 } else { 0.0 };
        
        report.push_str(&format!("**Citizen Savings Growth:** {:.2} ({:.2}%)\n\n", savings_growth, savings_growth_percent));
        
        if savings_growth_percent > 50.0 {
            report.push_str("⚠️ **WARNING:** Citizen savings growing rapidly - capital drain risk (no recycling mechanism).\n\n");
        }
    }
    
    // Analyze private capital trend
    let private_trend: Vec<f64> = telemetry_log.iter().map(|t| t.private_capital).collect();
    if private_trend.len() >= 2 {
        let initial_private = *private_trend.first().unwrap();
        let final_private = *private_trend.last().unwrap();
        let private_growth = final_private - initial_private;
        let private_growth_percent = if initial_private > 0.0 { (private_growth / initial_private) * 100.0 } else { 0.0 };
        
        report.push_str(&format!("**Private Capital Growth:** {:.2} ({:.2}%)\n\n", private_growth, private_growth_percent));
        
        if private_growth_percent < -20.0 {
            report.push_str("⚠️ **WARNING:** Private capital shrinking rapidly - corporate sector distress.\n\n");
        }
    }
    
    fs::write(&report_path, report).unwrap();
    
    println!("Simulation complete. Report written to: {:?}", report_path);
    
    // Cleanup
    let _ = fs::remove_dir_all(&data_dir);
}

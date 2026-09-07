//! v4: Snapshot tests for market clearing and banking state serialization.
//!
//! These tests use cargo-insta to snapshot complex state outputs, replacing
//! brittle assert_eq! blocks with reviewable .snap files.

use insta::assert_ron_snapshot;
use sim_engine::economy::market::market::GlobalMarket;
use sim_engine::registries::enums::Commodity;
use rustc_hash::FxHashMap;

/// Snapshot a minimal GlobalMarket with deterministic prices.
#[test]
fn test_snapshot_market_clearing_state() {
    let mut base_prices = FxHashMap::default();
    base_prices.insert(Commodity::Steel, 12.50);
    base_prices.insert(Commodity::Food, 3.20);
    base_prices.insert(Commodity::Energy, 8.40);
    base_prices.insert(Commodity::Iron, 5.00);

    let mut net_surplus = FxHashMap::default();
    net_surplus.insert(Commodity::Steel, 200.0);
    net_surplus.insert(Commodity::Food, -50.0);
    net_surplus.insert(Commodity::Energy, 0.0);
    net_surplus.insert(Commodity::Iron, 100.0);

    let mut supply_volume = FxHashMap::default();
    supply_volume.insert(Commodity::Steel, 5000.0);
    supply_volume.insert(Commodity::Food, 3000.0);

    let mut demand_volume = FxHashMap::default();
    demand_volume.insert(Commodity::Steel, 4800.0);
    demand_volume.insert(Commodity::Food, 3050.0);

    let market = GlobalMarket {
        base_prices,
        net_surplus,
        offshore_capital: 100_000.0,
        apostolic_see_ledger: Default::default(),
        supply_volume,
        demand_volume,
        ..Default::default()
    };

    // Serialize to a deterministic JSON string for snapshot comparison.
    // We use serde_json to ensure stable key ordering.
    let json = serde_json::json!({
        "base_prices": {
            "Steel": market.base_prices.get(&Commodity::Steel).unwrap(),
            "Food": market.base_prices.get(&Commodity::Food).unwrap(),
            "Energy": market.base_prices.get(&Commodity::Energy).unwrap(),
            "Iron": market.base_prices.get(&Commodity::Iron).unwrap(),
        },
        "net_surplus": {
            "Steel": market.net_surplus.get(&Commodity::Steel).unwrap(),
            "Food": market.net_surplus.get(&Commodity::Food).unwrap(),
            "Energy": market.net_surplus.get(&Commodity::Energy).unwrap(),
            "Iron": market.net_surplus.get(&Commodity::Iron).unwrap(),
        },
        "offshore_capital": market.offshore_capital,
        "supply_volume": {
            "Steel": market.supply_volume.get(&Commodity::Steel).unwrap(),
            "Food": market.supply_volume.get(&Commodity::Food).unwrap(),
        },
        "demand_volume": {
            "Steel": market.demand_volume.get(&Commodity::Steel).unwrap(),
            "Food": market.demand_volume.get(&Commodity::Food).unwrap(),
        },
    });

    assert_ron_snapshot!("market__clearing_state_deterministic", json.to_string());
}

/// Snapshot a minimal bank balance sheet.
#[test]
fn test_snapshot_bank_balance_sheet() {
    use sim_engine::state::banking::BankBalanceSheet;

    let bs = BankBalanceSheet {
        reserves_at_central_bank: 50_000.0,
        cb_deposit_facility_balance: 10_000.0,
        deposits: 240_000.0,
        cb_lombard_loans: 15_000.0,
        interbank_loans_given: std::collections::HashMap::new(),
        interbank_loans_taken: std::collections::HashMap::new(),
        securities: 30_000.0,
        tier_1_capital: 32_000.0,
        loans_issued: Vec::new(),
        ..Default::default()
    };

    let json = serde_json::json!({
        "reserves_at_central_bank": bs.reserves_at_central_bank,
        "cb_deposit_facility_balance": bs.cb_deposit_facility_balance,
        "deposits": bs.deposits,
        "cb_lombard_loans": bs.cb_lombard_loans,
        "securities": bs.securities,
        "tier_1_capital": bs.tier_1_capital,
        "total_assets": bs.total_assets(),
        "total_liabilities": bs.total_liabilities(),
        "total_equity": bs.total_equity(),
        "is_balanced": bs.is_balanced(),
    });

    assert_ron_snapshot!("banking__balance_sheet_deterministic", json.to_string());
}

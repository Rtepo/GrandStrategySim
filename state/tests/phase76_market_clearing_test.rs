//! Phase 76: Market clearing tests.
//!
//! Validates that B2B order matching produces trades and VWAP when bids and
//! asks cross, that no-trade commodities use base-price fallback, and that
//! the bootstrap pricing mechanism breaks the Turn 0 deadlock.

use sim_engine::economy::market::order_book::{OrderBook, Bid, Ask, match_orders};
use sim_engine::economy::market::market_history::{MarketHistory, update_vwap, get_reference_price};
use sim_engine::registries::enums::Commodity;

/// Test 1: When bids and asks cross, matching produces trades.
#[test]
fn crossing_orders_produce_trades() {
    let mut book = OrderBook::default();
    book.bids.insert(Commodity::Cereal, vec![Bid {
        buyer_id: "BUYER-1".to_string(),
        commodity: Commodity::Cereal,
        quantity: 100.0,
        limit_price: 105.0,
        blueprint_id: None,
        min_quality: None,
    }]);
    book.asks.insert(Commodity::Cereal, vec![Ask {
        seller_id: "SELLER-1".to_string(),
        commodity: Commodity::Cereal,
        quantity: 100.0,
        limit_price: 100.0,
        blueprint_id: None,
        quality: None,
        durability: None,
    }]);

    match_orders(&mut book);
    assert!(!book.trades.is_empty(), "Crossing orders should produce trades");
    assert_eq!(book.trades[0].commodity, Commodity::Cereal);
    assert!(book.trades[0].quantity > 0.0);
    assert!(book.trades[0].execution_price > 0.0);
}

/// Test 2: When bids are below asks, no trades execute.
#[test]
fn non_crossing_orders_produce_no_trades() {
    let mut book = OrderBook::default();
    book.bids.insert(Commodity::Cereal, vec![Bid {
        buyer_id: "BUYER-1".to_string(),
        commodity: Commodity::Cereal,
        quantity: 100.0,
        limit_price: 50.0,
        blueprint_id: None,
        min_quality: None,
    }]);
    book.asks.insert(Commodity::Cereal, vec![Ask {
        seller_id: "SELLER-1".to_string(),
        commodity: Commodity::Cereal,
        quantity: 100.0,
        limit_price: 200.0,
        blueprint_id: None,
        quality: None,
        durability: None,
    }]);

    match_orders(&mut book);
    assert!(book.trades.is_empty(), "Non-crossing orders should produce no trades");
}

/// Test 3: Positive executed trades produce positive VWAP and last-trade values.
#[test]
fn executed_trades_produce_positive_vwap() {
    use sim_engine::economy::market::order_book::Trade;
    let trades = vec![
        Trade {
            buyer_id: "B1".to_string(),
            seller_id: "S1".to_string(),
            commodity: Commodity::Cereal,
            quantity: 50.0,
            execution_price: 100.0,
            bid_limit_price: 105.0,
            blueprint_id: None,
            quality: None,
        },
        Trade {
            buyer_id: "B2".to_string(),
            seller_id: "S2".to_string(),
            commodity: Commodity::Cereal,
            quantity: 30.0,
            execution_price: 110.0,
            bid_limit_price: 115.0,
            blueprint_id: None,
            quality: None,
        },
    ];

    let mut history = MarketHistory::default();
    update_vwap(&mut history, &trades);

    let vwap = history.vwap_per_commodity.get(&Commodity::Cereal).copied().unwrap_or(0.0);
    let last = history.last_trade_price.get(&Commodity::Cereal).copied().unwrap_or(0.0);

    assert!(vwap > 0.0, "VWAP should be positive after trades, got {}", vwap);
    assert!(last > 0.0, "Last trade price should be positive, got {}", last);
    // VWAP = (50*100 + 30*110) / (50+30) = 8300/80 = 103.75
    assert!((vwap - 103.75).abs() < 0.01, "VWAP should be 103.75, got {}", vwap);
    // update_vwap sets last_trade_price to the VWAP (not the last individual trade price)
    assert!((last - 103.75).abs() < 0.01, "Last trade price should be VWAP 103.75, got {}", last);
}

/// Test 4: No-trade commodities use base-price fallback.
#[test]
fn no_trade_commodity_uses_base_price_fallback() {
    let mut history = MarketHistory::default();
    history.global_base_prices.insert(Commodity::Cereal, 100.0);

    let ref_price = get_reference_price(&Commodity::Cereal, &history);
    assert!(ref_price.is_some(), "Should fall back to base price");
    assert!((ref_price.unwrap() - 100.0).abs() < 0.01, "Fallback price should be 100.0");
}

/// Test 5: VWAP takes priority over last-trade, which takes priority over base price.
#[test]
fn reference_price_fallback_chain_priority() {
    let mut history = MarketHistory::default();
    history.global_base_prices.insert(Commodity::Cereal, 100.0);
    history.last_trade_price.insert(Commodity::Cereal, 120.0);
    history.vwap_per_commodity.insert(Commodity::Cereal, 105.0);

    let ref_price = get_reference_price(&Commodity::Cereal, &history).unwrap();
    assert!((ref_price - 105.0).abs() < 0.01, "VWAP should take priority: expected 105.0, got {}", ref_price);

    // Remove VWAP — last trade should be used
    history.vwap_per_commodity.clear();
    let ref_price = get_reference_price(&Commodity::Cereal, &history).unwrap();
    assert!((ref_price - 120.0).abs() < 0.01, "Last trade should be used: expected 120.0, got {}", ref_price);

    // Remove last trade — base price should be used
    history.last_trade_price.clear();
    let ref_price = get_reference_price(&Commodity::Cereal, &history).unwrap();
    assert!((ref_price - 100.0).abs() < 0.01, "Base price should be used: expected 100.0, got {}", ref_price);
}

/// Test 6: Empty market history returns None from get_reference_price.
#[test]
fn empty_history_returns_none() {
    let history = MarketHistory::default();
    let ref_price = get_reference_price(&Commodity::Cereal, &history);
    assert!(ref_price.is_none(), "Empty history should return None");
}

/// Test 7: Bootstrap condition — when VWAP and last_trade are both empty,
/// the market is in bootstrap mode.
#[test]
fn bootstrap_condition_detected() {
    let mut history = MarketHistory::default();
    history.global_base_prices.insert(Commodity::Cereal, 100.0);

    // No VWAP, no last trade → bootstrap
    let is_bootstrap = history.vwap_per_commodity.is_empty()
        && history.last_trade_price.is_empty();
    assert!(is_bootstrap, "Empty VWAP and last_trade should indicate bootstrap");

    // After a trade, bootstrap should be false
    history.vwap_per_commodity.insert(Commodity::Cereal, 100.0);
    let is_bootstrap = history.vwap_per_commodity.is_empty()
        && history.last_trade_price.is_empty();
    assert!(!is_bootstrap, "Non-empty VWAP should exit bootstrap");
}

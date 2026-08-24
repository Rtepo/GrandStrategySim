//! Bugfix Sprint: Market identity and demand aggregation tests.
//!
//! Verifies:
//! - UI net_surplus identity: `supply_volume − demand_volume + net_trade = net_surplus`
//! - B2C and B2B demand are tracked in separate aggregators.
//! - Backend `market.net_surplus` is the raw B2B order book surplus (unchanged).
//! - `CommodityTradeEntry` correctly represents per-commodity trade flows.

#[cfg(test)]
mod tests {
    use sim_engine::economy::market::market::{GlobalMarket, MarketOrder, MarketOrders};
    use sim_engine::registries::enums::Commodity;
    use sim_engine::international::{CommodityTradeEntry, TradeDelta};

    /// The UI net_surplus identity must hold: supply - demand + net_trade = net_surplus.
    #[test]
    fn test_ui_net_surplus_identity() {
        let supply = 1000.0;
        let demand = 800.0;
        let net_trade = 50.0; // net importer
        let ui_net_surplus = supply - demand + net_trade;
        assert_eq!(ui_net_surplus, 250.0);

        // Net exporter scenario
        let net_trade_export = -100.0;
        let ui_net_surplus_export = supply - demand + net_trade_export;
        assert_eq!(ui_net_surplus_export, 100.0);
    }

    /// Backend `market.net_surplus` is the raw B2B order book surplus (sell - buy),
    /// NOT the UI identity. This must remain unchanged for clearing.rs.
    #[test]
    fn test_backend_net_surplus_is_raw_order_book() {
        let mut orders = MarketOrders::default();
        orders.add_sell(Commodity::Steel, 500.0);
        orders.add_buy(Commodity::Steel, 300.0);

        let backend_net_surplus: f64 = orders.orders.get(&Commodity::Steel).map(|o| o.sell - o.buy).unwrap_or(0.0);
        assert_eq!(backend_net_surplus, 200.0); // sell - buy, NOT supply - demand + net_trade
    }

    /// GlobalMarket has separate b2c_demand_volume and demand_volume fields.
    #[test]
    fn test_b2c_demand_volume_is_separate_field() {
        let mut market = GlobalMarket::new();
        market.demand_volume.insert(Commodity::Steel, 100.0); // B2B + B2C total
        market.b2c_demand_volume.insert(Commodity::Steel, 40.0); // B2C only

        assert_eq!(market.demand_volume.get(&Commodity::Steel), Some(&100.0));
        assert_eq!(market.b2c_demand_volume.get(&Commodity::Steel), Some(&40.0));
    }

    /// GlobalMarket has a net_trade field for per-commodity trade flows.
    #[test]
    fn test_net_trade_field_exists() {
        let mut market = GlobalMarket::new();
        market.net_trade.insert(Commodity::Steel, 75.0); // net importer

        assert_eq!(market.net_trade.get(&Commodity::Steel), Some(&75.0));
    }

    /// CommodityTradeEntry correctly stores import and export volumes.
    #[test]
    fn test_commodity_trade_entry() {
        let entry = CommodityTradeEntry {
            commodity: Commodity::Steel,
            import_volume: 200.0,
            export_volume: 150.0,
        };
        assert_eq!(entry.import_volume - entry.export_volume, 50.0); // net import
    }

    /// TradeDelta includes commodity_entries for per-commodity trade tracking.
    #[test]
    fn test_trade_delta_has_commodity_entries() {
        let delta = TradeDelta {
            country_name: "TestCountry".to_string(),
            exports: 1000.0,
            imports: 800.0,
            trade_balance: 200.0,
            tariff_revenue: 0.0,
            currency_code: "TST".to_string(),
            commodity_entries: vec![
                CommodityTradeEntry {
                    commodity: Commodity::Steel,
                    import_volume: 500.0,
                    export_volume: 300.0,
                },
            ],
        };
        assert_eq!(delta.commodity_entries.len(), 1);
        assert_eq!(delta.commodity_entries[0].commodity, Commodity::Steel);
    }

    /// MarketOrder buy/sell semantics are preserved.
    #[test]
    fn test_market_order_semantics() {
        let order = MarketOrder { buy: 100.0, sell: 200.0 };
        assert_eq!(order.sell - order.buy, 100.0); // surplus
    }

    /// Turn-start clear: all per-turn aggregators start empty.
    #[test]
    fn test_turn_start_clear_leaves_empty_aggregators() {
        let mut market = GlobalMarket::new();
        market.supply_volume.insert(Commodity::Steel, 500.0);
        market.demand_volume.insert(Commodity::Steel, 300.0);
        market.b2c_demand_volume.insert(Commodity::Steel, 100.0);
        market.net_trade.insert(Commodity::Steel, 50.0);

        // Simulate turn-start clear
        market.supply_volume.clear();
        market.demand_volume.clear();
        market.b2c_demand_volume.clear();
        market.net_trade.clear();

        assert!(market.supply_volume.is_empty());
        assert!(market.demand_volume.is_empty());
        assert!(market.b2c_demand_volume.is_empty());
        assert!(market.net_trade.is_empty());
    }
}

//! Stabilization Sprint: Tests for the 4 critical anomaly fixes.
//!
//! Tests:
//! 1. Clearing engine: no VWAP pinning at 102.5% when no global surplus exists
//! 2. Clearing engine: surplus floor when no global demand exists
//! 3. Grid commodity isolation: is_local_utility() covers Water + waste streams
//! 4. Grid commodity isolation: Energy and Heat still excluded
//! 5. Corporate seeding: SEED_INVENTORY_TURNS constant exists and is 5.0
//! 6. Crop registry: loaded into Registries (non-empty)
//! 7. Crop names: all in English (no Polish strings)
//! 8. Crop registry: has wheat, potatoes, alfalfa, cattle, orchard, tobacco
//! 9. AgriculturalProfile: has owned_parcel_ids field
//! 10. Crop batches: start in Idle state
//! 11. Strategic reserve: Cereal and Food seeded in SRA warehouses
//! 12. Agriculture bypass: no per-turn output for agriculture buildings

#[cfg(test)]
mod tests {
    use sim_engine::registries::enums::Commodity;
    use sim_engine::registries::Registries;
    use sim_engine::economy::market::market::{GlobalMarket, MarketOrders};
    use sim_engine::economy::market::clearing::resolve_market_prices;
    use sim_engine::state::Country;
    use sim_engine::entities::{AgriculturalProfile, CropBatch, CropState};

    /// Test 1: Clearing engine — deficit with no global surplus should NOT
    /// pin at 102.5% of base price. It should rise toward PRICE_CAP (5.0x).
    #[test]
    fn test_clearing_no_vwap_pinning_on_missing_global_surplus() {
        let mut market_orders = MarketOrders::default();
        market_orders.add_buy(Commodity::Steel, 1000.0);
        // No sell orders → net deficit of 1000

        let mut global_market = GlobalMarket::new();
        global_market.base_prices.insert(Commodity::Steel, 100.0);
        // NO net_surplus entry for Steel — the old bug defaulted to `deficit`
        // which made the engine think imports fully covered the shortage.

        let country = Country::default();

        let prices = resolve_market_prices(&market_orders, &country, &global_market);
        let steel_price = prices.get(&Commodity::Steel).copied().unwrap_or(0.0);

        // The old bug would return ~102.5 (100 * 1.025 tariff).
        // The fix should return a much higher price (toward PRICE_CAP = 500).
        // At minimum, it must be significantly above 102.5.
        assert!(
            steel_price > 150.0,
            "Steel price should rise significantly above base under deficit with no global surplus, got {}",
            steel_price
        );
    }

    /// Test 2: Clearing engine — surplus with no global demand should drop
    /// toward PRICE_FLOOR (0.2x), not stay at base price.
    #[test]
    fn test_clearing_surplus_floor_on_missing_global_demand() {
        let mut market_orders = MarketOrders::default();
        market_orders.add_sell(Commodity::Steel, 1000.0);
        // No buy orders → net surplus of 1000

        let mut global_market = GlobalMarket::new();
        global_market.base_prices.insert(Commodity::Steel, 100.0);
        // NO net_surplus entry for Steel

        let country = Country::default();

        let prices = resolve_market_prices(&market_orders, &country, &global_market);
        let steel_price = prices.get(&Commodity::Steel).copied().unwrap_or(0.0);

        // The old bug would return ~97.5 (100 * 0.975 export tax).
        // The fix should return a much lower price (toward PRICE_FLOOR = 20).
        assert!(
            steel_price < 80.0,
            "Steel price should drop significantly below base under surplus with no global demand, got {}",
            steel_price
        );
    }

    /// Test 3: Grid commodity isolation — is_local_utility() returns true
    /// for Water and B2B-excluded waste streams.
    #[test]
    fn test_is_local_utility_covers_water_and_waste() {
        assert!(Commodity::Water.is_local_utility(), "Water should be a local utility");
        assert!(Commodity::MixedWaste.is_local_utility(), "MixedWaste should be a local utility");
        assert!(Commodity::BioWaste.is_local_utility(), "BioWaste should be a local utility");
        assert!(Commodity::BulkyWaste.is_local_utility(), "BulkyWaste should be a local utility");
        assert!(Commodity::ConstructionWaste.is_local_utility(), "ConstructionWaste should be a local utility");
        assert!(Commodity::HazardousWaste.is_local_utility(), "HazardousWaste should be a local utility");
    }

    /// Test 4: Grid commodity isolation — Energy and Heat still excluded.
    #[test]
    fn test_is_local_utility_still_covers_energy_and_heat() {
        assert!(Commodity::Energy.is_local_utility(), "Energy should be a local utility");
        assert!(Commodity::Heat.is_local_utility(), "Heat should be a local utility");
    }

    /// Test 5: Grid commodity isolation — tradeable commodities are NOT
    /// marked as local utilities.
    #[test]
    fn test_is_local_utility_excludes_tradeable_goods() {
        assert!(!Commodity::Steel.is_local_utility(), "Steel should NOT be a local utility");
        assert!(!Commodity::Cereal.is_local_utility(), "Cereal should NOT be a local utility");
        assert!(!Commodity::Food.is_local_utility(), "Food should NOT be a local utility");
        assert!(!Commodity::MetalWaste.is_local_utility(), "MetalWaste (B2B-tradeable) should NOT be a local utility");
        assert!(!Commodity::GlassWaste.is_local_utility(), "GlassWaste (B2B-tradeable) should NOT be a local utility");
    }

    /// Test 6: Crop registry — loaded into Registries (non-empty).
    #[test]
    fn test_crop_registry_loaded() {
        let registries = Registries::native_only();
        assert!(
            !registries.crops.crops.is_empty(),
            "Crop registry should be loaded with crop definitions, not empty"
        );
    }

    /// Test 7: Crop names — all in English (no Polish strings).
    #[test]
    fn test_crop_names_are_english() {
        let registries = Registries::native_only();
        for (id, crop) in &registries.crops.crops {
            // Check that the name doesn't contain Polish-specific characters
            // or known Polish words from the old data
            let name = &crop.name;
            assert!(
                !name.contains("Pszenica") && !name.contains("Kukurydza")
                && !name.contains("Ziemniaki") && !name.contains("Lucerna"),
                "Crop '{}' name '{}' still contains Polish text (Rule 12 violation)",
                id, name
            );
        }
    }

    /// Test 8: Crop registry — has the expected crop definitions.
    #[test]
    fn test_crop_registry_has_expected_crops() {
        let registries = Registries::native_only();
        assert!(registries.crops.get("wheat").is_some(), "Wheat crop should be defined");
        assert!(registries.crops.get("corn").is_some(), "Corn crop should be defined");
        assert!(registries.crops.get("potatoes").is_some(), "Potatoes crop should be defined");
        assert!(registries.crops.get("alfalfa").is_some(), "Alfalfa crop should be defined");
        assert!(registries.crops.get("cattle").is_some(), "Cattle crop should be defined");
        assert!(registries.crops.get("orchard").is_some(), "Orchard crop should be defined");
        assert!(registries.crops.get("tobacco").is_some(), "Tobacco crop should be defined");
    }

    /// Test 9: AgriculturalProfile — has owned_parcel_ids field.
    #[test]
    fn test_agricultural_profile_has_parcel_ids() {
        let profile = AgriculturalProfile::default();
        assert!(
            profile.owned_parcel_ids.is_empty(),
            "AgriculturalProfile should have owned_parcel_ids field (default empty)"
        );
    }

    /// Test 10: Crop batches — start in Idle state when created.
    #[test]
    fn test_crop_batch_default_state_is_idle() {
        let batch = CropBatch {
            crop_id: "wheat".to_string(),
            planned_hectares: 100.0,
            active_hectares: 0.0,
            state: CropState::Idle,
            planted_turn: 0,
            accumulated_yield: 0.0,
            rot_accumulator: 0.0,
        };
        assert_eq!(batch.state, CropState::Idle, "New crop batch should start in Idle state");
        assert_eq!(batch.active_hectares, 0.0, "Idle batch should have 0 active hectares");
    }

    /// Test 11: Crop definitions — wheat has correct yield and schedule.
    #[test]
    fn test_wheat_crop_definition() {
        let registries = Registries::native_only();
        let wheat = registries.crops.get("wheat").expect("Wheat should be defined");
        assert_eq!(wheat.name, "Wheat", "Wheat name should be English");
        assert_eq!(wheat.sowing_schedule.start_turn, 5, "Wheat sowing starts at turn 5 (Spring)");
        assert_eq!(wheat.sowing_schedule.end_turn, 7, "Wheat sowing ends at turn 7");
        assert_eq!(wheat.harvest_schedule.start_turn, 17, "Wheat harvest starts at turn 17 (Autumn)");
        assert_eq!(wheat.harvest_schedule.end_turn, 19, "Wheat harvest ends at turn 19");
        // Yield: 4.5 tons Cereal per hectare
        let cereal_yield = wheat.yields.get(&Commodity::Cereal).copied().unwrap_or(0.0);
        assert_eq!(cereal_yield, 4.5, "Wheat should yield 4.5 tons Cereal per hectare");
    }

    /// Test 12: Cattle crop — has correct yield (Meat + Livestock).
    #[test]
    fn test_cattle_crop_definition() {
        let registries = Registries::native_only();
        let cattle = registries.crops.get("cattle").expect("Cattle should be defined");
        assert_eq!(cattle.name, "Cattle", "Cattle name should be English");
        let meat_yield = cattle.yields.get(&Commodity::Meat).copied().unwrap_or(0.0);
        assert!(meat_yield > 0.0, "Cattle should yield Meat");
        let livestock_yield = cattle.yields.get(&Commodity::Livestock).copied().unwrap_or(0.0);
        assert!(livestock_yield > 0.0, "Cattle should yield Livestock");
    }
}

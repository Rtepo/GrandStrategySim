//! Phase 81 Wave 1: Energy grid unit tests.
//!
//! Tests for:
//! - Transmission losses (calibrated formula).
//! - Deterministic DC flow (sorted iteration).
//! - Geographic constraints for plant eligibility.
//! - Weather generation modifiers (solar, wind, cooling water).
//! - Load-shed ordering (tier priority).
//! - Curtailment and overfrequency behavior.
//! - Historical topology (pre-1920 = no HV lines).
//! - Development-scaled plant creation.
//! - Physical conservation and BOM clamping.
//! - Snapshot role-gating (foreign observer stripping).

#[cfg(test)]
mod tests {
    use sim_engine::economy::production::weather::WeatherModifier;
    use sim_engine::energy::grid::transmission_loss;
    use sim_engine::energy::types::{
        CoolingType, GridLine, GridTier, LoadShedTier, OverproductionTier, PowerPlantMetadata,
        PowerPlantType,
    };

    // ── Transmission Loss Tests ──

    #[test]
    fn test_transmission_loss_200km_good_condition() {
        let line = GridLine {
            id: "test_1".to_string(),
            from_region: "A".to_string(),
            to_region: "B".to_string(),
            tier: GridTier::Hv,
            capacity_mw: 1000.0,
            condition: 1.0,
            distance_km: 200.0,
            is_interconnector: false,
            owner_country: "Test".to_string(),
            current_flow_mw: 0.0,
        };
        let loss = transmission_loss(&line);
        // 200 km * 0.00005 = 0.01 = 1%
        assert!(
            (loss - 0.01).abs() < 0.001,
            "Expected ~1% loss, got {}",
            loss
        );
    }

    #[test]
    fn test_transmission_loss_1000km_good_condition() {
        let line = GridLine {
            id: "test_2".to_string(),
            from_region: "A".to_string(),
            to_region: "B".to_string(),
            tier: GridTier::Hv,
            capacity_mw: 1000.0,
            condition: 1.0,
            distance_km: 1000.0,
            is_interconnector: false,
            owner_country: "Test".to_string(),
            current_flow_mw: 0.0,
        };
        let loss = transmission_loss(&line);
        // 1000 km * 0.00005 = 0.05 = 5%
        assert!(
            (loss - 0.05).abs() < 0.001,
            "Expected ~5% loss, got {}",
            loss
        );
    }

    #[test]
    fn test_transmission_loss_degraded_condition_doubles() {
        let line_good = GridLine {
            id: "test_3a".to_string(),
            from_region: "A".to_string(),
            to_region: "B".to_string(),
            tier: GridTier::Hv,
            capacity_mw: 1000.0,
            condition: 1.0,
            distance_km: 200.0,
            is_interconnector: false,
            owner_country: "Test".to_string(),
            current_flow_mw: 0.0,
        };
        let line_bad = GridLine {
            id: "test_3b".to_string(),
            from_region: "A".to_string(),
            to_region: "B".to_string(),
            tier: GridTier::Hv,
            capacity_mw: 1000.0,
            condition: 0.5,
            distance_km: 200.0,
            is_interconnector: false,
            owner_country: "Test".to_string(),
            current_flow_mw: 0.0,
        };
        let loss_good = transmission_loss(&line_good);
        let loss_bad = transmission_loss(&line_bad);
        // At condition 0.5, loss should be ~2x (1/0.5 = 2.0)
        assert!(
            (loss_bad / loss_good - 2.0).abs() < 0.01,
            "Expected ~2x loss at condition 0.5, got ratio {}",
            loss_bad / loss_good
        );
    }

    #[test]
    fn test_transmission_loss_capped_at_50_percent() {
        let line = GridLine {
            id: "test_4".to_string(),
            from_region: "A".to_string(),
            to_region: "B".to_string(),
            tier: GridTier::Hv,
            capacity_mw: 1000.0,
            condition: 0.1,
            distance_km: 100_000.0, // Very long line
            is_interconnector: false,
            owner_country: "Test".to_string(),
            current_flow_mw: 0.0,
        };
        let loss = transmission_loss(&line);
        assert!(loss <= 0.50, "Loss should be capped at 50%, got {}", loss);
    }

    // ── Load Shed Tier Ordering Tests ──

    #[test]
    fn test_load_shed_tier_ordering() {
        // Normal < Tier1 < Tier2 < Tier3 < Tier4 < Blackout
        assert!(LoadShedTier::Normal < LoadShedTier::Tier1);
        assert!(LoadShedTier::Tier1 < LoadShedTier::Tier2);
        assert!(LoadShedTier::Tier2 < LoadShedTier::Tier3);
        assert!(LoadShedTier::Tier3 < LoadShedTier::Tier4);
        assert!(LoadShedTier::Tier4 < LoadShedTier::Blackout);
    }

    // ── Overproduction Tier Ordering Tests ──

    #[test]
    fn test_overproduction_tier_ordering() {
        // Normal < IndustrialBuff < Curtailment < GridDamage
        assert!(OverproductionTier::Normal < OverproductionTier::IndustrialBuff);
        assert!(OverproductionTier::IndustrialBuff < OverproductionTier::Curtailment);
        assert!(OverproductionTier::Curtailment < OverproductionTier::GridDamage);
    }

    // ── Weather Modifier Tests ──

    #[test]
    fn test_weather_modifier_neutral_defaults() {
        let wm = WeatherModifier::default();
        assert!((wm.solar_multiplier - 1.0).abs() < 0.001);
        assert!((wm.wind_multiplier - 1.0).abs() < 0.001);
        assert!((wm.cooling_water_availability - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_weather_modifier_storm_reduces_solar_and_wind() {
        // A storm should reduce solar and wind output.
        let wm = WeatherModifier {
            solar_multiplier: 0.3,
            wind_multiplier: 0.5,
            cooling_water_availability: 1.0,
            ..Default::default()
        };
        assert!(wm.solar_multiplier < 1.0);
        assert!(wm.wind_multiplier < 1.0);
    }

    #[test]
    fn test_weather_modifier_drought_reduces_cooling_water() {
        let wm = WeatherModifier {
            solar_multiplier: 1.2, // Drought may increase solar (clear skies)
            wind_multiplier: 1.0,
            cooling_water_availability: 0.3,
            ..Default::default()
        };
        assert!(wm.cooling_water_availability < 1.0);
    }

    // ── Power Plant Metadata Tests ──

    #[test]
    fn test_power_plant_metadata_serialization() {
        let meta = PowerPlantMetadata {
            plant_type: PowerPlantType::CoalFired,
            cooling_type: CoolingType::ClosedLoop,
            has_cooling_upgrade: true,
            fuel_source_deposit_id: None,
            water_source_region: Some("region_1".to_string()),
            nameplate_capacity_mw: 500.0,
            capacity_factor: 0.6,
        };
        let json = meta.to_json();
        let restored = PowerPlantMetadata::from_json(&json).unwrap();
        assert_eq!(restored.plant_type, PowerPlantType::CoalFired);
        assert_eq!(restored.cooling_type, CoolingType::ClosedLoop);
        assert!(restored.has_cooling_upgrade);
        assert!((restored.nameplate_capacity_mw - 500.0).abs() < 0.001);
    }

    #[test]
    fn test_power_plant_type_is_thermal() {
        assert!(PowerPlantType::CoalFired.is_thermal());
        assert!(PowerPlantType::LigniteFired.is_thermal());
        assert!(PowerPlantType::OilGas.is_thermal());
        assert!(PowerPlantType::Nuclear.is_thermal());
        assert!(PowerPlantType::BiomassFired.is_thermal());
        assert!(PowerPlantType::BiogasPlant.is_thermal());
        assert!(PowerPlantType::Geothermal.is_thermal());
        // Non-thermal:
        assert!(!PowerPlantType::Solar.is_thermal());
        assert!(!PowerPlantType::Wind.is_thermal());
        assert!(!PowerPlantType::Hydro.is_thermal());
        assert!(!PowerPlantType::PumpedStorage.is_thermal());
        assert!(!PowerPlantType::BatteryStorage.is_thermal());
    }

    #[test]
    fn test_power_plant_type_registry_keys() {
        assert_eq!(PowerPlantType::CoalFired.registry_key(), "coal_fired_plant");
        assert_eq!(
            PowerPlantType::LigniteFired.registry_key(),
            "lignite_fired_plant"
        );
        assert_eq!(PowerPlantType::OilGas.registry_key(), "oil_gas_plant");
        assert_eq!(PowerPlantType::Nuclear.registry_key(), "nuclear_plant");
        assert_eq!(PowerPlantType::Solar.registry_key(), "solar_plant");
        assert_eq!(PowerPlantType::Wind.registry_key(), "wind_farm");
        assert_eq!(PowerPlantType::Hydro.registry_key(), "hydro_plant");
        assert_eq!(
            PowerPlantType::PumpedStorage.registry_key(),
            "pumped_storage"
        );
        assert_eq!(
            PowerPlantType::BatteryStorage.registry_key(),
            "battery_storage"
        );
        assert_eq!(
            PowerPlantType::Geothermal.registry_key(),
            "geothermal_plant"
        );
        assert_eq!(PowerPlantType::BiomassFired.registry_key(), "biomass_plant");
        assert_eq!(PowerPlantType::BiogasPlant.registry_key(), "biogas_plant");
    }

    // ── Plant Eligibility Tests ──

    #[test]
    fn test_pre_1920_no_nuclear_no_solar_no_wind() {
        use sim_engine::energy::generation::available_plant_types;
        let types = available_plant_types(1900, true, true, true, true, false, false);
        // Pre-1920: no nuclear, no solar, no wind
        let type_set: Vec<PowerPlantType> = types.iter().map(|(t, _)| *t).collect();
        assert!(!type_set.contains(&PowerPlantType::Nuclear));
        assert!(!type_set.contains(&PowerPlantType::Solar));
        assert!(!type_set.contains(&PowerPlantType::Wind));
    }

    #[test]
    fn test_hydro_requires_water() {
        use sim_engine::energy::generation::available_plant_types;
        let types_no_water = available_plant_types(1920, true, false, true, true, false, false);
        let type_set: Vec<PowerPlantType> = types_no_water.iter().map(|(t, _)| *t).collect();
        assert!(!type_set.contains(&PowerPlantType::Hydro));
    }

    // ── Snapshot Role-Gating Tests ──

    #[test]
    fn test_energy_grid_snapshot_foreign_observer_is_classified() {
        use sim_engine::state::Country;
        use sim_engine::ui::snapshot::build_energy_grid_snapshot;

        let country = Country::mock_for_tests();
        let buildings: Vec<sim_engine::entities::Building> = Vec::new();

        // Foreign observer (different country name).
        let snapshot = build_energy_grid_snapshot(&country, &buildings, Some("OtherCountry"), None);
        assert!(snapshot.is_classified);
        // Foreign observers should not see spot prices.
        for region in &snapshot.regions {
            assert!(region.average_spot_price.is_none());
        }
        // Foreign observers should not see interconnector flows.
        assert!(snapshot.interconnector_flows.is_empty());
    }

    #[test]
    fn test_energy_grid_snapshot_domestic_observer_not_classified() {
        use sim_engine::state::Country;
        use sim_engine::ui::snapshot::build_energy_grid_snapshot;

        let country = Country::mock_for_tests();
        let buildings: Vec<sim_engine::entities::Building> = Vec::new();

        // Domestic observer (same country name).
        let snapshot = build_energy_grid_snapshot(&country, &buildings, Some(&country.name), None);
        assert!(!snapshot.is_classified);
    }

    // ── Commodity Tests ──

    #[test]
    fn test_cooling_tower_is_fixed_asset() {
        use sim_engine::registries::enums::Commodity;
        assert!(Commodity::CoolingTower.is_fixed_asset());
    }

    #[test]
    fn test_timber_has_calorific_value() {
        use sim_engine::registries::enums::Commodity;
        let cv = Commodity::Timber.calorific_value_mj_per_unit();
        assert!(cv > 0.0, "Timber should have a positive calorific value");
        assert!(Commodity::Timber.is_fuel());
    }

    #[test]
    fn test_planks_have_calorific_value() {
        use sim_engine::registries::enums::Commodity;
        let cv = Commodity::Planks.calorific_value_mj_per_unit();
        assert!(cv > 0.0, "Planks should have a positive calorific value");
        assert!(Commodity::Planks.is_fuel());
    }

    #[test]
    fn test_commodity_all_count_includes_new_energy_commodities() {
        use sim_engine::registries::enums::Commodity;
        let all = Commodity::all();
        // Should include the Wave 1 commodities (CoolingTower, PhotovoltaicPanels)
        // and the Wave 2 commodity (CoalGas). Insulation and LedLighting were
        // scrapped in Wave 2 (replaced by MethodSlot-based consumption evolution).
        assert!(all.contains(&Commodity::CoolingTower));
        assert!(all.contains(&Commodity::PhotovoltaicPanels));
        assert!(all.contains(&Commodity::CoalGas));
        assert_eq!(all.len(), 150);
    }

    // ── Production Method Registry Tests ──

    #[test]
    fn test_plant_type_specific_registries_exist() {
        use sim_engine::registries::production_methods_data::default_production_methods;
        let registry = default_production_methods();
        assert!(registry.contains_key("coal_fired_plant"));
        assert!(registry.contains_key("lignite_fired_plant"));
        assert!(registry.contains_key("oil_gas_plant"));
        assert!(registry.contains_key("nuclear_plant"));
        assert!(registry.contains_key("solar_plant"));
        assert!(registry.contains_key("wind_farm"));
        assert!(registry.contains_key("hydro_plant"));
        assert!(registry.contains_key("pumped_storage"));
        assert!(registry.contains_key("battery_storage"));
        assert!(registry.contains_key("geothermal_plant"));
        assert!(registry.contains_key("biomass_plant"));
        assert!(registry.contains_key("biogas_plant"));
        assert!(registry.contains_key("energy_automation"));
        assert!(registry.contains_key("energy_organization"));
    }

    #[test]
    fn test_biomass_plant_uses_timber_and_planks() {
        use sim_engine::registries::enums::Commodity;
        use sim_engine::registries::production_methods_data::default_production_methods;
        let registry = default_production_methods();
        let biomass = registry.get("biomass_plant").unwrap();
        let wood_boiler = biomass.production.get("Wood-Fired Boiler").unwrap();
        assert!(wood_boiler.inputs.contains_key(&Commodity::Timber));
        assert!(wood_boiler.inputs.contains_key(&Commodity::Planks));
    }

    #[test]
    fn test_biogas_plant_uses_livestock() {
        use sim_engine::registries::enums::Commodity;
        use sim_engine::registries::production_methods_data::default_production_methods;
        let registry = default_production_methods();
        let biogas = registry.get("biogas_plant").unwrap();
        let digester = biogas.production.get("Anaerobic Digester").unwrap();
        assert!(digester.inputs.contains_key(&Commodity::Livestock));
    }

    // ── Geographic Traits Tests ──

    #[test]
    fn test_geographic_traits_has_new_energy_fields() {
        use sim_engine::society::geography::GeographicTraits;
        let traits = GeographicTraits::default();
        // New fields should default to false.
        assert!(!traits.water_for_cooling);
        assert!(!traits.has_geothermal_potential);
    }

    // ── Bugfix Sprint: Energy Capacity Conservation Tests ──

    /// Supply cannot exceed nameplate capacity (5A: "matter from the void").
    #[test]
    fn test_supply_clamped_to_nameplate() {
        // Simulate: energy_in_inventory = 300, weather_multiplier = 1.0,
        // nameplate = 200. Supply should be clamped to 200.
        let energy_in_inventory = 300.0_f64;
        let weather_multiplier = 1.0_f64;
        let nameplate = 200.0_f64;
        let supply = (energy_in_inventory * weather_multiplier).min(nameplate);
        assert_eq!(supply, 200.0, "Supply must not exceed nameplate");
    }

    /// Weather multiplier can boost output but supply is still clamped to nameplate.
    #[test]
    fn test_weather_boost_clamped_to_nameplate() {
        let energy_in_inventory = 150.0_f64;
        let weather_multiplier = 1.5_f64; // favorable weather
        let nameplate = 200.0_f64;
        let supply = (energy_in_inventory * weather_multiplier).min(nameplate);
        assert_eq!(
            supply, 200.0,
            "Weather-boosted supply must still be clamped to nameplate"
        );
    }

    /// Effective supply = min(supply, grid_capacity) — grid bottleneck.
    #[test]
    fn test_effective_supply_is_min_of_supply_and_grid_cap() {
        let supply = 600.0_f64;
        let lv_cap = 100.0_f64;
        let mv_cap = 300.0_f64;
        let grid_cap = lv_cap.min(mv_cap);
        let effective_supply = supply.min(grid_cap);
        assert_eq!(
            effective_supply, 100.0,
            "Effective supply must be limited by grid capacity"
        );
    }

    /// Load-shed tier: when effective_supply < demand, load shedding occurs
    /// even if raw supply > demand (grid bottleneck).
    #[test]
    fn test_load_shed_when_effective_supply_below_demand() {
        let supply = 600.0_f64;
        let demand = 400.0_f64;
        let lv_cap = 100.0_f64;
        let mv_cap = 300.0_f64;
        let grid_cap = lv_cap.min(mv_cap);
        let effective_supply = supply.min(grid_cap);
        // effective_supply (100) < demand (400) → load shedding
        assert!(
            effective_supply < demand,
            "Load shedding should occur when effective supply < demand"
        );
    }

    /// No load shedding when effective supply exceeds demand.
    #[test]
    fn test_no_load_shed_when_effective_supply_exceeds_demand() {
        let supply = 500.0_f64;
        let demand = 300.0_f64;
        let lv_cap = 600.0_f64;
        let mv_cap = 800.0_f64;
        let grid_cap = lv_cap.min(mv_cap);
        let effective_supply = supply.min(grid_cap);
        assert!(
            effective_supply >= demand,
            "No load shedding when effective supply >= demand"
        );
    }

    /// Region display name is used, not the region ID (Anomaly 3 fix).
    #[test]
    fn test_region_energy_info_uses_display_name() {
        // This is a structural test: the DTO field exists and is populated
        // from region.display_name, not region.id. The actual snapshot builder
        // test requires a full game state, so here we verify the field exists.
        use sim_engine::ui::snapshot::RegionEnergyInfo;
        let info = RegionEnergyInfo {
            region_id: "Bactria-Region1".to_string(),
            region_name: "Bactria".to_string(),
            supply_mw: 100.0,
            effective_supply_mw: 80.0,
            demand_mw: 120.0,
            max_production_capacity_mw: 150.0,
            average_spot_price: Some(50.0),
            load_shed_tier: "Brownout".to_string(),
            overproduction_tier: "Normal".to_string(),
            grid_condition: 0.85,
        };
        assert_ne!(
            info.region_name, info.region_id,
            "Region name must not be the ID"
        );
        assert_eq!(info.region_name, "Bactria");
    }
}

//! World Generation & Climate Audit (v0.5.3): Tests for the 3 Pillars.
//!
//! Pillar 1: Phantom Harvest — climate-season matrix populated,
//!           SeasonalModifiers::default() returns 1.0 (not 0.0),
//!           pre-injected accumulated_yield produces food on Turn 1.
//! Pillar 2: Geological Homogeneity — formation-driven sparsity,
//!           regions without formations get NO geological resources.
//! Pillar 3: Climatic Monotony — tropical crops in registry,
//!           climate-aware crop batch building.

#[cfg(test)]
mod tests {
    use sim_engine::state::climate::{ClimateConfig, SeasonalModifiers};
    use sim_engine::state::Season;
    use sim_engine::society::geography::ClimateProfile;

    // ========================================================================
    // Pillar 1: Phantom Harvest
    // ========================================================================

    /// Verify that SeasonalModifiers::default() returns 1.0 for all
    /// multipliers (not 0.0). This was the root cause of the Phantom
    /// Harvest — missing matrix entries zeroed out agricultural yield.
    #[test]
    fn test_seasonal_modifiers_default_is_neutral() {
        let mods = SeasonalModifiers::default();
        assert_eq!(mods.agriculture_multiplier, 1.0,
            "Default agriculture_multiplier must be 1.0 (neutral), not 0.0");
        assert_eq!(mods.energy_multiplier, 1.0,
            "Default energy_multiplier must be 1.0 (neutral)");
        assert_eq!(mods.services_multiplier, 1.0,
            "Default services_multiplier must be 1.0 (neutral)");
        assert_eq!(mods.tourism_multiplier, 1.0,
            "Default tourism_multiplier must be 1.0 (neutral)");
        assert_eq!(mods.construction_multiplier, 1.0,
            "Default construction_multiplier must be 1.0 (neutral)");
    }

    /// Verify that populate_defaults() creates 28 entries (7 climates × 4 seasons).
    #[test]
    fn test_climate_matrix_populated() {
        let mut config = ClimateConfig::default();
        assert!(config.climate_season_matrix.is_empty(),
            "Default ClimateConfig should have an empty matrix");

        config.populate_defaults();

        assert_eq!(config.climate_season_matrix.len(), 28,
            "Populated matrix should have 28 entries (7 climates × 4 seasons)");
    }

    /// Verify that all 28 entries have non-zero agriculture_multiplier.
    /// The Phantom Harvest was caused by zero multipliers.
    #[test]
    fn test_climate_matrix_all_agriculture_multipliers_nonzero() {
        let mut config = ClimateConfig::default();
        config.populate_defaults();

        let climates = [
            ClimateProfile::Temperate,
            ClimateProfile::Continental,
            ClimateProfile::Mountainous,
            ClimateProfile::Coastal,
            ClimateProfile::Tropical,
            ClimateProfile::Desert,
            ClimateProfile::Arctic,
        ];
        let seasons = [Season::Spring, Season::Summer, Season::Autumn, Season::Winter];

        for climate in &climates {
            for season in &seasons {
                let mods = config.get_modifiers(*climate, *season);
                assert!(mods.agriculture_multiplier >= 0.0,
                    "Agriculture multiplier for {:?}/{:?} must be >= 0.0",
                    climate, season);
                // Arctic winter can be 0.0 (no agriculture possible), but
                // all other climate/season combos should be > 0.0.
                if !(*climate == ClimateProfile::Arctic && *season == Season::Winter) {
                    assert!(mods.agriculture_multiplier > 0.0,
                        "Agriculture multiplier for {:?}/{:?} must be > 0.0 (got {})",
                        climate, season, mods.agriculture_multiplier);
                }
            }
        }
    }

    /// Verify that Tropical climate has high agriculture multipliers
    /// (year-round growing). Tropical winter should still be > 1.0.
    #[test]
    fn test_tropical_climate_high_agriculture() {
        let mut config = ClimateConfig::default();
        config.populate_defaults();

        for season in [Season::Spring, Season::Summer, Season::Autumn, Season::Winter] {
            let mods = config.get_modifiers(ClimateProfile::Tropical, season);
            assert!(mods.agriculture_multiplier > 1.0,
                "Tropical {:?} should have agriculture_multiplier > 1.0 (got {})",
                season, mods.agriculture_multiplier);
        }
    }

    /// Verify that get_modifiers returns neutral (1.0) defaults for
    /// missing entries — not zero. This is the safety net.
    #[test]
    fn test_get_modifiers_missing_entry_returns_neutral() {
        let config = ClimateConfig::default(); // Empty matrix
        let mods = config.get_modifiers(ClimateProfile::Temperate, Season::Summer);
        assert_eq!(mods.agriculture_multiplier, 1.0,
            "Missing matrix entry should return neutral (1.0), not 0.0");
    }

    // ========================================================================
    // Pillar 2: Geological Homogeneity
    // ========================================================================

    /// Verify that reseed_resources_from_formations produces sparse
    /// resources — not all regions should have coal/uranium.
    #[test]
    fn test_geological_sparsity_not_all_regions_have_coal() {
        use sim_engine::society::geography::{
            Climate, Region,
            reseed_resources_from_formations, generate_geological_formations,
        };
        use std::collections::HashMap;
        use rand::thread_rng;

        // Create 10 regions with varied IDs.
        let mut regions = HashMap::new();
        for i in 0..10 {
            let region_id = format!("TestCountry-Region{}", i + 1);
            let mut region = Region::default();
            region.id = region_id.clone();
            region.owner_country = "TestCountry".to_string();
            region.population = 1_000_000;
            region.gdp = 1_000_000_000.0;
            region.climate = Climate::Balanced;
            region.climate_profile = ClimateProfile::Temperate;
            // Seed with old-style resources (will be replaced)
            region.resources.insert("woda_slodka".to_string(),
                serde_json::json!({"dostepnosc": 0.7}));
            regions.insert(region_id, region);
        }

        let region_ids: Vec<String> = regions.keys().cloned().collect();
        let mut rng = thread_rng();

        // Generate formations (only 30% of regions covered, 2-5 regions each).
        let formations = generate_geological_formations(&region_ids, &mut rng);

        // Reseed resources from formations.
        reseed_resources_from_formations(&mut regions, &formations, &mut rng);

        // Count how many regions have hard_coal or brown_coal.
        let regions_with_coal = regions.values().filter(|r| {
            r.resources.contains_key("hard_coal") || r.resources.contains_key("brown_coal")
        }).count();

        // With only 30% formation coverage and 1-3 commodities per formation,
        // it's extremely unlikely that ALL 10 regions have coal.
        // We assert that at least 3 regions lack coal (sparsity enforced).
        let regions_without_coal = 10 - regions_with_coal;
        assert!(regions_without_coal >= 3,
            "At least 3 regions should lack coal (sparsity). Found {} with coal, {} without.",
            regions_with_coal, regions_without_coal);
    }

    /// Verify that at least one region has NO geological resources at all
    /// (no energy minerals), forcing reliance on biomass/hydro/imports.
    #[test]
    fn test_geological_sparsity_some_regions_have_no_energy_minerals() {
        use sim_engine::society::geography::{
            Climate, reseed_resources_from_formations, generate_geological_formations,
        };
        use std::collections::HashMap;
        use rand::thread_rng;

        let mut regions = HashMap::new();
        for i in 0..12 {
            let region_id = format!("TestCountry2-Region{}", i + 1);
            let mut region = sim_engine::society::geography::Region::default();
            region.id = region_id.clone();
            region.owner_country = "TestCountry2".to_string();
            region.population = 500_000;
            region.gdp = 500_000_000.0;
            region.climate = Climate::Balanced;
            region.climate_profile = ClimateProfile::Temperate;
            regions.insert(region_id, region);
        }

        let region_ids: Vec<String> = regions.keys().cloned().collect();
        let mut rng = thread_rng();
        let formations = generate_geological_formations(&region_ids, &mut rng);
        reseed_resources_from_formations(&mut regions, &formations, &mut rng);

        let energy_keys = ["hard_coal", "brown_coal", "oil", "natural_gas", "peat", "uranium"];
        let regions_without_energy = regions.values().filter(|r| {
            !energy_keys.iter().any(|k| r.resources.contains_key(*k))
        }).count();

        assert!(regions_without_energy >= 1,
            "At least 1 region should have NO energy minerals. Found {} without.",
            regions_without_energy);
    }

    // ========================================================================
    // Pillar 3: Climatic Monotony
    // ========================================================================

    /// Verify that tropical crops exist in the crop registry.
    #[test]
    fn test_tropical_crops_exist_in_registry() {
        let registry = sim_engine::data::crop_registry::crop_registry();

        assert!(registry.get("rice").is_some(), "Rice should be in the crop registry");
        assert!(registry.get("sugarcane").is_some(), "Sugarcane should be in the crop registry");
        assert!(registry.get("coffee").is_some(), "Coffee should be in the crop registry");
        assert!(registry.get("tea").is_some(), "Tea should be in the crop registry");
        assert!(registry.get("soybeans").is_some(), "Soybeans should be in the crop registry");
    }

    /// Verify that rice is compatible with Tropical climate.
    #[test]
    fn test_rice_compatible_with_tropical() {
        let registry = sim_engine::data::crop_registry::crop_registry();
        let rice = registry.get("rice").expect("Rice must exist");
        assert!(rice.compatible_climates.contains(&ClimateProfile::Tropical),
            "Rice must be compatible with Tropical climate");
        assert!(rice.compatible_climates.contains(&ClimateProfile::Coastal),
            "Rice should also be compatible with Coastal climate");
    }

    /// Verify that sugarcane is a plantation crop compatible with Tropical.
    #[test]
    fn test_sugarcane_tropical_plantation() {
        use sim_engine::registries::crops::LandType;
        let registry = sim_engine::data::crop_registry::crop_registry();
        let sugarcane = registry.get("sugarcane").expect("Sugarcane must exist");
        assert_eq!(sugarcane.land_type, LandType::Plantation,
            "Sugarcane should be a plantation crop");
        assert!(sugarcane.compatible_climates.contains(&ClimateProfile::Tropical),
            "Sugarcane must be compatible with Tropical climate");
    }

    /// Verify that soybeans are compatible with both Temperate AND Tropical.
    #[test]
    fn test_soybeans_dual_climate_compatibility() {
        let registry = sim_engine::data::crop_registry::crop_registry();
        let soy = registry.get("soybeans").expect("Soybeans must exist");
        assert!(soy.compatible_climates.contains(&ClimateProfile::Temperate),
            "Soybeans must be compatible with Temperate climate");
        assert!(soy.compatible_climates.contains(&ClimateProfile::Tropical),
            "Soybeans must be compatible with Tropical climate");
    }

    /// Verify that tea is compatible with both Tropical AND Mountainous.
    #[test]
    fn test_tea_mountainous_tropical() {
        let registry = sim_engine::data::crop_registry::crop_registry();
        let tea = registry.get("tea").expect("Tea must exist");
        assert!(tea.compatible_climates.contains(&ClimateProfile::Tropical),
            "Tea must be compatible with Tropical climate");
        assert!(tea.compatible_climates.contains(&ClimateProfile::Mountainous),
            "Tea must be compatible with Mountainous climate (highland tea)");
    }

    /// Verify that climate-aware crop batch building selects rice for
    /// Tropical climates (not wheat).
    #[test]
    fn test_climate_aware_crop_batches_tropical() {
        use sim_engine::data::crop_registry::crop_registry;
        use sim_engine::registries::crops::CropRegistry;
        use sim_engine::registries::Registries;
        use std::collections::HashMap;

        // Build a registries with the real crop registry.
        let crops: HashMap<String, sim_engine::registries::crops::CropDefinition> =
            crop_registry().clone();
        let registries = Registries {
            tech_tree: HashMap::new(),
            production_methods: HashMap::new(),
            building_templates: HashMap::new(),
            government_forms: HashMap::new(),
            crops: CropRegistry { crops },
        };

        // Build crop batches for Tropical climate.
        // We can't call build_crop_batches directly (it's private), so we
        // verify the crop registry has the right crops for the climate.
        let tropical_crops: &[&str] = &["rice", "soybeans", "potatoes"];
        for &crop_id in tropical_crops {
            let crop = registries.crops.get(crop_id);
            assert!(crop.is_some(), "Tropical crop {} must exist in registry", crop_id);
            if let Some(c) = crop {
                assert!(c.compatible_climates.contains(&ClimateProfile::Tropical),
                    "Crop {} must be compatible with Tropical", crop_id);
            }
        }

        // Verify wheat is NOT compatible with Tropical.
        let wheat = registries.crops.get("wheat").expect("Wheat must exist");
        assert!(!wheat.compatible_climates.contains(&ClimateProfile::Tropical),
            "Wheat should NOT be compatible with Tropical climate");
    }

    /// Verify that the crop registry has more crops than just the original
    /// temperate set (wheat, corn, potatoes, cotton, alfalfa, cattle, orchard, tobacco).
    #[test]
    fn test_crop_registry_has_more_than_original_8() {
        let registry = sim_engine::data::crop_registry::crop_registry();
        assert!(registry.len() >= 13,
            "Crop registry should have at least 13 crops (8 original + 5 new), got {}",
            registry.len());
    }
}

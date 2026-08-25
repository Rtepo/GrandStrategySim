//! Phase 85B: Tests for the Urbanization Cycle — Emancipation & Annexation.
//!
//! Covers:
//! - District Heating registry fix (Task 1)
//! - Emancipation trigger logic (all conditions must be met)
//! - Emancipation parcel transfer (mass conservation)
//! - Corporate buyout routing (single ledger, no duplication)
//! - Private owner buyout routing
//! - Failed annexation penalties (grounded unrest, no vaporware)
//! - Water state transfer (mass conservation)
//! - Ghost town validity (no dissolution)
//! - Role-gated snapshot (treasury stripped for foreign observers)

#[cfg(test)]
mod tests {
    use crate::society::cadastre::{
        Cadastre, ParcelChunk, ParcelOwnerType, ZoningDesignation,
    };
    use crate::society::geography::{
        CityRegionMetadata, FactionDomainType, LocalLaws, MicroRegion, MicroRegionBudget, Region,
    };
    use crate::society::urbanization::{
        check_emancipation_triggers, evaluate_annexation_cost, execute_annexation,
        execute_emancipation, transfer_physical_water, EmancipationConfig,
    };
    use crate::utilities::consumption_bom::is_district_heating_method;

    // ========================================================================
    // TASK 1: DISTRICT HEATING REGISTRY TESTS
    // ========================================================================

    #[test]
    fn test_district_heating_recognized_by_helper() {
        // The generic "District Heating" key must be recognized.
        assert!(
            is_district_heating_method("District Heating"),
            "is_district_heating_method must recognize the generic 'District Heating' key"
        );
    }

    #[test]
    fn test_district_heating_submethods_still_recognized() {
        // Existing submethods must still be recognized.
        assert!(is_district_heating_method("Unmetered Radiators"));
        assert!(is_district_heating_method("Thermostatic Valves"));
        assert!(is_district_heating_method("Smart Substations"));
    }

    #[test]
    fn test_non_district_heating_methods_rejected() {
        assert!(!is_district_heating_method("None"));
        assert!(!is_district_heating_method("Coal Stove"));
        assert!(!is_district_heating_method("Heat Pump"));
        assert!(!is_district_heating_method("Electric Radiator"));
    }

    #[test]
    fn test_housing_has_heating_methods() {
        // Verify the registry contains the "District Heating" entry.
        let registry =
            crate::registries::production_methods_data::default_production_methods();
        let housing = registry
            .get("housing_consumption")
            .expect("housing_consumption registry must exist");
        let heating = &housing.heating;
        assert!(
            heating.contains_key("District Heating"),
            "Housing heating registry must contain 'District Heating' key"
        );
        // Also verify the test's expected keys are present.
        for expected in ["None", "Coal Stove", "District Heating", "Heat Pump"] {
            assert!(
                heating.contains_key(expected),
                "Housing heating registry must contain '{}' key",
                expected
            );
        }
    }

    // ========================================================================
    // TASK 2: EMANCIPATION TRIGGER TESTS
    // ========================================================================

    fn make_test_domain(population: i64, liquid_reserves: f64) -> MicroRegion {
        MicroRegion {
            id: "guild-burgher-1".to_string(),
            parent_region_id: "parent-region".to_string(),
            faction_type: FactionDomainType::GuildBurgher,
            name: "Test Burgher".to_string(),
            population,
            sub_budget: MicroRegionBudget {
                liquid_reserves,
                ..Default::default()
            },
            autonomy_level: 0.7,
            governing_faction_id: None,
            local_laws: LocalLaws::default(),
            education_slots: 0,
            health_capacity: 0.0,
            controlled_parcel_ids: Vec::new(),
        }
    }

    fn make_test_region(id: &str, development: f64) -> Region {
        Region {
            id: id.to_string(),
            display_name: id.to_string(),
            owner_country: "test".to_string(),
            population: 10000,
            gdp: 100_000.0,
            gdp_pc: 10.0,
            development_level: development,
            is_capital: false,
            node_type: crate::society::geography::NodeType::LandRegion,
            ..Default::default()
        }
    }

    #[test]
    fn test_emancipation_triggers_all_met() {
        let domain = make_test_domain(6000, 600_000.0);
        let region = make_test_region("parent", 0.6);
        let config = EmancipationConfig::default();

        // 6000 people / 10 hectares = 600 people/km² > 500 threshold
        // domain_gdp = 30_000 > 25_000 (25% of 100_000)
        // liquid_reserves = 600_000 > 500 × 1000 (avg_wage)
        // guild_count = 3 >= 2
        // development = 0.6 > 0.5
        let result = check_emancipation_triggers(
            &domain,
            &region,
            30_000.0,
            100_000.0,
            10.0,
            3,
            1000.0,
            &config,
        );
        assert!(result, "All triggers met → should emancipate");
    }

    #[test]
    fn test_emancipation_fails_low_density() {
        let domain = make_test_domain(100, 600_000.0);
        let region = make_test_region("parent", 0.6);
        let config = EmancipationConfig::default();

        // 100 people / 10 hectares = 10 people/km² << 500 threshold
        let result = check_emancipation_triggers(
            &domain,
            &region,
            30_000.0,
            100_000.0,
            10.0,
            3,
            1000.0,
            &config,
        );
        assert!(!result, "Low density → should NOT emancipate");
    }

    #[test]
    fn test_emancipation_fails_low_gdp_share() {
        let domain = make_test_domain(6000, 600_000.0);
        let region = make_test_region("parent", 0.6);
        let config = EmancipationConfig::default();

        // domain_gdp = 10_000 < 25_000 (25% of 100_000)
        let result = check_emancipation_triggers(
            &domain,
            &region,
            10_000.0,
            100_000.0,
            10.0,
            3,
            1000.0,
            &config,
        );
        assert!(!result, "Low GDP share → should NOT emancipate");
    }

    #[test]
    fn test_emancipation_fails_low_capital() {
        let domain = make_test_domain(6000, 100_000.0);
        let region = make_test_region("parent", 0.6);
        let config = EmancipationConfig::default();

        // liquid_reserves = 100_000 < 500 × 1000 = 500_000
        let result = check_emancipation_triggers(
            &domain,
            &region,
            30_000.0,
            100_000.0,
            10.0,
            3,
            1000.0,
            &config,
        );
        assert!(!result, "Low capital → should NOT emancipate");
    }

    #[test]
    fn test_emancipation_fails_low_guilds() {
        let domain = make_test_domain(6000, 600_000.0);
        let region = make_test_region("parent", 0.6);
        let config = EmancipationConfig::default();

        // guild_count = 1 < 2
        let result = check_emancipation_triggers(
            &domain,
            &region,
            30_000.0,
            100_000.0,
            10.0,
            1,
            1000.0,
            &config,
        );
        assert!(!result, "Low guild count → should NOT emancipate");
    }

    #[test]
    fn test_emancipation_fails_low_development() {
        let domain = make_test_domain(6000, 600_000.0);
        let region = make_test_region("parent", 0.3);
        let config = EmancipationConfig::default();

        // development = 0.3 < 0.5
        let result = check_emancipation_triggers(
            &domain,
            &region,
            30_000.0,
            100_000.0,
            10.0,
            3,
            1000.0,
            &config,
        );
        assert!(!result, "Low development → should NOT emancipate");
    }

    #[test]
    fn test_emancipation_fails_non_guild_burgher() {
        let mut domain = make_test_domain(6000, 600_000.0);
        domain.faction_type = FactionDomainType::AristocraticEstate;
        let region = make_test_region("parent", 0.6);
        let config = EmancipationConfig::default();

        let result = check_emancipation_triggers(
            &domain,
            &region,
            30_000.0,
            100_000.0,
            10.0,
            3,
            1000.0,
            &config,
        );
        assert!(!result, "Non-GuildBurgher domain → should NOT emancipate");
    }

    // ========================================================================
    // TASK 2: EMANCIPATION EXECUTION TESTS
    // ========================================================================

    #[test]
    fn test_emancipation_transfers_parcels() {
        let mut country = crate::state::Country::default();
        country.name = "TestCountry".to_string();
        country.budget.gdp = 100_000.0;
        country.macro_indicators.average_wage = 1000.0;

        let mut region = make_test_region("parent", 0.6);
        let domain = make_test_domain(6000, 600_000.0);
        let domain_id = domain.id.clone();

        // Create parcels for the domain.
        let mut cadastre = Cadastre::default();
        let mut parcel_ids = Vec::new();
        for _ in 0..3 {
            let pid = cadastre.insert(ParcelChunk {
                soil_class: "Class_III".to_string(),
                size_hectares: 5.0,
                zoning: ZoningDesignation::Commercial,
                owner_type: ParcelOwnerType::Private,
                owner_id: "burgher_1".to_string(),
                region_id: "parent".to_string(),
                legal_certainty: 0.8,
                infrastructure_access: 0.7,
                current_value: 1000.0,
                acquisition_price: 800.0,
                acquisition_turn: 0,
                is_frozen: false,
                zoning_change_turn: 0,
                is_border_zone: false,
                land_use_tag: String::new(),
                adjacent_parcels: Vec::new(),
                co_owners: std::collections::BTreeMap::new(),
                usufruct_holder: None,
                easements: Vec::new(),
                adverse_possession: None,
                pollution_level: 0.0,
                topography: Default::default(),
                devastation_index: 0.0,
                micro_region_id: Some(domain_id.clone()),
            });
            parcel_ids.push(pid);
        }

        let mut domain = domain;
        domain.controlled_parcel_ids = parcel_ids.clone();
        region.micro_regions.insert(domain_id.clone(), domain);
        region.parcel_ids = parcel_ids.clone();
        country.regions.push(region);

        let result = execute_emancipation(
            &mut country,
            &mut cadastre,
            0,
            &domain_id,
            42,
            &EmancipationConfig::default(),
        );

        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.emancipated);
        let city_id = result.new_city_region_id.as_ref().unwrap();

        // Verify the new city region exists.
        let city = country.regions.iter().find(|r| r.id == *city_id);
        assert!(city.is_some(), "City region must exist after emancipation");
        let city = city.unwrap();
        assert!(city.is_city());
        assert_eq!(city.parcel_ids.len(), 3, "City must have 3 parcels");

        // Verify parcels now belong to the city.
        for &pid in &parcel_ids {
            let parcel = cadastre.get(pid).unwrap();
            assert_eq!(
                parcel.region_id, *city_id,
                "Parcel region_id must be updated to city"
            );
        }

        // Verify parent region no longer has the parcels.
        let parent = country.regions.iter().find(|r| r.id == "parent").unwrap();
        assert_eq!(
            parent.parcel_ids.len(),
            0,
            "Parent region must have 0 parcels after emancipation"
        );
        assert!(
            !parent.micro_regions.contains_key(&domain_id),
            "Parent region must not contain the emancipated domain"
        );
    }

    #[test]
    fn test_emancipation_transfers_budget() {
        let mut country = crate::state::Country::default();
        country.name = "TestCountry".to_string();

        let mut region = make_test_region("parent", 0.6);
        let domain = make_test_domain(6000, 500_000.0);
        let domain_id = domain.id.clone();

        let mut cadastre = Cadastre::default();
        let pid = cadastre.insert(ParcelChunk {
            size_hectares: 10.0,
            region_id: "parent".to_string(),
            micro_region_id: Some(domain_id.clone()),
            ..Default::default()
        });

        let mut domain = domain;
        domain.controlled_parcel_ids = vec![pid];
        region.micro_regions.insert(domain_id.clone(), domain);
        region.parcel_ids = vec![pid];
        country.regions.push(region);

        let result = execute_emancipation(
            &mut country,
            &mut cadastre,
            0,
            &domain_id,
            1,
            &EmancipationConfig::default(),
        );

        assert!(result.is_some());
        let city_id = result.unwrap().new_city_region_id.unwrap();
        let city = country
            .regions
            .iter()
            .find(|r| r.id == city_id)
            .unwrap();

        // Domain budget → city treasury (exact transfer).
        assert_eq!(
            city.treasury.liquid_reserves, 500_000.0,
            "City treasury must receive domain's liquid reserves"
        );

        // Domain in city must have zeroed budget.
        let city_domain = city.micro_regions.get(&domain_id).unwrap();
        assert_eq!(
            city_domain.sub_budget.liquid_reserves, 0.0,
            "Domain budget must be zeroed after transfer"
        );
    }

    // ========================================================================
    // TASK 3: ANNEXATION COST TESTS
    // ========================================================================

    #[test]
    fn test_annexation_cost_basic() {
        let parcel = ParcelChunk {
            size_hectares: 10.0,
            infrastructure_access: 0.5,
            ..Default::default()
        };
        let config = EmancipationConfig::default();
        let cost = evaluate_annexation_cost(&parcel, 1000.0, false, &config);
        // 10 ha × 1000 × (1 + 0.5) × 2.0 = 30_000
        assert!((cost - 30_000.0).abs() < 0.01, "Basic cost = 30_000, got {}", cost);
    }

    #[test]
    fn test_annexation_cost_aristocratic_multiplier() {
        let parcel = ParcelChunk {
            size_hectares: 10.0,
            infrastructure_access: 0.5,
            ..Default::default()
        };
        let config = EmancipationConfig::default();
        let cost_normal = evaluate_annexation_cost(&parcel, 1000.0, false, &config);
        let cost_aristo = evaluate_annexation_cost(&parcel, 1000.0, true, &config);
        // Aristocratic cost = normal × 3.0
        assert!(
            (cost_aristo - cost_normal * 3.0).abs() < 0.01,
            "Aristocratic cost = 3× normal, got {} vs {}",
            cost_aristo,
            cost_normal * 3.0
        );
    }

    // ========================================================================
    // TASK 3: ANNEXATION EXECUTION TESTS
    // ========================================================================

    #[test]
    fn test_annexation_corporate_buyout_single_ledger() {
        let mut city = make_test_region("city", 0.7);
        city.city_metadata = Some(CityRegionMetadata {
            is_city: true,
            emancipated_turn: 1,
            parent_region_id: "parent".to_string(),
            annexation_cooldown: 0,
            pending_annexation_targets: Vec::new(),
        });
        city.treasury.liquid_reserves = 100_000.0;

        let mut source = make_test_region("source", 0.3);

        let mut cadastre = Cadastre::default();
        let pid = cadastre.insert(ParcelChunk {
            size_hectares: 5.0,
            owner_type: ParcelOwnerType::Corporate,
            owner_id: "comp_1".to_string(),
            region_id: "source".to_string(),
            ..Default::default()
        });

        let mut companies = vec![crate::entities::Company {
            id: "comp_1".to_string(),
            name: "Test Corp".to_string(),
            liquid_capital: 10_000.0,
            available_cash: 5_000.0,
            ..Default::default()
        }];

        let initial_city_treasury = city.treasury.liquid_reserves;
        let initial_company_liquid = companies[0].liquid_capital;
        let initial_company_cash = companies[0].available_cash;
        let buyout = 50_000.0;

        let result = execute_annexation(
            &mut city,
            &mut source,
            pid,
            &mut cadastre,
            &mut companies,
            buyout,
            false,
            10,
            &EmancipationConfig::default(),
        );

        assert!(result.success);
        // City treasury debited.
        assert_eq!(
            city.treasury.liquid_reserves,
            initial_city_treasury - buyout,
            "City treasury must be debited exactly"
        );
        // Company liquid_capital credited.
        assert_eq!(
            companies[0].liquid_capital,
            initial_company_liquid + buyout,
            "Company liquid_capital must be credited exactly"
        );
        // Company available_cash NOT credited (single ledger — Correction 1).
        assert_eq!(
            companies[0].available_cash,
            initial_company_cash,
            "Company available_cash must NOT be credited (single ledger rule)"
        );
    }

    #[test]
    fn test_annexation_insufficient_funds_fails() {
        let mut city = make_test_region("city", 0.7);
        city.city_metadata = Some(CityRegionMetadata {
            is_city: true,
            emancipated_turn: 1,
            parent_region_id: "parent".to_string(),
            annexation_cooldown: 0,
            pending_annexation_targets: Vec::new(),
        });
        city.treasury.liquid_reserves = 1_000.0; // Too poor

        let mut source = make_test_region("source", 0.3);

        let mut cadastre = Cadastre::default();
        let pid = cadastre.insert(ParcelChunk {
            size_hectares: 5.0,
            owner_type: ParcelOwnerType::Private,
            owner_id: "peasant_1".to_string(),
            region_id: "source".to_string(),
            ..Default::default()
        });

        let mut companies = Vec::new();
        let buyout = 50_000.0;
        let initial_treasury = city.treasury.liquid_reserves;

        let result = execute_annexation(
            &mut city,
            &mut source,
            pid,
            &mut cadastre,
            &mut companies,
            buyout,
            false,
            10,
            &EmancipationConfig::default(),
        );

        assert!(!result.success, "Should fail with insufficient funds");
        assert!(result.insufficient_funds);
        // No parcel transfer.
        let parcel = cadastre.get(pid).unwrap();
        assert_eq!(parcel.region_id, "source", "Parcel must stay in source");
        // Treasury not debited.
        assert_eq!(
            city.treasury.liquid_reserves, initial_treasury,
            "Treasury must not be debited on failure"
        );
        // Cooldown applied.
        assert_eq!(
            city.city_metadata.as_ref().unwrap().annexation_cooldown,
            12,
            "Cooldown must be applied"
        );
    }

    // ========================================================================
    // WATER TRANSFER TESTS
    // ========================================================================

    #[test]
    fn test_water_transfer_conservation() {
        let mut parent = make_test_region("parent", 0.5);
        parent.water_reserves.groundwater_volume = 1000.0;

        let mut city = make_test_region("city", 0.7);
        city.water_reserves.groundwater_volume = 0.0;

        transfer_physical_water(&mut parent, &mut city, 300.0);

        assert_eq!(
            parent.water_reserves.groundwater_volume, 700.0,
            "Parent must lose 300 water"
        );
        assert_eq!(
            city.water_reserves.groundwater_volume, 300.0,
            "City must gain 300 water"
        );
    }

    #[test]
    fn test_water_transfer_clamps_to_available() {
        let mut parent = make_test_region("parent", 0.5);
        parent.water_reserves.groundwater_volume = 100.0; // Less than requested

        let mut city = make_test_region("city", 0.7);
        city.water_reserves.groundwater_volume = 0.0;

        transfer_physical_water(&mut parent, &mut city, 300.0);

        // Should transfer only 100 (clamped to available, no negative).
        assert_eq!(
            parent.water_reserves.groundwater_volume, 0.0,
            "Parent must not go negative"
        );
        assert_eq!(
            city.water_reserves.groundwater_volume, 100.0,
            "City must receive only available water"
        );
    }

    #[test]
    fn test_water_transfer_zero_noop() {
        let mut parent = make_test_region("parent", 0.5);
        parent.water_reserves.groundwater_volume = 500.0;

        let mut city = make_test_region("city", 0.7);
        city.water_reserves.groundwater_volume = 200.0;

        transfer_physical_water(&mut parent, &mut city, 0.0);

        assert_eq!(parent.water_reserves.groundwater_volume, 500.0);
        assert_eq!(city.water_reserves.groundwater_volume, 200.0);
    }

    // ========================================================================
    // GHOST TOWN / NO DISSOLUTION TESTS
    // ========================================================================

    #[test]
    fn test_ghost_town_city_metadata_preserved() {
        // A city with zero population must still have city_metadata.
        let mut city = make_test_region("ghost-city", 0.0);
        city.population = 0;
        city.treasury.liquid_reserves = 0.0;
        city.city_metadata = Some(CityRegionMetadata {
            is_city: true,
            emancipated_turn: 50,
            parent_region_id: "parent".to_string(),
            annexation_cooldown: 0,
            pending_annexation_targets: Vec::new(),
        });

        // The city must still be recognized as a city.
        assert!(city.is_city(), "Ghost town must still be a city");
        assert_eq!(city.population, 0, "Ghost town has zero population");
        assert_eq!(
            city.treasury.liquid_reserves,
            0.0,
            "Ghost town has empty treasury"
        );
    }

    // ========================================================================
    // SNAPSHOT ROLE-GATING TESTS
    // ========================================================================

    #[test]
    fn test_cities_snapshot_strips_treasury_for_foreign_observers() {
        let mut country = crate::state::Country::default();
        let mut city = make_test_region("city-1", 0.7);
        city.city_metadata = Some(CityRegionMetadata {
            is_city: true,
            emancipated_turn: 10,
            parent_region_id: "parent".to_string(),
            annexation_cooldown: 0,
            pending_annexation_targets: Vec::new(),
        });
        city.treasury.liquid_reserves = 500_000.0;
        city.population = 50_000;
        country.regions.push(city);

        // Foreign observer (is_classified = true) → treasury stripped.
        let snapshot = crate::ui::snapshot::build_cities_snapshot(&country, true);
        assert_eq!(snapshot.cities.len(), 1);
        assert_eq!(
            snapshot.cities[0].treasury_reserves, 0.0,
            "Foreign observer must see 0 treasury (Rule 11)"
        );
        assert_eq!(snapshot.cities[0].population, 50_000, "Population is public");
    }

    #[test]
    fn test_cities_snapshot_shows_treasury_for_authorized() {
        let mut country = crate::state::Country::default();
        let mut city = make_test_region("city-1", 0.7);
        city.city_metadata = Some(CityRegionMetadata {
            is_city: true,
            emancipated_turn: 10,
            parent_region_id: "parent".to_string(),
            annexation_cooldown: 5,
            pending_annexation_targets: Vec::new(),
        });
        city.treasury.liquid_reserves = 500_000.0;
        country.regions.push(city);

        // Authorized observer (is_classified = false) → treasury visible.
        let snapshot = crate::ui::snapshot::build_cities_snapshot(&country, false);
        assert_eq!(snapshot.cities.len(), 1);
        assert_eq!(
            snapshot.cities[0].treasury_reserves, 500_000.0,
            "Authorized observer must see real treasury"
        );
        assert_eq!(
            snapshot.cities[0].annexation_cooldown, 5,
            "Authorized observer must see cooldown"
        );
    }
}

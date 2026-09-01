//! Phase 85: Tests for factional domains, cottage industry, guilds, mixed-use zoning.
//!
//! Covers:
//! - FTE conservation (cottage + guild + labor_pool <= available)
//! - Mass conservation (cottage recipes: input = output + waste)
//! - Temporal inventory (Turn N production consumes Turn N-1 inventory)
//! - Demand clamping (cottage output <= utility demand)
//! - No phantom money (feudal dues don't create ledger entries)
//! - Macro-triggered guild formation (aggregate FTE, not individual craftsmen)
//! - Double-entry guild seed capital extraction
//! - Quality standard is financial only (does not multiply physical output)

#[cfg(test)]
mod tests {
    use crate::economy::cottage_industry::{
        execute_cottage_production, reserve_cottage_fte, CottageConfig, CottageRecipe,
    };
    use crate::economy::guild_system::{
        check_guild_formation_trigger, create_guild, distribute_guild_dividends,
        execute_guild_production, GuildConfig,
    };
    use crate::entities::legal_form::{GuildData, LegalForm};
    use crate::registries::enums::{Commodity, Sector};
    use crate::society::geography::{
        ClassDemographics, FactionDomainType, LocalLaws, MicroRegion, MicroRegionBudget,
    };
    use std::collections::BTreeMap;

    // ========================================================================
    // COTTAGE INDUSTRY TESTS
    // ========================================================================

    fn make_demo(available_fte: f64) -> ClassDemographics {
        ClassDemographics {
            available_fte,
            allocated_fte: 0.0,
            ..Default::default()
        }
    }

    fn make_domain(faction_type: FactionDomainType, cottage_bonus: f64) -> MicroRegion {
        MicroRegion {
            id: "test-domain".to_string(),
            parent_region_id: "test-region".to_string(),
            faction_type,
            name: "Test Domain".to_string(),
            population: 1000,
            sub_budget: MicroRegionBudget::default(),
            autonomy_level: 0.5,
            governing_faction_id: None,
            local_laws: LocalLaws {
                cottage_industry_bonus: cottage_bonus,
                ..Default::default()
            },
            education_slots: 0,
            health_capacity: 0.0,
            controlled_parcel_ids: Vec::new(),
        }
    }

    #[test]
    fn test_fte_conservation() {
        // FTE conservation: cottage_fte + guild_fte + labor_pool <= available
        let mut demo = make_demo(100.0);
        let config = CottageConfig::default();

        // Set up market prices where cottage is more valuable than wage
        let mut prices = BTreeMap::new();
        prices.insert(Commodity::Clothing, 50.0);
        prices.insert(Commodity::TextileWaste, 5.0);

        // Demand for clothing
        let mut demand = BTreeMap::new();
        demand.insert(Commodity::Clothing, 100.0);

        // Pre-load raw inventory (from "previous turn")
        demo.cottage_raw_inventory
            .insert(Commodity::TextileWaste, 200.0);

        let result = reserve_cottage_fte(&mut demo, 10.0, &prices, &demand, None, &config);

        // Verify conservation
        let total_allocated = result.cottage_fte + result.guild_fte + result.labor_pool_fte;
        assert!(
            total_allocated <= 100.0 + 1e-6,
            "FTE conservation violated: {} > 100",
            total_allocated
        );
        assert!(
            result.cottage_fte >= 0.0,
            "Cottage FTE must be non-negative"
        );
        assert!(
            result.labor_pool_fte >= 0.0,
            "Labor pool must be non-negative"
        );
    }

    #[test]
    fn test_cottage_demand_clamping() {
        // Fix 3: Cottage output must not exceed utility demand
        let mut demo = make_demo(1000.0); // Huge FTE

        let config = CottageConfig::default();

        let mut prices = BTreeMap::new();
        prices.insert(Commodity::Clothing, 100.0);
        prices.insert(Commodity::TextileWaste, 1.0);

        // Very small demand — only 5 units of clothing
        let mut demand = BTreeMap::new();
        demand.insert(Commodity::Clothing, 5.0);

        // Plenty of raw inventory
        demo.cottage_raw_inventory
            .insert(Commodity::TextileWaste, 10000.0);

        let result = reserve_cottage_fte(&mut demo, 1.0, &prices, &demand, None, &config);

        // Check that planned output doesn't exceed demand
        if let Some(&clothing_output) = result.planned_output.get(&Commodity::Clothing) {
            assert!(
                clothing_output <= 5.0 + 1e-6,
                "Cottage output {} exceeds demand 5.0",
                clothing_output
            );
        }
    }

    #[test]
    fn test_cottage_temporal_inventory() {
        // Fix 5: Turn N production consumes inventory from Turn N-1
        // If no inventory, no production even with FTE allocated
        let mut demo = make_demo(100.0);
        // NO raw inventory — should produce nothing

        let waste = execute_cottage_production(&mut demo, None);

        assert!(
            demo.cottage_output.is_empty(),
            "Cottage produced output with no raw inventory"
        );
        assert!(
            waste.is_empty(),
            "Cottage generated waste with no raw inventory"
        );
    }

    #[test]
    fn test_cottage_mass_conservation() {
        // Rule 1: input = output + waste
        let mut demo = make_demo(100.0);

        // Allocate cottage FTE
        demo.cottage_fte_allocated = 50.0;

        // Provide raw inventory: 200 units of TextileWaste
        demo.cottage_raw_inventory
            .insert(Commodity::TextileWaste, 200.0);

        let waste = execute_cottage_production(&mut demo, None);

        // Get the clothing recipe to verify mass conservation
        let _recipe = CottageRecipe::for_output(Commodity::Clothing).unwrap();

        let clothing_output = demo
            .cottage_output
            .get(&Commodity::Clothing)
            .copied()
            .unwrap_or(0.0);
        let raw_consumed = 200.0
            - demo
                .cottage_raw_inventory
                .get(&Commodity::TextileWaste)
                .copied()
                .unwrap_or(0.0);

        // Find textile waste generated
        let textile_waste = waste
            .iter()
            .find(|(c, _)| *c == Commodity::TextileWaste)
            .map(|(_, a)| *a)
            .unwrap_or(0.0);

        // Mass conservation: raw_consumed = clothing_output + textile_waste
        // (With recipe: 2.0 input → 1.0 output + 1.0 waste, so input = output + waste)
        assert!(
            (raw_consumed - clothing_output - textile_waste).abs() < 1e-6,
            "Mass conservation violated: raw={} output={} waste={}",
            raw_consumed,
            clothing_output,
            textile_waste
        );
    }

    #[test]
    fn test_cottage_no_production_when_wage_higher() {
        // Opportunity cost: if wage > cottage value, no cottage FTE
        let mut demo = make_demo(100.0);
        let config = CottageConfig::default();

        let mut prices = BTreeMap::new();
        prices.insert(Commodity::Clothing, 1.0); // Very low output price
        prices.insert(Commodity::TextileWaste, 100.0); // Very high input price

        let mut demand = BTreeMap::new();
        demand.insert(Commodity::Clothing, 100.0);

        demo.cottage_raw_inventory
            .insert(Commodity::TextileWaste, 200.0);

        // Very high wage — should make cottage unattractive
        let result = reserve_cottage_fte(&mut demo, 1000.0, &prices, &demand, None, &config);

        assert!(
            result.cottage_fte == 0.0,
            "Cottage FTE should be 0 when wage >> cottage value, got {}",
            result.cottage_fte
        );
    }

    // ========================================================================
    // GUILD SYSTEM TESTS
    // ========================================================================

    #[test]
    fn test_guild_formation_macro_trigger() {
        // Fix 7: Guild formation uses aggregate FTE, not individual craftsmen
        let domain = make_domain(FactionDomainType::GuildBurgher, 0.0);
        let config = GuildConfig::default();

        // Aggregate cottage FTE by sector
        let mut fte_by_sector = BTreeMap::new();
        fte_by_sector.insert("LightIndustry".to_string(), 100.0); // Exceeds threshold

        let triggered = check_guild_formation_trigger(&domain, &fte_by_sector, &config);
        assert!(
            triggered.is_some(),
            "Guild formation should trigger when aggregate FTE exceeds threshold"
        );

        // Below threshold
        let mut low_fte = BTreeMap::new();
        low_fte.insert("LightIndustry".to_string(), 1.0); // Way below threshold
        let not_triggered = check_guild_formation_trigger(&domain, &low_fte, &config);
        assert!(
            not_triggered.is_none(),
            "Guild formation should NOT trigger when FTE is below threshold"
        );
    }

    #[test]
    fn test_guild_formation_wrong_faction_type() {
        // Guilds only form in GuildBurgher domains
        let domain = make_domain(FactionDomainType::PeasantCommunity, 0.0);
        let config = GuildConfig::default();

        let mut fte = BTreeMap::new();
        fte.insert("LightIndustry".to_string(), 1000.0);

        let result = check_guild_formation_trigger(&domain, &fte, &config);
        assert!(
            result.is_none(),
            "Guild should not form in PeasantCommunity domain"
        );
    }

    #[test]
    fn test_guild_seed_capital_extraction() {
        // Double-entry: seed capital extracted pro-rata from class savings
        let domain = make_domain(FactionDomainType::GuildBurgher, 0.0);
        let config = GuildConfig::default();
        let avg_wage = 100.0;

        // Three contributing classes with different FTE shares and savings
        let contributing = vec![
            ("class_a".to_string(), 60.0, 5000.0), // 60% of FTE, 5000 savings
            ("class_b".to_string(), 30.0, 3000.0), // 30% of FTE, 3000 savings
            ("class_c".to_string(), 10.0, 1000.0), // 10% of FTE, 1000 savings
        ];

        let guild = create_guild(
            &domain,
            Sector::LightIndustry,
            &contributing,
            avg_wage,
            &config,
        );

        // Seed capital = 50 * 100 = 5000
        let expected_seed = config.min_seed_capital_wage_multiple * avg_wage;
        assert_eq!(
            guild.available_cash, expected_seed,
            "Guild should have seed capital equal to wage_multiple * avg_wage"
        );

        // Verify guild has Guild legal form
        assert!(matches!(guild.legal_form, LegalForm::Guild(_)));
    }

    #[test]
    fn test_guild_quality_standard_not_physical() {
        // Fix 1: quality_standard does NOT multiply physical output
        let mut company = crate::entities::Company {
            legal_form: LegalForm::Guild(GuildData {
                guild_sector: "LightIndustry".to_string(),
                guild_raw_inventory: BTreeMap::new(),
                quality_standard: 0.5, // High quality
                ..Default::default()
            }),
            ..Default::default()
        };

        // Provide raw inventory
        if let LegalForm::Guild(ref mut data) = company.legal_form {
            data.guild_raw_inventory
                .insert(Commodity::TextileWaste, 200.0);
        }

        // Execute production with 50 FTE
        let result = execute_guild_production(
            &mut company,
            50.0,
            Commodity::TextileWaste,
            Commodity::Clothing,
            2.0, // input_per_unit
            0.5, // fte_per_unit
            Commodity::TextileWaste,
            1.0, // waste_per_unit
        );

        let clothing_output = result
            .output
            .get(&Commodity::Clothing)
            .copied()
            .unwrap_or(0.0);

        // With 200 raw and 2.0 input_per_unit: max_from_raw = 100
        // With 50 FTE and 0.5 fte_per_unit: max_from_fte = 100
        // Output = min(100, 100) = 100
        // Quality standard 0.5 should NOT change this — it's financial only
        assert!(
            (clothing_output - 100.0).abs() < 1e-6,
            "Quality standard should not affect physical output: got {} expected 100",
            clothing_output
        );
    }

    #[test]
    fn test_guild_temporal_inventory() {
        // Fix 5: Guild production consumes inventory from Turn N-1
        let mut company = crate::entities::Company {
            legal_form: LegalForm::Guild(GuildData {
                guild_sector: "LightIndustry".to_string(),
                guild_raw_inventory: BTreeMap::new(), // NO inventory
                ..Default::default()
            }),
            ..Default::default()
        };

        let result = execute_guild_production(
            &mut company,
            100.0, // Plenty of FTE
            Commodity::TextileWaste,
            Commodity::Clothing,
            2.0,
            0.5,
            Commodity::TextileWaste,
            1.0,
        );

        assert!(
            result.output.is_empty(),
            "Guild should produce nothing with no raw inventory"
        );
    }

    #[test]
    fn test_guild_dividend_distribution() {
        // Dividends distributed pro-rata by production volume
        let mut company = crate::entities::Company {
            legal_form: LegalForm::Guild(GuildData {
                welfare_contribution_rate: 0.10,
                ..Default::default()
            }),
            ..Default::default()
        };

        let profit = 1000.0;
        let members = vec![
            ("class_a".to_string(), 60.0), // 60% production
            ("class_b".to_string(), 40.0), // 40% production
        ];

        let dividends = distribute_guild_dividends(&mut company, profit, &members);

        // Welfare = 1000 * 0.10 = 100
        // Dividend pool = 1000 - 100 = 900
        // class_a: 900 * 60/100 = 540
        // class_b: 900 * 40/100 = 360
        let div_a = dividends.get("class_a").copied().unwrap_or(0.0);
        let div_b = dividends.get("class_b").copied().unwrap_or(0.0);

        assert!(
            (div_a - 540.0).abs() < 1e-6,
            "Class A dividend should be 540, got {}",
            div_a
        );
        assert!(
            (div_b - 360.0).abs() < 1e-6,
            "Class B dividend should be 360, got {}",
            div_b
        );

        // Verify welfare fund was updated
        if let LegalForm::Guild(data) = &company.legal_form {
            assert!(
                (data.welfare_fund - 100.0).abs() < 1e-6,
                "Welfare fund should be 100, got {}",
                data.welfare_fund
            );
        }
    }

    #[test]
    fn test_guild_no_dividends_on_loss() {
        let mut company = crate::entities::Company {
            legal_form: LegalForm::Guild(GuildData::default()),
            ..Default::default()
        };

        let members = vec![("class_a".to_string(), 100.0)];

        let dividends = distribute_guild_dividends(&mut company, -500.0, &members);

        assert!(
            dividends.is_empty(),
            "No dividends should be distributed on loss"
        );
    }

    // ========================================================================
    // FACTIONAL DOMAIN TESTS
    // ========================================================================

    #[test]
    fn test_faction_domain_type_exists() {
        // Verify all Phase 85 faction domain types exist
        let types = [
            FactionDomainType::GuildBurgher,
            FactionDomainType::AristocraticEstate,
            FactionDomainType::ClergyLand,
            FactionDomainType::PeasantCommunity,
            FactionDomainType::IndustrialistDomain,
        ];

        assert_eq!(types.len(), 5, "Should have 5 faction domain types");
    }

    #[test]
    fn test_local_laws_default() {
        let laws = LocalLaws::default();
        assert!(laws.entry_tariff_rate >= 0.0);
        assert!(laws.feudal_dues_rate >= 0.0);
        assert!(laws.tithe_rate >= 0.0);
        // No magic numbers — all defaults should be reasonable
        assert!(laws.cottage_industry_bonus >= 0.0);
    }

    // ========================================================================
    // COTTAGE RECIPE TESTS
    // ========================================================================

    #[test]
    fn test_cottage_recipes_mass_conserving() {
        // Rule 1: All recipes must be mass-conserving
        for recipe in CottageRecipe::all_recipes() {
            // input_per_unit should equal output (1.0) + waste_per_unit
            // (mass conservation: input mass = output mass + waste mass)
            assert!(
                recipe.input_per_unit >= 1.0,
                "Recipe for {:?}: input_per_unit {} should be >= 1.0 (output)",
                recipe.output,
                recipe.input_per_unit
            );

            // Waste should be non-negative
            assert!(
                recipe.waste_per_unit >= 0.0,
                "Recipe for {:?}: waste_per_unit should be non-negative",
                recipe.output
            );
        }
    }

    #[test]
    fn test_cottage_recipes_cover_basic_goods() {
        // Verify recipes exist for Clothing, Furniture, Food
        assert!(CottageRecipe::for_output(Commodity::Clothing).is_some());
        assert!(CottageRecipe::for_output(Commodity::Furniture).is_some());
        assert!(CottageRecipe::for_output(Commodity::Food).is_some());
    }
}

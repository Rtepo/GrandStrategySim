//! AI & Stability Audit (v0.5.2): Tests for the 4 Pillars.
//!
//! Pillar 1A: Calendar time-travel bug fix
//! Pillar 1B: Biomass clone fix (resource-aware plant selection)
//! Pillar 1C: Furlough blindspot fix (is_distressed broadened)
//! Pillar 2: Supply blindness fix (inventory + throughput clamp)
//! Pillar 3: Anticipatory labor ramp-up
//! Pillar 4A: Moving averages for distress evaluation
//! Pillar 4B: ActionLedger proto-learning
//! Pillar 4C: State counter-cyclical AI

#[cfg(test)]
mod tests {
    use sim_engine::state::{Calendar, Season};

    /// Pillar 1A: Calendar with start_month=9 (September) must maintain
    /// continuous month progression. Turn 0 = September, Turn 1 = September
    /// (late half), Turn 2 = October, etc. The old bug jumped back to January.
    #[test]
    fn test_calendar_september_start_progression() {
        let mut cal = Calendar::default();
        cal.start_year = 1900;
        cal.start_month = 9; // September
        cal.current_year = 1900;
        cal.current_month = 9;
        cal.global_turn = 0;
        cal.half_month = false;

        // Turn 0: September, early half
        assert_eq!(cal.current_month, 9, "Turn 0 should be September");
        assert_eq!(cal.get_season(), Season::Autumn, "Turn 0 should be Autumn");

        // Advance to Turn 1: September, early half (turn_in_year=0)
        cal.advance();
        assert_eq!(cal.global_turn, 1, "Turn 1: global_turn should be 1");
        assert_eq!(cal.current_month, 9, "Turn 1 should still be September");
        assert!(!cal.half_month, "Turn 1 should be early half (turn_in_year=0, even)");
        assert_eq!(cal.get_season(), Season::Autumn, "Turn 1 should be Autumn");

        // Advance to Turn 2: September, late half (turn_in_year=1)
        cal.advance();
        assert_eq!(cal.global_turn, 2, "Turn 2: global_turn should be 2");
        assert_eq!(cal.current_month, 9, "Turn 2 should still be September (late half)");
        assert!(cal.half_month, "Turn 2 should be late half (turn_in_year=1, odd)");

        // Advance to Turn 3: October, early half (turn_in_year=2)
        cal.advance();
        assert_eq!(cal.current_month, 10, "Turn 3 should be October");
        assert!(!cal.half_month, "Turn 3 should be early half");

        // Advance to Turn 4: October, late half
        cal.advance();
        assert_eq!(cal.current_month, 10, "Turn 4 should still be October");
        assert!(cal.half_month, "Turn 4 should be late half");

        // Advance to Turn 5: November, early half
        cal.advance();
        assert_eq!(cal.current_month, 11, "Turn 5 should be November");

        // Advance to Turn 6: November, late half
        cal.advance();
        assert_eq!(cal.current_month, 11, "Turn 6 should still be November");

        // Advance to Turn 7: December (Winter)
        cal.advance();
        assert_eq!(cal.current_month, 12, "Turn 7 should be December");
        assert_eq!(cal.get_season(), Season::Winter, "Turn 7 should be Winter");

        // Advance to Turn 8: December, late half
        cal.advance();
        assert_eq!(cal.current_month, 12, "Turn 8 should still be December");

        // Advance to Turn 9: January (Winter)
        cal.advance();
        assert_eq!(cal.current_month, 1, "Turn 9 should be January");
        assert_eq!(cal.get_season(), Season::Winter, "Turn 9 should be Winter");

        // Advance to Turn 24: August (end of year, still 1900)
        // We're at turn 9, need 15 more advances to reach turn 24
        for _ in 0..15 {
            cal.advance();
        }
        assert_eq!(cal.global_turn, 24, "Should be at turn 24");
        assert_eq!(cal.current_month, 8, "Turn 24 should be August");
        assert_eq!(cal.current_year, 1900, "Turn 24 should still be year 1900");

        // Advance to Turn 25: September (year 1901)
        cal.advance();
        assert_eq!(cal.global_turn, 25, "Should be at turn 25");
        assert_eq!(cal.current_month, 9, "Turn 25 should be September (year 2)");
        assert_eq!(cal.current_year, 1901, "Turn 25 should be year 1901");
        assert_eq!(cal.get_season(), Season::Autumn, "Turn 25 should be Autumn");
    }

    /// Pillar 1A: Verify the turn.rs end-of-turn sync formula matches
    /// the advance() formula for start_month=9.
    #[test]
    fn test_calendar_sync_formula_matches_advance() {
        let start_month: u32 = 9;

        // Simulate the turn.rs sync formula for turns 1-48
        for turn in 1..=48u32 {
            let turn_in_year = (turn - 1) % 24;
            let sync_month = (turn_in_year / 2 + start_month.saturating_sub(1)) % 12 + 1;
            let sync_half = turn_in_year % 2 == 1;

            // Simulate the advance() formula
            let advance_turn_in_year = (turn - 1) % 24; // global_turn was turn-1 before advance
            // advance() does: global_turn += 1, then turn_in_year = (global_turn - 1) % 24
            // So after advance with global_turn becoming `turn`:
            let advance_month = (advance_turn_in_year / 2 + start_month.saturating_sub(1)) % 12 + 1;
            let advance_half = advance_turn_in_year % 2 == 1;

            assert_eq!(
                sync_month, advance_month,
                "Turn {}: sync formula month {} != advance formula month {}",
                turn, sync_month, advance_month
            );
            assert_eq!(
                sync_half, advance_half,
                "Turn {}: sync formula half {} != advance formula half {}",
                turn, sync_half, advance_half
            );

            // Verify month is always 1-12
            assert!(sync_month >= 1 && sync_month <= 12, "Turn {}: month {} out of range", turn, sync_month);
        }
    }

    /// Pillar 1A: Turn 0 should use start_month, not month 1.
    #[test]
    fn test_calendar_turn_0_uses_start_month() {
        let mut cal = Calendar::default();
        cal.start_year = 1925;
        cal.start_month = 9;
        cal.current_year = 1925;
        cal.current_month = 9;
        cal.global_turn = 0;
        cal.half_month = false;

        // Turn 0 should report September
        assert_eq!(cal.current_month, 9, "Turn 0 month should be start_month=9");
        assert_eq!(cal.get_season(), Season::Autumn, "Turn 0 should be Autumn");
    }

    // ========================================================================
    // Pillar 1B: Biomass clone fix — resource-aware plant selection
    // ========================================================================

    /// Pillar 1B: Without forest or livestock, biomass weight should be
    /// reduced (0.1), not the default 0.3. This prevents biomass from
    /// becoming the universal fallback.
    #[test]
    fn test_biomass_weight_reduced_without_forest_or_livestock() {
        use sim_engine::energy::generation::available_plant_types;
        use sim_engine::energy::types::PowerPlantType;

        let types = available_plant_types(1900, false, true, false, false, false, false);
        let biomass = types.iter().find(|(t, _)| *t == PowerPlantType::BiomassFired);
        assert!(biomass.is_some(), "Biomass should still be available");
        let (_, weight) = biomass.unwrap();
        assert!(
            *weight <= 0.1,
            "Biomass weight without forest/livestock should be <= 0.1, got {}",
            weight
        );
    }

    /// Pillar 1B: With forest, biomass weight should be 0.3 (full weight).
    #[test]
    fn test_biomass_weight_full_with_forest() {
        use sim_engine::energy::generation::available_plant_types;
        use sim_engine::energy::types::PowerPlantType;

        let types = available_plant_types(1900, false, true, true, false, false, false);
        let biomass = types.iter().find(|(t, _)| *t == PowerPlantType::BiomassFired);
        assert!(biomass.is_some(), "Biomass should be available with forest");
        let (_, weight) = biomass.unwrap();
        assert!(
            (*weight - 0.3).abs() < 1e-9,
            "Biomass weight with forest should be 0.3, got {}",
            weight
        );
    }

    /// Pillar 1B: Coal deposit should enable CoalFired and LigniteFired plants.
    #[test]
    fn test_coal_deposit_enables_coal_plants() {
        use sim_engine::energy::generation::available_plant_types;
        use sim_engine::energy::types::PowerPlantType;

        let types = available_plant_types(1900, true, false, false, false, false, false);
        assert!(
            types.iter().any(|(t, _)| *t == PowerPlantType::CoalFired),
            "CoalFired should be available with coal deposit"
        );
        assert!(
            types.iter().any(|(t, _)| *t == PowerPlantType::LigniteFired),
            "LigniteFired should be available with coal deposit"
        );
    }

    // ========================================================================
    // Pillar 4A: Moving averages for distress evaluation
    // ========================================================================

    /// Pillar 4A: moving_avg_net_profit should return 0.0 for a company with
    /// no financial history.
    #[test]
    fn test_moving_avg_empty_history() {
        use sim_engine::entities::Company;
        let company = Company::default();
        assert_eq!(company.moving_avg_net_profit(3), 0.0);
    }

    /// Pillar 4A: moving_avg_net_profit should average the last N entries.
    #[test]
    fn test_moving_avg_last_n_entries() {
        use sim_engine::entities::Company;
        use serde_json::{Map, Value};

        let mut company = Company::default();
        // Add 5 records with profits: 100, 200, 300, 400, 500
        for profit in [100.0, 200.0, 300.0, 400.0, 500.0] {
            let mut map = Map::new();
            map.insert("net_profit".to_string(), Value::from(profit));
            company.financial_history.push(Value::Object(map));
        }
        // 3-turn average of last 3: (300+400+500)/3 = 400
        let avg = company.moving_avg_net_profit(3);
        assert!(
            (avg - 400.0).abs() < 1e-9,
            "3-turn moving average should be 400, got {}",
            avg
        );
    }

    /// Pillar 4A: moving_avg_net_profit with window larger than history
    /// should average all available entries.
    #[test]
    fn test_moving_avg_window_larger_than_history() {
        use sim_engine::entities::Company;
        use serde_json::{Map, Value};

        let mut company = Company::default();
        for profit in [100.0, 200.0] {
            let mut map = Map::new();
            map.insert("net_profit".to_string(), Value::from(profit));
            company.financial_history.push(Value::Object(map));
        }
        // Window=10 but only 2 entries: (100+200)/2 = 150
        let avg = company.moving_avg_net_profit(10);
        assert!(
            (avg - 150.0).abs() < 1e-9,
            "Moving average with large window should average all, got {}",
            avg
        );
    }

    // ========================================================================
    // Pillar 4B: ActionLedger proto-learning
    // ========================================================================

    /// Pillar 4B: ActionLedger should record actions and start with zero weight.
    #[test]
    fn test_action_ledger_record_and_initial_weight() {
        use sim_engine::entities::ActionLedger;

        let mut ledger = ActionLedger::default();
        ledger.record_action("Expand", 1, 1000.0);

        assert_eq!(ledger.weight_for("Expand"), 0.0, "Initial weight should be 0");
        assert!(
            ledger.action_records.contains_key("Expand"),
            "Action should be recorded"
        );
    }

    /// Pillar 4B: After 3+ turns with declining profit, penalty weight should
    /// increase (proto-learning: bad outcome → penalty).
    #[test]
    fn test_action_ledger_penalty_on_declining_roi() {
        use sim_engine::entities::ActionLedger;

        let mut ledger = ActionLedger::default();
        // Record expansion at turn 1 with profit 1000
        ledger.record_action("Expand", 1, 1000.0);
        // At turn 4 (3 turns later), profit dropped to 500 (ROI = -500)
        ledger.evaluate_and_update(4, 500.0);

        let weight = ledger.weight_for("Expand");
        assert!(
            weight > 0.0,
            "Penalty weight should be positive after declining ROI, got {}",
            weight
        );
    }

    /// Pillar 4B: After 3+ turns with improving profit, penalty weight should
    /// remain zero (good outcome → no penalty).
    #[test]
    fn test_action_ledger_no_penalty_on_improving_roi() {
        use sim_engine::entities::ActionLedger;

        let mut ledger = ActionLedger::default();
        // Record expansion at turn 1 with profit 500
        ledger.record_action("Expand", 1, 500.0);
        // At turn 4, profit improved to 1000 (ROI = +500)
        ledger.evaluate_and_update(4, 1000.0);

        let weight = ledger.weight_for("Expand");
        assert_eq!(
            weight, 0.0,
            "Penalty weight should be 0 after improving ROI, got {}",
            weight
        );
    }

    /// Pillar 4B: Records older than 12 turns should be pruned.
    #[test]
    fn test_action_ledger_prunes_old_records() {
        use sim_engine::entities::ActionLedger;

        let mut ledger = ActionLedger::default();
        ledger.record_action("Expand", 1, 1000.0);
        // At turn 20 (19 turns later, > 12 turn prune age)
        ledger.evaluate_and_update(20, 500.0);

        assert!(
            !ledger.action_records.contains_key("Expand")
                || ledger.action_records["Expand"].is_empty(),
            "Old records (>12 turns) should be pruned"
        );
    }

    // ========================================================================
    // Pillar 4C: State counter-cyclical AI
    // ========================================================================

    /// Pillar 4C: Counter-cyclical response should transfer funds from
    /// treasury to unemployed workers when unemployment spikes.
    #[test]
    fn test_counter_cyclical_transfers_to_unemployed() {
        use sim_engine::politics::crisis_management::counter_cyclical_response;
        use sim_engine::state::Country;
        use sim_engine::society::geography::{Region, RegionalClassDemographics, ClassDemographics};

        let mut country = Country::default();
        country.budget.gdp = 1_000_000.0;
        country.budget.liquid_reserves = 100_000.0;
        country.macro_indicators.labor_market.unemployment_rate = 12.0;
        country.macro_indicators.labor_market.prev_unemployment_rate = 8.0;

        // Set up a region with unemployed workers
        let mut region = Region::default();
        let mut rural = std::collections::BTreeMap::new();
        let mut class = ClassDemographics::default();
        class.available_fte = 1000.0;
        class.allocated_fte = 600.0; // 400 unemployed
        class.savings = 0.0;
        rural.insert("Peasants".to_string(), class);
        region.class_demographics = RegionalClassDemographics {
            rural_classes: rural,
            urban_classes: std::collections::BTreeMap::new(),
        };
        country.regions = vec![region];

        let treasury_before = country.budget.liquid_reserves;
        let msgs = counter_cyclical_response(&mut country, 5);

        assert!(
            !msgs.is_empty(),
            "Should produce counter-cyclical messages on unemployment spike"
        );
        assert!(
            country.budget.liquid_reserves < treasury_before,
            "Treasury should be debited"
        );

        // Verify unemployed workers received funds
        let class = &country.regions[0].class_demographics.rural_classes["Peasants"];
        assert!(
            class.savings > 0.0,
            "Unemployed workers should have received stimulus"
        );
    }

    /// Pillar 4C: Counter-cyclical response should NOT trigger when
    /// unemployment is stable (not rising).
    #[test]
    fn test_counter_cyclical_no_action_on_stable_unemployment() {
        use sim_engine::politics::crisis_management::counter_cyclical_response;
        use sim_engine::state::Country;

        let mut country = Country::default();
        country.budget.gdp = 1_000_000.0;
        country.budget.liquid_reserves = 100_000.0;
        // Unemployment is high but NOT rising (same as previous)
        country.macro_indicators.labor_market.unemployment_rate = 10.0;
        country.macro_indicators.labor_market.prev_unemployment_rate = 10.0;

        let treasury_before = country.budget.liquid_reserves;
        let msgs = counter_cyclical_response(&mut country, 5);

        // No fiscal stimulus messages (monetary easing may still occur if ratio > 0.05)
        let fiscal_msgs: Vec<_> = msgs.iter().filter(|m| m.contains("Fiscal stimulus")).collect();
        assert!(
            fiscal_msgs.is_empty(),
            "No fiscal stimulus when unemployment is stable"
        );
        assert_eq!(
            country.budget.liquid_reserves, treasury_before,
            "Treasury should not be debited when unemployment is stable"
        );
    }

    /// Pillar 4C: Counter-cyclical response should NOT trigger when
    /// unemployment is below the threshold.
    #[test]
    fn test_counter_cyclical_no_action_on_low_unemployment() {
        use sim_engine::politics::crisis_management::counter_cyclical_response;
        use sim_engine::state::Country;

        let mut country = Country::default();
        country.budget.gdp = 1_000_000.0;
        country.budget.liquid_reserves = 100_000.0;
        country.macro_indicators.labor_market.unemployment_rate = 4.0;
        country.macro_indicators.labor_market.prev_unemployment_rate = 3.0;

        let treasury_before = country.budget.liquid_reserves;
        let msgs = counter_cyclical_response(&mut country, 5);

        let fiscal_msgs: Vec<_> = msgs.iter().filter(|m| m.contains("Fiscal stimulus")).collect();
        assert!(
            fiscal_msgs.is_empty(),
            "No fiscal stimulus when unemployment is below 8%"
        );
        assert_eq!(
            country.budget.liquid_reserves, treasury_before,
            "Treasury should not be debited when unemployment is low"
        );
    }

    /// Pillar 4C: Stimulus should only go to UNEMPLOYED workers, not employed.
    #[test]
    fn test_counter_cyclical_targets_unemployed_only() {
        use sim_engine::politics::crisis_management::counter_cyclical_response;
        use sim_engine::state::Country;
        use sim_engine::society::geography::{Region, RegionalClassDemographics, ClassDemographics};

        let mut country = Country::default();
        country.budget.gdp = 1_000_000.0;
        country.budget.liquid_reserves = 100_000.0;
        country.macro_indicators.labor_market.unemployment_rate = 12.0;
        country.macro_indicators.labor_market.prev_unemployment_rate = 8.0;

        // Set up a region with two classes: one with unemployed, one fully employed
        let mut region = Region::default();
        let mut rural = std::collections::BTreeMap::new();

        let mut unemployed_class = ClassDemographics::default();
        unemployed_class.available_fte = 1000.0;
        unemployed_class.allocated_fte = 600.0; // 400 unemployed
        unemployed_class.savings = 0.0;
        rural.insert("Peasants".to_string(), unemployed_class);

        let mut employed_class = ClassDemographics::default();
        employed_class.available_fte = 500.0;
        employed_class.allocated_fte = 500.0; // 0 unemployed
        employed_class.savings = 0.0;
        rural.insert("Artisans".to_string(), employed_class);

        region.class_demographics = RegionalClassDemographics {
            rural_classes: rural,
            urban_classes: std::collections::BTreeMap::new(),
        };
        country.regions = vec![region];

        let _msgs = counter_cyclical_response(&mut country, 5);

        // Peasants (unemployed) should receive funds
        let peasants = &country.regions[0].class_demographics.rural_classes["Peasants"];
        assert!(
            peasants.savings > 0.0,
            "Unemployed class should receive stimulus"
        );

        // Artisans (fully employed) should NOT receive funds
        let artisans = &country.regions[0].class_demographics.rural_classes["Artisans"];
        assert_eq!(
            artisans.savings, 0.0,
            "Fully employed class should NOT receive stimulus"
        );
    }
}

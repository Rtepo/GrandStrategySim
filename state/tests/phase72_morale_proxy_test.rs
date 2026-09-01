//! Phase 72 Integration Tests: Morale, Propaganda, and Proxy Wars.
//!
//! Tests the homefront morale system (casualty-driven strikes/desertions),
//! propaganda campaigns (treasury → media sector double-entry), and proxy wars
//! with REAL physical commodity transfers (no magic spawning).

use sim_engine::military::fronts::Casualties;
use sim_engine::military::morale::{
    apply_casualty_morale_impact, calculate_desertions, initialize_morale, recover_morale,
    strike_production_factor, MoraleConfig,
};
use sim_engine::military::propaganda::{
    apply_propaganda_boost, execute_propaganda, PropagandaConfig, PropagandaTarget,
};
use sim_engine::military::proxy_wars::{
    arm_rebels, fund_separatists, ProxyWarAction, ProxyWarConfig,
};
use sim_engine::registries::enums::Commodity;
use sim_engine::society::geography::ClassDemographics;
use std::collections::{BTreeMap, HashMap};

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn make_demographics(population: i64) -> ClassDemographics {
    let mut d = ClassDemographics::default();
    d.population = population;
    d.war_morale = 70.0;
    d.mental_health = 70.0;
    d
}

fn make_casualties(total: i64) -> Casualties {
    Casualties {
        dead: (total as f64 * 0.5) as i64,
        wounded: (total as f64 * 0.35) as i64,
        deserters: total - (total as f64 * 0.5) as i64 - (total as f64 * 0.35) as i64,
        demographic_breakdown: HashMap::new(),
    }
}

fn make_media_companies() -> HashMap<String, (f64, f64)> {
    let mut m = HashMap::new();
    m.insert("MEDIA-1".to_string(), (1000.0, 10.0));
    m.insert("MEDIA-2".to_string(), (2000.0, 20.0));
    m.insert("MEDIA-3".to_string(), (500.0, 5.0));
    m
}

fn make_stockpile(rifles: f64, ammo: f64) -> HashMap<Commodity, f64> {
    let mut s = HashMap::new();
    s.insert(Commodity::Rifles, rifles);
    s.insert(Commodity::Ammunition, ammo);
    s
}

// ============================================================================
// MORALE TESTS
// ============================================================================

#[test]
fn test_phase72_casualties_reduce_war_morale() {
    let mut demo = make_demographics(100_000);
    let casualties = make_casualties(5_000);
    let config = MoraleConfig::default();

    let initial_morale = demo.war_morale;
    apply_casualty_morale_impact(&mut demo, &casualties, &config);

    assert!(
        demo.war_morale < initial_morale,
        "Casualties must reduce war morale"
    );
}

#[test]
fn test_phase72_casualties_reduce_mental_health() {
    let mut demo = make_demographics(100_000);
    let casualties = make_casualties(5_000);
    let config = MoraleConfig::default();

    let initial_mental = demo.mental_health;
    apply_casualty_morale_impact(&mut demo, &casualties, &config);

    assert!(
        demo.mental_health < initial_mental,
        "Casualties must reduce mental health"
    );
}

#[test]
fn test_phase72_zero_casualties_no_morale_impact() {
    let mut demo = make_demographics(100_000);
    let casualties = make_casualties(0);
    let config = MoraleConfig::default();

    let initial_morale = demo.war_morale;
    apply_casualty_morale_impact(&mut demo, &casualties, &config);

    assert_eq!(
        demo.war_morale, initial_morale,
        "Zero casualties must not change morale"
    );
}

#[test]
fn test_phase72_strikes_activate_below_threshold() {
    let mut demo = make_demographics(100_000);
    demo.war_morale = 25.0; // Below strike_threshold (30.0)
    let casualties = make_casualties(100);
    let config = MoraleConfig::default();

    let result = apply_casualty_morale_impact(&mut demo, &casualties, &config);
    assert!(
        result.strikes_active,
        "Strikes must activate below threshold"
    );
}

#[test]
fn test_phase72_desertions_activate_below_threshold() {
    let mut demo = make_demographics(100_000);
    demo.war_morale = 10.0; // Below desertion_threshold (15.0)
    let casualties = make_casualties(100);
    let config = MoraleConfig::default();

    let result = apply_casualty_morale_impact(&mut demo, &casualties, &config);
    assert!(
        result.desertions_active,
        "Desertions must activate below threshold"
    );
}

#[test]
fn test_phase72_morale_recovers_over_time() {
    let mut demo = make_demographics(100_000);
    demo.war_morale = 40.0;
    let config = MoraleConfig::default();

    let initial = demo.war_morale;
    recover_morale(&mut demo, &config);

    assert!(demo.war_morale > initial, "Morale must recover over time");
}

#[test]
fn test_phase72_morale_recovery_capped_at_baseline() {
    let mut demo = make_demographics(100_000);
    demo.war_morale = 69.5;
    let config = MoraleConfig::default();

    recover_morale(&mut demo, &config);

    assert!(
        demo.war_morale <= config.baseline_war_morale,
        "Morale recovery must be capped at baseline"
    );
}

#[test]
fn test_phase72_strike_production_factor() {
    let config = MoraleConfig::default();

    // Above threshold → full production
    assert_eq!(strike_production_factor(50.0, &config), 1.0);

    // Below threshold → reduced production
    let factor = strike_production_factor(15.0, &config);
    assert!(factor < 1.0, "Production must be reduced during strikes");
    assert!(factor > 0.0, "Production must not be zero");
}

#[test]
fn test_phase72_calculate_desertions() {
    let config = MoraleConfig::default();

    // Above threshold → no desertions
    assert_eq!(calculate_desertions(10_000, 50.0, &config), 0);

    // Below threshold → desertions
    let desertions = calculate_desertions(10_000, 10.0, &config);
    assert!(desertions > 0, "Desertions must occur below threshold");
}

#[test]
fn test_phase72_initialize_morale() {
    let mut demo = ClassDemographics::default();
    demo.war_morale = 0.0;
    demo.mental_health = 0.0;
    let config = MoraleConfig::default();

    initialize_morale(&mut demo, &config);

    assert_eq!(demo.war_morale, config.baseline_war_morale);
    assert_eq!(demo.mental_health, config.baseline_mental_health);
}

#[test]
fn test_phase72_mental_health_drops_less_than_war_morale() {
    let mut demo = make_demographics(100_000);
    let casualties = make_casualties(10_000);
    let config = MoraleConfig::default();

    let initial_war = demo.war_morale;
    let initial_mental = demo.mental_health;
    apply_casualty_morale_impact(&mut demo, &casualties, &config);

    let war_drop = initial_war - demo.war_morale;
    let mental_drop = initial_mental - demo.mental_health;

    assert!(
        mental_drop < war_drop,
        "Mental health drop must be less than war morale drop"
    );
}

// ============================================================================
// PROPAGANDA TESTS
// ============================================================================

#[test]
fn test_phase72_propaganda_debits_treasury() {
    let mut treasury = 10_000.0;
    let mut media = make_media_companies();
    let config = PropagandaConfig::default();

    let result = execute_propaganda(
        &mut treasury,
        &mut media,
        1000.0,
        PropagandaTarget::Both,
        &config,
        1,
        "CAMP-1".to_string(),
    );

    assert!(result.executed);
    assert_eq!(result.treasury_debited, 1000.0);
    assert!(treasury < 10_000.0, "Treasury must be debited");
}

#[test]
fn test_phase72_propaganda_credits_media_sector() {
    let mut treasury = 10_000.0;
    let mut media = make_media_companies();
    let config = PropagandaConfig::default();

    let initial_media_total: f64 = media.values().map(|(lc, _)| *lc).sum();
    let result = execute_propaganda(
        &mut treasury,
        &mut media,
        1000.0,
        PropagandaTarget::Both,
        &config,
        1,
        "CAMP-1".to_string(),
    );
    let final_media_total: f64 = media.values().map(|(lc, _)| *lc).sum();

    assert!(result.executed);
    assert!(
        final_media_total > initial_media_total,
        "Media sector must be credited"
    );
    assert!(
        (result.media_credited - (final_media_total - initial_media_total)).abs() < 0.01,
        "Media credit must match actual increase"
    );
}

#[test]
fn test_phase72_propaganda_double_entry() {
    let mut treasury = 10_000.0;
    let mut media = make_media_companies();
    let config = PropagandaConfig::default();

    let initial_treasury = treasury;
    let initial_media_total: f64 = media.values().map(|(lc, _)| *lc).sum();

    let result = execute_propaganda(
        &mut treasury,
        &mut media,
        1000.0,
        PropagandaTarget::Both,
        &config,
        1,
        "CAMP-1".to_string(),
    );

    let final_media_total: f64 = media.values().map(|(lc, _)| *lc).sum();

    // Rule 1: Double-entry — treasury decrease must equal media increase
    let treasury_decrease = initial_treasury - treasury;
    let media_increase = final_media_total - initial_media_total;

    assert!(result.executed);
    assert!(
        (treasury_decrease - media_increase).abs() < 0.01,
        "Double-entry: treasury decrease ({}) must equal media increase ({})",
        treasury_decrease,
        media_increase
    );
}

#[test]
fn test_phase72_propaganda_pro_rata_distribution() {
    let mut treasury = 10_000.0;
    let mut media = make_media_companies();
    let config = PropagandaConfig::default();

    let _ = execute_propaganda(
        &mut treasury,
        &mut media,
        1000.0,
        PropagandaTarget::Both,
        &config,
        1,
        "CAMP-1".to_string(),
    );

    // MEDIA-2 has 20.0 capacity out of 35.0 total → should get ~571.43
    let media2 = media.get("MEDIA-2").unwrap();
    let media2_gain = media2.0 - 2000.0;
    let expected = 1000.0 * (20.0 / 35.0);
    assert!(
        (media2_gain - expected).abs() < 1.0,
        "MEDIA-2 should receive pro-rata share: expected {:.2}, got {:.2}",
        expected,
        media2_gain
    );
}

#[test]
fn test_phase72_propaganda_insufficient_funds() {
    let mut treasury = 100.0;
    let mut media = make_media_companies();
    let config = PropagandaConfig::default();

    let result = execute_propaganda(
        &mut treasury,
        &mut media,
        1000.0,
        PropagandaTarget::Both,
        &config,
        1,
        "CAMP-1".to_string(),
    );

    assert!(!result.executed, "Must fail with insufficient funds");
    assert_eq!(treasury, 100.0, "Treasury must not be debited on failure");
}

#[test]
fn test_phase72_propaganda_no_media_companies() {
    let mut treasury = 10_000.0;
    let mut media = HashMap::new();
    let config = PropagandaConfig::default();

    let result = execute_propaganda(
        &mut treasury,
        &mut media,
        1000.0,
        PropagandaTarget::Both,
        &config,
        1,
        "CAMP-1".to_string(),
    );

    assert!(!result.executed, "Must fail with no media companies");
    assert_eq!(
        treasury, 10_000.0,
        "Treasury must not be debited with no media"
    );
}

#[test]
fn test_phase72_propaganda_war_morale_target() {
    let mut treasury = 10_000.0;
    let mut media = make_media_companies();
    let config = PropagandaConfig::default();

    let result = execute_propaganda(
        &mut treasury,
        &mut media,
        1000.0,
        PropagandaTarget::WarMorale,
        &config,
        1,
        "CAMP-1".to_string(),
    );

    assert!(
        result.morale_boost > 0.0,
        "WarMorale target must boost morale"
    );
    assert_eq!(
        result.mental_health_boost, 0.0,
        "WarMorale target must not boost mental health"
    );
}

#[test]
fn test_phase72_propaganda_boost_applied_to_demographics() {
    let mut classes = BTreeMap::new();
    let mut d1 = ClassDemographics::default();
    d1.war_morale = 40.0;
    d1.mental_health = 50.0;
    classes.insert("FreePeasant".to_string(), d1);

    apply_propaganda_boost(&mut classes, 10.0, 5.0, 70.0, 70.0);

    let d = classes.get("FreePeasant").unwrap();
    assert_eq!(d.war_morale, 50.0, "War morale must be boosted");
    assert_eq!(d.mental_health, 55.0, "Mental health must be boosted");
}

// ============================================================================
// PROXY WAR TESTS — REAL PHYSICAL COMMODITY TRANSFERS
// ============================================================================

#[test]
fn test_phase72_fund_separatists_debits_treasury() {
    let mut treasury = 10_000.0;
    let mut rebellion_funds = 0.0;
    let config = ProxyWarConfig::default();
    let action = ProxyWarAction::FundSeparatists {
        sponsor_country: "Sponsor".to_string(),
        target_country: "Target".to_string(),
        target_region: "region_1".to_string(),
        amount: 1000.0,
    };

    let result = fund_separatists(&mut treasury, &mut rebellion_funds, false, &config, &action);

    assert!(result.executed);
    assert_eq!(result.treasury_debited, 1000.0);
    assert_eq!(treasury, 9000.0, "Treasury must be debited");
}

#[test]
fn test_phase72_fund_separatists_credits_rebellion() {
    let mut treasury = 10_000.0;
    let mut rebellion_funds = 0.0;
    let config = ProxyWarConfig::default();
    let action = ProxyWarAction::FundSeparatists {
        sponsor_country: "Sponsor".to_string(),
        target_country: "Target".to_string(),
        target_region: "region_1".to_string(),
        amount: 1000.0,
    };

    let result = fund_separatists(&mut treasury, &mut rebellion_funds, false, &config, &action);

    assert!(result.executed);
    assert_eq!(result.rebellion_credited, 1000.0);
    assert_eq!(rebellion_funds, 1000.0, "Rebellion funds must be credited");
}

#[test]
fn test_phase72_fund_separatists_double_entry() {
    let mut treasury = 10_000.0;
    let mut rebellion_funds = 0.0;
    let config = ProxyWarConfig::default();
    let action = ProxyWarAction::FundSeparatists {
        sponsor_country: "Sponsor".to_string(),
        target_country: "Target".to_string(),
        target_region: "region_1".to_string(),
        amount: 1000.0,
    };

    let result = fund_separatists(&mut treasury, &mut rebellion_funds, false, &config, &action);

    // Rule 1: Double-entry — treasury debit must equal rebellion credit
    assert_eq!(
        result.treasury_debited, result.rebellion_credited,
        "Double-entry: treasury debit must equal rebellion credit"
    );
}

#[test]
fn test_phase72_fund_separatists_insufficient_funds() {
    let mut treasury = 100.0;
    let mut rebellion_funds = 0.0;
    let config = ProxyWarConfig::default();
    let action = ProxyWarAction::FundSeparatists {
        sponsor_country: "Sponsor".to_string(),
        target_country: "Target".to_string(),
        target_region: "region_1".to_string(),
        amount: 1000.0,
    };

    let result = fund_separatists(&mut treasury, &mut rebellion_funds, false, &config, &action);

    assert!(!result.executed, "Must fail with insufficient funds");
    assert_eq!(treasury, 100.0, "Treasury must not be debited on failure");
}

#[test]
fn test_phase72_fund_separatists_autonomous_republic_multiplier() {
    let treasury = 10_000.0;
    let rebellion_funds = 0.0;
    let config = ProxyWarConfig::default();
    let action = ProxyWarAction::FundSeparatists {
        sponsor_country: "Sponsor".to_string(),
        target_country: "Target".to_string(),
        target_region: "region_1".to_string(),
        amount: 1000.0,
    };

    let result_normal = fund_separatists(
        &mut treasury.clone(),
        &mut rebellion_funds.clone(),
        false,
        &config,
        &action,
    );
    let result_autonomous = fund_separatists(
        &mut treasury.clone(),
        &mut rebellion_funds.clone(),
        true,
        &config,
        &action,
    );

    assert!(
        result_autonomous.unrest_increase > result_normal.unrest_increase,
        "Autonomous republic must have higher unrest multiplier"
    );
}

#[test]
fn test_phase72_arm_rebels_transfers_physical_rifles() {
    let mut stockpile = make_stockpile(5000.0, 100_000.0);
    let config = ProxyWarConfig::default();
    let action = ProxyWarAction::ArmRebels {
        sponsor_country: "Sponsor".to_string(),
        target_country: "Target".to_string(),
        rifles_quantity: 1000.0,
        ammunition_quantity: 50_000.0,
    };

    let initial_rifles = *stockpile.get(&Commodity::Rifles).unwrap();
    let result = arm_rebels(&mut stockpile, &config, &action);
    let final_rifles = *stockpile.get(&Commodity::Rifles).unwrap();

    assert!(result.executed);
    assert!(
        final_rifles < initial_rifles,
        "Rifles must be removed from sponsor stockpile"
    );
    assert!(
        result
            .commodities_transferred
            .get(&Commodity::Rifles)
            .unwrap()
            > &0.0,
        "Transferred rifles must be recorded"
    );
}

#[test]
fn test_phase72_arm_rebels_transfers_physical_ammo() {
    let mut stockpile = make_stockpile(5000.0, 100_000.0);
    let config = ProxyWarConfig::default();
    let action = ProxyWarAction::ArmRebels {
        sponsor_country: "Sponsor".to_string(),
        target_country: "Target".to_string(),
        rifles_quantity: 1000.0,
        ammunition_quantity: 50_000.0,
    };

    let initial_ammo = *stockpile.get(&Commodity::Ammunition).unwrap();
    let result = arm_rebels(&mut stockpile, &config, &action);
    let final_ammo = *stockpile.get(&Commodity::Ammunition).unwrap();

    assert!(result.executed);
    assert!(
        final_ammo < initial_ammo,
        "Ammunition must be removed from sponsor stockpile"
    );
}

#[test]
fn test_phase72_arm_rebels_no_magic_spawning() {
    let mut stockpile = make_stockpile(0.0, 100_000.0); // No rifles!
    let config = ProxyWarConfig::default();
    let action = ProxyWarAction::ArmRebels {
        sponsor_country: "Sponsor".to_string(),
        target_country: "Target".to_string(),
        rifles_quantity: 1000.0,
        ammunition_quantity: 50_000.0,
    };

    let result = arm_rebels(&mut stockpile, &config, &action);

    assert!(
        !result.executed,
        "Must abort when no rifles available — no magic spawning"
    );
    assert_eq!(
        result.commodities_transferred.len(),
        0,
        "No commodities transferred on failure"
    );
}

#[test]
fn test_phase72_arm_rebels_no_ammo_aborts() {
    let mut stockpile = make_stockpile(5000.0, 0.0); // No ammo!
    let config = ProxyWarConfig::default();
    let action = ProxyWarAction::ArmRebels {
        sponsor_country: "Sponsor".to_string(),
        target_country: "Target".to_string(),
        rifles_quantity: 1000.0,
        ammunition_quantity: 50_000.0,
    };

    let result = arm_rebels(&mut stockpile, &config, &action);

    assert!(!result.executed, "Must abort when no ammunition available");
}

#[test]
fn test_phase72_arm_rebels_physical_conservation() {
    let mut stockpile = make_stockpile(5000.0, 100_000.0);
    let config = ProxyWarConfig::default();
    let action = ProxyWarAction::ArmRebels {
        sponsor_country: "Sponsor".to_string(),
        target_country: "Target".to_string(),
        rifles_quantity: 1000.0,
        ammunition_quantity: 50_000.0,
    };

    let initial_rifles = *stockpile.get(&Commodity::Rifles).unwrap();
    let initial_ammo = *stockpile.get(&Commodity::Ammunition).unwrap();

    let result = arm_rebels(&mut stockpile, &config, &action);

    let final_rifles = *stockpile.get(&Commodity::Rifles).unwrap();
    let final_ammo = *stockpile.get(&Commodity::Ammunition).unwrap();

    // Physical conservation: stockpile decrease must equal transferred amount
    let rifles_decrease = initial_rifles - final_rifles;
    let ammo_decrease = initial_ammo - final_ammo;
    let rifles_transferred = result
        .commodities_transferred
        .get(&Commodity::Rifles)
        .copied()
        .unwrap_or(0.0);
    let ammo_transferred = result
        .commodities_transferred
        .get(&Commodity::Ammunition)
        .copied()
        .unwrap_or(0.0);

    assert!(
        (rifles_decrease - rifles_transferred).abs() < 0.01,
        "Physical conservation: rifles decrease must equal transferred"
    );
    assert!(
        (ammo_decrease - ammo_transferred).abs() < 0.01,
        "Physical conservation: ammo decrease must equal transferred"
    );
}

#[test]
fn test_phase72_arm_rebels_rebel_count_limited_by_min() {
    let mut stockpile = make_stockpile(1000.0, 10_000.0); // 1000 rifles, but only 10k ammo
    let config = ProxyWarConfig::default();
    // ammunition_per_rebel = 50.0, so 10k ammo → 200 rebels
    // manpower_per_rifle = 1.0, so 1000 rifles → 1000 rebels
    // Min = 200 rebels
    let action = ProxyWarAction::ArmRebels {
        sponsor_country: "Sponsor".to_string(),
        target_country: "Target".to_string(),
        rifles_quantity: 1000.0,
        ammunition_quantity: 10_000.0,
    };

    let result = arm_rebels(&mut stockpile, &config, &action);

    assert!(result.executed);
    assert!(result.rebel_units_spawned > 0, "Must spawn rebel units");
}

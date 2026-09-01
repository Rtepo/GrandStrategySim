//! Phase 76: OOB generator heterogeneity tests.
//!
//! Validates that the rewritten `generate_asymmetric_oob` produces
//! materially heterogeneous military structures across countries with
//! different GDP, population, and average wage levels.

use sim_engine::military::oob::generate_asymmetric_oob;
use sim_engine::military::units::UnitType;

/// Test 1: A rich, populous country should have more units than a poor, small one.
#[test]
fn rich_country_has_more_units_than_poor() {
    let mut rng = rand::thread_rng();

    let rich_oob = generate_asymmetric_oob(
        "RichNation",
        50_000_000_000.0, // 50B GDP
        5000.0,           // High GDP per capita
        4000.0,           // High average wage
        40_000_000,       // 40M population
        vec!["r1".to_string(), "r2".to_string(), "r3".to_string()],
        &mut rng,
    );

    let poor_oob = generate_asymmetric_oob(
        "PoorNation",
        30_000_000.0, // 30M GDP
        300.0,        // Low GDP per capita
        240.0,        // Low average wage
        2_000_000,    // 2M population
        vec!["r1".to_string()],
        &mut rng,
    );

    assert!(
        rich_oob.unit_count() > poor_oob.unit_count(),
        "Rich country should have more units: rich={} poor={}",
        rich_oob.unit_count(),
        poor_oob.unit_count()
    );
    assert!(
        rich_oob.total_manpower() > poor_oob.total_manpower(),
        "Rich country should have more manpower: rich={} poor={}",
        rich_oob.total_manpower(),
        poor_oob.total_manpower()
    );
}

/// Test 2: Poor countries should not have tanks (converted to infantry).
#[test]
fn poor_country_has_no_tanks() {
    let mut rng = rand::thread_rng();
    let poor_oob = generate_asymmetric_oob(
        "PoorNation",
        30_000_000.0,
        300.0,
        240.0,
        2_000_000,
        vec!["r1".to_string()],
        &mut rng,
    );

    let tanks = poor_oob.collect_units_by_type(UnitType::Tanks);
    assert!(tanks.is_empty(), "Poor country should have no tanks");
    let infantry = poor_oob.collect_units_by_type(UnitType::Infantry);
    assert!(!infantry.is_empty(), "Poor country should have infantry");
}

/// Test 3: Rich countries should have tanks.
#[test]
fn rich_country_has_tanks() {
    let mut rng = rand::thread_rng();
    let rich_oob = generate_asymmetric_oob(
        "RichNation",
        50_000_000_000.0,
        5000.0,
        4000.0,
        40_000_000,
        vec!["r1".to_string(), "r2".to_string(), "r3".to_string()],
        &mut rng,
    );

    let tanks = rich_oob.collect_units_by_type(UnitType::Tanks);
    assert!(!tanks.is_empty(), "Rich country should have tanks");
}

/// Test 4: Two countries with different GDP per capita should have different OOB structures.
#[test]
fn different_gdp_pc_produces_different_oob() {
    let mut rng = rand::thread_rng();

    let oob_a = generate_asymmetric_oob(
        "CountryA",
        10_000_000_000.0,
        2000.0,
        1600.0,
        5_000_000,
        vec!["r1".to_string(), "r2".to_string()],
        &mut rng,
    );

    let oob_b = generate_asymmetric_oob(
        "CountryB",
        10_000_000_000.0,
        4000.0,
        3200.0,
        5_000_000,
        vec!["r1".to_string(), "r2".to_string()],
        &mut rng,
    );

    // Countries with different GDP per capita should have different unit counts
    // or different regiment/unit structure (richer = more regiments/units).
    let struct_a = (
        oob_a.armies.len(),
        oob_a.unit_count(),
        oob_a.total_manpower(),
    );
    let struct_b = (
        oob_b.armies.len(),
        oob_b.unit_count(),
        oob_b.total_manpower(),
    );
    assert!(
        struct_a != struct_b || oob_a.unit_count() != oob_b.unit_count(),
        "Countries with different GDP per capita should have different OOB structure"
    );
}

/// Test 5: All unit IDs should be unique within an OOB.
#[test]
fn all_unit_ids_are_unique() {
    let mut rng = rand::thread_rng();
    let oob = generate_asymmetric_oob(
        "TestCountry",
        5_000_000_000.0,
        3000.0,
        2400.0,
        10_000_000,
        vec!["r1".to_string(), "r2".to_string(), "r3".to_string()],
        &mut rng,
    );

    let ids = oob.all_unit_ids();
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(ids.len(), unique.len(), "All unit IDs should be unique");
}

/// Test 6: Very small country should still have at least 1 army with positive manpower.
#[test]
fn tiny_country_has_minimal_oob() {
    let mut rng = rand::thread_rng();
    let oob = generate_asymmetric_oob(
        "TinyNation",
        1_000_000.0, // 1M GDP
        100.0,       // Very low GDP per capita
        80.0,        // Very low average wage
        100_000,     // 100K population
        vec!["r1".to_string()],
        &mut rng,
    );

    assert!(
        !oob.armies.is_empty(),
        "Tiny country should have at least 1 army"
    );
    assert!(
        oob.total_manpower() > 0,
        "Tiny country should have positive manpower"
    );
    assert!(
        oob.unit_count() > 0,
        "Tiny country should have at least 1 unit"
    );
}

/// Test 7: Base unit manpower should not be artificially inflated to 5000 for small countries.
#[test]
fn small_country_unit_manpower_not_inflated() {
    let mut rng = rand::thread_rng();
    let oob = generate_asymmetric_oob(
        "SmallNation",
        5_000_000.0,
        200.0,
        160.0,
        500_000, // 500K population
        vec!["r1".to_string()],
        &mut rng,
    );

    // With 500K population and a conscription rate around 2%, manpower pool ~10K.
    // Total units should be small enough that unit manpower is reasonable.
    // The old code clamped to min 100, max 5000 — we removed the max clamp.
    // So unit manpower should be well below 5000 for a small country.
    let max_manpower: i64 = oob
        .armies
        .iter()
        .flat_map(|a| a.divisions.iter())
        .flat_map(|d| d.regiments.iter())
        .flat_map(|r| r.units.iter())
        .map(|u| u.manpower)
        .max()
        .unwrap_or(0);

    // For a 500K population country, no unit should have 5000+ manpower
    // (that would be 1% of the entire population in one unit).
    assert!(
        max_manpower < 5000,
        "Small country unit manpower should not be inflated to 5000: max={}",
        max_manpower
    );
}

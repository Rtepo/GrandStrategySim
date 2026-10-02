//! Demographic Dynamics — Epic Test (W4, macro-viability plan §4)
//!
//! Regression coverage for the static-demographics failure: the rural class
//! mix was a pure `match start_year` table, pinning the "peasant" classes
//! (Serf + FreePeasant + LandlessLaborer) to ~47–48% of every region at a
//! 1950 start regardless of `development_level` — a field that previously
//! only scaled savings. World-gen demographics are now
//! `f(start_year, development_level)`:
//!
//!   * `rural_share = era_baseline × (1 − dev × URBAN_DEV_ELASTICITY)`,
//!     clamped to `[MIN_RURAL_SHARE, era_baseline]` (food-system floor);
//!   * surviving serfdom scales with `(1 − dev)`, freed serfs splitting
//!     between FreePeasant and LandlessLaborer at the era's own ratio;
//!   * the urban Worker share drifts with development (early eras
//!     industrialize, the late anchor tertiarizes).
//!
//! The function is pure — it consumes no RNG — so the shared worldgen rng
//! stream and seed determinism are untouched.
//!
//! Assertions:
//!   A — structure varies with development: at identical population and
//!        start_year, dev = 0.1 vs dev = 0.9 differ in peasant share by
//!        ≥10pp, and the urban classes (Worker + Bourgeoisie) grow with
//!        development in every era.
//!   B — validity/conservation: Σ class populations == region population
//!        exactly and every class population ≥ 0, swept across the
//!        era × development grid including degenerate populations.
//!   C — worldgen variation: in generated worlds, peasant shares are not
//!        constant — pooled region spread ≥ 10pp, country means differ,
//!        and the legacy ~0.475 static pin no longer describes the world.
//!   D — determinism-by-construction: every generated region's stored
//!        demographics equal a fresh evaluation of the pure
//!        `f(population, start_year, development_level)` model.
//!
//! Note (plan §4.2 D2): the runtime `process_rural_urban_class_transitions`
//! mechanism exists but is not wired into the turn loop, so this test
//! covers the worldgen deliverable only.
//!
//! # Run
//! ```bash
//! CI=true cargo nextest run --release --features "epic-tests diagnostic" -E 'test(demographic)'
//! ```

use std::collections::BTreeMap;

use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
use sim_engine::registries::Registries;
use sim_engine::society::geography::{
    generate_class_demographics, RegionalClassDemographics, RuralClass, UrbanClass,
};
use tempfile::TempDir;

/// Legacy static share this test must retire: at a 1950 start every region
/// used to be 50% rural with 95% of rural in peasant classes → 0.475.
const LEGACY_STATIC_PEASANT_SHARE: f64 = 0.475;

fn total_pop(demo: &RegionalClassDemographics) -> i64 {
    demo.rural_classes.values().map(|d| d.population).sum::<i64>()
        + demo.urban_classes.values().map(|d| d.population).sum::<i64>()
}

fn class_pop(demo: &RegionalClassDemographics, rural: RuralClass) -> i64 {
    demo.rural_classes
        .get(&rural)
        .map(|d| d.population)
        .unwrap_or(0)
}

fn urban_pop(demo: &RegionalClassDemographics, class: UrbanClass) -> i64 {
    demo.urban_classes
        .get(&class)
        .map(|d| d.population)
        .unwrap_or(0)
}

/// "Peasants": the agrarian laboring classes (everyone rural except the
/// Aristocracy). This is the aggregate that was pinned at ~47–48%.
fn peasant_share(demo: &RegionalClassDemographics) -> f64 {
    let peasants = class_pop(demo, RuralClass::Serf)
        + class_pop(demo, RuralClass::FreePeasant)
        + class_pop(demo, RuralClass::LandlessLaborer);
    peasants as f64 / total_pop(demo).max(1) as f64
}

fn share_of(demo: &RegionalClassDemographics, pop: i64) -> f64 {
    pop as f64 / total_pop(demo).max(1) as f64
}

// ────────────────────────────────────────────────────────────────────────
// A — structure varies with development at identical population and era
// ────────────────────────────────────────────────────────────────────────

#[test]
fn test_demographic_structure_varies_with_development() {
    let ethnic: BTreeMap<String, f64> = BTreeMap::new();
    let pop = 1_000_000_i64;

    for year in [1900_u32, 1925, 1950, 1975] {
        let low = generate_class_demographics(pop, year, 0.1, "TestCulture", &ethnic);
        let high = generate_class_demographics(pop, year, 0.9, "TestCulture", &ethnic);

        let peasant_low = peasant_share(&low);
        let peasant_high = peasant_share(&high);
        assert!(
            peasant_low - peasant_high >= 0.10,
            "year {}: peasant share must drop ≥10pp from dev=0.1 ({:.3}) to dev=0.9 ({:.3})",
            year,
            peasant_low,
            peasant_high
        );

        // Urbanization: both urban classes grow in absolute share with dev.
        let worker_low = share_of(&low, urban_pop(&low, UrbanClass::Worker));
        let worker_high = share_of(&high, urban_pop(&high, UrbanClass::Worker));
        let bourg_low = share_of(&low, urban_pop(&low, UrbanClass::Bourgeoisie));
        let bourg_high = share_of(&high, urban_pop(&high, UrbanClass::Bourgeoisie));
        assert!(
            worker_high > worker_low,
            "year {}: Worker share must grow with development ({:.3} → {:.3})",
            year,
            worker_low,
            worker_high
        );
        assert!(
            bourg_high > bourg_low,
            "year {}: Bourgeoisie share must grow with development ({:.3} → {:.3})",
            year,
            bourg_low,
            bourg_high
        );

        // Land reform: where the era still has serfs, development emancipates.
        if year <= 1925 {
            let serf_low = share_of(&low, class_pop(&low, RuralClass::Serf));
            let serf_high = share_of(&high, class_pop(&high, RuralClass::Serf));
            assert!(
                serf_high < serf_low,
                "year {}: Serf share must shrink with development ({:.3} → {:.3})",
                year,
                serf_low,
                serf_high
            );
        }
    }

    // Sanity: dev = 0 reproduces the documented Phase 44 era baselines.
    for (year, expected_rural) in [(1900_u32, 0.80), (1925, 0.65), (1950, 0.50), (1975, 0.40)] {
        let base = generate_class_demographics(pop, year, 0.0, "TestCulture", &ethnic);
        let rural = share_of(
            &base,
            base.rural_classes.values().map(|d| d.population).sum(),
        );
        assert!(
            (rural - expected_rural).abs() < 0.001,
            "year {}: dev=0 must sit on the era baseline (expected rural {:.2}, got {:.3})",
            year,
            expected_rural,
            rural
        );
    }

    // Monotonicity: peasant share never increases as development rises.
    for year in [1900_u32, 1925, 1950, 1975] {
        let mut prev = f64::MAX;
        for dev_i in 0..=10 {
            let dev = dev_i as f64 / 10.0;
            let demo = generate_class_demographics(pop, year, dev, "TestCulture", &ethnic);
            let share = peasant_share(&demo);
            assert!(
                share <= prev + 1e-9,
                "year {}: peasant share must be non-increasing in dev (dev {:.1}: {:.3} > prev {:.3})",
                year,
                dev,
                share,
                prev
            );
            prev = share;
        }
    }
}

// ────────────────────────────────────────────────────────────────────────
// B — shares are valid: conserved, nonnegative, degenerate-safe
// ────────────────────────────────────────────────────────────────────────

#[test]
fn test_demographic_shares_conserved_and_nonnegative() {
    let ethnic: BTreeMap<String, f64> = BTreeMap::new();

    for year in [1850_u32, 1900, 1901, 1925, 1926, 1950, 1951, 1975, 2000] {
        for dev_i in 0..=10 {
            let dev = dev_i as f64 / 10.0;
            for &pop in &[0_i64, 1, 3, 999, 12_345, 1_000_000] {
                let demo = generate_class_demographics(pop, year, dev, "C", &ethnic);
                for (class, d) in demo
                    .rural_classes
                    .iter()
                    .map(|(c, d)| (format!("{:?}", c), d))
                    .chain(demo.urban_classes.iter().map(|(c, d)| (format!("{:?}", c), d)))
                {
                    assert!(
                        d.population >= 0,
                        "year {} dev {:.1} pop {}: class {} has negative population {}",
                        year,
                        dev,
                        pop,
                        class,
                        d.population
                    );
                }
                assert_eq!(
                    total_pop(&demo),
                    pop,
                    "year {} dev {:.1}: class populations must sum to region population {}",
                    year,
                    dev,
                    pop
                );
            }
        }
    }
}

// ────────────────────────────────────────────────────────────────────────
// C/D — generated worlds: no static pin, cross-country spread, determinism
// ────────────────────────────────────────────────────────────────────────

#[test]
fn test_demographic_worldgen_spread_and_determinism() {
    let registries = Registries::native_only();
    let tmp = TempDir::new().expect("temp dir");
    let options = GenerateOptions {
        country_count: 6,
        start_year: StartYear::Y1950,
        seed: Some(42),
    };

    // `country.regions` is empty immediately after worldgen — the flat
    // `GeneratedWorld.regions` map is authoritative; `owner_country` links
    // each region back to its country.
    let GeneratedWorld { regions, .. } =
        generate_world(tmp.path(), options, &registries).expect("world generation failed");

    // Collect (country, region_id, dev, peasant_share) for all regions.
    let mut shares: Vec<(String, String, f64, f64)> = Vec::new();
    let mut country_means: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    for region in regions.values() {
        let demo = &region.class_demographics;
        assert_eq!(
            total_pop(demo),
            region.population,
            "worldgen [{}/{}]: class populations must sum to region population",
            region.owner_country,
            region.id
        );
        let peasant = peasant_share(demo);
        shares.push((
            region.owner_country.clone(),
            region.id.clone(),
            region.development_level,
            peasant,
        ));
        let entry = country_means
            .entry(region.owner_country.clone())
            .or_insert((0.0, 0));
        entry.0 += peasant;
        entry.1 += 1;
    }
    assert!(
        shares.len() >= 12,
        "expected ≥12 regions across 6 countries, found {}",
        shares.len()
    );

    // C1 — the static split is gone: pooled peasant-share spread ≥ 10pp.
    let min_peasant = shares.iter().map(|s| s.3).fold(f64::MAX, f64::min);
    let max_peasant = shares.iter().map(|s| s.3).fold(f64::MIN, f64::max);
    assert!(
        max_peasant - min_peasant >= 0.10,
        "peasant share must vary ≥10pp across regions at identical seed/year (min {:.3}, max {:.3})",
        min_peasant,
        max_peasant
    );

    // C2 — the legacy ~0.475 pin no longer describes the world: fewer than
    // half of all regions may sit within ±1pp of the old constant.
    let pinned = shares
        .iter()
        .filter(|s| (s.3 - LEGACY_STATIC_PEASANT_SHARE).abs() < 0.01)
        .count();
    assert!(
        pinned * 2 < shares.len(),
        "{} of {} regions still pinned at the legacy {:.1}% static share",
        pinned,
        shares.len(),
        LEGACY_STATIC_PEASANT_SHARE * 100.0
    );

    // C3 — shares are not constant across countries.
    let means: Vec<f64> = country_means
        .values()
        .map(|(sum, n)| sum / *n as f64)
        .collect();
    let mean_min = means.iter().copied().fold(f64::MAX, f64::min);
    let mean_max = means.iter().copied().fold(f64::MIN, f64::max);
    assert!(
        mean_max - mean_min > 0.01,
        "per-country mean peasant shares must differ (range {:.3})",
        mean_max - mean_min
    );

    // C4 — variation is development-driven: the most developed region is
    // meaningfully more urbanized than the least developed one.
    let (low_dev, high_dev) = shares.iter().fold(
        (f64::MAX, f64::MIN),
        |(lo, hi), s| (lo.min(s.2), hi.max(s.2)),
    );
    assert!(
        high_dev - low_dev >= 0.3,
        "test premise: need a real development spread in the generated world ({:.2}–{:.2})",
        low_dev,
        high_dev
    );
    let peasant_at_low_dev = shares
        .iter()
        .filter(|s| s.2 <= low_dev + 0.05)
        .map(|s| s.3)
        .fold(0.0, f64::max);
    let peasant_at_high_dev = shares
        .iter()
        .filter(|s| s.2 >= high_dev - 0.05)
        .map(|s| s.3)
        .fold(f64::MAX, f64::min);
    assert!(
        peasant_at_low_dev - peasant_at_high_dev >= 0.10,
        "low-dev regions ({:.3} peasant) must exceed high-dev regions ({:.3}) by ≥10pp",
        peasant_at_low_dev,
        peasant_at_high_dev
    );

    // D — determinism-by-construction: the stored demographics of every
    // generated region must equal a fresh evaluation of the pure function
    // `generate_class_demographics` at the region's own
    // (population, start_year, development_level). That proves the worldgen
    // pipeline stores exactly the deterministic model output — no hidden
    // per-region RNG draws inside the function and no post-hoc mutation.
    //
    // NOTE: a two-`generate_world` same-seed equality check is *not*
    // asserted here: upstream worldgen consumes the seeded stream while
    // iterating `HashMap`s (e.g. `assign_regional_heads` over
    // `regions.values_mut()`), so sequential same-seed generations already
    // diverge independent of demographics. That upstream gap is orthogonal
    // to this workstream.
    let ethnic: BTreeMap<String, f64> = BTreeMap::new();
    for region in regions.values() {
        let recomputed =
            generate_class_demographics(region.population, 1950, region.development_level, "C", &ethnic);
        let stored_rural: Vec<i64> = region
            .class_demographics
            .rural_classes
            .values()
            .map(|d| d.population)
            .collect();
        let recomputed_rural: Vec<i64> = recomputed
            .rural_classes
            .values()
            .map(|d| d.population)
            .collect();
        let stored_urban: Vec<i64> = region
            .class_demographics
            .urban_classes
            .values()
            .map(|d| d.population)
            .collect();
        let recomputed_urban: Vec<i64> = recomputed
            .urban_classes
            .values()
            .map(|d| d.population)
            .collect();
        assert_eq!(
            stored_rural, recomputed_rural,
            "[{}]: stored rural demographics must equal f(pop={}, year=1950, dev={:.3})",
            region.id, region.population, region.development_level
        );
        assert_eq!(
            stored_urban, recomputed_urban,
            "[{}]: stored urban demographics must equal f(pop={}, year=1950, dev={:.3})",
            region.id, region.population, region.development_level
        );
    }

    eprintln!(
        "DEMOGRAPHICS PASS: {} regions, peasant share {:.1}%–{:.1}%, dev range {:.2}–{:.2}",
        shares.len(),
        min_peasant * 100.0,
        max_peasant * 100.0,
        low_dev,
        high_dev
    );
}

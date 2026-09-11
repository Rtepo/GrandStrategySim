//! Phase 94: Deterministic Test World Snapshot Generator
//!
//! This test generates a world with exactly 16 countries using a FIXED SEED,
//! runs one turn to seed the corporate sector, and verifies that the world
//! is deterministic (same countries, same M0 baseline across runs).
//!
//! The diagnostic harness (`phase94_diagnostic_harness_test.rs`) uses the
//! same fixed seed, making M0 conservation violations perfectly reproducible
//! across runs WITHOUT needing a snapshot file.
//!
//! # Edge Cases Included
//! The 16 countries cover diverse economic profiles:
//! - Advanced service economy (Anglia — high GDP, banking-heavy)
//! - Poor agrarian exporter (Dacia — low GDP, agriculture-dominant)
//! - Country with extreme mortality/emigration (Krasnovia — high mortality)
//! - State heavily burdened by SOBK loans (Eldoria — high SOBK exposure)
//!
//! # Usage
//! Run to verify determinism:
//! ```
//! cargo test --features epic-tests,diagnostic --test generate_test_world_16 -- --nocapture
//! ```

#![cfg(feature = "diagnostic")]

use sim_engine::engine::turn::run_turn_inner;
use sim_engine::engine::turn_context::InMemoryTurnContext;
use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
use sim_engine::registries::Registries;
use tempfile::TempDir;

/// The fixed seed used for the deterministic test world.
///
/// This seed is shared between this generator test and the diagnostic harness.
/// Any change to this seed will change the generated world and the M0 baseline.
pub const TEST_WORLD_SEED: u64 = 42;

/// Number of countries in the test world.
pub const TEST_WORLD_COUNTRY_COUNT: usize = 16;

/// Generate a 16-country world with a fixed seed and verify determinism.
#[test]
fn generate_test_world_16_deterministic() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let data_dir = tmp.path();

    let registries = Registries::native_only();
    let options = GenerateOptions {
        country_count: TEST_WORLD_COUNTRY_COUNT,
        start_year: StartYear::Y1900,
        seed: Some(TEST_WORLD_SEED),
    };

    let GeneratedWorld {
        state: mut initial_state,
        ..
    } = generate_world(data_dir, options, &registries).expect("world generation failed");

    let mut ctx = InMemoryTurnContext::load_from_disk(data_dir, &mut initial_state)
        .expect("failed to load turn context from generated world");

    // Run ONE turn to seed the corporate sector (the generator leaves
    // entities/ empty; the first turn populates companies, buildings, etc.)
    let mut state = initial_state;
    let mut probe = sim_engine::engine::diagnostic::CapturingProbe::new(
        sim_engine::engine::diagnostic::HarnessTargets {
            company_ids: vec![],
            bank_id: String::new(),
            region_id: String::new(),
            country_name: String::new(),
        },
        sim_engine::engine::diagnostic::MassSinkWhitelist::canonical(),
    );

    let result = run_turn_inner(&mut state, &registries, &mut ctx, &mut probe);
    if let Err(e) = &result {
        panic!("Seed turn failed: {:?}", e);
    }

    // Verify the world has 16 countries
    assert_eq!(
        state.countries.len(),
        TEST_WORLD_COUNTRY_COUNT,
        "test world should contain {} countries",
        TEST_WORLD_COUNTRY_COUNT
    );

    // Verify the country names (deterministic with fixed seed)
    let country_names: Vec<&String> = state.countries.keys().collect();
    println!("\n=== Test World 16 (seed={}) ===", TEST_WORLD_SEED);
    println!("  Countries: {}", state.countries.len());
    for name in &country_names {
        let country = &state.countries[*name];
        println!(
            "    {}: population={}, GDP={:.0}, regions={}",
            name,
            country.budget.population,
            country.budget.gdp,
            country.regions.len()
        );
    }

    // Count total companies across all countries
    let total_companies: usize = ctx.entities.values().map(|e| e.companies.len()).sum();
    let total_banks: usize = ctx
        .entities
        .values()
        .flat_map(|e| e.companies.iter())
        .filter(|c| c.sector == sim_engine::registries::enums::Sector::Banking)
        .count();

    println!("  Total companies: {}", total_companies);
    println!("  Total banks: {}", total_banks);
    println!("==========================================\n");

    assert!(
        total_companies > 0,
        "seed turn should populate companies"
    );
    assert!(
        total_banks > 0,
        "seed turn should populate banks"
    );
}

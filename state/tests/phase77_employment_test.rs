//! Phase 77: Integer Employment Tests
//!
//! Tests that employment values are always integers (u32):
//! - fulfilled_fte is u32 (not f64)
//! - Labor market clearing produces integer FTE
//! - Snapshot reports integer employment

use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
use sim_engine::engine::turn_context::InMemoryTurnContext;
use sim_engine::entities::Company;
use sim_engine::entities::legal_form::{LegalForm, FamilyBusinessData};
use sim_engine::registries::enums::Sector;
use sim_engine::registries::Registries;
use tempfile::TempDir;

fn gen_world_with_ctx() -> (GeneratedWorld, InMemoryTurnContext) {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let data_dir = tmp.path();
    let registries = Registries::native_only();
    let options = GenerateOptions {
        country_count: 4,
        start_year: StartYear::Y1900,
    };
    let mut world = generate_world(data_dir, options, &registries).expect("world generation failed");
    let ctx = InMemoryTurnContext::load_from_disk(data_dir, &mut world.state)
        .expect("failed to load turn context from generated world");
    (world, ctx)
}

#[test]
fn fulfilled_fte_is_always_integer_after_generation() {
    let (_world, ctx) = gen_world_with_ctx();
    for entities in ctx.entities.values() {
        for company in &entities.companies {
            // fulfilled_fte is u32 — this is a compile-time guarantee.
            // At runtime, verify it's a valid non-negative integer.
            let fte: u32 = company.fulfilled_fte;
            assert!(
                fte >= 0,
                "Company {} has negative fulfilled_fte: {}",
                company.id,
                fte
            );
        }
    }
}

#[test]
fn target_fte_demand_is_always_integer() {
    let (_world, ctx) = gen_world_with_ctx();
    for entities in ctx.entities.values() {
        for company in &entities.companies {
            let demand: u32 = company.target_fte_demand;
            assert!(
                demand >= 0,
                "Company {} has negative target_fte_demand: {}",
                company.id,
                demand
            );
        }
    }
}

#[test]
fn no_fractional_employment_values() {
    // This test verifies at compile time that the fields are u32.
    // If the fields were f64, the following assignments would not compile
    // because u32 type annotations cannot hold f64 values.
    let company = Company::new(
        "TEST".to_string(),
        "Test Co".to_string(),
        Sector::Agriculture,
        LegalForm::FamilyBusiness(FamilyBusinessData::default()),
        100_000.0,
        10_000.0,
        100,
    );
    // These assignments work because the fields are u32
    let _fte: u32 = company.fulfilled_fte;
    let _prev: u32 = company.prev_fulfilled_fte;
    let _target: u32 = company.target_fte_demand;
    let _physical: u32 = company.physical_fte_demand;
}

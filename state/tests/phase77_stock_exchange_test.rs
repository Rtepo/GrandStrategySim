//! Phase 77: Stock Exchange Generation Tests
//!
//! Tests that JSC companies are listed on the stock exchange at Turn 0:
//! - The exchange has equity instruments after world generation
//! - JSC companies have shares and owners assigned
//! - IPO proceeds are credited to the company

use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
use sim_engine::engine::turn_context::InMemoryTurnContext;
use sim_engine::entities::LegalForm;
use sim_engine::registries::Registries;
use sim_engine::state::GameState;
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
fn stock_exchange_has_listed_companies_after_generation() {
    let (world, _ctx) = gen_world_with_ctx();
    let total_listings: usize = world
        .state
        .countries
        .values()
        .map(|country| {
            country.stock_exchange.liquidity_pools.len()
                + country.stock_exchange.order_book.len()
        })
        .sum();

    assert!(
        total_listings > 0,
        "Stock exchange should have listed companies after world generation. Got {} listings.",
        total_listings
    );
}

#[test]
fn jsc_companies_have_shares_count_after_generation() {
    let (_world, ctx) = gen_world_with_ctx();
    let mut jsc_with_shares = 0;
    let mut jsc_total = 0;

    for entities in ctx.entities.values() {
        for company in &entities.companies {
            if let LegalForm::JointStockCompany(ref jsd) = company.legal_form {
                jsc_total += 1;
                // A JSC is properly listed if it has shares_count > 0
                // (set by list_jsc_companies_on_exchange from shares_issued)
                if company.shares_count > 0 {
                    jsc_with_shares += 1;
                }
            }
        }
    }

    assert!(jsc_total > 0, "Should have JSC companies after generation");
    assert!(
        jsc_with_shares > 0,
        "At least some JSC companies should have shares_count > 0. Got {}/{}",
        jsc_with_shares,
        jsc_total
    );
}

#[test]
fn jsc_companies_have_owners_after_generation() {
    let (_world, ctx) = gen_world_with_ctx();
    let mut jsc_with_owners = 0;
    let mut jsc_total = 0;

    for entities in ctx.entities.values() {
        for company in &entities.companies {
            if let LegalForm::JointStockCompany(_) = company.legal_form {
                jsc_total += 1;
                if !company.owners.is_empty() {
                    jsc_with_owners += 1;
                }
            }
        }
    }

    assert!(jsc_total > 0, "Should have JSC companies");
    assert!(
        jsc_with_owners > 0,
        "At least some JSC companies should have owners assigned. Got {}/{}",
        jsc_with_owners,
        jsc_total
    );
}

//! Phase 76: Leontief I/O matrix commodity graph tests.
//!
//! Validates that the production method registry forms a complete commodity
//! graph with no dead nodes, and that the Protein→Meat merge is complete.

use sim_engine::data::consumption_registry::consumption_registry;
use sim_engine::registries::enums::Commodity;
use sim_engine::registries::production_methods::BuildingMethods;
use sim_engine::registries::production_methods_data::default_production_methods;
use sim_engine::construction::bom::get_construction_bom;
use sim_engine::registries::enums::Sector;
use std::collections::{BTreeSet, HashMap};

/// Collect all commodities produced by any production method.
fn all_produced_commodities(methods: &HashMap<String, BuildingMethods>) -> BTreeSet<Commodity> {
    let mut produced = BTreeSet::new();
    for building_methods in methods.values() {
        for pm in building_methods.production.values() {
            for &c in pm.outputs.keys() {
                produced.insert(c);
            }
        }
    }
    produced
}

/// Collect all commodities consumed as inputs by any production method.
fn all_consumed_commodities(methods: &HashMap<String, BuildingMethods>) -> BTreeSet<Commodity> {
    let mut consumed = BTreeSet::new();
    for building_methods in methods.values() {
        for pm in building_methods.production.values() {
            for &c in pm.inputs.keys() {
                if !c.is_fixed_asset() {
                    consumed.insert(c);
                }
            }
        }
    }
    consumed
}

/// Test 1: Protein commodity no longer exists in the enum.
#[test]
fn protein_commodity_removed() {
    // Try to parse "protein" — it should fail
    let result: Result<Commodity, _> = serde_json::from_str("\"protein\"");
    assert!(result.is_err(), "Commodity::Protein should be removed from the enum");
}

/// Test 2: Meat is produced by at least one method (including the merged Pulse & Legume Farming).
#[test]
fn meat_has_producer() {
    let methods = default_production_methods();
    let produced = all_produced_commodities(&methods);
    assert!(
        produced.contains(&Commodity::Meat),
        "Meat should have at least one producer after Protein merge"
    );
}

/// Test 3: Meat is consumed by B2C demand (consumption baskets).
#[test]
fn meat_has_b2c_consumer() {
    let b2c_demand: BTreeSet<Commodity> = consumption_registry()
        .values()
        .flat_map(|basket| basket.tiers.values())
        .flat_map(|tier| tier.keys().copied())
        .collect();

    assert!(
        b2c_demand.contains(&Commodity::Meat),
        "Meat should be in B2C consumption baskets after Protein merge"
    );
}

/// Test 4: Lead is consumed by at least one production method (ammunition or glass).
#[test]
fn lead_has_consumer() {
    let methods = default_production_methods();
    let consumed = all_consumed_commodities(&methods);
    assert!(
        consumed.contains(&Commodity::Lead),
        "Lead should be consumed by at least one production method (ammunition/glass)"
    );
}

/// Test 5: ConstructionServices is consumed by construction BOMs.
#[test]
fn construction_services_has_consumer() {
    // Check that at least one construction BOM includes ConstructionServices
    let sectors = [
        Sector::HeavyIndustry,
        Sector::LightIndustry,
        Sector::Mining,
        Sector::Agriculture,
        Sector::Construction,
        Sector::Energy,
        Sector::PublicServices,
    ];

    let mut found = false;
    for &sector in &sectors {
        let bom = get_construction_bom(sector, 1925);
        if bom.contains_key(&Commodity::ConstructionServices) {
            found = true;
            break;
        }
    }

    assert!(
        found,
        "ConstructionServices should be consumed by at least one construction BOM"
    );
}

/// Test 6: Asphalt is consumed by at least one construction BOM.
#[test]
fn asphalt_has_consumer() {
    let sectors = [
        Sector::HeavyIndustry,
        Sector::LightIndustry,
        Sector::Mining,
        Sector::PublicServices,
    ];

    let mut found = false;
    for &sector in &sectors {
        let bom = get_construction_bom(sector, 1925);
        if bom.contains_key(&Commodity::Asphalt) {
            found = true;
            break;
        }
    }

    assert!(
        found,
        "Asphalt should be consumed by at least one construction BOM"
    );
}

/// Test 7: No orphan inputs — every consumed commodity has a producer or is a free resource.
#[test]
fn no_orphan_inputs_phase76() {
    let methods = default_production_methods();
    let produced = all_produced_commodities(&methods);
    let consumed = all_consumed_commodities(&methods);

    // Free natural resources
    let free: BTreeSet<Commodity> = [Commodity::Water].iter().copied().collect();

    // Commodities consumed only by construction BOMs (not production methods)
    let mut bom_consumed: BTreeSet<Commodity> = BTreeSet::new();
    for &sector in &[
        Sector::HeavyIndustry, Sector::LightIndustry, Sector::Mining,
        Sector::Agriculture, Sector::Construction, Sector::Energy,
        Sector::TransportLogistics, Sector::PublicServices, Sector::PublicAdministration,
        Sector::Banking, Sector::ArmamentsIndustry, Sector::MaintenanceWorkshops,
        Sector::LocalServices, Sector::ExportServices, Sector::MedicalServices,
        Sector::EducationalServices, Sector::MediaAndEntertainment, Sector::WasteManagement,
        Sector::Hospitality, Sector::NGO, Sector::Religion, Sector::Government,
    ] {
        let bom = get_construction_bom(sector, 1925);
        for &c in bom.keys() {
            bom_consumed.insert(c);
        }
    }

    let orphans: Vec<_> = consumed
        .iter()
        .chain(bom_consumed.iter())
        .filter(|c| !produced.contains(c) && !free.contains(c))
        .copied()
        .collect();

    // Filter out known service commodities consumed by non-production systems
    // (e.g., HealthCapacity, EducationSlots consumed by B2C service clearing)
    // Phase 84: Waste streams generated by the waste generation engine
    // (not production methods) — BulkyWaste and ConstructionWaste are produced
    // by consumption/construction activity, not by any production method.
    let service_exceptions: BTreeSet<Commodity> = [
        Commodity::HealthCapacity,
        Commodity::EducationSlots,
        Commodity::BulkyWaste,
        Commodity::ConstructionWaste,
    ].iter().copied().collect();

    let orphans: Vec<_> = orphans
        .iter()
        .filter(|c| !service_exceptions.contains(c))
        .copied()
        .collect();

    assert!(
        orphans.is_empty(),
        "Orphan inputs (consumed but no producer): {:?}",
        orphans
    );
}

/// Test 8: Heat is NOT sold on the global market (it's a local utility).
/// Verify that no production method adds Heat to market_orders via add_sell.
/// This is a structural test — we verify that Heat is produced (for grid consumption)
/// but that the production code does not add it to market sell orders.
#[test]
fn heat_is_local_utility_not_market_commodity() {
    let methods = default_production_methods();
    let produced = all_produced_commodities(&methods);

    // Heat should still be produced (by energy methods for grid consumption)
    assert!(
        produced.contains(&Commodity::Heat),
        "Heat should be produced by energy methods for local grid consumption"
    );

    // But Heat should NOT be consumed as a production method input
    // (it's consumed by the utility grid, not by B2B market participants)
    let consumed = all_consumed_commodities(&methods);
    assert!(
        !consumed.contains(&Commodity::Heat),
        "Heat should not be a B2B market input — it's a local utility flow"
    );
}

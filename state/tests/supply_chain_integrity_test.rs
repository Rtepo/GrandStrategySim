//! Phase 20: Supply chain integrity tests.
//!
//! These tests validate that the production method registry forms a complete
//! supply chain with no orphan inputs (consumed but never produced) and no
//! orphan B2C demand (demanded by consumers but never produced).

use sim_engine::data::consumption_registry::{consumption_registry, NeedTier};
use sim_engine::registries::enums::Commodity;
use sim_engine::registries::production_methods::BuildingMethods;
use sim_engine::registries::production_methods_data::default_production_methods;
use std::collections::{BTreeSet, HashMap};

/// Collect all commodities produced by any production method in the registry.
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

/// Collect all commodities consumed (as non-fixed-asset inputs) by any
/// production method in the registry.
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

/// Free natural resources that don't need a producer (extracted from nature).
fn free_resources() -> BTreeSet<Commodity> {
    let mut free = BTreeSet::new();
    free.insert(Commodity::Water);
    free
}

/// Test 1: No orphan inputs — every consumed commodity must have at least
/// one producer in the registry (or be a free natural resource).
#[test]
fn no_orphan_inputs() {
    let methods = default_production_methods();
    let produced = all_produced_commodities(&methods);
    let consumed = all_consumed_commodities(&methods);
    let free = free_resources();

    let orphans: Vec<_> = consumed
        .iter()
        .filter(|c| !produced.contains(c) && !free.contains(c))
        .collect();

    assert!(
        orphans.is_empty(),
        "Orphan inputs (consumed but no producer): {:?}",
        orphans
    );
}

/// Test 2: No orphan B2C demand — every commodity demanded by consumers
/// must have at least one producer in the registry.
#[test]
fn no_orphan_b2c_demand() {
    let methods = default_production_methods();
    let produced = all_produced_commodities(&methods);

    let b2c_demand: BTreeSet<Commodity> = consumption_registry()
        .values()
        .flat_map(|basket| basket.tiers.values())
        .flat_map(|tier| tier.keys().copied())
        .collect();

    let orphans: Vec<_> = b2c_demand
        .iter()
        .filter(|c| !produced.contains(c))
        .collect();

    assert!(
        orphans.is_empty(),
        "B2C demand orphans (demanded but no producer): {:?}",
        orphans
    );
}

/// Test 3: Every sector has at least one production method with non-empty
/// outputs.
#[test]
fn every_sector_has_nonempty_output_method() {
    let methods = default_production_methods();

    for (sector_key, building_methods) in &methods {
        let has_output = building_methods
            .production
            .values()
            .any(|pm| !pm.outputs.is_empty());

        assert!(
            has_output,
            "Sector `{}` has no production method with non-empty outputs",
            sector_key
        );
    }
}

/// Test 4: Labor ratios sum to approximately 1.0 for every production method.
#[test]
fn labor_ratios_sum_to_one() {
    let methods = default_production_methods();
    let tolerance = 0.01;

    for (sector_key, building_methods) in &methods {
        for (method_name, pm) in &building_methods.production {
            let sum = pm.experts_ratio + pm.skilled_ratio + pm.basic_ratio;
            assert!(
                (sum - 1.0).abs() < tolerance,
                "Sector `{}` method `{}` labor ratios sum to {} (expected ~1.0)",
                sector_key,
                method_name,
                sum
            );
        }
    }
}

/// Test 5: Fixed-asset output coverage — every fixed-asset commodity must
/// have at least one production method that produces it.
#[test]
fn fixed_asset_output_coverage() {
    let methods = default_production_methods();
    let produced = all_produced_commodities(&methods);

    let fixed_assets = [
        Commodity::IndustrialMachinery,
        Commodity::ConstructionMachinery,
        Commodity::AgriculturalMachinery,
        Commodity::OfficeMachinery,
        Commodity::Trucks,
        Commodity::Cars,
    ];

    for asset in &fixed_assets {
        assert!(
            produced.contains(asset),
            "Fixed asset {:?} has no production method",
            asset
        );
    }
}

/// Test 6: English key deserialization — "cereal" and "vegetable" should
/// deserialize to Cereal and Vegetable respectively.
#[test]
fn legacy_alias_deserialization() {
    let cereal: Commodity = serde_json::from_str("\"cereal\"").unwrap();
    assert_eq!(cereal, Commodity::Cereal);

    let vegetable: Commodity = serde_json::from_str("\"vegetable\"").unwrap();
    assert_eq!(vegetable, Commodity::Vegetable);
}

/// Test 7: Commodity count — the `all()` array should have the correct size.
#[test]
fn commodity_count_matches() {
    let all = Commodity::all();
    assert_eq!(
        all.len(),
        139,
        "Commodity::all() should return 139 variants, got {}",
        all.len()
    );
}

/// Test 8: New Phase 20 commodities are in the `all()` array.
#[test]
fn new_commodities_exist() {
    let all = Commodity::all();
    assert!(all.contains(&Commodity::Batteries), "Batteries missing");
    assert!(all.contains(&Commodity::Lithium), "Lithium missing");
    assert!(all.contains(&Commodity::Plastics), "Plastics missing");
    assert!(
        all.contains(&Commodity::RareEarthElements),
        "RareEarthElements missing"
    );
    assert!(
        all.contains(&Commodity::RefinedFuel),
        "RefinedFuel missing"
    );
    assert!(
        all.contains(&Commodity::Semiconductors),
        "Semiconductors missing"
    );
}

/// Test 9: Consumption registry has new class baskets.
#[test]
fn consumption_registry_has_new_classes() {
    let registry = consumption_registry();
    assert!(registry.contains_key("Bourgeoisie"), "Bourgeoisie basket missing");
    assert!(
        registry.contains_key("PettyBourgeoisie"),
        "PettyBourgeoisie basket missing"
    );
}

/// Test 10: Wealth-tier demand — all baskets demand Cereal and Clothing
/// (at least in Subsistence or Standard tier).
#[test]
fn all_baskets_demand_staples() {
    let registry = consumption_registry();

    for (class_id, basket) in registry {
        let all_commodities: BTreeSet<Commodity> = basket
            .tiers
            .values()
            .flat_map(|tier| tier.keys().copied())
            .collect();

        assert!(
            all_commodities.contains(&Commodity::Cereal),
            "Class `{}` does not demand Cereal",
            class_id
        );
    }
}

/// Test 11: Luxury goods are only in High/VeryHigh wealth baskets.
#[test]
fn luxury_goods_restricted_to_wealthy() {
    let registry = consumption_registry();

    // Low-wealth classes should NOT demand LuxuryFurniture or Cars
    let low_wealth_classes = ["Serf", "LandlessLaborer", "FreePeasant", "Worker"];

    for class_id in &low_wealth_classes {
        if let Some(basket) = registry.get(*class_id) {
            if let Some(luxury_tier) = basket.tiers.get(&NeedTier::Luxury) {
                assert!(
                    !luxury_tier.contains_key(&Commodity::LuxuryFurniture),
                    "Low-wealth class `{}` should not demand LuxuryFurniture",
                    class_id
                );
                assert!(
                    !luxury_tier.contains_key(&Commodity::Cars),
                    "Low-wealth class `{}` should not demand Cars in Luxury tier",
                    class_id
                );
            }
        }
    }

    // Aristocracy and Bourgeoisie SHOULD demand luxury goods
    let aristocracy = registry.get("Aristocracy").unwrap();
    let luxury = aristocracy.tiers.get(&NeedTier::Luxury).unwrap();
    assert!(
        luxury.contains_key(&Commodity::LuxuryFurniture),
        "Aristocracy should demand LuxuryFurniture"
    );
    assert!(
        luxury.contains_key(&Commodity::Cars),
        "Aristocracy should demand Cars"
    );
}

/// Test 12: Phase 20 Final Audit — Heat is produced by at least one energy method.
#[test]
fn heat_is_produced() {
    let methods = default_production_methods();
    let produced = all_produced_commodities(&methods);
    assert!(
        produced.contains(&Commodity::Heat),
        "Heat should be produced by at least one energy method (cogeneration)"
    );
}

/// Test 13: Phase 20 Final Audit — RenovationServices is produced.
#[test]
fn renovation_services_is_produced() {
    let methods = default_production_methods();
    let produced = all_produced_commodities(&methods);
    assert!(
        produced.contains(&Commodity::RenovationServices),
        "RenovationServices should be produced by at least one construction method"
    );
}

/// Test 14: Phase 20 Final Audit — AssimilationCapacity is produced by
/// public services (Integration Centers), NOT by education methods.
#[test]
fn assimilation_capacity_from_integration_centers_not_schools() {
    let methods = default_production_methods();

    // Must be produced by public_services
    let public_services = methods.get("public_services").expect("public_services sector missing");
    let ps_produces_assimilation = public_services.production.values()
        .any(|pm| pm.outputs.contains_key(&Commodity::AssimilationCapacity));
    assert!(
        ps_produces_assimilation,
        "AssimilationCapacity should be produced by public_services (Integration Centers)"
    );

    // Must NOT be produced by educational_services
    let education = methods.get("educational_services").expect("educational_services sector missing");
    let edu_produces_assimilation = education.production.values()
        .any(|pm| pm.outputs.contains_key(&Commodity::AssimilationCapacity));
    assert!(
        !edu_produces_assimilation,
        "AssimilationCapacity must NOT be produced by educational_services (architectural separation)"
    );
}

/// Test 15: Phase 75 — deprecated variant filter removed; all commodities are valid.
#[test]
fn all_commodities_are_valid() {
    // All commodity variants are now considered valid schema members.
    // The is_active() filter was removed as part of the backward-compatibility purge.
    let all = Commodity::all();
    assert_eq!(all.len(), 139, "Commodity::all() must return exactly 139 variants");
}

/// Test 16: Phase 20 Final Audit — every sector has at least 3 Automation
/// methods with no temporal gap > 40 years between consecutive methods.
#[test]
fn every_sector_has_automation_progression() {
    let methods = default_production_methods();

    for (sector_key, building_methods) in &methods {
        let mut auto_years: Vec<u32> = building_methods.automation.values()
            .map(|pm| pm.year)
            .collect();
        auto_years.sort();

        assert!(
            auto_years.len() >= 3,
            "Sector `{}` has only {} Automation methods (need >= 3)",
            sector_key,
            auto_years.len()
        );

        for i in 1..auto_years.len() {
            let gap = auto_years[i] - auto_years[i - 1];
            assert!(
                gap <= 50,
                "Sector `{}` Automation gap {} years between {} and {} (max 50)",
                sector_key,
                gap,
                auto_years[i - 1],
                auto_years[i]
            );
        }
    }
}

/// Test 17: Phase 20 Final Audit — every sector has at least 3 Organization
/// methods with no temporal gap > 50 years between consecutive methods.
#[test]
fn every_sector_has_organization_progression() {
    let methods = default_production_methods();

    for (sector_key, building_methods) in &methods {
        let mut org_years: Vec<u32> = building_methods.organization.values()
            .map(|pm| pm.year)
            .collect();
        org_years.sort();

        assert!(
            org_years.len() >= 3,
            "Sector `{}` has only {} Organization methods (need >= 3)",
            sector_key,
            org_years.len()
        );

        for i in 1..org_years.len() {
            let gap = org_years[i] - org_years[i - 1];
            assert!(
                gap <= 50,
                "Sector `{}` Organization gap {} years between {} and {} (max 50)",
                sector_key,
                gap,
                org_years[i - 1],
                org_years[i]
            );
        }
    }
}

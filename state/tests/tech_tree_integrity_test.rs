//! Tech tree integrity tests.
//!
//! These tests validate the cross-references between the hardcoded tech tree
//! (`tech_tree_data.rs`) and the hardcoded production methods
//! (`production_methods_data.rs`).
//!
//! # Tests
//! 1. `every_unlocked_method_exists_in_correct_slot` — Verifies that for every
//!    `TechNode.unlocks_methods` entry, the referenced `ProductionMethod` exists
//!    in the correct slot of the corresponding sector's `BuildingMethods`.
//! 2. `every_required_tech_id_exists` — Verifies that for every
//!    `ProductionMethod` with `required_tech = Some(tech_id)`, a `TechNode` with
//!    that `TechId` exists in the tech tree.
//! 3. `every_prerequisite_tech_id_exists` — Verifies that every `TechNode`'s
//!    `prerequisites` reference existing `TechId`s.

use sim_engine::registries::production_methods::{BuildingMethods, MethodSlot};
use sim_engine::registries::production_methods_data::default_production_methods;
use sim_engine::registries::tech_tree_data::default_tech_tree;
use std::collections::HashMap;

/// Test 1: Every `unlocks_methods` reference in the tech tree must point to a
/// `ProductionMethod` that exists in the correct slot of the corresponding
/// sector's `BuildingMethods`.
#[test]
fn every_unlocked_method_exists_in_correct_slot() {
    let tech_tree = default_tech_tree();
    let production_methods: HashMap<String, BuildingMethods> = default_production_methods();

    let mut errors: Vec<String> = Vec::new();

    for (tech_id, tech_node) in &tech_tree {
        for (sector_key, slot_map) in &tech_node.unlocks_methods {
            // Check that the sector exists in production_methods
            let methods = match production_methods.get(sector_key) {
                Some(m) => m,
                None => {
                    errors.push(format!(
                        "Tech `{tech_id}` unlocks methods for sector `{sector_key}` \
                         but no BuildingMethods found for that sector"
                    ));
                    continue;
                }
            };

            for (slot_key, method_name) in slot_map {
                // Parse the slot key
                let slot = match MethodSlot::from_key(slot_key) {
                    Some(s) => s,
                    None => {
                        errors.push(format!(
                            "Tech `{tech_id}` sector `{sector_key}` has invalid slot key `{slot_key}`"
                        ));
                        continue;
                    }
                };

                // Check that the method exists in the correct slot
                if methods.get(slot, method_name).is_none() {
                    errors.push(format!(
                        "Tech `{tech_id}` unlocks method `{method_name}` in sector `{sector_key}` \
                         slot `{slot_key}` but no such method exists"
                    ));
                }
            }
        }
    }

    assert!(
        errors.is_empty(),
        "Integrity violations (unlocks_methods → production methods):\n{}",
        errors.join("\n")
    );
}

/// Test 2: Every `ProductionMethod` with `required_tech = Some(tech_id)` must
/// reference a `TechId` that exists in the tech tree.
#[test]
fn every_required_tech_id_exists() {
    let tech_tree = default_tech_tree();
    let production_methods = default_production_methods();

    let mut errors: Vec<String> = Vec::new();

    for (sector_key, methods) in &production_methods {
        for pm in methods.iter_all() {
            if let Some(ref tech_id) = pm.required_tech {
                if !tech_tree.contains_key(tech_id) {
                    errors.push(format!(
                        "Production method in sector `{sector_key}` (year {}) \
                         requires tech `{tech_id}` but no TechNode with that id exists",
                        pm.year
                    ));
                }
            }
        }
    }

    assert!(
        errors.is_empty(),
        "Integrity violations (required_tech → tech_tree):\n{}",
        errors.join("\n")
    );
}

/// Test 3: Every `TechNode`'s `prerequisites` must reference existing `TechId`s.
#[test]
fn every_prerequisite_tech_id_exists() {
    let tech_tree = default_tech_tree();

    let mut errors: Vec<String> = Vec::new();

    for (tech_id, tech_node) in &tech_tree {
        for prereq in &tech_node.prerequisites {
            if !tech_tree.contains_key(prereq) {
                errors.push(format!(
                    "Tech `{tech_id}` has prerequisite `{prereq}` \
                     but no TechNode with that id exists"
                ));
            }
        }
    }

    assert!(
        errors.is_empty(),
        "Integrity violations (prerequisites → tech_tree):\n{}",
        errors.join("\n")
    );
}

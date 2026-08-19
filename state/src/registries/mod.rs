//! Immutable, load-once game data ("registries").
//!
//! Registries hold static definitions that never change during a simulation:
//! the technology tree, production methods, building templates, and government
//! forms. Per the approved architecture (open question #2), the bundle is
//! shared via [`std::sync::Arc`] so that many read-only consumers — and, later,
//! parallel per-country workers — can access it without cloning, while leaving
//! the door open to hot-reloading static JSON in development.

pub mod buildings;
pub mod blueprint_specs;
pub mod crops;
pub mod enums;
pub mod government;
pub mod production_methods;
pub mod production_methods_data;
pub mod tech_tree;
pub mod tech_tree_data;

use std::collections::HashMap;
use std::sync::Arc;

use buildings::{state_apparatus_templates, BuildingKind, BuildingTemplate};
use crops::CropRegistry;
use government::{government_forms, GovernmentForm};
use production_methods::{industrial_production_methods, state_building_methods, BuildingMethods};
use production_methods_data::default_production_methods;
use tech_tree::{load_tech_tree, TechId, TechNode};

/// Phase 24A.5: Add English snake_case sector aliases to the production methods
/// registry. This bridges the duplicate Polish-keyed registry by adding canonical
/// English keys alongside the existing Polish display names.
///
/// # Rules
/// * For each Polish-keyed entry that corresponds to a known sector, insert an
///   alias under the English snake_case sector key.
/// * If the English key already exists (from `default_production_methods`), it
///   is NOT overwritten — the dedicated sector methods take precedence.
fn add_sector_aliases(registry: &mut HashMap<String, BuildingMethods>) {
    // Map Polish building names to English snake_case sector keys
    let aliases: &[(&str, &str)] = &[
        ("Baza Wojskowa", "military_base"),
        ("Komisariat", "police_station"),
        ("Sąd", "courthouse"),
        ("Siedziba Służb", "intelligence_hq"),
        ("Więzienie", "prison"),
        ("Straż Pożarna", "fire_station"),
        ("Schron Przeciwpowodziowy", "flood_shelter"),
        ("Straż Graniczna", "border_guard"),
        ("Urząd Celny", "customs_office"),
        ("Sanepid", "sanepid"),
        ("Inspektorat Nadzoru Budowlanego", "construction_inspectorate"),
        ("Inspektorat Ochrony Środowiska", "environmental_inspectorate"),
        ("Zakład Solvaya", "soda_ash_plant"),
        ("Młyn Nasienny", "seed_mill"),
        ("StateForest", "forest_district"),
        ("Targ", "marketplace"),
        ("Hurtownia", "wholesale"),
        ("Sklep Detaliczny", "retail_shop"),
        ("Supermarket", "supermarket"),
        ("Dom Towarowy", "department_store"),
        ("Centrum Handlowe", "shopping_mall"),
        ("Uniwersytet", "university"),
        ("Politechnika", "technical_university"),
        ("Przychodnia", "clinic"),
        ("Szpital", "hospital"),
        ("Szpital Badawczy", "research_hospital"),
        ("Szkoła Podstawowa", "primary_school"),
        ("Liceum", "high_school"),
        ("Remiza OSP", "volunteer_fire_station"),
    ];

    for (polish_key, english_key) in aliases {
        if let Some(methods) = registry.get(*polish_key).cloned() {
            registry.entry(english_key.to_string()).or_insert(methods);
        }
    }
}

/// The complete bundle of immutable game registries.
///
/// # Rules
/// * Construct once at startup, then share as `Arc<Registries>`.
/// * Never mutated during a turn; safe to read concurrently.
#[derive(Debug, Clone, PartialEq)]
pub struct Registries {
    /// The full technology tree, keyed by [`TechId`].
    pub tech_tree: HashMap<TechId, TechNode>,

    /// Production methods keyed by sector, grouped by slot (automation, production, organization).
    pub production_methods: HashMap<String, BuildingMethods>,

    /// Building templates keyed by [`BuildingKind`].
    pub building_templates: HashMap<BuildingKind, BuildingTemplate>,

    /// Government forms keyed by form name.
    pub government_forms: HashMap<String, GovernmentForm>,

    /// Phase 6.3: Crop registry for agricultural simulation
    pub crops: CropRegistry,
}

impl Registries {
    /// Builds the registry bundle from the natively-encoded Target 0 data,
    /// loading the technology tree from the supplied JSON.
    ///
    /// # Arguments
    /// * `tech_tree_json` - Raw JSON of the technology tree, e.g. produced from
    ///   `society/science/tech_registry.py`.
    ///
    /// # Returns
    /// `Ok(Arc<Registries>)` on success, or a [`serde_json::Error`] if the
    /// tech-tree JSON is malformed.
    ///
    /// # Rules
    /// * Production methods and state-apparatus building templates are taken
    ///   from the native Target 0 encoders.
    /// * Additional (bulk) building templates can be merged later via
    ///   [`buildings::load_building_registry`] as Target 3 is ported.
    pub fn from_tech_tree_json(tech_tree_json: &str) -> Result<Arc<Self>, serde_json::Error> {
        let tech_tree = load_tech_tree(tech_tree_json)?;
        let mut production_methods = state_building_methods();
        production_methods.extend(industrial_production_methods());
        add_sector_aliases(&mut production_methods);
        Ok(Arc::new(Self {
            tech_tree,
            production_methods,
            building_templates: state_apparatus_templates(),
            government_forms: government_forms(),
            crops: CropRegistry::default(),
        }))
    }

    /// Builds the registry bundle with an empty technology tree.
    ///
    /// # Returns
    /// An `Arc<Registries>` containing only the natively-encoded production
    /// methods, state-apparatus templates, and government forms.
    ///
    /// # Rules
    /// * Useful for unit tests and early Target 0 wiring where the full tech
    ///   tree JSON is not yet loaded.
    pub fn native_only() -> Arc<Self> {
        let mut production_methods = state_building_methods();
        production_methods.extend(industrial_production_methods());
        production_methods.extend(default_production_methods());
        add_sector_aliases(&mut production_methods);
        Arc::new(Self {
            tech_tree: tech_tree_data::default_tech_tree(),
            production_methods,
            building_templates: state_apparatus_templates(),
            government_forms: government_forms(),
            crops: CropRegistry::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_bundle_is_populated() {
        let reg = Registries::native_only();
        assert!(!reg.tech_tree.is_empty());
        assert_eq!(reg.government_forms.len(), 11);
        assert!(reg.production_methods.contains_key("Baza Wojskowa"));
        assert!(reg.building_templates.contains_key("Sąd"));
    }

    #[test]
    fn builds_from_tech_tree_json() {
        let json = r#"{
            "tech_001": {
                "nazwa": "Spawanie elektryczne",
                "rok": 1881,
                "koszt": 100,
                "opis": "Test.",
                "prerequisites": []
            }
        }"#;
        let reg = Registries::from_tech_tree_json(json).unwrap();
        assert_eq!(reg.tech_tree.len(), 1);
        assert_eq!(reg.tech_tree["tech_001"].year, 1881);
    }

    #[test]
    fn arc_is_shareable() {
        let reg = Registries::native_only();
        let clone = Arc::clone(&reg);
        assert_eq!(reg, clone);
        assert_eq!(Arc::strong_count(&reg), 2);
    }
}

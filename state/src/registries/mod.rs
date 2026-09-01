//! Immutable, load-once game data ("registries").
//!
//! Registries hold static definitions that never change during a simulation:
//! the technology tree, production methods, building templates, and government
//! forms. Per the approved architecture (open question #2), the bundle is
//! shared via [`std::sync::Arc`] so that many read-only consumers — and, later,
//! parallel per-country workers — can access it without cloning, while leaving
//! the door open to hot-reloading static JSON in development.

pub mod blueprint_specs;
pub mod buildings;
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
    /// Builds the registry bundle from the natively-encoded Stage 0 data,
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
    ///   from the native Stage 0 encoders.
    /// * Additional (bulk) building templates can be merged later via
    ///   [`buildings::load_building_registry`] as Stage 3 is ported.
    pub fn from_tech_tree_json(tech_tree_json: &str) -> Result<Arc<Self>, serde_json::Error> {
        let tech_tree = load_tech_tree(tech_tree_json)?;
        let mut production_methods = state_building_methods();
        production_methods.extend(industrial_production_methods());
        Ok(Arc::new(Self {
            tech_tree,
            production_methods,
            building_templates: state_apparatus_templates(),
            government_forms: government_forms(),
            // Stabilization Sprint: Load the static crop registry (was empty default).
            crops: CropRegistry {
                crops: crate::data::crop_registry::crop_registry().clone(),
            },
        }))
    }

    /// Builds the registry bundle with an empty technology tree.
    ///
    /// # Returns
    /// An `Arc<Registries>` containing only the natively-encoded production
    /// methods, state-apparatus templates, and government forms.
    ///
    /// # Rules
    /// * Useful for unit tests and early Stage 0 wiring where the full tech
    ///   tree JSON is not yet loaded.
    pub fn native_only() -> Arc<Self> {
        let mut production_methods = state_building_methods();
        production_methods.extend(industrial_production_methods());
        production_methods.extend(default_production_methods());
        Arc::new(Self {
            tech_tree: tech_tree_data::default_tech_tree(),
            production_methods,
            building_templates: state_apparatus_templates(),
            government_forms: government_forms(),
            // Stabilization Sprint: Load the static crop registry (was empty default).
            crops: CropRegistry {
                crops: crate::data::crop_registry::crop_registry().clone(),
            },
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
        assert!(reg.production_methods.contains_key("military_base"));
        assert!(reg.building_templates.contains_key("courthouse"));
    }

    #[test]
    fn builds_from_tech_tree_json() {
        let json = r#"{
            "tech_001": {
                "name": "Electric Welding",
                "year": 1881,
                "cost": 100,
                "description": "Test.",
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

//! The technology tree registry.
//!
//! Mirrors the Python `TECH_TREE` constant in
//! `society/science/tech_registry.py` (96 technologies). Because the tree is
//! large and already lives in a single canonical Python/JSON structure, this
//! module defines the typed [`TechNode`] and loads the full set from JSON via
//! [`load_tech_tree`], preserving Polish keys through `#[serde(rename)]`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Stable identifier of a technology, e.g. `"tech_001"`.
pub type TechId = String;

/// Type of technology: Fundamental (state research) or Commercial (corporate research).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TechType {
    /// Fundamental concepts researched by the State using Innovation Points.
    #[default]
    Fundamental,
    /// Commercial production methods researched by Companies using fiat cash.
    Commercial,
}

/// A single node in the technology tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TechNode {
    /// Display name (`"nazwa"`), e.g. `"Spawanie elektryczne"`.
    #[serde(rename = "nazwa")]
    pub name: String,

    /// Historical year the technology becomes available (`"rok"`).
    #[serde(rename = "rok")]
    pub year: u32,

    /// Research cost in innovation points (`"koszt"`).
    #[serde(rename = "koszt")]
    pub cost: u32,

    /// Human-readable description (`"opis"`).
    #[serde(rename = "opis")]
    pub description: String,

    /// Production methods unlocked, keyed by sector then method-slot
    /// (`"odblokowuje_metody"`). Absent in the JSON when empty.
    #[serde(rename = "odblokowuje_metody", default)]
    pub unlocks_methods: HashMap<String, HashMap<String, String>>,

    /// State projects unlocked by this technology (`"odblokowuje_projekty"`).
    /// Absent in the JSON when empty.
    #[serde(rename = "odblokowuje_projekty", default)]
    pub unlocks_projects: Vec<String>,

    /// Prerequisite technology IDs (`"prerequisites"`).
    #[serde(rename = "prerequisites", default)]
    pub prerequisites: Vec<TechId>,

    /// Type of technology: Fundamental or Commercial.
    #[serde(rename = "typ_tech", default)]
    pub tech_type: TechType,

    /// Patent duration in turns (default 240 for 20 years at 1 turn/month).
    #[serde(rename = "czas_patentu", default = "default_patent_duration")]
    pub patent_duration_turns: u32,

    /// VWAP ratio for royalty calculation (e.g., 0.05 for 5% of output commodity VWAP).
    #[serde(rename = "stawka_royalty_vwap", default = "default_royalty_ratio")]
    pub royalty_vwap_ratio: f64,
}

fn default_patent_duration() -> u32 {
    240
}

fn default_royalty_ratio() -> f64 {
    0.05
}

/// Deserializes the full technology tree from its JSON representation.
///
/// # Arguments
/// * `json` - The raw JSON text of the tech tree (a map of `TechId -> node`),
///   e.g. the contents produced from `society/science/tech_registry.py`.
///
/// # Returns
/// `Ok(HashMap<TechId, TechNode>)` on success, or a [`serde_json::Error`] if
/// the JSON does not match the expected schema.
///
/// # Rules
/// * Polish field names are preserved verbatim via `#[serde(rename)]`.
/// * Missing optional collections (`odblokowuje_metody`,
///   `odblokowuje_projekty`) default to empty rather than erroring.
pub fn load_tech_tree(json: &str) -> Result<HashMap<TechId, TechNode>, serde_json::Error> {
    serde_json::from_str(json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_node() {
        let json = r#"{
            "tech_001": {
                "nazwa": "Spawanie elektryczne",
                "rok": 1881,
                "koszt": 100,
                "opis": "Fundamentalne laczenie metali.",
                "odblokowuje_metody": {
                    "przemysl_ciezki": { "automatyzacja": "Fabryki Zelektryfikowane" }
                },
                "prerequisites": []
            }
        }"#;
        let tree = load_tech_tree(json).unwrap();
        let node = &tree["tech_001"];
        assert_eq!(node.name, "Spawanie elektryczne");
        assert_eq!(node.year, 1881);
        assert_eq!(node.cost, 100);
        assert!(node.prerequisites.is_empty());
        assert_eq!(
            node.unlocks_methods["przemysl_ciezki"]["automatyzacja"],
            "Fabryki Zelektryfikowane"
        );
    }

    #[test]
    fn defaults_missing_optionals() {
        let json = r#"{
            "tech_094": {
                "nazwa": "Samoobslugowy terminal platniczy",
                "rok": 1995,
                "koszt": 6700,
                "opis": "Poczatki likwidacji zawodu kasjera.",
                "prerequisites": []
            }
        }"#;
        let tree = load_tech_tree(json).unwrap();
        let node = &tree["tech_094"];
        assert!(node.unlocks_methods.is_empty());
        assert!(node.unlocks_projects.is_empty());
    }
}

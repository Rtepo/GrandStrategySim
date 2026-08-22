//! Culture and religion registry with English engine keys.
//!
//! Provides static definitions for cultures and religions used in the game.
//! All engine-internal keys are English. English display names are stored
//! alongside for serde bridging and UI display.
//!
//! # Rules
//! * Engine logic (cultural_distance, assimilation, conversion) uses `key` fields, never `display_name`.
//! * `from_display_name()` bridges from Polish save-file strings to engine keys.
//! * `from_key()` is used for all internal logic lookups.

use crate::registries::enums::Commodity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Static definition of a culture.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CultureDefinition {
    /// Engine key (e.g., "Illyrian", "wenetian").
    pub key: String,
    /// English display name (e.g., "Illyria", "Venedia").
    pub display_name: String,
    /// Engine key for cultural group (e.g., "slavic", "germanic").
    pub cultural_group: String,
    /// English display name for cultural group (e.g., "Slavic", "Germanic").
    #[serde(default)]
    pub cultural_group_display: String,
    /// English demonym for this culture's people (e.g., "Lechians", "Bactrians").
    #[serde(default)]
    pub demonym: String,
    /// Engine key for language (e.g., "illyrian", "wenetian").
    pub language: String,
    /// Engine key for language family (e.g., "slavic", "germanic").
    pub language_family: String,
    /// Commodities this culture refuses to consume (taboos).
    #[serde(default)]
    pub taboos: Vec<Commodity>,
    /// (commodity, demand_multiplier) pairs for obsessions.
    #[serde(default)]
    pub obsessions: Vec<(Commodity, f64)>,
}

/// Static definition of a religion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ReligionDefinition {
    /// Engine key (e.g., "catholicism", "islam", "protestantism").
    pub key: String,
    /// English display name (e.g., "Catholicism", "Islam").
    pub display_name: String,
    /// Engine key for religious group (e.g., "christianity", "islamic").
    pub religious_group: String,
    /// Commodities forbidden by this religion (taboos).
    #[serde(default)]
    pub taboos: Vec<Commodity>,
    /// (commodity, demand_multiplier) pairs for religious obsessions.
    #[serde(default)]
    pub obsessions: Vec<(Commodity, f64)>,
    /// True for centralized religions with an Apostolic See (e.g., Catholic Church).
    #[serde(default)]
    pub is_centralized: bool,
    /// Engine key of the country hosting the See, if centralized.
    #[serde(default)]
    pub apostolic_see_country: Option<String>,
    /// True if this religion requires state funding (no separation).
    #[serde(default)]
    pub requires_state_funding: bool,
}

/// Registry holding all culture and religion definitions.
pub struct CultureRegistry {
    cultures: HashMap<String, CultureDefinition>,
    religions: HashMap<String, ReligionDefinition>,
    culture_by_display: HashMap<String, String>,
    religion_by_display: HashMap<String, String>,
}

impl CultureRegistry {
    /// Look up a culture by engine key.
    ///
    /// # Returns
    /// `Some(&CultureDefinition)` if found, `None` otherwise.
    pub fn from_key(&self, key: &str) -> Option<&CultureDefinition> {
        self.cultures.get(key)
    }

    /// Look up a religion by engine key.
    ///
    /// # Returns
    /// `Some(&ReligionDefinition)` if found, `None` otherwise.
    pub fn religion_from_key(&self, key: &str) -> Option<&ReligionDefinition> {
        self.religions.get(key)
    }

    /// Look up a culture by English display name (for save-file bridging).
    ///
    /// # Returns
    /// `Some(&CultureDefinition)` if found, `None` otherwise.
    pub fn from_display_name(&self, display_name: &str) -> Option<&CultureDefinition> {
        let key = self.culture_by_display.get(display_name)?;
        self.cultures.get(key)
    }

    /// Look up a religion by English display name (for save-file bridging).
    ///
    /// # Returns
    /// `Some(&ReligionDefinition)` if found, `None` otherwise.
    pub fn religion_from_display_name(&self, display_name: &str) -> Option<&ReligionDefinition> {
        let key = self.religion_by_display.get(display_name)?;
        self.religions.get(key)
    }

    /// Get the engine key for a culture from its English display name.
    ///
    /// # Returns
    /// Engine key string, or the original input if no mapping exists.
    pub fn culture_key_from_display(&self, display_name: &str) -> String {
        self.culture_by_display
            .get(display_name)
            .cloned()
            .unwrap_or_else(|| display_name.to_lowercase().replace(' ', "_"))
    }

    /// Get the engine key for a religion from its English display name.
    ///
    /// # Returns
    /// Engine key string, or the original input if no mapping exists.
    pub fn religion_key_from_display(&self, display_name: &str) -> String {
        self.religion_by_display
            .get(display_name)
            .cloned()
            .unwrap_or_else(|| display_name.to_lowercase().replace(' ', "_"))
    }

    /// Get all registered culture keys.
    pub fn all_culture_keys(&self) -> Vec<String> {
        self.cultures.keys().cloned().collect()
    }

    /// Get all registered religion keys.
    pub fn all_religion_keys(&self) -> Vec<String> {
        self.religions.keys().cloned().collect()
    }
}

/// Compute cultural distance between two cultures.
///
/// Distance ∈ [0.0, 1.0]: 0.0 = identical, 1.0 = maximally distant.
/// All comparisons use engine keys, never English display strings.
///
/// # Rules
/// * Language family difference: +0.4
/// * Same family, different language: +0.2
/// * Different cultural group: +0.3
/// * Taboo overlap penalty: 0.0–0.3 (less overlap = more distant)
pub fn cultural_distance(a: &CultureDefinition, b: &CultureDefinition) -> f64 {
    let mut distance = 0.0;

    if a.language_family != b.language_family {
        distance += 0.4;
    } else if a.language != b.language {
        distance += 0.2;
    }

    if a.cultural_group != b.cultural_group {
        distance += 0.3;
    }

    let taboo_overlap = a.taboos.iter().filter(|t| b.taboos.contains(t)).count();
    let total_taboos = a.taboos.len() + b.taboos.len();
    let taboo_penalty = if total_taboos == 0 {
        0.0
    } else {
        0.3 * (1.0 - taboo_overlap as f64 / total_taboos as f64)
    };
    distance += taboo_penalty;

    distance.min(1.0)
}

static REGISTRY: OnceLock<CultureRegistry> = OnceLock::new();

/// Get the global culture registry instance.
pub fn registry() -> &'static CultureRegistry {
    REGISTRY.get_or_init(build_registry)
}

fn build_registry() -> CultureRegistry {
    let cultures = build_cultures();
    let religions = build_religions();

    let mut culture_by_display = HashMap::new();
    for (key, def) in &cultures {
        culture_by_display.insert(def.display_name.clone(), key.clone());
    }

    let mut religion_by_display = HashMap::new();
    for (key, def) in &religions {
        religion_by_display.insert(def.display_name.clone(), key.clone());
    }

    CultureRegistry {
        cultures,
        religions,
        culture_by_display,
        religion_by_display,
    }
}

fn build_cultures() -> HashMap<String, CultureDefinition> {
    let defs = vec![
        // Slavic group
        CultureDefinition {
            key: "lechia".into(),
            display_name: "Lechia".into(),
            cultural_group: "slavic".into(),
            cultural_group_display: "Slavic".into(),
            language: "lechian".into(),
            language_family: "slavic".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        CultureDefinition {
            key: "sarmatia".into(),
            display_name: "Sarmatia".into(),
            cultural_group: "slavic".into(),
            cultural_group_display: "Slavic".into(),
            language: "sarmatian".into(),
            language_family: "slavic".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        CultureDefinition {
            key: "wenetian".into(),
            display_name: "Venedia".into(),
            cultural_group: "slavic".into(),
            cultural_group_display: "Slavic".into(),
            language: "wenetian".into(),
            language_family: "slavic".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        CultureDefinition {
            key: "krasnovian".into(),
            display_name: "Krasnovia".into(),
            cultural_group: "slavic".into(),
            cultural_group_display: "Slavic".into(),
            language: "krasnovian".into(),
            language_family: "slavic".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        // Germanic group
        CultureDefinition {
            key: "nordian".into(),
            display_name: "Nordia".into(),
            cultural_group: "germanic".into(),
            cultural_group_display: "Germanic".into(),
            language: "nordian".into(),
            language_family: "germanic".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        CultureDefinition {
            key: "anglian".into(),
            display_name: "Anglia".into(),
            cultural_group: "germanic".into(),
            cultural_group_display: "Germanic".into(),
            language: "anglian".into(),
            language_family: "germanic".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        CultureDefinition {
            key: "helvetian".into(),
            display_name: "Helvetia".into(),
            cultural_group: "germanic".into(),
            cultural_group_display: "Germanic".into(),
            language: "helvetian".into(),
            language_family: "germanic".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        // Latin group
        CultureDefinition {
            key: "gallian".into(),
            display_name: "Gallia".into(),
            cultural_group: "latin".into(),
            cultural_group_display: "Latin".into(),
            language: "gallian".into(),
            language_family: "romance".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        CultureDefinition {
            key: "iberian".into(),
            display_name: "Iberia".into(),
            cultural_group: "latin".into(),
            cultural_group_display: "Latin".into(),
            language: "iberian".into(),
            language_family: "romance".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        CultureDefinition {
            key: "occitan".into(),
            display_name: "Occitania".into(),
            cultural_group: "latin".into(),
            cultural_group_display: "Latin".into(),
            language: "occitan".into(),
            language_family: "romance".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        CultureDefinition {
            key: "dacian".into(),
            display_name: "Dacia".into(),
            cultural_group: "latin".into(),
            cultural_group_display: "Latin".into(),
            language: "dacian".into(),
            language_family: "romance".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        // Middle Eastern group
        CultureDefinition {
            key: "persian".into(),
            display_name: "Persia".into(),
            cultural_group: "middle_eastern".into(),
            cultural_group_display: "Middle Eastern".into(),
            language: "persian".into(),
            language_family: "indo_iranian".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        CultureDefinition {
            key: "bactrian".into(),
            display_name: "Bactria".into(),
            cultural_group: "middle_eastern".into(),
            cultural_group_display: "Middle Eastern".into(),
            language: "bactrian".into(),
            language_family: "indo_iranian".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        CultureDefinition {
            key: "anatolian".into(),
            display_name: "Anatolia".into(),
            cultural_group: "middle_eastern".into(),
            cultural_group_display: "Middle Eastern".into(),
            language: "anatolian".into(),
            language_family: "turkic".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        CultureDefinition {
            key: "eldorian".into(),
            display_name: "Eldoria".into(),
            cultural_group: "middle_eastern".into(),
            cultural_group_display: "Middle Eastern".into(),
            language: "eldorian".into(),
            language_family: "semitic".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        // Balkan group
        CultureDefinition {
            key: "Illyrian".into(),
            display_name: "Illyria".into(),
            cultural_group: "balkan".into(),
            cultural_group_display: "Balkan".into(),
            language: "Illyrian".into(),
            language_family: "slavic".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        CultureDefinition {
            key: "thracian".into(),
            display_name: "Thracia".into(),
            cultural_group: "balkan".into(),
            cultural_group_display: "Balkan".into(),
            language: "thracian".into(),
            language_family: "slavic".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        CultureDefinition {
            key: "pannonian".into(),
            display_name: "Pannonia".into(),
            cultural_group: "balkan".into(),
            cultural_group_display: "Balkan".into(),
            language: "pannonian".into(),
            language_family: "slavic".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
        CultureDefinition {
            key: "dardanian".into(),
            display_name: "Dardania".into(),
            cultural_group: "balkan".into(),
            cultural_group_display: "Balkan".into(),
            language: "dardanian".into(),
            language_family: "slavic".into(),
            demonym: String::new(),
            taboos: vec![],
            obsessions: vec![],
        },
    ];

    defs.into_iter().map(|d| (d.key.clone(), d)).collect()
}

fn build_religions() -> HashMap<String, ReligionDefinition> {
    let defs = vec![
        ReligionDefinition {
            key: "catholicism".into(),
            display_name: "Catholicism".into(),
            religious_group: "christianity".into(),
            taboos: vec![],
            obsessions: vec![(Commodity::Luxury, 1.2), (Commodity::Paper, 1.1)],
            is_centralized: true,
            apostolic_see_country: Some("watykan".into()),
            requires_state_funding: false,
        },
        ReligionDefinition {
            key: "islam".into(),
            display_name: "Islam".into(),
            religious_group: "islamic".into(),
            taboos: vec![],
            obsessions: vec![(Commodity::Luxury, 1.15)],
            is_centralized: false,
            apostolic_see_country: None,
            requires_state_funding: false,
        },
        ReligionDefinition {
            key: "orthodoxy".into(),
            display_name: "Orthodoxy".into(),
            religious_group: "christianity".into(),
            taboos: vec![],
            obsessions: vec![(Commodity::Luxury, 1.1), (Commodity::Paper, 1.1)],
            is_centralized: false,
            apostolic_see_country: None,
            requires_state_funding: false,
        },
        ReligionDefinition {
            key: "protestantism".into(),
            display_name: "Protestantism".into(),
            religious_group: "christianity".into(),
            taboos: vec![Commodity::Luxury],
            obsessions: vec![(Commodity::Paper, 1.2)],
            is_centralized: false,
            apostolic_see_country: None,
            requires_state_funding: false,
        },
        ReligionDefinition {
            key: "undeclared".into(),
            display_name: "Atheism / Agnosticism".into(),
            religious_group: "secular".into(),
            taboos: vec![],
            obsessions: vec![],
            is_centralized: false,
            apostolic_see_country: None,
            requires_state_funding: false,
        },
        ReligionDefinition {
            key: "folk_beliefs".into(),
            display_name: "Folk Beliefs".into(),
            religious_group: "animist".into(),
            taboos: vec![],
            obsessions: vec![(Commodity::Luxury, 1.3)],
            is_centralized: false,
            apostolic_see_country: None,
            requires_state_funding: false,
        },
        ReligionDefinition {
            key: "shamanism".into(),
            display_name: "Shamanism".into(),
            religious_group: "animist".into(),
            taboos: vec![],
            obsessions: vec![(Commodity::Luxury, 1.25)],
            is_centralized: false,
            apostolic_see_country: None,
            requires_state_funding: false,
        },
        ReligionDefinition {
            key: "pagan_cults".into(),
            display_name: "Pagan Cults".into(),
            religious_group: "pagan".into(),
            taboos: vec![],
            obsessions: vec![(Commodity::Luxury, 1.15)],
            is_centralized: false,
            apostolic_see_country: None,
            requires_state_funding: false,
        },
    ];

    defs.into_iter().map(|d| (d.key.clone(), d)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_lookups() {
        let reg = registry();
        assert!(reg.from_key("Illyrian").is_some());
        assert!(reg.from_key("nonexistent").is_none());
        assert!(reg.religion_from_key("catholicism").is_some());
        assert!(reg.religion_from_key("nonexistent").is_none());
    }

    #[test]
    fn test_display_name_bridging() {
        let reg = registry();
        let culture = reg.from_display_name("Illyria");
        assert!(culture.is_some());
        assert_eq!(culture.unwrap().key, "Illyrian");

        let religion = reg.religion_from_display_name("Catholicism");
        assert!(religion.is_some());
        assert_eq!(religion.unwrap().key, "catholicism");
    }

    #[test]
    fn test_cultural_distance_identical() {
        let reg = registry();
        let a = reg.from_key("Illyrian").unwrap();
        let distance = cultural_distance(a, a);
        assert!((distance - 0.0).abs() < 0.01, "identical cultures should have ~0 distance, got {}", distance);
    }

    #[test]
    fn test_cultural_distance_different_group() {
        let reg = registry();
        let a = reg.from_key("Illyrian").unwrap();
        let b = reg.from_key("nordian").unwrap();
        let distance = cultural_distance(a, b);
        assert!(distance > 0.5, "different group + different family should be > 0.5, got {}", distance);
    }

    #[test]
    fn test_cultural_distance_same_group_different_nation() {
        let reg = registry();
        let a = reg.from_key("Illyrian").unwrap();
        let b = reg.from_key("thracian").unwrap();
        let distance = cultural_distance(a, b);
        assert!(distance < 0.3, "same group + same family should be < 0.3, got {}", distance);
    }

    #[test]
    fn test_culture_key_from_display() {
        let reg = registry();
        assert_eq!(reg.culture_key_from_display("Illyria"), "Illyrian");
        assert_eq!(reg.religion_key_from_display("Catholicism"), "catholicism");
    }

    #[test]
    fn test_religion_centralized() {
        let reg = registry();
        let catholic = reg.religion_from_key("catholicism").unwrap();
        assert!(catholic.is_centralized);
        let islam = reg.religion_from_key("islam").unwrap();
        assert!(!islam.is_centralized);
    }
}

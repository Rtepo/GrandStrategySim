#![allow(missing_docs)]

use rand::seq::SliceRandom;
use rand::Rng;
use std::collections::BTreeMap;

/// Cultural background generated for a country at world generation.
#[derive(Debug, Clone, PartialEq)]
pub struct CulturalBackground {
    pub nation: String,
    /// English demonym for the nation's people (e.g., "Bactrians", "Nordians").
    pub demonym: String,
    pub cultural_group: String,
    pub religion: String,
    pub ethnic_composition: BTreeMap<String, f64>,
    pub religious_composition: BTreeMap<String, f64>,
    pub birth_rate: f64,
    pub mortality: f64,
    pub age_groups: AgeGroups,
    pub activity_rate: f64,
}

/// Age distribution returned by the cultural generator.
#[derive(Debug, Clone, PartialEq)]
pub struct AgeGroups {
    pub children: f64,
    pub working: f64,
    pub elderly: f64,
}

#[allow(dead_code)]
struct CulturalGroup {
    nations: &'static [&'static str],
    obsessions: &'static [&'static str],
    taboos: &'static [&'static str],
}

const CULTURAL_GROUPS: &[(&str, CulturalGroup)] = &[
    (
        "slavic",
        CulturalGroup {
            nations: &["Lechia", "Sarmatia", "Wenedia", "Krasnowia"],
            obsessions: &["agriculture", "heavy_industry"],
            taboos: &["export_services"],
        },
    ),
    (
        "germanic",
        CulturalGroup {
            nations: &["Nordia", "Anglia", "Helwecja"],
            obsessions: &["heavy_industry", "export_services"],
            taboos: &["agriculture"],
        },
    ),
    (
        "latin",
        CulturalGroup {
            nations: &["Galia", "Iberia", "Oksytania", "Dacja"],
            obsessions: &["local_services", "light_industry"],
            taboos: &["extractive_sector"],
        },
    ),
    (
        "middle_eastern",
        CulturalGroup {
            nations: &["Persja", "Baktria", "Anatolia", "Eldoria"],
            obsessions: &["extractive_sector", "local_services"],
            taboos: &["heavy_industry"],
        },
    ),
    (
        "balkan",
        CulturalGroup {
            nations: &["Iliria", "Tracja", "Pannonia", "Dardania"],
            obsessions: &["agriculture", "light_industry"],
            taboos: &["export_services"],
        },
    ),
];

/// Religion definitions: (engine_key, fertility_multiplier, particular_churches).
/// All keys are English engine keys matching `culture_registry::ReligionDefinition`.
const RELIGIONS: &[(&str, f64, &[&str])] = &[
    ("islam", 1.15, &["sunni", "shia"]),
    ("catholicism", 1.05, &["roman", "greek_catholic"]),
    ("orthodoxy", 1.0, &["constantinopolitan", "russian"]),
    ("protestantism", 0.95, &["lutheran", "calvinist"]),
    ("undeclared", 0.7, &[]),
    ("folk_beliefs", 1.2, &[]),
    ("shamanism", 1.25, &[]),
    ("pagan_cults", 1.15, &[]),
];

fn all_nations() -> Vec<&'static str> {
    CULTURAL_GROUPS
        .iter()
        .flat_map(|(_, g)| g.nations.iter().copied())
        .collect()
}

/// Generates an English demonym from a nation name using suffix rules.
///
/// # Rules
/// * Ends in "cja" → replace with "cians" (Helwecja → Helwecians)
/// * Ends in "sja" → replace with "sians" (Persja → Persians)
/// * Ends in "tja" → replace with "tians" (Tracja → Tracians)
/// * Ends in "ia" → replace with "ians" (Baktria → Baktrians, Nordia → Nordians)
/// * Ends in "a" → replace with "ans" (rare fallback)
/// * Ends in "s" or "x" → unchanged (already plural-sounding)
/// * Default → append "s"
pub fn generate_demonym(nation: &str) -> String {
    let lower = nation.to_lowercase();
    if lower.ends_with("cja") {
        format!("{}cians", &nation[..nation.len() - 3])
    } else if lower.ends_with("sja") {
        format!("{}sians", &nation[..nation.len() - 3])
    } else if lower.ends_with("tja") {
        format!("{}tians", &nation[..nation.len() - 3])
    } else if lower.ends_with("ia") {
        format!("{}ians", &nation[..nation.len() - 2])
    } else if lower.ends_with("a") {
        format!("{}ans", &nation[..nation.len() - 1])
    } else if lower.ends_with("s") || lower.ends_with("x") {
        nation.to_string()
    } else {
        format!("{}s", nation)
    }
}

fn pick_religion(group: &str, rng: &mut impl Rng) -> &'static str {
    match group {
        "middle_eastern" => {
            let choices = [("islam", 75), ("undeclared", 10), ("folk_beliefs", 10), ("shamanism", 5)];
            weighted_choice(&choices, rng)
        }
        "latin" => {
            let choices = [("catholicism", 65), ("undeclared", 25), ("elite_religion", 10)];
            weighted_choice(&choices, rng)
        }
        "slavic" => {
            let choices = [
                ("orthodoxy", 35),
                ("catholicism", 35),
                ("undeclared", 15),
                ("pagan_cults", 15),
            ];
            weighted_choice(&choices, rng)
        }
        "germanic" => {
            let choices = [
                ("protestantism", 45),
                ("catholicism", 15),
                ("undeclared", 30),
                ("pagan_cults", 10),
            ];
            weighted_choice(&choices, rng)
        }
        "balkan" => {
            let choices = [
                ("orthodoxy", 55),
                ("islam", 15),
                ("catholicism", 15),
                ("folk_beliefs", 15),
            ];
            weighted_choice(&choices, rng)
        }
        _ => RELIGIONS.choose(rng).unwrap().0,
    }
}

fn weighted_choice<'a>(choices: &[(&'a str, i32)], rng: &mut impl Rng) -> &'a str {
    let total: i32 = choices.iter().map(|(_, w)| w).sum();
    let mut roll = rng.gen_range(0..total);
    for (name, weight) in choices {
        roll -= *weight;
        if roll < 0 {
            return *name;
        }
    }
    choices.last().unwrap().0
}

fn fertility_for_religion(religion: &str) -> f64 {
    RELIGIONS
        .iter()
        .find(|(name, _, _)| *name == religion)
        .map(|(_, f, _)| *f)
        .unwrap_or(1.0)
}

fn particular_churches(religion: &str) -> &'static [&'static str] {
    RELIGIONS
        .iter()
        .find(|(name, _, _)| *name == religion)
        .map(|(_, _, churches)| *churches)
        .unwrap_or(&[])
}

/// Generates a randomized cultural background for a new country.
///
/// Mirrors `society.cultures.generate_cultural_background` from the Python
/// engine and returns the demographics needed to seed `MacroData`.
///
/// # SAFEGUARD: Urban/Rural Labor Separation
/// When populating LaborMarket.unskilled_tier during macro-data generation:
/// - unskilled_tier MUST contain ONLY urban workers (factory workers, urban service workers)
/// - Rural classes (LandlessLaborers, FreePeasants, Serfs) must be stored in RegionalClassDemographics
/// - This prevents double-counting when calculate_available_unskilled_labor explicitly builds the pool
pub fn generate_cultural_background(_country_name: &str) -> CulturalBackground {
    let mut rng = rand::thread_rng();

    let (group_name, group) = CULTURAL_GROUPS.choose(&mut rng).unwrap();
    let nation = *group.nations.choose(&mut rng).unwrap();
    let religion = pick_religion(*group_name, &mut rng);
    let demonym = generate_demonym(nation);

    let fertility = fertility_for_religion(religion);
    let mortality = rng.gen_range(0.6..1.2);
    let growth = fertility - mortality;

    let (children, elderly) = if growth > 0.5 {
        (rng.gen_range(0.25..0.40), rng.gen_range(0.05..0.15))
    } else if growth < 0.0 {
        (rng.gen_range(0.10..0.20), rng.gen_range(0.20..0.35))
    } else {
        (rng.gen_range(0.18..0.25), rng.gen_range(0.15..0.25))
    };
    let working = 1.0 - (children + elderly);

    let activity_rate = rng.gen_range(55.0..85.0);

    let mut ethnic = BTreeMap::new();
    let dominant = rng.gen_range(0.60..0.95);
    ethnic.insert(demonym.clone(), round2(dominant));
    let mut rest = 1.0 - dominant;

    let mut others = all_nations();
    others.retain(|n| *n != nation);
    others.shuffle(&mut rng);

    for other in others.iter().take(rng.gen_range(1..=3)) {
        if rest <= 0.01 {
            break;
        }
        let other_demonym = generate_demonym(other);
        let share = rng.gen_range(0.01..rest).min(rest);
        ethnic.insert(other_demonym, round2(share));
        rest -= share;
    }
    if rest > 0.0 {
        ethnic.insert("Other Minorities".to_string(), round2(rest));
    }

    let mut religious = BTreeMap::new();
    let dominant_religious = rng.gen_range(0.50..0.90);
    religious.insert(religion.to_string(), round2(dominant_religious));
    let mut rest_religion = round2(1.0 - dominant_religious);

    let churches = particular_churches(religion);
    if !churches.is_empty() && rest_religion > 0.05 {
        let branch = *churches.choose(&mut rng).unwrap();
        let name = format!("{}_{}", religion, branch);
        let share = rest_religion * rng.gen_range(0.3..0.7);
        religious.insert(name, round2(share));
        rest_religion -= share;
    }

    let mut other_religions: Vec<_> = RELIGIONS.iter().map(|(n, _, _)| *n).filter(|n| *n != religion).collect();
    other_religions.shuffle(&mut rng);
    for other in other_religions.iter().take(rng.gen_range(1..=2)) {
        if rest_religion <= 0.01 {
            break;
        }
        let share = rng.gen_range(0.01..rest_religion);
        religious.insert((*other).to_string(), round2(share));
        rest_religion -= share;
    }
    if rest_religion > 0.0 {
        religious.insert("undeclared".to_string(), round2(rest_religion));
    }

    CulturalBackground {
        nation: nation.to_string(),
        demonym,
        cultural_group: (*group_name).to_string(),
        religion: religion.to_string(),
        ethnic_composition: ethnic,
        religious_composition: religious,
        birth_rate: fertility,
        mortality,
        age_groups: AgeGroups { children, working, elderly },
        activity_rate,
    }
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_demonym_ia_suffix() {
        assert_eq!(generate_demonym("Baktria"), "Baktrians");
        assert_eq!(generate_demonym("Nordia"), "Nordians");
        assert_eq!(generate_demonym("Lechia"), "Lechians");
        assert_eq!(generate_demonym("Sarmatia"), "Sarmatians");
        assert_eq!(generate_demonym("Iberia"), "Iberians");
        assert_eq!(generate_demonym("Anatolia"), "Anatolians");
        assert_eq!(generate_demonym("Pannonia"), "Pannonians");
        assert_eq!(generate_demonym("Dardania"), "Dardanians");
    }

    #[test]
    fn test_demonym_cja_suffix() {
        assert_eq!(generate_demonym("Helwecja"), "Helwecians");
        assert_eq!(generate_demonym("Dacja"), "Dacians");
    }

    #[test]
    fn test_demonym_sja_suffix() {
        assert_eq!(generate_demonym("Persja"), "Persians");
    }

    #[test]
    fn test_demonym_tja_suffix() {
        assert_eq!(generate_demonym("Tracja"), "Tracians");
    }

    #[test]
    fn test_demonym_already_plural() {
        assert_eq!(generate_demonym("Atlas"), "Atlas");
    }

    #[test]
    fn test_demonym_default_suffix() {
        assert_eq!(generate_demonym("Eldor"), "Eldors");
    }

    #[test]
    fn test_cultural_background_has_demonym() {
        let bg = generate_cultural_background("TestCountry");
        assert!(!bg.demonym.is_empty(), "Demonym must not be empty");
        assert_ne!(bg.demonym, bg.nation, "Demonym must differ from nation name");
    }

    #[test]
    fn test_cultural_background_english_religion_keys() {
        let bg = generate_cultural_background("TestCountry");
        // Religion should be an English engine key, not a Polish display name
        let known_keys = ["islam", "catholicism", "orthodoxy", "protestantism",
                          "undeclared", "folk_beliefs", "shamanism", "pagan_cults", "elite_religion"];
        assert!(known_keys.contains(&bg.religion.as_str()),
            "Religion '{}' should be an English engine key", bg.religion);
    }

    #[test]
    fn test_cultural_background_english_ethnic_labels() {
        let bg = generate_cultural_background("TestCountry");
        // Check that no Polish labels remain
        for key in bg.ethnic_composition.keys() {
            assert!(!key.contains("mniejszości"), "Ethnic label '{}' must not be Polish", key);
        }
        for key in bg.religious_composition.keys() {
            assert!(!key.contains("Niezadeklarowani"), "Religious label '{}' must not be Polish", key);
            assert!(!key.contains("Autonomiczny"), "Religious label '{}' must not be Polish", key);
            assert!(!key.contains("Lokalny"), "Religious label '{}' must not be Polish", key);
        }
    }
}

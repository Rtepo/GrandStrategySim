#![allow(missing_docs)]

use rand::seq::SliceRandom;
use rand::Rng;
use std::collections::BTreeMap;

/// Cultural background generated for a country at world generation.
#[derive(Debug, Clone, PartialEq)]
pub struct CulturalBackground {
    pub nation: String,
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
            obsessions: &["rolnictwo", "przemysł_ciężki"],
            taboos: &["usługi_eksportowe"],
        },
    ),
    (
        "germanic",
        CulturalGroup {
            nations: &["Nordia", "Anglia", "Helwecja"],
            obsessions: &["przemysł_ciężki", "usługi_eksportowe"],
            taboos: &["rolnictwo"],
        },
    ),
    (
        "latin",
        CulturalGroup {
            nations: &["Galia", "Iberia", "Oksytania", "Dacja"],
            obsessions: &["usługi_lokalne", "przemysł_lekki"],
            taboos: &["sektor_wydobywczy"],
        },
    ),
    (
        "middle_eastern",
        CulturalGroup {
            nations: &["Persja", "Baktria", "Anatolia", "Eldoria"],
            obsessions: &["sektor_wydobywczy", "usługi_lokalne"],
            taboos: &["przemysł_ciężki"],
        },
    ),
    (
        "balkan",
        CulturalGroup {
            nations: &["Iliria", "Tracja", "Pannonia", "Dardania"],
            obsessions: &["rolnictwo", "przemysł_lekki"],
            taboos: &["usługi_eksportowe"],
        },
    ),
];

const RELIGIONS: &[(&str, f64, &[&str])] = &[
    ("Islam", 1.15, &["Sunnizm", "Szyizm"]),
    ("Katolicyzm", 1.05, &["Rzymski", "Grekokatolicyzm"]),
    ("Prawosławie", 1.0, &["Patriarchat Konstantynopolitański", "Rosyjski"]),
    ("Protestantyzm", 0.95, &["Luteranizm", "Kalwinizm"]),
    ("Ateizm / Agnostycyzm", 0.7, &[]),
    ("Wierzenia Ludowe", 1.2, &[]),
    ("Szamanizm", 1.25, &[]),
    ("Kulty Pogańskie", 1.15, &[]),
];

fn all_nations() -> Vec<&'static str> {
    CULTURAL_GROUPS
        .iter()
        .flat_map(|(_, g)| g.nations.iter().copied())
        .collect()
}

fn pick_religion(group: &str, rng: &mut impl Rng) -> &'static str {
    match group {
        "middle_eastern" => {
            let choices = [("Islam", 75), ("Ateizm / Agnostycyzm", 10), ("Wierzenia Ludowe", 10), ("Szamanizm", 5)];
            weighted_choice(&choices, rng)
        }
        "latin" => {
            let choices = [("Katolicyzm", 65), ("Ateizm / Agnostycyzm", 25), ("Religia Elit", 10)];
            weighted_choice(&choices, rng)
        }
        "slavic" => {
            let choices = [
                ("Prawosławie", 35),
                ("Katolicyzm", 35),
                ("Ateizm / Agnostycyzm", 15),
                ("Kulty Pogańskie", 15),
            ];
            weighted_choice(&choices, rng)
        }
        "germanic" => {
            let choices = [
                ("Protestantyzm", 45),
                ("Katolicyzm", 15),
                ("Ateizm / Agnostycyzm", 30),
                ("Kulty Pogańskie", 10),
            ];
            weighted_choice(&choices, rng)
        }
        "balkan" => {
            let choices = [
                ("Prawosławie", 55),
                ("Islam", 15),
                ("Katolicyzm", 15),
                ("Wierzenia Ludowe", 15),
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
    ethnic.insert(nation.to_string(), round2(dominant));
    let mut rest = 1.0 - dominant;

    let mut others = all_nations();
    others.retain(|n| *n != nation);
    others.shuffle(&mut rng);

    for other in others.iter().take(rng.gen_range(1..=3)) {
        if rest <= 0.01 {
            break;
        }
        let share = rng.gen_range(0.01..rest).min(rest);
        ethnic.insert((*other).to_string(), round2(share));
        rest -= share;
    }
    if rest > 0.0 {
        ethnic.insert("Inne mniejszości".to_string(), round2(rest));
    }

    let mut religious = BTreeMap::new();
    let dominant_religious = rng.gen_range(0.50..0.90);
    religious.insert(religion.to_string(), round2(dominant_religious));
    let mut rest_religion = round2(1.0 - dominant_religious);

    let churches = particular_churches(religion);
    if !churches.is_empty() && rest_religion > 0.05 {
        let branch = *churches.choose(&mut rng).unwrap();
        let prefix = if branch.contains("sui iuris") {
            "Autonomiczny"
        } else {
            "Lokalny"
        };
        let name = format!("{prefix} {religion}");
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
        religious.insert("Niezadeklarowani".to_string(), round2(rest_religion));
    }

    CulturalBackground {
        nation: nation.to_string(),
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

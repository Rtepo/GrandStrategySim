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
    /// v4.24.0 WS2: stable-population sixteen-band cohort shares (see
    /// [`stable_age_cohorts`]) — the Turn-0 seed for the per-turn
    /// cohort-component aging model persisted on `Demographics::age_cohorts`.
    pub age_cohorts: Vec<f64>,
    pub activity_rate: f64,
}

/// Age distribution returned by the cultural generator.
#[derive(Debug, Clone, PartialEq)]
pub struct AgeGroups {
    pub children: f64,
    pub working: f64,
    pub elderly: f64,
}

// ============================================================================
// v4.24.0 WS2: STABLE POPULATION MODEL (Lotka–Euler / model life table)
// ============================================================================
//
// Turn-0 countries previously drew children/elderly shares from arbitrary
// bands keyed on the natural-increase sign — for stagnating or declining
// countries that produced a 20–35% elderly share against a ~48–52 yr life
// expectancy, an impossible age structure whose phantom elderly then died
// en masse in the first year, collapsing the pyramid into an extreme
// bottom-heavy triangle.
//
// The stable-population identity fixes the age profile to the country's
// real vital rates: in a population growing at intrinsic rate `r` the age
// density is c(a) ∝ exp(−r·a)·l(a), where l(a) is cohort survival from a
// model life table. We fit the table's scale so life expectancy at birth
// equals the country's real `average_lifespan` anchor, then integrate
// c(a) into sixteen five-year cohorts — the same vector persisted on
// `Demographics::age_cohorts` and aged band-by-band every turn.

/// Number of persisted five-year age bands on `Demographics::age_cohorts`:
/// [0–4] … [70–74] plus the open [75+] tail.
pub const AGE_COHORT_BANDS: usize = 16;
/// Width of each closed age band in years.
pub const AGE_BAND_YEARS: f64 = 5.0;
/// First band index of working age (15–19).
pub const ADULT_BAND_START: usize = 3;
/// First band index of old age (65–69).
pub const ELDERLY_BAND_START: usize = 13;
/// Fraction of a band that graduates to the next band per turn
/// (Δt = 1/24 yr over a 5-yr band → 1/120).
pub const AGE_GRADUATION_PER_TURN: f64 =
    1.0 / (AGE_BAND_YEARS * crate::state::macro_data::TURNS_PER_YEAR as f64);
/// Fraction of a year elapsed per engine turn (24 half-months per year).
pub const TURN_YEARS: f64 = 1.0 / crate::state::macro_data::TURNS_PER_YEAR as f64;

/// Model life table: Weibull survival `l(a) = exp(−(a/s)^k)`. The shape
/// k = 6 concentrates deaths around the mean lifespan the way pre-
/// transition life tables do (senescent mortality regime) — a flat
/// exponential table would instead smear deaths uniformly across ages and
/// grossly overstate elderly shares at low life expectancy.
const LIFE_TABLE_SHAPE: f64 = 6.0;
/// Quadrature horizon and step for life-table integrals. Beyond ~120 yr
/// Weibull survival is identically 0 at double precision for any plausible
/// scale, so 160 is a safe effective ∞.
const LIFE_TABLE_HORIZON: f64 = 160.0;
const LIFE_TABLE_STEP: f64 = 0.5;

/// Survival from birth to `age` under a life table with the given scale.
fn life_table_survival(age: f64, scale: f64) -> f64 {
    if age <= 0.0 || scale <= 0.0 {
        return if age <= 0.0 { 1.0 } else { 0.0 };
    }
    (-(age / scale).powf(LIFE_TABLE_SHAPE)).exp()
}

/// Life expectancy at birth for a life-table scale: e0 = ∫₀^∞ l(a)da.
fn life_table_expectancy(scale: f64) -> f64 {
    let mut acc = 0.0;
    let mut a = LIFE_TABLE_STEP / 2.0;
    while a < LIFE_TABLE_HORIZON {
        acc += life_table_survival(a, scale);
        a += LIFE_TABLE_STEP;
    }
    acc * LIFE_TABLE_STEP
}

/// Fit the life-table scale so its implied e0 equals `life_expectancy`.
/// e0(scale) is strictly increasing — bisection is deterministic.
fn fit_life_table_scale(life_expectancy: f64) -> f64 {
    let target = life_expectancy.clamp(20.0, 110.0);
    let (mut lo, mut hi) = (1.0_f64, 250.0_f64);
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if life_table_expectancy(mid) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0
}

/// Stable-population cohort shares (Lotka–Euler form) for a country with
/// the given crude birth/death rates (percent per year) and life
/// expectancy. Age density c(a) ∝ exp(−r·a)·l(a) with
/// r = (b − d)/100 — the birth history that produced each surviving
/// cohort — integrated over the sixteen five-year bands.
///
/// Fully deterministic: no RNG, fixed quadrature.
pub fn stable_age_cohorts(
    birth_rate_pct: f64,
    death_rate_pct: f64,
    life_expectancy: f64,
) -> [f64; AGE_COHORT_BANDS] {
    let r = ((birth_rate_pct - death_rate_pct) / 100.0).clamp(-0.08, 0.08);
    let scale = fit_life_table_scale(life_expectancy);
    let mut bands = [0.0_f64; AGE_COHORT_BANDS];
    for (i, band) in bands.iter_mut().enumerate() {
        let start = i as f64 * AGE_BAND_YEARS;
        let end = if i + 1 == AGE_COHORT_BANDS {
            LIFE_TABLE_HORIZON
        } else {
            start + AGE_BAND_YEARS
        };
        let mut acc = 0.0;
        let mut a = start + LIFE_TABLE_STEP / 2.0;
        while a < end {
            acc += (-r * a).exp() * life_table_survival(a, scale);
            a += LIFE_TABLE_STEP;
        }
        *band = acc * LIFE_TABLE_STEP;
    }
    let total: f64 = bands.iter().sum();
    if total > 0.0 {
        for band in &mut bands {
            *band /= total;
        }
    }
    bands
}

/// Aggregate the 16-band cohort vector into the legacy three-bucket split
/// (children 0–14 / adults 15–64 / elderly 65+).
pub fn aggregate_three_cohorts(cohorts: &[f64]) -> (f64, f64, f64) {
    let children: f64 = cohorts
        .iter()
        .take(ADULT_BAND_START)
        .copied()
        .sum::<f64>();
    let adults: f64 = cohorts
        .iter()
        .skip(ADULT_BAND_START)
        .take(ELDERLY_BAND_START - ADULT_BAND_START)
        .copied()
        .sum::<f64>();
    let elderly: f64 = cohorts
        .iter()
        .skip(ELDERLY_BAND_START)
        .copied()
        .sum::<f64>();
    (children, adults, elderly)
}

/// Annual per-band mortality hazard rates implied by a life table fitted
/// to `life_expectancy`: h_i = −ln(l(a+5)/l(a))/5 for closed bands. The
/// runtime cohort model rescales all hazards by a common factor to match
/// the authoritative per-turn death count (proportional hazards — the
/// standard way demography represents a regime shift in overall
/// mortality), so only the cross-band profile matters here.
pub fn cohort_band_hazards(life_expectancy: f64) -> [f64; AGE_COHORT_BANDS] {
    let scale = fit_life_table_scale(life_expectancy);
    let mut hazards = [0.0_f64; AGE_COHORT_BANDS];
    for (i, h) in hazards.iter_mut().enumerate() {
        let a0 = i as f64 * AGE_BAND_YEARS;
        let a1 = a0 + AGE_BAND_YEARS;
        let l0 = life_table_survival(a0, scale);
        let l1 = life_table_survival(a1, scale);
        *h = if l0 > 0.0 && l1 > 0.0 {
            (-(l1 / l0).ln() / AGE_BAND_YEARS).max(1e-9)
        } else {
            // Deep tail: survival ~0 — use the local mortality rate
            // μ(a) = k·a^(k−1)/s^k evaluated at the band start.
            (LIFE_TABLE_SHAPE * a0.powf(LIFE_TABLE_SHAPE - 1.0)
                / scale.powf(LIFE_TABLE_SHAPE))
            .max(1e-9)
        };
    }
    hazards
}

/// Mean and median age of a cohort distribution. Closed bands are
/// midpoint-weighted; the open 75+ band uses the model life table's
/// representative mean of ~80 (residual life at 75 under a ~50 yr e0
/// senescent table is 5–8 yr — the band's small share makes the exact
/// figure low-sensitivity).
pub fn cohort_age_stats(cohorts: &[f64]) -> (f64, f64) {
    const OPEN_BAND_MEAN_AGE: f64 = 80.0;
    let total: f64 = cohorts.iter().sum();
    if total <= 0.0 {
        return (0.0, 0.0);
    }
    let mut mean = 0.0;
    let mut cumulative = 0.0;
    let mut median = 0.0;
    let mut median_found = false;
    for (i, &share) in cohorts.iter().enumerate() {
        let mid = if i + 1 == AGE_COHORT_BANDS {
            OPEN_BAND_MEAN_AGE
        } else {
            i as f64 * AGE_BAND_YEARS + AGE_BAND_YEARS / 2.0
        };
        mean += share * mid;
        if !median_found && cumulative + share >= 0.5 * total && share > 0.0 {
            let within = (0.5 * total - cumulative) / share;
            median = i as f64 * AGE_BAND_YEARS + within * AGE_BAND_YEARS;
            median_found = true;
        }
        cumulative += share;
    }
    (mean / total, median)
}

/// Age profile of cross-border migrants as band offsets from
/// [`ADULT_BAND_START`]: labor migration is concentrated in the 15–39
/// prime working ages. Deterministic constant — mirrors the population
/// structure of every documented migrant stream (young workers move).
/// Index 0 → band 3 (15–19) … index 6 → band 9 (45–49).
pub const MIGRANT_AGE_WEIGHTS: [f64; 7] = [0.28, 0.26, 0.20, 0.12, 0.08, 0.04, 0.02];

/// Solve the proportional-hazards scale λ such that per-turn cohort
/// deaths Σ cᵢ·(1 − exp(−λ·hᵢ·Δt)) equal the target per-turn death
/// share (deaths ÷ population). Standard proportional-hazards regime
/// shift: band hazards scale together so mortality shocks spread over the
/// life table instead of landing on a single bucket. Deterministic
/// bisection (48 iters — f64-tight). Returns 0.0 when there are no
/// deaths to allocate.
pub fn solve_cohort_death_scale(
    cohorts: &[f64],
    hazards: &[f64; AGE_COHORT_BANDS],
    death_share: f64,
) -> f64 {
    if death_share <= 0.0 || cohorts.len() != AGE_COHORT_BANDS {
        return 0.0;
    }
    let eval = |lambda: f64| -> f64 {
        let mut acc = 0.0;
        for i in 0..AGE_COHORT_BANDS {
            acc += cohorts[i] * (1.0 - (-lambda * hazards[i] * TURN_YEARS).exp());
        }
        acc
    };
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    // Bracket: λ→∞ saturates deaths at Σcᵢ ≈ 1 > any real death share.
    while eval(hi) < death_share && hi < 1.0e6 {
        hi *= 2.0;
    }
    for _ in 0..48 {
        let mid = (lo + hi) / 2.0;
        if eval(mid) < death_share {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0
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
            nations: &["Lechia", "Sarmatia", "Venedia", "Krasnovia"],
            obsessions: &["agriculture", "heavy_industry"],
            taboos: &["export_services"],
        },
    ),
    (
        "germanic",
        CulturalGroup {
            nations: &["Nordia", "Anglia", "Helvetia"],
            obsessions: &["heavy_industry", "export_services"],
            taboos: &["agriculture"],
        },
    ),
    (
        "latin",
        CulturalGroup {
            nations: &["Gallia", "Iberia", "Occitania", "Dacia"],
            obsessions: &["local_services", "light_industry"],
            taboos: &["extractive_sector"],
        },
    ),
    (
        "middle_eastern",
        CulturalGroup {
            nations: &["Persia", "Bactria", "Anatolia", "Eldoria"],
            obsessions: &["extractive_sector", "local_services"],
            taboos: &["heavy_industry"],
        },
    ),
    (
        "balkan",
        CulturalGroup {
            nations: &["Illyria", "Thracia", "Pannonia", "Dardania"],
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
/// * Ends in "tia" → replace with "tians" (Helvetia → Helvetians)
/// * Ends in "sia" → replace with "sians" (Persia → Persians)
/// * Ends in "ia" → replace with "ians" (Bactria → Bactrians, Nordia → Nordians)
/// * Ends in "a" → replace with "ans" (rare fallback)
/// * Ends in "s" or "x" → unchanged (already plural-sounding)
/// * Default → append "s"
pub fn generate_demonym(nation: &str) -> String {
    let lower = nation.to_lowercase();
    if lower.ends_with("tia") {
        format!("{}tians", &nation[..nation.len() - 3])
    } else if lower.ends_with("sia") {
        format!("{}sians", &nation[..nation.len() - 3])
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
            let choices = [
                ("islam", 75),
                ("undeclared", 10),
                ("folk_beliefs", 10),
                ("shamanism", 5),
            ];
            weighted_choice(&choices, rng)
        }
        "latin" => {
            let choices = [
                ("catholicism", 65),
                ("undeclared", 25),
                ("elite_religion", 10),
            ];
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
            return name;
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
///
/// # Arguments
/// * `life_expectancy` - the country's worldgen life-expectancy anchor
///   (`40 + gdp_pc·10`, same expression that seeds `average_lifespan`),
///   used to fit the model life table behind the stable age distribution.
pub fn generate_cultural_background(
    _country_name: &str,
    life_expectancy: f64,
) -> CulturalBackground {
    let mut rng = crate::engine::seeded_rng::thread_rng();

    let (group_name, group) = CULTURAL_GROUPS.choose(&mut rng).unwrap();
    let nation = *group.nations.choose(&mut rng).unwrap();
    let religion = pick_religion(group_name, &mut rng);
    let demonym = generate_demonym(nation);

    let fertility = fertility_for_religion(religion);
    // v4.23.0 WS6: era-plausible vital rates (percent/yr).
    // `birth_rate`/`mortality` are persisted as `Demographics` *percent*
    // rates and consumed as annual rates by the labor model
    // (`demographics.birth_rate / 100.0`). Pre-WS6 the raw 0.7–1.25
    // religious multiplier was stored, which the UI's fertility estimate
    // rendered as 33.33 and which implied a sub-1%/yr crude birth rate.
    // Scale the multiplier onto the 1900-era band: base 4.2%/yr CBR ×
    // religion → clamped [2.0, 5.5]%/yr (islam ~4.8, folk ~5.0,
    // undeclared ~2.9 — pre-transition differentiation preserved).
    let birth_rate = (4.2 * fertility).clamp(2.0, 5.5);
    // 1900-era crude death rate ~18–34‰ (percent/yr).
    let mortality = rng.gen_range(1.8..3.4);

    // v4.24.0 WS2: Turn-0 age structure from the stable population
    // identity (Lotka–Euler / model life table), fitted to the country's
    // real vital rates and life expectancy — replacing the arbitrary
    // growth-sign band draws that seeded an impossible 20–35% elderly
    // glut and a mass Year-1 die-off.
    let cohorts = stable_age_cohorts(birth_rate, mortality, life_expectancy);
    let (children, working, elderly) = aggregate_three_cohorts(&cohorts);

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

    let mut other_religions: Vec<_> = RELIGIONS
        .iter()
        .map(|(n, _, _)| *n)
        .filter(|n| *n != religion)
        .collect();
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
        birth_rate,
        mortality,
        age_groups: AgeGroups {
            children,
            working,
            elderly,
        },
        age_cohorts: cohorts.to_vec(),
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
        assert_eq!(generate_demonym("Bactria"), "Bactrians");
        assert_eq!(generate_demonym("Nordia"), "Nordians");
        assert_eq!(generate_demonym("Lechia"), "Lechians");
        assert_eq!(generate_demonym("Sarmatia"), "Sarmatians");
        assert_eq!(generate_demonym("Iberia"), "Iberians");
        assert_eq!(generate_demonym("Anatolia"), "Anatolians");
        assert_eq!(generate_demonym("Pannonia"), "Pannonians");
        assert_eq!(generate_demonym("Dardania"), "Dardanians");
    }

    #[test]
    fn test_demonym_tia_suffix() {
        assert_eq!(generate_demonym("Helvetia"), "Helvetians");
        assert_eq!(generate_demonym("Dacia"), "Dacians");
        assert_eq!(generate_demonym("Thracia"), "Thracians");
    }

    #[test]
    fn test_demonym_sia_suffix() {
        assert_eq!(generate_demonym("Persia"), "Persians");
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
        let bg = generate_cultural_background("TestCountry", 50.0);
        assert!(!bg.demonym.is_empty(), "Demonym must not be empty");
        assert_ne!(
            bg.demonym, bg.nation,
            "Demonym must differ from nation name"
        );
    }

    #[test]
    fn test_cultural_background_english_religion_keys() {
        let bg = generate_cultural_background("TestCountry", 50.0);
        // Religion should be an English engine key, not a Polish display name
        let known_keys = [
            "islam",
            "catholicism",
            "orthodoxy",
            "protestantism",
            "undeclared",
            "folk_beliefs",
            "shamanism",
            "pagan_cults",
            "elite_religion",
        ];
        assert!(
            known_keys.contains(&bg.religion.as_str()),
            "Religion '{}' should be an English engine key",
            bg.religion
        );
    }

    #[test]
    fn test_cultural_background_english_ethnic_labels() {
        let bg = generate_cultural_background("TestCountry", 50.0);
        // Check that no Polish labels remain
        for key in bg.ethnic_composition.keys() {
            assert!(
                !key.contains("minority"),
                "Ethnic label '{}' must not be Polish",
                key
            );
        }
        for key in bg.religious_composition.keys() {
            assert!(
                !key.contains("Niezadeklarowani"),
                "Religious label '{}' must not be Polish",
                key
            );
            assert!(
                !key.contains("Autonomiczny"),
                "Religious label '{}' must not be Polish",
                key
            );
            assert!(
                !key.contains("Lokalny"),
                "Religious label '{}' must not be Polish",
                key
            );
        }
    }

    // === v4.24.0 WS2: STABLE POPULATION MODEL ===

    #[test]
    fn test_stable_age_cohorts_normalized_and_plausible() {
        // 1900-era vital rates: CBR 4.2%, CDR 2.4%, e0 50.
        let cohorts = stable_age_cohorts(4.2, 2.4, 50.0);
        let total: f64 = cohorts.iter().sum();
        assert!(
            (total - 1.0).abs() < 1e-9,
            "cohort shares must sum to 1.0, got {total}"
        );
        assert!(
            cohorts.iter().all(|s| *s >= 0.0),
            "no negative cohort shares"
        );
        // Era-plausible: strong young base, small elderly tail — NOT the
        // old 20–35% elderly glut.
        let (children, adults, elderly) = aggregate_three_cohorts(&cohorts);
        assert!(
            children > 0.30 && children < 0.50,
            "children share {children} should sit in the pre-transition band"
        );
        assert!(
            adults > 0.45 && adults < 0.65,
            "adult share {adults} implausible"
        );
        assert!(
            elderly < 0.08,
            "elderly share {elderly} must be a thin tail at e0=50, not a glut"
        );
    }

    #[test]
    fn test_stable_age_cohorts_high_fertility_is_younger() {
        let young = stable_age_cohorts(5.0, 2.0, 50.0);
        let old = stable_age_cohorts(2.2, 3.0, 50.0);
        let (yc, _, ye) = aggregate_three_cohorts(&young);
        let (oc, _, oe) = aggregate_three_cohorts(&old);
        assert!(
            yc > oc,
            "higher intrinsic growth must produce a younger pyramid ({yc} vs {oc})"
        );
        assert!(
            ye < oe,
            "declining population must produce an older pyramid ({ye} vs {oe})"
        );
    }

    #[test]
    fn test_stable_age_cohorts_longer_life_fattens_tail() {
        let short = stable_age_cohorts(3.5, 2.5, 45.0);
        let long = stable_age_cohorts(3.5, 2.5, 65.0);
        let (_, _, se) = aggregate_three_cohorts(&short);
        let (_, _, le) = aggregate_three_cohorts(&long);
        assert!(
            le > se,
            "higher life expectancy must raise the elderly share ({le} vs {se})"
        );
    }

    #[test]
    fn test_cohort_age_stats_sane() {
        let cohorts = stable_age_cohorts(4.2, 2.4, 50.0);
        let (mean, median) = cohort_age_stats(&cohorts);
        // A stable pre-transition pyramid has mean ~25–30, median ~18–25.
        assert!(
            mean > 20.0 && mean < 35.0,
            "mean age {mean} outside era-plausible band"
        );
        assert!(
            median > 15.0 && median < 30.0,
            "median age {median} outside era-plausible band"
        );
        assert!(median < mean, "pyramids are right-skewed: median < mean");
    }

    #[test]
    fn test_band_hazards_increase_with_age() {
        let hazards = cohort_band_hazards(50.0);
        // Senescent mortality: old bands must carry far higher hazard
        // rates than young adult bands.
        assert!(
            hazards[14] > hazards[5] * 10.0,
            "old-age hazard {} should dominate midlife hazard {}",
            hazards[14],
            hazards[5]
        );
        assert!(hazards.iter().all(|h| *h > 0.0));
    }
}

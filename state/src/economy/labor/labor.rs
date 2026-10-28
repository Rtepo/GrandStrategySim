//! Labor market and demographics update.
//!
//! This module ports the deterministic per-turn population and labor-market
//! logic from Python's `society/demographics.py` (`update_population`) and the
//! wage/labor-supply parts of `economy/labor/core.py`, `wages.py`,
//! `workforce.py` and `unemployment.py`.
//!
//! It mutates `Treasury.population`, `MacroData.demographics`,
//! `MacroData.labor_market` and updates `MacroData.average_wage`.

use crate::economy::CountryTurnCtx;
use crate::society::geography::{RuralClass, UrbanClass};
use crate::state::macro_data::{annual_to_per_turn_rate, ImmigrantCohort};
use serde_json::{Map, Value};

/// Extracts an `f64` from a JSON value, falling back to `default`.
fn f64_from_value(value: Option<&Value>, default: f64) -> f64 {
    value.and_then(|v| v.as_f64()).unwrap_or(default)
}

/// Extracts a string from a JSON value, falling back to `default`.
fn string_from_value(value: Option<&Value>, default: &str) -> String {
    value
        .and_then(|v| v.as_str())
        .unwrap_or(default)
        .to_string()
}

/// Extracts a bool from a JSON value, falling back to `default`.
fn bool_from_value(value: Option<&Value>, default: bool) -> bool {
    value.and_then(|v| v.as_bool()).unwrap_or(default)
}

/// Returns the fertility multiplier implied by the emancipation law.
fn fertility_multiplier(emancipation_law: &str) -> f64 {
    match emancipation_law {
        "Traditionalism" => 1.25,
        "Property Rights" => 1.10,
        "Limited Suffrage" => 0.90,
        "Full Emancipation" => 0.75,
        _ => 1.0,
    }
}

/// Returns the emancipation-driven participation modifier for each tier.
fn emancipation_modifiers(emancipation_law: &str) -> (f64, f64, f64) {
    match emancipation_law {
        "Traditionalism" => (0.55, 0.65, 0.85),
        "Property Rights" => (0.70, 0.80, 0.90),
        "Limited Suffrage" => (0.85, 0.95, 1.0),
        "Full Emancipation" => (1.0, 1.0, 1.0),
        _ => (1.0, 1.0, 1.0),
    }
}

/// Returns the minimum-wage coefficient implied by labor law.
fn minimum_wage_multiplier(labor_law: &str) -> f64 {
    match labor_law {
        "Rigid Minimum Wage" => 0.70,
        "Worker Protection" => 0.45,
        _ => 0.0,
    }
}

/// v4.20 WS3: Adaptive inflation expectations for the wage Phillips curve.
///
/// Workers and firms form expectations adaptively — an exponentially
/// weighted average of *realized* per-turn CPI inflation read from the
/// telemetry history (newest sample has the most weight). This is the
/// standard backward-looking expectations channel: no synthetic anchor,
/// no random state, and no new persisted field — the expectation is
/// re-derived deterministically from recorded history each turn.
///
/// * `history` - the rolling `TelemetryHistory` buffer (`cpi_inflation`
///   is stored in PERCENT per turn).
/// * `current` - this turn's realized CPI inflation (percent, per turn).
///
/// Returns expected per-turn CPI inflation in percent. `λ = 0.20` per
/// lag gives an effective memory of ~5 turns (~2.5 months), so a
/// persistent deflation regime drags expectations negative within a
/// quarter while single-turn noise is averaged out.
pub fn expected_inflation_ewma(
    history: &crate::state::macro_data::TelemetryHistory,
    current: f64,
) -> f64 {
    const LAMBDA: f64 = 0.20;
    let mut weight_sum = 0.0;
    let mut acc = 0.0;
    // Current turn's print anchors the expectation (weight 1 − λ on the
    // freshest observation comes first in the fold below: we fold the
    // current value as lag 0 by seeding with it directly).
    acc += current * LAMBDA;
    weight_sum += LAMBDA;
    let mut decay = 1.0 - LAMBDA;
    for sample in history.samples.iter().rev() {
        let w = LAMBDA * decay;
        acc += sample.cpi_inflation * w;
        weight_sum += w;
        decay *= 1.0 - LAMBDA;
    }
    if weight_sum > 0.0 {
        acc / weight_sum
    } else {
        current
    }
}

/// v4.20 WS3: Phillips-curve slope — the per-turn nominal-wage response
/// to one point of labor-market slack (fraction). κ = 0.15 means each
/// +1pp of unemployment above the frictional floor cuts nominal wage
/// growth by 0.15%/turn (≈ −3.5%/yr); symmetric upside when the market
/// is tighter than the frictional norm. Combined with the 1.0
/// pass-through of expected per-turn inflation this yields:
///   wage_inflation_turn = E[π] − κ × slack
const PHILLIPS_KAPPA: f64 = 0.15;
/// Bound the per-turn wage adjustment so a single turn cannot move the
/// nominal wage schedule by more than ±2.5% — keeps the wage channel
/// bounded under extreme deflation/overheating while still allowing a
/// real (signed) decline in slack, deflationary markets.
const WAGE_INFLATION_TURN_CAP: f64 = 0.025;

/// v4.21 WS2: Target labor share of value added per labor-law regime.
/// Collective-bargaining and statutory-minimum regimes anchor a larger
/// fraction of VA to wages; free-market regimes let the Phillips cycle
/// do more of the work. All are far below 1.0 — profits fund I.
fn wage_share_target(labor_law: &str) -> f64 {
    match labor_law {
        "Rigid Minimum Wage" => 0.55,
        "Worker Protection" => 0.50,
        _ => 0.40,
    }
}

/// v4.21 WS2: Convergence speed of the value-added wage norm — a 0.10
/// labor-share gap pulls wages ±0.8%/turn, so a missing-income-share
/// regime repairs over ~1-2 sim-years rather than lurching.
const WAGE_NORM_GAIN: f64 = 0.08;
/// v4.21 WS2: The norm term alone can never push nominal wages more than
/// ±2%/turn — stays subordinate to the ±2.5% total cap above.
const WAGE_NORM_TURN_CAP: f64 = 0.02;

/// Updates demographics and labor market for one turn.
///
/// # Arguments
/// * `ctx` - Mutable country context (turn, year, country state).
///
/// # Rules
/// * Population grows by births minus deaths minus workplace deaths plus net
///   migration, then clamps to at least one citizen.
/// * Age groups, gender split, immigrant cohorts, and ethnic composition are
///   recomputed deterministically.
/// * Labor supply is derived from education shares and emancipation modifiers.
/// * Wages are built from `average_wage`, the labor law, social-program budget,
///   energy-shield status and a dynamic friction term when unemployment is
///   above its frictional floor.
/// * `MacroData.average_wage` is updated to the weighted average of the tier
///   wages actually paid.
pub fn process_demographics_and_labor(ctx: &mut CountryTurnCtx) {
    let country = &mut ctx.country;

    let population = country.budget.population as f64;
    let prev_avg = country.macro_indicators.average_wage;
    let _dominant_culture = country.macro_indicators.culture.clone();

    // Phase 8: Compute winter mortality multiplier from regions before splitting borrows
    let mut _total_pop: f64 = 0.0;
    let mut _weighted_multiplier: f64 = 0.0;
    for region in &country.regions {
        let pop = region.population as f64;
        _total_pop += pop;
        _weighted_multiplier += region.winter_mortality_multiplier * pop;
    }
    let winter_mortality = if _total_pop > 0.0 {
        _weighted_multiplier / _total_pop
    } else {
        1.0
    };

    // v4.24.0 WS2: urbanization split from real class populations —
    // Worker/Bourgeoisie population IS the urban population, rural
    // classes the rural population. Replaces the static worldgen
    // logistic seed which never moved as industrialization pulled
    // classes into cities. Computed before the borrow split.
    let (mut class_urban_pop, mut class_rural_pop) = (0.0_f64, 0.0_f64);
    for region in &country.regions {
        for demo in region.class_demographics.rural_classes.values() {
            class_rural_pop += demo.population as f64;
        }
        for demo in region.class_demographics.urban_classes.values() {
            class_urban_pop += demo.population as f64;
        }
    }
    let class_pop_total = class_urban_pop + class_rural_pop;
    let (urban_share, rural_share) = if class_pop_total > 0.0 {
        (
            class_urban_pop / class_pop_total,
            class_rural_pop / class_pop_total,
        )
    } else {
        (
            country.macro_indicators.demographics.city_urban,
            country.macro_indicators.demographics.rural,
        )
    };

    let budget = &mut country.budget;
    let macro_indicators = &mut country.macro_indicators;

    // Phase 18S: Read sports health config before splitting borrows.
    let sports_le_bonus = country.labor_config.sports_health_bonus_per_capacity_unit;
    let sports_hle_bonus = country.labor_config.sports_healthy_life_bonus_per_capacity_unit;

    // Health read and update — Phase 86.5A: Use typed fields, not extra.
    let medical_infrastructure_base = macro_indicators.health_statistics.hospital_coverage;
    let healthcare_quality = macro_indicators.health_statistics.service_quality;
    let work_deaths = 0.0; // Tracked via mortality_rate

    // Phase 18S: Per-capita sports access term (physical access, not
    // demographic abstraction). sports_access_coverage is the ratio of
    // consumed SportsCapacity to region population, clamped [0.0, 1.0].
    let sports_access_coverage = macro_indicators
        .health_statistics
        .sports_capacity_per_capita
        .clamp(0.0, 1.0);

    let life_expectancy = (60.0
        + medical_infrastructure_base * 0.20
        + (healthcare_quality / 100.0) * 15.0
        + sports_access_coverage * sports_le_bonus)
        .min(95.0);
    let healthy_life_expectancy = (50.0
        + medical_infrastructure_base * 0.15
        + (healthcare_quality / 100.0) * 10.0
        + sports_access_coverage * sports_hle_bonus)
        .min(85.0);

    macro_indicators.health_statistics.average_lifespan = life_expectancy;
    macro_indicators.health_statistics.mortality_rate = work_deaths;

    // Policy and crime read.
    let policy = macro_indicators
        .extra
        .get("policy")
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));
    let crime_rate = macro_indicators
        .extra
        .get("crime_rate")
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));

    let emancipation_law = string_from_value(policy.get("emancipation_law"), "Traditionalism");
    let civil_law = string_from_value(policy.get("civil_law"), "5_year_assimilation");
    let labor_law = string_from_value(policy.get("labor_law"), "Free Market");
    let job_agency_active = bool_from_value(policy.get("job_agency_active"), false);
    let _energy_shield = bool_from_value(policy.get("energy_shield"), false);

    let crimes = f64_from_value(crime_rate.get("crimes"), 0.0);
    let _safety_index = f64_from_value(crime_rate.get("safety_index"), 80.0);

    let (new_avg, new_population) = {
        let demographics = &mut macro_indicators.demographics;
        let labor_market = &mut macro_indicators.labor_market;

        // Fertility, mortality, migration.
        let fertility_multiplier_val = fertility_multiplier(&emancipation_law);
        let birth_rate_index = (demographics.birth_rate / 100.0) * fertility_multiplier_val;
        let base_death_rate = demographics.death_rate / 100.0;
        let criminal_deaths = (crimes / 100.0) * 0.002;
        let reduced_death_rate = (base_death_rate
            - medical_infrastructure_base * 0.00005
            - (healthcare_quality / 100.0) * 0.003
            + criminal_deaths)
            .max(0.003);

        // Phase 8: Apply winter mortality multiplier (computed from regions before closure)
        let winter_death_rate = reduced_death_rate * winter_mortality;

        // Phase R5: Removed parallel fear-flight emigration from labor.rs.
        // Safety-index-driven emigration is now handled exclusively by the
        // dedicated migration system in migration.rs (calculate_migration_pressure
        // includes a safety-index fear factor). This parallel system violated
        // Rule 14 (no redundant parallel systems) and Rule 19 (no teleportation)
        // by reducing population without transport capacity or wealth extraction.
        let migration_rate = demographics.net_migration;

        // Phase 74: Convert annual rates to compound per-turn rates.
        // Birth/death/migration rates are annual fractions that compound over time.
        // Using the annual rate directly per turn caused 24× drift.
        let per_turn_birth_rate = annual_to_per_turn_rate(birth_rate_index);
        let per_turn_death_rate = annual_to_per_turn_rate(winter_death_rate);
        let per_turn_migration_rate =
            annual_to_per_turn_rate(migration_rate.abs()) * migration_rate.signum();

        let births = population * per_turn_birth_rate;
        let natural_deaths = population * per_turn_death_rate;
        let migrants = population * per_turn_migration_rate;
        let population_change = births - natural_deaths - work_deaths + migrants;
        let new_population = (population + population_change).max(1.0).floor() as u64;

        demographics.last_births = births;
        demographics.last_deaths = natural_deaths + work_deaths;
        demographics.last_migration = migrants;
        demographics.population_size = new_population as f64;

        // Gender update.
        let male_population =
            (population * demographics.gender.male) - (work_deaths * 0.90) + (births * 0.505);
        let female_population =
            (population * demographics.gender.female) - (work_deaths * 0.10) + (births * 0.495);
        let new_total_population = (male_population + female_population).max(1.0);
        demographics.gender.male = male_population / new_total_population;
        demographics.gender.female = female_population / new_total_population;

        // Immigrant cohorts.
        if migrants > 0.0 {
            demographics.immigrant_cohorts.push(ImmigrantCohort {
                count: migrants,
                seniority: 0,
                legal_status: crate::economy::legal_status::LegalStatus::TemporaryWorker,
                remittance_rate: 0.10,
                starting_savings: 0.0,
                extra: Map::new(),
            });
        }

        let mut death_emigration_factor = 1.0 - reduced_death_rate;
        if migrants < 0.0 {
            death_emigration_factor -= migrants.abs() / population;
        }
        death_emigration_factor = death_emigration_factor.clamp(0.0, 1.0);

        let mut active_immigrants = 0.0;

        for k in &mut demographics.immigrant_cohorts {
            k.count *= death_emigration_factor;

            let rate = if civil_law == "segregation" {
                let mut r = 0.50;
                if k.seniority > 10 {
                    r = (0.50 - ((k.seniority - 10) as f64 * 0.01)).max(0.40);
                }
                r
            } else if civil_law == "10_year_assimilation" {
                let mut r = 0.40;
                if k.seniority > 10 {
                    r = (0.40 - ((k.seniority - 10) as f64 * 0.08)).max(0.0);
                }
                r
            } else {
                let mut r = 0.30;
                if k.seniority > 5 {
                    r = (0.30 - ((k.seniority - 5) as f64 * 0.06)).max(0.0);
                }
                r
            };

            if rate > 0.0 {
                active_immigrants += k.count;
            }

            k.seniority += 1;
        }

        let max_seniority = if civil_law == "segregation" {
            u32::MAX
        } else if civil_law == "10_year_assimilation" {
            20
        } else {
            15
        };
        demographics
            .immigrant_cohorts
            .retain(|k| k.count > 10.0 && k.seniority <= max_seniority);
        demographics.unassimilated_immigrants = active_immigrants;

        // Age groups — v4.24.0 WS2 cohort-component model.
        // Sixteen five-year bands ([0–4]…[70–74], open [75+]) age per turn:
        // 1/120 of each band graduates (Δt = 1/24 yr ÷ 5-yr band), deaths
        // follow life-table hazards scaled so the realized total equals
        // the authoritative natural_deaths + work_deaths count above, and
        // births enter band 0 while net migrants join prime working ages.
        // This replaces the legacy 3-bucket update, which applied ANNUAL
        // birth/death/migration rates every engine turn — ~24× too fast —
        // producing a phantom mass die-off and a bottom-heavy triangle.
        use crate::society::cultures::{
            aggregate_three_cohorts, cohort_age_stats, cohort_band_hazards,
            solve_cohort_death_scale, stable_age_cohorts, ADULT_BAND_START,
            AGE_COHORT_BANDS, AGE_GRADUATION_PER_TURN, ELDERLY_BAND_START,
            MIGRANT_AGE_WEIGHTS, TURN_YEARS,
        };

        // Reseed legacy/empty cohort state from the stable-population
        // identity so old saves migrate forward deterministically.
        if demographics.age_cohorts.len() != AGE_COHORT_BANDS
            || demographics.age_cohorts.iter().sum::<f64>() <= 0.0
        {
            demographics.age_cohorts = stable_age_cohorts(
                demographics.birth_rate,
                demographics.death_rate,
                life_expectancy,
            )
            .to_vec();
        }

        // Children entering adulthood = survivors graduating out of the
        // 10–14 band into the 15–19 band this turn.
        let children_entering_adulthood_count = demographics.age_cohorts
            [ADULT_BAND_START - 1]
            * AGE_GRADUATION_PER_TURN
            * population;
        let inborn_working_disabled = children_entering_adulthood_count * 0.0010;
        let inborn_unable_to_work = children_entering_adulthood_count * 0.0005;

        labor_market.active_disabled = (labor_market.active_disabled
            * (1.0 - per_turn_death_rate))
            + inborn_working_disabled;
        labor_market.unable_to_work = (labor_market.unable_to_work
            * (1.0 - per_turn_death_rate))
            + inborn_unable_to_work;

        let total_death_share = (natural_deaths + work_deaths) / population.max(1.0);
        let mut band_hazards = cohort_band_hazards(life_expectancy);
        // Healthy-life-expectancy relief: when the healthy-years horizon
        // approaches total lifespan, midlife mortality attenuates toward
        // the senescent regime (deaths concentrate in childhood + old
        // age) — the same pipeline the legacy productive_period_years
        // term approximated.
        let hle_relief =
            (healthy_life_expectancy / life_expectancy.max(1.0)).clamp(0.5, 1.0);
        for hazard in band_hazards
            .iter_mut()
            .take(ELDERLY_BAND_START)
            .skip(ADULT_BAND_START)
        {
            *hazard *= hle_relief;
        }
        let death_scale = solve_cohort_death_scale(
            &demographics.age_cohorts,
            &band_hazards,
            total_death_share,
        );

        let mut next = [0.0_f64; AGE_COHORT_BANDS];
        for i in 0..AGE_COHORT_BANDS {
            let survive = (-death_scale * band_hazards[i] * TURN_YEARS).exp();
            let survivors = demographics.age_cohorts[i] * survive;
            next[i] += survivors * (1.0 - AGE_GRADUATION_PER_TURN);
            if i + 1 < AGE_COHORT_BANDS {
                next[i + 1] += survivors * AGE_GRADUATION_PER_TURN;
            } else {
                // Open 75+ band graduates in place.
                next[i] += survivors * AGE_GRADUATION_PER_TURN;
            }
        }
        // Births enter the youngest band; net cross-border migration is
        // distributed over prime working ages (emigration removes the
        // same profile, floored at zero).
        next[0] += births / population.max(1.0);
        let migrant_share = migrants / population.max(1.0);
        if migrant_share != 0.0 {
            for (offset, weight) in MIGRANT_AGE_WEIGHTS.iter().enumerate() {
                let band = ADULT_BAND_START + offset;
                next[band] = (next[band] + migrant_share * weight).max(0.0);
            }
        }
        let next_total: f64 = next.iter().sum();
        if next_total > 0.0 {
            for band in &mut next {
                *band /= next_total;
            }
            demographics.age_cohorts.copy_from_slice(&next);
        }

        // Re-derive the legacy three-bucket aggregate and headline ages
        // from the real cohort state — no more free parameters.
        let (children, adults, elderly) = aggregate_three_cohorts(&demographics.age_cohorts);
        demographics.age_groups.children = children;
        demographics.age_groups.adults = adults;
        demographics.age_groups.elderly = elderly;
        let (average_age, median_age) = cohort_age_stats(&demographics.age_cohorts);
        demographics.average_age = average_age;
        demographics.median_age = median_age;

        // v4.24.0 WS2: write back the urbanization split derived from
        // real class populations (computed above). Nomads are a residual
        // mobile population that settles as the urban share advances —
        // their share decays by the realized urban growth each turn.
        let urban_growth = (urban_share - demographics.city_urban).max(0.0);
        demographics.city_urban = urban_share;
        demographics.rural = rural_share;
        demographics.nomads *= (1.0 - urban_growth).clamp(0.0, 1.0);

        // Ethnic assimilation is now handled by Phase 17B process_assimilation_turn
        // in economy/assimilation.rs, which uses dual-channel coverage
        // (education + Integration Centers) instead of a magic timer.
        // The old placeholder code has been removed.

        // Labor supply and wages.
        let labor_force = (population * labor_market.labor_force_participation / 100.0).max(1.0);
        let higher_edu_share = demographics.education.higher_share();
        let secondary_edu_share = demographics.education.secondary_share();
        let basic_edu_share = demographics.education.basic;
        let illiterate_share = demographics.education.none;

        // Phase B.2: Secondary education now feeds into the skilled labor tier.
        // Previously, secondary-educated workers were silently dropped from the
        // labor-tier calculation (F8). Now both `basic` and `secondary` map to
        // skilled labor, while `higher` maps to expert and `none` to unskilled.
        // The three shares should sum to 1.0 (Rule 20: conservation).
        let expert_share = higher_edu_share;
        let skilled_share = basic_edu_share + secondary_edu_share;
        let unskilled_share = illiterate_share;

        let (mod_eksperci, mod_sredni, mod_szeregowi) = emancipation_modifiers(&emancipation_law);

        let eksperci_dostepni = labor_force * expert_share * mod_eksperci;
        let skilled_available = labor_force * skilled_share * mod_sredni;
        let unskilled_available = labor_force * unskilled_share * mod_szeregowi;

        // Wage base.
        // Phase 25: Removed the `subsistence_wage` subsistence floor.
        // Wages are now set purely by labor market clearing (supply and demand).
        // If companies have no money, wages drop to 0, and workers starve,
        // riot, or emigrate. We do not prop up the simulation with phantom floors.
        // `minimum_wage` is a labor-law multiplier on the previous average
        // wage — this is policy (set by Politics/LaborLaw), not a floor.
        // Phase 25 fix: when there is no statutory minimum wage (minimum_wage_multiplier = 0),
        // the base wage is the previous average wage (market-clearing reference).
        // The statutory minimum only applies when explicitly set by labor law.
        let statutory_multiplier = minimum_wage_multiplier(&labor_law);
        let base_wage = if statutory_multiplier > 0.0 {
            prev_avg * statutory_multiplier
        } else {
            prev_avg
        };

        // Unemployment structure.
        let unemployed = (labor_force - labor_market.employed_total).max(0.0);
        let mut raw_unemployment_rate = (unemployed / labor_force) * 100.0;
        if job_agency_active {
            raw_unemployment_rate = (raw_unemployment_rate - 2.0).max(0.0);
        }
        let frictional_unemployment_floor = if job_agency_active { 1.5 } else { 3.0 };
        let unemployment_rate = raw_unemployment_rate.max(frictional_unemployment_floor);
        let excess_unemployment = (unemployment_rate - frictional_unemployment_floor).max(0.0);

        labor_market.unemployment_rate = unemployment_rate;
        labor_market.unemployment_structure.friction = frictional_unemployment_floor / 100.0;
        labor_market.unemployment_structure.cyclical = (excess_unemployment * 0.6) / 100.0;
        labor_market.unemployment_structure.structural = (excess_unemployment * 0.4) / 100.0;
        labor_market.poverty_pool_percent = labor_market.unemployment_structure.cyclical * 0.2
            + labor_market.unemployment_structure.structural * 0.3;
        labor_market.unemployed = unemployed;

        // v4.20 WS3: Signed Phillips-curve wage inflation replaces the old
        // one-sided `wage_pressure` friction (which could only shrink or
        // freeze the base wage — combined with the sticky company-offer
        // targets it produced an automatic +1–2%/turn ratchet regardless of
        // labor-market conditions or the price level).
        //
        //   wage_inflation_turn = E[π]_turn − κ × slack_fraction
        //
        // * E[π]_turn = adaptive expected per-turn CPI inflation (EWMA of
        //   realized `cpi_inflation` samples) — full 1.0 pass-through, the
        //   expectations-augmented Phillips curve. Positive expected
        //   inflation lifts nominal wages; expected deflation drags them.
        // * slack = raw_unemployment_rate − frictional_floor, SIGNED and in
        //   percent: positive slack (weak labor market) cuts wages at
        //   κ = 0.15 per pp (≈ −0.15%/turn each, ~−3.5%/yr); negative slack
        //   (labor tighter than the frictional norm — possible because the
        //   raw rate is measured BEFORE the floor clamp) lifts them.
        //
        // Consequences: wages rise ONLY when the labor market is tight or
        // inflation is genuinely positive; they are flat-to-negative in a
        // slack, deflationary economy — ending the automatic ratchet.
        let expected_inflation_pct = expected_inflation_ewma(
            &macro_indicators.telemetry_history,
            macro_indicators.inflation,
        );
        let slack_pp = raw_unemployment_rate - frictional_unemployment_floor;
        // v4.21 WS2: Value-added wage norm. The Phillips term handles the
        // labor-market CYCLE; it cannot fix the LEVEL — the reference wage
        // is otherwise a unit-root process (w_{t+1} = w_t · (1+π−κ·slack)),
        // so with persistent slack the labor share of value added erodes
        // without bound while nominal GDP keeps inflating. Real economies
        // anchor trend wages via collective-bargaining/statutory norms: a
        // labor-law-dependent share of VA per worker. We add a slow signed
        // pull toward that norm, measured on the realized bill
        // (prev_avg × employed vs official GDP), still bounded by the
        // ±WAGE_INFLATION_TURN_CAP below.
        let wage_norm_adjust = {
            let gdp_prev = macro_indicators.gdp_breakdown.official_gdp;
            // Realized employment carried over from last turn's clearing
            // write-back (employed_total is overwritten below); fall back
            // to the labor-force estimate before the first clearing.
            let realized_fte = labor_market.employed_total;
            let employed_est = if realized_fte > 0.0 {
                realized_fte
            } else {
                labor_force * (1.0 - (unemployment_rate / 100.0).clamp(0.0, 1.0))
            };
            if gdp_prev > 0.0 && employed_est > 0.0 && prev_avg > 0.0 {
                let implied_share = (prev_avg * employed_est) / gdp_prev;
                let target_share = wage_share_target(&labor_law);
                (WAGE_NORM_GAIN * (target_share - implied_share))
                    .clamp(-WAGE_NORM_TURN_CAP, WAGE_NORM_TURN_CAP)
            } else {
                0.0
            }
        };
        let wage_inflation_turn = (expected_inflation_pct / 100.0
            - PHILLIPS_KAPPA * (slack_pp / 100.0)
            + wage_norm_adjust)
        .clamp(-WAGE_INFLATION_TURN_CAP, WAGE_INFLATION_TURN_CAP);
        let adjusted_base_wage = base_wage * (1.0 + wage_inflation_turn).max(0.0);

        let brain_drain = demographics.brain_drain_index;
        let expert_premium = (3.0 + brain_drain * 5.0) * (1.0 + (0.2 - expert_share).max(0.0));
        let skilled_premium = 1.5 + brain_drain * 2.0;

        let expert_wage = adjusted_base_wage * expert_premium;
        let skilled_wage = adjusted_base_wage * skilled_premium;
        let unskilled_wage = adjusted_base_wage;

        let employment_factor = 1.0 - (unemployment_rate / 100.0);

        labor_market.expert_tier.supply = eksperci_dostepni;
        labor_market.expert_tier.wage = expert_wage;
        labor_market.expert_tier.employed = eksperci_dostepni * employment_factor;
        labor_market.expert_tier.unemployed = eksperci_dostepni - labor_market.expert_tier.employed;

        labor_market.skilled_tier.supply = skilled_available;
        labor_market.skilled_tier.wage = skilled_wage;
        labor_market.skilled_tier.employed = skilled_available * employment_factor;
        labor_market.skilled_tier.unemployed =
            skilled_available - labor_market.skilled_tier.employed;

        labor_market.unskilled_tier.supply = unskilled_available;
        labor_market.unskilled_tier.wage = unskilled_wage;
        labor_market.unskilled_tier.employed = unskilled_available * employment_factor;
        labor_market.unskilled_tier.unemployed =
            unskilled_available - labor_market.unskilled_tier.employed;

        let total_employed = labor_market.expert_tier.employed
            + labor_market.skilled_tier.employed
            + labor_market.unskilled_tier.employed;
        labor_market.employed_total = total_employed;

        // Recompute the headline unemployment figures from the actual tier
        // employment so that `employed_total`, `unemployed` and `unemployment_rate`
        // are mutually consistent at the end of the turn.
        let actual_unemployed = (labor_force - total_employed).max(0.0);
        let mut actual_unemployment_rate = (actual_unemployed / labor_force) * 100.0;
        if job_agency_active {
            actual_unemployment_rate = (actual_unemployment_rate - 2.0).max(0.0);
        }
        let actual_unemployment_rate = actual_unemployment_rate.max(frictional_unemployment_floor);
        let actual_excess_unemployment =
            (actual_unemployment_rate - frictional_unemployment_floor).max(0.0);

        labor_market.unemployed = actual_unemployed;
        labor_market.unemployment_rate = actual_unemployment_rate;
        labor_market.unemployment_structure.friction = frictional_unemployment_floor / 100.0;
        labor_market.unemployment_structure.cyclical = (actual_excess_unemployment * 0.6) / 100.0;
        labor_market.unemployment_structure.structural = (actual_excess_unemployment * 0.4) / 100.0;
        labor_market.poverty_pool_percent = labor_market.unemployment_structure.cyclical * 0.2
            + labor_market.unemployment_structure.structural * 0.3;

        let new_avg = if total_employed > 0.0 {
            let expert_wage_fund = labor_market.expert_tier.employed * expert_wage;
            let skilled_wage_fund = labor_market.skilled_tier.employed * skilled_wage;
            let unskilled_wage_fund = labor_market.unskilled_tier.employed * unskilled_wage;
            (expert_wage_fund + skilled_wage_fund + unskilled_wage_fund) / total_employed
        } else {
            // Phase 25: No artificial wage floor. If nobody is employed,
            // the average wage is 0. Workers with no income starve, riot,
            // or emigrate. We do not prop up the simulation with phantom wages.
            0.0
        };

        (new_avg, new_population)
    };

    macro_indicators.average_wage = new_avg;
    let old_population = budget.population;

    // Phase 36: STRICT BOTTOM-UP POPULATION AGGREGATION.
    //
    // The demographic model computes a national-level population delta from
    // births, deaths, and migration. We distribute this delta proportionally
    // to class demographics (the authoritative source), then derive region
    // and national totals as strict bottom-up sums.
    //
    // INVARIANT: budget.population == sum(region.population)
    //            == sum(class.population across all regions)
    //
    // No top-down write to budget.population is authoritative. The national
    // total is ALWAYS the exact sum of all class demographics.
    let pop_delta = new_population as i64 - old_population as i64;

    if pop_delta != 0 {
        // Compute total class population across all regions for proportional distribution
        let total_class_pop: i64 = country
            .regions
            .iter()
            .flat_map(|r| {
                r.class_demographics
                    .rural_classes
                    .values()
                    .chain(r.class_demographics.urban_classes.values())
            })
            .map(|d| d.population)
            .sum();

        if total_class_pop > 0 {
            let total_class_pop_f = total_class_pop as f64;
            let mut distributed: i64 = 0;
            for region in country.regions.iter_mut() {
                // Compute this region's total class population for share calculation
                let region_class_pop: i64 = region
                    .class_demographics
                    .rural_classes
                    .values()
                    .chain(region.class_demographics.urban_classes.values())
                    .map(|d| d.population)
                    .sum();
                if region_class_pop == 0 {
                    continue;
                }
                let region_share = region_class_pop as f64 / total_class_pop_f;
                let region_delta = (pop_delta as f64 * region_share).round() as i64;
                distributed += region_delta;

                // Distribute region_delta proportionally across all classes in this region
                let all_classes_count = region.class_demographics.rural_classes.len()
                    + region.class_demographics.urban_classes.len();
                if all_classes_count == 0 {
                    continue;
                }

                // Distribute to rural classes proportionally by population
                let rural_pop: i64 = region
                    .class_demographics
                    .rural_classes
                    .values()
                    .map(|d| d.population)
                    .sum();
                let urban_pop: i64 = region
                    .class_demographics
                    .urban_classes
                    .values()
                    .map(|d| d.population)
                    .sum();
                let total_pop = rural_pop + urban_pop;
                if total_pop == 0 {
                    // Equal distribution if no population data
                    let per_class = region_delta / all_classes_count as i64;
                    let remainder = region_delta - per_class * all_classes_count as i64;
                    let mut applied = 0i64;
                    for (i, demo) in region
                        .class_demographics
                        .rural_classes
                        .values_mut()
                        .enumerate()
                    {
                        let extra = if i == 0 { remainder } else { 0 };
                        demo.population = (demo.population + per_class + extra).max(0);
                        applied += per_class + extra;
                    }
                    for demo in region.class_demographics.urban_classes.values_mut() {
                        demo.population = (demo.population + per_class).max(0);
                        applied += per_class;
                    }
                    // Any residue goes to first rural class
                    let residue = region_delta - applied;
                    if residue != 0 {
                        if let Some(first) =
                            region.class_demographics.rural_classes.values_mut().next()
                        {
                            first.population = (first.population + residue).max(0);
                        }
                    }
                } else {
                    // Proportional distribution by population share
                    let rural_delta =
                        (region_delta as f64 * rural_pop as f64 / total_pop as f64).round() as i64;
                    let urban_delta = region_delta - rural_delta;

                    // Distribute rural_delta across rural classes
                    if rural_pop > 0 && !region.class_demographics.rural_classes.is_empty() {
                        let mut rural_distributed: i64 = 0;
                        let rural_classes: Vec<RuralClass> = region
                            .class_demographics
                            .rural_classes
                            .keys()
                            .cloned()
                            .collect();
                        for key in rural_classes.iter() {
                            if let Some(demo) = region.class_demographics.rural_classes.get_mut(key)
                            {
                                let share = demo.population as f64 / rural_pop as f64;
                                let delta = (rural_delta as f64 * share).round() as i64;
                                demo.population = (demo.population + delta).max(0);
                                rural_distributed += delta;
                            }
                        }
                        // Fix rounding residue on last rural class
                        let residue = rural_delta - rural_distributed;
                        if residue != 0 {
                            if let Some(key) = rural_classes.last() {
                                if let Some(demo) =
                                    region.class_demographics.rural_classes.get_mut(key)
                                {
                                    demo.population = (demo.population + residue).max(0);
                                }
                            }
                        }
                    }

                    // Distribute urban_delta across urban classes
                    if urban_pop > 0 && !region.class_demographics.urban_classes.is_empty() {
                        let mut urban_distributed: i64 = 0;
                        let urban_classes: Vec<UrbanClass> = region
                            .class_demographics
                            .urban_classes
                            .keys()
                            .cloned()
                            .collect();
                        for key in urban_classes.iter() {
                            if let Some(demo) = region.class_demographics.urban_classes.get_mut(key)
                            {
                                let share = demo.population as f64 / urban_pop as f64;
                                let delta = (urban_delta as f64 * share).round() as i64;
                                demo.population = (demo.population + delta).max(0);
                                urban_distributed += delta;
                            }
                        }
                        // Fix rounding residue on last urban class
                        let residue = urban_delta - urban_distributed;
                        if residue != 0 {
                            if let Some(key) = urban_classes.last() {
                                if let Some(demo) =
                                    region.class_demographics.urban_classes.get_mut(key)
                                {
                                    demo.population = (demo.population + residue).max(0);
                                }
                            }
                        }
                    }
                }
            }
            // Fix any global rounding residue on the last region's last class
            let residue = pop_delta - distributed;
            if residue != 0 {
                if let Some(last_region) = country.regions.last_mut() {
                    if let Some(last_demo) = last_region
                        .class_demographics
                        .rural_classes
                        .values_mut()
                        .next()
                    {
                        last_demo.population = (last_demo.population + residue).max(0);
                    } else if let Some(last_demo) = last_region
                        .class_demographics
                        .urban_classes
                        .values_mut()
                        .next()
                    {
                        last_demo.population = (last_demo.population + residue).max(0);
                    }
                }
            }
        }
    }

    // Phase 36: STRICT BOTTOM-UP RECONCILIATION.
    // region.population = sum(rural_classes.population) + sum(urban_classes.population)
    // budget.population = sum(region.population)
    // This runs unconditionally every turn to guarantee the invariant.
    for region in &mut country.regions {
        let rural_sum: i64 = region
            .class_demographics
            .rural_classes
            .values()
            .map(|d| d.population)
            .sum();
        let urban_sum: i64 = region
            .class_demographics
            .urban_classes
            .values()
            .map(|d| d.population)
            .sum();
        region.population = rural_sum + urban_sum;
    }
    let total_pop: u64 = country
        .regions
        .iter()
        .map(|r| r.population)
        .filter(|p| *p > 0)
        .sum::<i64>() as u64;
    budget.population = total_pop;
    macro_indicators.demographics.population_size = total_pop as f64;

    // Phase 25: Compute available_fte for each class demographic from
    // population and labor force participation. This is the critical fix —
    // without it, available_fte defaults to 0.0 and the labor market clearing
    // sees an empty labor pool, causing 100% unemployment regardless of wages.
    // Formula: available_fte = population × labor_participation
    // (capped at 1.5 × population for full-time + half-time secondary jobs)
    for region in &mut country.regions {
        for demo in region.class_demographics.rural_classes.values_mut() {
            let pop = demo.population as f64;
            let participation = demo.labor_participation.max(0.0).min(1.0);
            demo.available_fte = (pop * participation).min(pop * 1.5);
        }
        for demo in region.class_demographics.urban_classes.values_mut() {
            let pop = demo.population as f64;
            let participation = demo.labor_participation.max(0.0).min(1.0);
            demo.available_fte = (pop * participation).min(pop * 1.5);
        }
    }

    // Phase 8: Reset winter mortality multiplier on all regions (penalty doesn't persist into Spring)
    for region in &mut country.regions {
        region.winter_mortality_multiplier = 1.0;
    }

    // Aggregate regional class savings into national citizen_savings
    aggregate_citizen_savings(country);
}

/// Aggregates regional class savings into the national citizen_savings total.
///
/// This function fixes the "frozen citizen savings" bug by ensuring that
/// regional class savings (updated in society/geography.rs) are properly
/// aggregated into the national budget.citizen_savings field each turn.
///
/// # Arguments
/// * `country` - Mutable country state to update citizen_savings
pub fn aggregate_citizen_savings(country: &mut crate::state::Country) {
    // Phase 43: Include BOTH rural and urban class savings.
    // The previous version only summed rural_classes, ignoring urban_classes
    // (Workers, Bourgeoisie), which caused citizen_savings to be severely
    // understated and PIT collection (capped by citizen_savings) to collect ~0.
    let total: f64 = country
        .regions
        .iter()
        .flat_map(|r| {
            r.class_demographics
                .rural_classes
                .values()
                .chain(r.class_demographics.urban_classes.values())
        })
        .map(|d| d.savings)
        .sum();
    country.budget.citizen_savings = total;
}

/// Phase 36: Strict bottom-up population reconciliation.
///
/// Recomputes `region.population` as the exact sum of all class demographics
/// within the region, then recomputes `budget.population` as the exact sum of
/// all region populations. Also updates `demographics.population_size`.
///
/// This function should be called after ANY code that mutates class demographic
/// populations (migration, casualties, births, etc.) to guarantee the invariant:
///
///   budget.population == sum(region.population) == sum(class.population)
///
/// # Arguments
/// * `country` - Mutable country state to reconcile
pub fn reconcile_population_bottom_up(country: &mut crate::state::Country) {
    for region in &mut country.regions {
        let rural_sum: i64 = region
            .class_demographics
            .rural_classes
            .values()
            .map(|d| d.population)
            .sum();
        let urban_sum: i64 = region
            .class_demographics
            .urban_classes
            .values()
            .map(|d| d.population)
            .sum();
        region.population = rural_sum + urban_sum;
    }
    let total_pop: u64 = country
        .regions
        .iter()
        .map(|r| r.population)
        .filter(|p| *p > 0)
        .sum::<i64>() as u64;
    country.budget.population = total_pop;
    country.macro_indicators.demographics.population_size = total_pop as f64;
}

/// Phase 36: Distribute a population delta to a country's class demographics
/// proportionally, then reconcile bottom-up.
///
/// Used by migration and other systems that need to add/remove population at
/// the national level while maintaining the bottom-up invariant.
///
/// # Arguments
/// * `country` - Mutable country state
/// * `delta` - Population change (positive = growth, negative = decline)
pub fn distribute_population_delta_and_reconcile(country: &mut crate::state::Country, delta: i64) {
    if delta == 0 {
        reconcile_population_bottom_up(country);
        return;
    }

    let total_class_pop: i64 = country
        .regions
        .iter()
        .flat_map(|r| {
            r.class_demographics
                .rural_classes
                .values()
                .chain(r.class_demographics.urban_classes.values())
        })
        .map(|d| d.population)
        .sum();

    if total_class_pop <= 0 {
        // No existing class population — add to first available class
        for region in &mut country.regions {
            if let Some(demo) = region.class_demographics.rural_classes.values_mut().next() {
                demo.population = (demo.population + delta).max(0);
                break;
            }
            if let Some(demo) = region.class_demographics.urban_classes.values_mut().next() {
                demo.population = (demo.population + delta).max(0);
                break;
            }
        }
        reconcile_population_bottom_up(country);
        return;
    }

    let total_class_pop_f = total_class_pop as f64;
    let mut distributed: i64 = 0;
    for region in &mut country.regions {
        let region_class_pop: i64 = region
            .class_demographics
            .rural_classes
            .values()
            .chain(region.class_demographics.urban_classes.values())
            .map(|d| d.population)
            .sum();
        if region_class_pop == 0 {
            continue;
        }
        let region_share = region_class_pop as f64 / total_class_pop_f;
        let region_delta = (delta as f64 * region_share).round() as i64;
        distributed += region_delta;

        // Distribute region_delta proportionally across all classes by population
        // E.6.3: Typed keys — iterate rural and urban separately to avoid
        // incompatible BTreeMap key types in chained iterators.
        let total_pop: i64 = region_class_pop;
        let mut region_distributed: i64 = 0;
        let rural_keys: Vec<RuralClass> =
            region.class_demographics.rural_classes.keys().cloned().collect();
        let urban_keys: Vec<UrbanClass> =
            region.class_demographics.urban_classes.keys().cloned().collect();
        for key in &rural_keys {
            if let Some(demo) = region.class_demographics.rural_classes.get_mut(key) {
                let share = demo.population as f64 / total_pop as f64;
                let class_delta = (region_delta as f64 * share).round() as i64;
                demo.population = (demo.population + class_delta).max(0);
                region_distributed += class_delta;
            }
        }
        for key in &urban_keys {
            if let Some(demo) = region.class_demographics.urban_classes.get_mut(key) {
                let share = demo.population as f64 / total_pop as f64;
                let class_delta = (region_delta as f64 * share).round() as i64;
                demo.population = (demo.population + class_delta).max(0);
                region_distributed += class_delta;
            }
        }
        // Fix rounding residue on last class (try urban last, then rural)
        let residue = region_delta - region_distributed;
        if residue != 0 {
            if let Some(key) = urban_keys.last() {
                if let Some(demo) = region.class_demographics.urban_classes.get_mut(key) {
                    demo.population = (demo.population + residue).max(0);
                }
            } else if let Some(key) = rural_keys.last() {
                if let Some(demo) = region.class_demographics.rural_classes.get_mut(key) {
                    demo.population = (demo.population + residue).max(0);
                }
            }
        }
    }
    // Fix global rounding residue
    let residue = delta - distributed;
    if residue != 0 {
        for region in &mut country.regions {
            if let Some(demo) = region.class_demographics.rural_classes.values_mut().next() {
                demo.population = (demo.population + residue).max(0);
                break;
            }
            if let Some(demo) = region.class_demographics.urban_classes.values_mut().next() {
                demo.population = (demo.population + residue).max(0);
                break;
            }
        }
    }
    reconcile_population_bottom_up(country);
}

#[cfg(test)]
mod tests {
    use crate::economy::labor::labor_config::LaborConfig;
    use crate::state::macro_data::HealthStatistics;

    #[test]
    fn life_expectancy_increases_with_sports_access() {
        // Phase 18S: Life expectancy must include a per-capita sports access
        // term (physical access, not demographic abstraction).
        let config = LaborConfig::default();
        let sports_bonus = config.sports_health_bonus_per_capacity_unit;

        // Base life expectancy (no sports access)
        let base_le = 60.0 + 50.0 * 0.20 + (70.0 / 100.0) * 15.0;
        // With full sports access (coverage = 1.0)
        let full_le = base_le + 1.0 * sports_bonus;
        // With partial sports access (coverage = 0.5)
        let half_le = base_le + 0.5 * sports_bonus;

        assert!(
            full_le > base_le,
            "Life expectancy must increase with full sports access"
        );
        assert!(
            half_le > base_le,
            "Life expectancy must increase with partial sports access"
        );
        assert!(
            full_le > half_le,
            "Full sports access must yield higher life expectancy than partial"
        );
        assert!(
            sports_bonus > 0.0,
            "Sports health bonus config must be positive"
        );
    }

    #[test]
    fn healthy_life_expectancy_increases_with_sports_access() {
        let config = LaborConfig::default();
        let sports_bonus = config.sports_healthy_life_bonus_per_capacity_unit;

        let base_hle = 50.0 + 50.0 * 0.15 + (70.0 / 100.0) * 10.0;
        let full_hle = base_hle + 1.0 * sports_bonus;

        assert!(
            full_hle > base_hle,
            "Healthy life expectancy must increase with sports access"
        );
        assert!(
            sports_bonus > 0.0,
            "Sports healthy life bonus config must be positive"
        );
    }

    #[test]
    fn sports_capacity_per_capita_field_exists() {
        // Verify the HealthStatistics struct has the sports_capacity_per_capita field.
        let mut hs = HealthStatistics::default();
        hs.sports_capacity_per_capita = 0.5;
        assert_eq!(hs.sports_capacity_per_capita, 0.5);
    }
}

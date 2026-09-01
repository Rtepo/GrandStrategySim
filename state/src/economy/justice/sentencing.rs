//! Dynamic sentencing, legal dualism, and institutional checks (Phase 18B).
//!
//! This module implements:
//! - Crime category determination and sentence generation based on SentencingLaw.
//! - Legal dualism: harsher sentences for minorities (Resident, Illegal, or
//!   non-dominant religion).
//! - Death penalty execution: permanent population removal.
//! - Community service: source-level wage garnishment (handled in labor_market.rs).
//! - Administrative courts: block illegal state actions.
//! - Ombudsman (Ombudsman): detect rights violations and generate unrest.
//! - Vigilante justice: summary executions in low-capacity regions.

use crate::economy::disasters::{DisasterEvent, DisasterType};
use crate::economy::legal_status::LegalStatus;
use crate::politics::system::JusticeSystemState;
use crate::state::Country;
use serde::{Deserialize, Serialize};

/// Crime severity category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CrimeCategory {
    #[default]
    /// Minor offense, typically resulting in community service or short imprisonment.
    Misdemeanor,
    /// Serious crime requiring imprisonment.
    Felony,
    /// Most severe category, eligible for death penalty or life imprisonment.
    Capital,
}

/// Sentence outcome for a prisoner cohort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SentenceOutcome {
    /// Imprisonment for a fixed number of turns.
    Imprisonment(u32),
    /// Life imprisonment — sentence never expires.
    LifeImprisonment,
    /// Death penalty — permanent population removal on execution.
    DeathPenalty,
    /// Community service for a number of turns — wage garnishment at source.
    CommunityService(u32),
    /// Acquitted — released immediately.
    Acquittal,
}

impl Default for SentenceOutcome {
    fn default() -> Self {
        SentenceOutcome::Imprisonment(0)
    }
}

/// Sentencing law configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SentencingLaw {
    /// Sentence range in months (turns) for misdemeanors.
    #[serde(default = "default_misdemeanor_range")]
    pub misdemeanor_range_months: (u32, u32),
    /// Sentence range in months (turns) for felonies.
    #[serde(default = "default_felony_range")]
    pub felony_range_months: (u32, u32),
    /// Available capital punishment methods (e.g., "hanging", "firing_squad").
    #[serde(default)]
    pub capital_methods: Vec<String>,
    /// Whether the death penalty is enabled.
    #[serde(default)]
    pub death_penalty_enabled: bool,
    /// Whether life imprisonment is enabled.
    #[serde(default)]
    pub life_imprisonment_enabled: bool,
    /// Whether community service is enabled for misdemeanors.
    #[serde(default = "default_true")]
    pub community_service_enabled: bool,
    /// Whether legal dualism (harsher sentences for minorities) is enabled.
    #[serde(default)]
    pub legal_dualism_enabled: bool,
    /// Sentence length multiplier for minorities (default 2.0).
    #[serde(default = "default_minority_multiplier")]
    pub minority_sentence_multiplier: f64,
    /// Sentence length multiplier for residents (default 1.5).
    #[serde(default = "default_resident_multiplier")]
    pub resident_sentence_multiplier: f64,
    /// Sentence length multiplier for illegals (default 3.0).
    #[serde(default = "default_illegal_multiplier")]
    pub illegal_sentence_multiplier: f64,
}

fn default_misdemeanor_range() -> (u32, u32) {
    (1, 6)
}
fn default_felony_range() -> (u32, u32) {
    (5, 20)
}
fn default_true() -> bool {
    true
}
fn default_minority_multiplier() -> f64 {
    2.0
}
fn default_resident_multiplier() -> f64 {
    1.5
}
fn default_illegal_multiplier() -> f64 {
    3.0
}

impl Default for SentencingLaw {
    fn default() -> Self {
        SentencingLaw {
            misdemeanor_range_months: default_misdemeanor_range(),
            felony_range_months: default_felony_range(),
            capital_methods: Vec::new(),
            death_penalty_enabled: false,
            life_imprisonment_enabled: false,
            community_service_enabled: true,
            legal_dualism_enabled: false,
            minority_sentence_multiplier: default_minority_multiplier(),
            resident_sentence_multiplier: default_resident_multiplier(),
            illegal_sentence_multiplier: default_illegal_multiplier(),
        }
    }
}

/// Administrative court state — can block illegal state actions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AdministrativeCourtState {
    /// Number of state actions blocked this turn.
    #[serde(default)]
    pub blocked_state_actions: u32,
    /// Pending administrative reviews.
    #[serde(default)]
    pub pending_reviews: u32,
    /// Total rulings overturned historically.
    #[serde(default)]
    pub rulings_overturned: u32,
}

/// Ombudsman (Ombudsman) state — monitors rights violations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct OmbudsmanState {
    /// Active complaints currently under investigation.
    #[serde(default)]
    pub active_complaints: u32,
    /// Total scandals generated (accumulates).
    #[serde(default)]
    pub scandals_generated: f64,
    /// Rights violations detected this turn.
    #[serde(default)]
    pub rights_violations_detected: u32,
    /// Unrest generated this turn from violations.
    #[serde(default)]
    pub unrest_generated: f64,
}

/// Result of processing the ombudsman turn.
#[derive(Debug, Clone, Default)]
pub struct OmbudsmanTurnResult {
    /// Rights violations detected.
    pub violations_detected: u32,
    /// Unrest added to the country.
    pub unrest_added: f64,
    /// Scandals generated.
    pub scandals_generated: f64,
}

/// Result of vigilante justice processing.
#[derive(Debug, Clone, Default)]
pub struct VigilanteJusticeResult {
    /// Disaster events created.
    pub events: Vec<DisasterEvent>,
    /// Total casualties (summary executions).
    pub total_casualties: i64,
    /// Total FTE disabled (mutilations).
    pub total_disabled_fte: f64,
    /// Unrest added.
    pub unrest_added: f64,
}

/// Determine crime category from coverage gap and radical fraction.
///
/// # Arguments
/// * `coverage_gap` - 1.0 - justice_coverage (0.0 = full coverage, 1.0 = no coverage).
/// * `radical_fraction` - Fraction of radicals in the class (0.0–1.0).
///
/// # Returns
/// Crime category: high gap + high radicals → Capital, medium → Felony, low → Misdemeanor.
pub fn determine_crime_category(coverage_gap: f64, radical_fraction: f64) -> CrimeCategory {
    let severity_score = coverage_gap * 0.5 + radical_fraction * 0.5;
    if severity_score > 0.7 {
        CrimeCategory::Capital
    } else if severity_score > 0.3 {
        CrimeCategory::Felony
    } else {
        CrimeCategory::Misdemeanor
    }
}

/// Generate a sentence outcome based on crime category and sentencing law.
///
/// # Arguments
/// * `category` - Crime severity category.
/// * `law` - Sentencing law configuration.
/// * `legal_status` - Legal status of the prisoner (for dualism).
/// * `is_minority_religion` - Whether the prisoner's religion differs from dominant.
/// * `rng_val` - Random value in [0.0, 1.0) for sentence length randomization.
///
/// # Returns
/// Tuple of (SentenceOutcome, sentence_length_in_turns).
pub fn generate_sentence(
    category: CrimeCategory,
    law: &SentencingLaw,
    legal_status: LegalStatus,
    is_minority_religion: bool,
    rng_val: f64,
) -> (SentenceOutcome, u32) {
    // Determine base sentence range from crime category
    let (min_months, max_months) = match category {
        CrimeCategory::Misdemeanor => law.misdemeanor_range_months,
        CrimeCategory::Felony => law.felony_range_months,
        CrimeCategory::Capital => (20, 50), // Capital crimes get long minimums
    };

    // Random sentence length within range
    let range = max_months.saturating_sub(min_months) as f64;
    let base_months = min_months as f64 + rng_val * range;
    let mut sentence_months = base_months;

    // Apply legal dualism multipliers
    if law.legal_dualism_enabled {
        let multiplier = match legal_status {
            LegalStatus::Illegal => law.illegal_sentence_multiplier,
            LegalStatus::Resident => law.resident_sentence_multiplier,
            LegalStatus::TemporaryWorker => law.resident_sentence_multiplier,
            LegalStatus::Citizen => {
                if is_minority_religion {
                    law.minority_sentence_multiplier
                } else {
                    1.0
                }
            }
        };
        sentence_months *= multiplier;

        // Misdemeanor upgrade to Felony for minorities if multiplier pushes it past felony range
        if category == CrimeCategory::Misdemeanor
            && sentence_months > law.felony_range_months.0 as f64
        {
            // The sentence is now in felony territory
        }
    } else if is_minority_religion {
        // Even without explicit dualism, minority religion can get slight increase
        sentence_months *= 1.1;
    }

    let sentence_turns = sentence_months.round() as u32;

    // Determine outcome based on category and law
    let outcome = match category {
        CrimeCategory::Capital if law.death_penalty_enabled => SentenceOutcome::DeathPenalty,
        CrimeCategory::Capital if law.life_imprisonment_enabled => {
            SentenceOutcome::LifeImprisonment
        }
        CrimeCategory::Capital => SentenceOutcome::Imprisonment(sentence_turns),
        CrimeCategory::Felony => SentenceOutcome::Imprisonment(sentence_turns),
        CrimeCategory::Misdemeanor if law.community_service_enabled => {
            // Community service for misdemeanors: shorter, garnishment-based
            SentenceOutcome::CommunityService(sentence_turns)
        }
        CrimeCategory::Misdemeanor => SentenceOutcome::Imprisonment(sentence_turns),
    };

    (outcome, sentence_turns)
}

/// Process death penalty executions in cohort releases.
///
/// Cohorts with `SentenceOutcome::DeathPenalty` are executed:
/// - Removed from prisoner cohorts.
/// - `country.budget.population` is permanently reduced.
/// - Generates unrest spike.
///
/// # Returns
/// (executed_count, unrest_spike)
pub fn process_death_penalties(
    country: &mut Country,
    justice_state: &mut JusticeSystemState,
) -> (i64, f64) {
    let mut executed_total = 0_i64;
    let mut unrest_spike = 0.0_f64;

    let total_pop: f64 = country
        .regions
        .iter()
        .flat_map(|r| {
            r.class_demographics
                .rural_classes
                .values()
                .chain(r.class_demographics.urban_classes.values())
        })
        .map(|c| c.population as f64)
        .sum::<f64>()
        .max(1.0);

    // Partition: death penalty cohorts are removed, others stay
    let mut remaining = Vec::new();
    for cohort in justice_state.prisoner_cohorts.drain(..) {
        if cohort.sentence_outcome == SentenceOutcome::DeathPenalty
            && cohort.sentence_remaining == 0
        {
            executed_total += cohort.count;
            // Unrest spike: executed_count * 10.0 / total_pop * 100.0
            unrest_spike += cohort.count as f64 * 10.0 / total_pop * 100.0;
        } else {
            remaining.push(cohort);
        }
    }
    justice_state.prisoner_cohorts = remaining;

    if executed_total > 0 {
        country.budget.population -= executed_total as u64;
        country.macro_indicators.demographics.population_size = country.budget.population as f64;
        country.macro_indicators.social_unrest += unrest_spike;
    }

    (executed_total, unrest_spike)
}

/// Compute per-class garnishment rates from community service cohorts.
///
/// # Arguments
/// * `justice_state` - Justice state containing prisoner cohorts.
/// * `country` - Country for class population lookup.
///
/// # Returns
/// Map of (region_id, is_urban, class_id) → garnishment_rate (fraction of wages garnished).
pub fn compute_garnishment_rates(
    justice_state: &JusticeSystemState,
    country: &Country,
) -> std::collections::BTreeMap<(String, bool, String), f64> {
    let mut rates = std::collections::BTreeMap::new();

    for cohort in &justice_state.prisoner_cohorts {
        if let SentenceOutcome::CommunityService(_) = cohort.sentence_outcome {
            let key = (
                cohort.origin_region_id.clone(),
                cohort.origin_is_urban,
                cohort.origin_class_id.clone(),
            );
            // Find class population
            let class_pop = country
                .regions
                .iter()
                .find(|r| r.id == cohort.origin_region_id)
                .and_then(|r| {
                    if cohort.origin_is_urban {
                        r.class_demographics
                            .urban_classes
                            .get(&cohort.origin_class_id)
                    } else {
                        r.class_demographics
                            .rural_classes
                            .get(&cohort.origin_class_id)
                    }
                })
                .map(|c| c.population as f64)
                .unwrap_or(1.0)
                .max(1.0);

            // Garnishment rate = convicts / class_population * 0.25 (25% of their wages)
            let rate = (cohort.count as f64 / class_pop) * 0.25;
            *rates.entry(key).or_insert(0.0) += rate;
        }
    }

    rates
}

/// Check if a state action can be executed given administrative court oversight.
///
/// # Arguments
/// * `country` - Country with politics state.
/// * `action_description` - Description of the action (for logging).
///
/// # Returns
/// `true` if the action can proceed, `false` if blocked by administrative court.
pub fn can_execute_state_action(country: &Country, _action_description: &str) -> bool {
    if let Some(ref admin_court) = country.politics.administrative_court {
        // If justice coverage is high and there are pending reviews, the court can block
        let justice_coverage = country
            .politics
            .justice_state
            .as_ref()
            .map(|js| js.justice_coverage)
            .unwrap_or(0.0);

        if justice_coverage > 0.5 && admin_court.pending_reviews > 0 {
            // High justice coverage + pending reviews = court is active and can block
            return false;
        }
    }
    true
}

/// Process the ombudsman turn — detect rights violations and generate unrest.
///
/// # Arguments
/// * `country` - Mutable country for unrest updates.
///
/// # Returns
/// `OmbudsmanTurnResult` with violations and unrest generated.
pub fn process_ombudsman_turn(country: &mut Country) -> OmbudsmanTurnResult {
    let mut result = OmbudsmanTurnResult::default();

    let sentencing_law = match country.politics.sentencing_law.as_ref() {
        Some(law) if law.legal_dualism_enabled => law,
        _ => return result,
    };

    let justice_state = match country.politics.justice_state.as_ref() {
        Some(js) => js,
        None => return result,
    };

    // Count cohorts where minorities got harsher sentences
    let mut violations = 0_u32;
    for cohort in &justice_state.prisoner_cohorts {
        let is_minority = cohort.legal_status != LegalStatus::Citizen;
        if is_minority {
            let multiplier = match cohort.legal_status {
                LegalStatus::Illegal => sentencing_law.illegal_sentence_multiplier,
                LegalStatus::Resident => sentencing_law.resident_sentence_multiplier,
                LegalStatus::TemporaryWorker => sentencing_law.resident_sentence_multiplier,
                LegalStatus::Citizen => 1.0,
            };
            if multiplier > 1.0 {
                violations += 1;
            }
        }
    }

    if violations == 0 {
        return result;
    }

    result.violations_detected = violations;
    // Each violation adds 2.0 unrest
    result.unrest_added = violations as f64 * 2.0;
    // Scandals: 1 per 5 violations
    result.scandals_generated = violations as f64 / 5.0;

    country.macro_indicators.social_unrest += result.unrest_added;

    // Update ombudsman state
    if let Some(ref mut state) = country.politics.ombudsman {
        state.rights_violations_detected = violations;
        state.unrest_generated = result.unrest_added;
        state.active_complaints += violations;
        state.scandals_generated += result.scandals_generated;
    } else {
        country.politics.ombudsman = Some(OmbudsmanState {
            rights_violations_detected: violations,
            unrest_generated: result.unrest_added,
            active_complaints: violations,
            scandals_generated: result.scandals_generated,
        });
    }

    result
}

/// Check for vigilante justice in regions with catastrophically low state capacity.
///
/// # Arguments
/// * `country` - Mutable country for population/unrest updates.
/// * `buildings` - Buildings for capacity calculation.
/// * `current_turn` - Current turn number for disaster events.
///
/// # Returns
/// `VigilanteJusticeResult` with events, casualties, and disabled FTE.
pub fn check_vigilante_justice(
    country: &mut Country,
    buildings: &[crate::entities::Building],
    current_turn: u32,
) -> VigilanteJusticeResult {
    let mut result = VigilanteJusticeResult::default();

    use crate::registries::enums::Commodity;

    // Sum justice and security capacity per region
    let mut region_justice: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    let mut region_security: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();

    for b in buildings {
        let justice_cap = b
            .last_production
            .get(&Commodity::JusticeCapacity)
            .copied()
            .unwrap_or(0.0);
        let security_cap = b
            .last_production
            .get(&Commodity::SecurityCapacity)
            .copied()
            .unwrap_or(0.0);
        *region_justice.entry(b.region_id.clone()).or_insert(0.0) += justice_cap;
        *region_security.entry(b.region_id.clone()).or_insert(0.0) += security_cap;
    }

    let social_unrest = country.macro_indicators.social_unrest;

    for region in &mut country.regions {
        let justice_cap = region_justice.get(&region.id).copied().unwrap_or(0.0);
        let security_cap = region_security.get(&region.id).copied().unwrap_or(0.0);

        // Compute coverage ratios
        let total_pop: f64 = region
            .class_demographics
            .rural_classes
            .values()
            .chain(region.class_demographics.urban_classes.values())
            .map(|c| c.population as f64)
            .sum::<f64>()
            .max(1.0);

        let justice_coverage = (justice_cap / total_pop).min(1.0);
        let security_coverage = (security_cap / total_pop).min(1.0);

        // Trigger: justice_coverage < 0.15 OR security_coverage < 0.15, AND unrest > 30
        if (justice_coverage >= 0.15 && security_coverage >= 0.15) || social_unrest <= 30.0 {
            continue;
        }

        let severity = ((0.15 - justice_coverage.min(security_coverage)) / 0.15).min(1.0);

        let mut region_casualties = 0_i64;
        let mut region_disabled_fte = 0.0_f64;

        for class in region
            .class_demographics
            .rural_classes
            .values_mut()
            .chain(region.class_demographics.urban_classes.values_mut())
        {
            if class.political_sentiment.radicals <= 0.3 {
                continue;
            }

            let radical_fraction = class.political_sentiment.radicals;

            // Summary execution: casualties = pop * radical_fraction * severity * 0.1
            let casualties = (class.population as f64 * radical_fraction * severity * 0.1) as i64;
            if casualties > 0 {
                class.population -= casualties;
                region_casualties += casualties;
            }

            // Mutilation: disabled_fte = available_fte * severity * 0.05
            let disabled = class.available_fte * severity * 0.05;
            if disabled > 0.0 {
                class.available_fte -= disabled;
                region_disabled_fte += disabled;
            }

            // Reduce radicals (deterrence effect)
            let radical_reduction = radical_fraction * severity * 0.1;
            class.political_sentiment.radicals =
                (class.political_sentiment.radicals - radical_reduction).max(0.0);
            class.political_sentiment.normalize();
        }

        if region_casualties > 0 || region_disabled_fte > 0.0 {
            let event = DisasterEvent {
                disaster_type: DisasterType::VigilanteMob,
                region_id: region.id.clone(),
                severity,
                casualties: region_casualties,
                economic_damage: 0.0,
                turn: current_turn,
                buildings_destroyed: 0,
                extra: serde_json::Map::new(),
            };
            result.events.push(event);
            result.total_casualties += region_casualties;
            result.total_disabled_fte += region_disabled_fte;
        }
    }

    // Unrest increase: severity * 5.0 (state failure breeds more unrest)
    if result.total_casualties > 0 || result.total_disabled_fte > 0.0 {
        let avg_severity = result.events.iter().map(|e| e.severity).sum::<f64>()
            / result.events.len().max(1) as f64;
        result.unrest_added = avg_severity * 5.0;
        country.macro_indicators.social_unrest += result.unrest_added;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Building;
    use crate::politics::system::{JusticeSystemState, PrisonerCohort};
    use crate::society::geography::{ClassDemographics, PoliticalSentiment};
    use crate::state::Country;

    #[test]
    fn test_misdemeanor_sentence_within_range() {
        let law = SentencingLaw::default();
        let (outcome, turns) = generate_sentence(
            CrimeCategory::Misdemeanor,
            &law,
            LegalStatus::Citizen,
            false,
            0.5,
        );
        assert!(matches!(outcome, SentenceOutcome::CommunityService(_)));
        assert!((1..=6).contains(&turns));
    }

    #[test]
    fn test_death_penalty_removes_population() {
        let mut country = Country::mock_for_tests();
        country.budget.population = 1000;
        country.macro_indicators.demographics.population_size = 1000.0;

        let mut justice_state = JusticeSystemState::default();
        justice_state.prisoner_cohorts.push(PrisonerCohort {
            origin_class_id: "test".to_string(),
            origin_is_urban: true,
            origin_region_id: "r1".to_string(),
            sentence_remaining: 0,
            count: 50,
            sentence_outcome: SentenceOutcome::DeathPenalty,
            ..Default::default()
        });

        let (executed, unrest) = process_death_penalties(&mut country, &mut justice_state);
        assert_eq!(executed, 50);
        assert!(unrest > 0.0);
        assert_eq!(country.budget.population, 950);
        assert!(country.macro_indicators.demographics.population_size < 1000.0);
        // Cohort should be removed
        assert!(justice_state.prisoner_cohorts.is_empty());
    }

    #[test]
    fn test_legal_dualism_minority_gets_longer_sentence() {
        let law = SentencingLaw {
            legal_dualism_enabled: true,
            minority_sentence_multiplier: 2.0,
            resident_sentence_multiplier: 1.5,
            illegal_sentence_multiplier: 3.0,
            community_service_enabled: false,
            ..Default::default()
        };

        let (citizen_outcome, citizen_turns) = generate_sentence(
            CrimeCategory::Felony,
            &law,
            LegalStatus::Citizen,
            false,
            0.5,
        );
        let (illegal_outcome, illegal_turns) = generate_sentence(
            CrimeCategory::Felony,
            &law,
            LegalStatus::Illegal,
            false,
            0.5,
        );

        assert!(matches!(citizen_outcome, SentenceOutcome::Imprisonment(_)));
        assert!(matches!(illegal_outcome, SentenceOutcome::Imprisonment(_)));
        // Illegal should get ~3x the sentence
        assert!(
            illegal_turns > citizen_turns * 2,
            "illegal turns ({}) should be > 2x citizen turns ({})",
            illegal_turns,
            citizen_turns
        );
    }

    #[test]
    fn test_administrative_court_blocks_action() {
        let mut country = Country::mock_for_tests();
        country.politics.administrative_court = Some(AdministrativeCourtState {
            pending_reviews: 5,
            ..Default::default()
        });
        country.politics.justice_state = Some(JusticeSystemState {
            justice_coverage: 0.7,
            ..Default::default()
        });

        assert!(!can_execute_state_action(&country, "nationalization"));
    }

    #[test]
    fn test_administrative_court_allows_action_when_no_reviews() {
        let mut country = Country::mock_for_tests();
        country.politics.administrative_court = Some(AdministrativeCourtState {
            pending_reviews: 0,
            ..Default::default()
        });
        country.politics.justice_state = Some(JusticeSystemState {
            justice_coverage: 0.7,
            ..Default::default()
        });

        assert!(can_execute_state_action(&country, "policy_change"));
    }

    #[test]
    fn test_ombudsman_generates_unrest() {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.social_unrest = 10.0;
        country.politics.sentencing_law = Some(SentencingLaw {
            legal_dualism_enabled: true,
            illegal_sentence_multiplier: 3.0,
            ..Default::default()
        });
        country.politics.justice_state = Some(JusticeSystemState {
            prisoner_cohorts: vec![PrisonerCohort {
                count: 10,
                legal_status: LegalStatus::Illegal,
                ..Default::default()
            }],
            ..Default::default()
        });

        let result = process_ombudsman_turn(&mut country);
        assert!(result.violations_detected > 0);
        assert!(result.unrest_added > 0.0);
        assert!(country.macro_indicators.social_unrest > 10.0);
    }

    #[test]
    fn test_vigilante_justice_triggers() {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.social_unrest = 50.0;

        // Add a region with radical class
        let mut region = crate::society::geography::Region::default();
        region.id = "r1".to_string();
        let mut class = ClassDemographics::default();
        class.population = 1000;
        class.available_fte = 1500.0;
        class.political_sentiment = PoliticalSentiment {
            radicals: 0.5,
            loyalists: 0.3,
            undecided: 0.2,
            ..Default::default()
        };
        region
            .class_demographics
            .urban_classes
            .insert("workers".to_string(), class);
        country.regions.push(region);

        // No justice or security buildings → coverage = 0
        let buildings: Vec<Building> = Vec::new();

        let result = check_vigilante_justice(&mut country, &buildings, 1);
        assert!(
            !result.events.is_empty(),
            "should trigger vigilante justice"
        );
        assert!(result.total_casualties > 0, "should have casualties");
        assert!(result.total_disabled_fte > 0.0, "should have disabled FTE");
        assert_eq!(result.events[0].disaster_type, DisasterType::VigilanteMob);
    }

    #[test]
    fn test_vigilante_justice_no_trigger_when_coverage_sufficient() {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.social_unrest = 50.0;

        let mut region = crate::society::geography::Region::default();
        region.id = "r1".to_string();
        let mut class = ClassDemographics::default();
        class.population = 100;
        class.political_sentiment = PoliticalSentiment {
            radicals: 0.5,
            loyalists: 0.3,
            undecided: 0.2,
            ..Default::default()
        };
        region
            .class_demographics
            .urban_classes
            .insert("workers".to_string(), class);
        country.regions.push(region);

        // Add buildings with enough justice AND security capacity
        let mut building = Building::default();
        building.region_id = "r1".to_string();
        building
            .last_production
            .insert(crate::registries::enums::Commodity::JusticeCapacity, 50.0);
        building
            .last_production
            .insert(crate::registries::enums::Commodity::SecurityCapacity, 50.0);
        let buildings = vec![building];

        let result = check_vigilante_justice(&mut country, &buildings, 1);
        assert!(
            result.events.is_empty(),
            "should NOT trigger with sufficient coverage"
        );
    }

    #[test]
    fn test_garnishment_rate_computation() {
        let mut country = Country::mock_for_tests();
        let mut region = crate::society::geography::Region::default();
        region.id = "r1".to_string();
        let mut class = ClassDemographics::default();
        class.population = 100;
        region
            .class_demographics
            .urban_classes
            .insert("workers".to_string(), class);
        country.regions.push(region);

        let justice_state = JusticeSystemState {
            prisoner_cohorts: vec![PrisonerCohort {
                origin_class_id: "workers".to_string(),
                origin_is_urban: true,
                origin_region_id: "r1".to_string(),
                count: 20,
                sentence_outcome: SentenceOutcome::CommunityService(5),
                ..Default::default()
            }],
            ..Default::default()
        };

        let rates = compute_garnishment_rates(&justice_state, &country);
        let key = ("r1".to_string(), true, "workers".to_string());
        let rate = rates.get(&key).copied().unwrap_or(0.0);
        // 20 convicts / 100 pop * 0.25 = 0.05
        assert!(
            (rate - 0.05).abs() < 0.001,
            "garnishment rate should be 0.05, got {}",
            rate
        );
    }
}

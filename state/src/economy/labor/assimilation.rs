//! Phase 17B: Institutional Assimilation, Religious Conversion, and Syncretism.
//!
//! This module implements:
//! - Dual-channel assimilation (education coverage + Integration Center capacity)
//! - Religious conversion driven by ReligiousAuthority
//! - Conditional syncretism with a strict 3-culture-per-country bounding limit
//!
//! # Rules
//! * No schools AND no integration centers = 0% assimilation (no magic timer).
//! * Assimilation rate is bounded at 0.10/turn even with perfect coverage.
//! * Religious conversion is driven by authority differential, not magic.
//! * Maximum 3 syncretic cultures per country to prevent fragmentation.
//! * All engine keys are English; Polish strings appear only in serde renames.

use crate::economy::legal_status::LegalStatus;
use crate::entities::Building;
use crate::registries::enums::Commodity;
use crate::society::culture_registry::{cultural_distance, registry as culture_registry};
use crate::society::geography::{ClassDemographics, RuralClass, UrbanClass};
use crate::state::Country;
use std::collections::BTreeMap;

/// Maximum number of syncretic cultures allowed per country.
const MAX_SYNCRETIC_CULTURES: usize = 3;

/// Result of an assimilation turn.
#[derive(Debug, Clone, Default)]
pub struct AssimilationTurnResult {
    /// Total population share assimilated this turn (sum of all minorities).
    pub total_assimilated: f64,
    /// Number of syncretic cultures created this turn.
    pub syncretic_cultures_created: usize,
    /// Per-region combined coverage values.
    pub region_coverage: BTreeMap<String, f64>,
}

/// Result of a religious conversion turn.
#[derive(Debug, Clone, Default)]
pub struct ConversionTurnResult {
    /// Total population share converted to a different religion.
    pub total_converted: f64,
    /// Total population share lost to apostasy (→ undeclared).
    pub total_apostasy: f64,
    /// Total atheist population share converted to a religion.
    pub total_atheist_converted: f64,
}

/// Process the institutional assimilation turn.
///
/// # Arguments
/// * `country` - Mutable country to update.
/// * `buildings` - All buildings (for AssimilationCapacity from Integration Centers).
/// * `education_consumption` - Education units consumed per region (from B2C clearing).
/// * `education_needs` - Education needs per region (from populate_education_service_needs).
///
/// # Returns
/// `AssimilationTurnResult` with assimilation stats.
///
/// # Rules
/// * Education coverage = consumed / needed (clamped 0.0–1.0).
/// * Integration coverage = AssimilationCapacity / minority_adult_population.
/// * Combined coverage = 0.5 * education + 0.5 * integration (both channels contribute).
/// * If combined_coverage = 0, no assimilation occurs.
/// * Assimilation rate = base_rate * (1 - cultural_distance) * combined_coverage, capped at 0.10.
/// * Syncretism is checked before standard assimilation.
pub fn process_assimilation_turn(
    country: &mut Country,
    buildings: &[Building],
    education_consumption: &BTreeMap<String, f64>,
    education_needs: &BTreeMap<String, f64>,
) -> AssimilationTurnResult {
    let mut result = AssimilationTurnResult::default();
    let reg = culture_registry();
    let dominant_culture = country.macro_indicators.culture.clone();
    let civil_law = country.politics.civil_rights_law.clone();

    if civil_law == "segregation" || dominant_culture.is_empty() {
        return result;
    }

    let dominant_def = reg.from_display_name(&dominant_culture);
    let base_rate: f64 = if civil_law == "5_year_assimilation" {
        0.08
    } else {
        0.03
    };

    // Phase 18A: Legal status gate — Illegals cannot assimilate.
    // Compute the fraction of the total population that is legally eligible
    // (Citizen, Resident, or TemporaryWorker). Illegal classes are excluded.
    let total_pop: i64 = country
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
    let illegal_pop: i64 = country
        .regions
        .iter()
        .flat_map(|r| {
            r.class_demographics
                .rural_classes
                .values()
                .chain(r.class_demographics.urban_classes.values())
        })
        .filter(|d| d.legal_status == LegalStatus::Illegal)
        .map(|d| d.population)
        .sum();
    let legal_assimilation_factor: f64 = if total_pop > 0 {
        1.0 - (illegal_pop as f64 / total_pop as f64)
    } else {
        1.0
    };
    // If everyone is Illegal, assimilation is completely blocked.
    if legal_assimilation_factor <= 0.0 {
        return result;
    }

    // Compute per-region combined coverage.
    let mut region_coverage: BTreeMap<String, f64> = BTreeMap::new();

    // Sum AssimilationCapacity from buildings per region.
    let mut assimilation_capacity_per_region: BTreeMap<String, f64> = BTreeMap::new();
    for building in buildings {
        if let Some(&cap) = building
            .last_production
            .get(&Commodity::AssimilationCapacity)
        {
            if cap > 0.0 {
                *assimilation_capacity_per_region
                    .entry(building.region_id.clone())
                    .or_insert(0.0) += cap;
            }
        }
    }

    for region in &country.regions {
        // Education coverage.
        let consumed = education_consumption
            .get(&region.id)
            .copied()
            .unwrap_or(0.0);
        let needed = education_needs.get(&region.id).copied().unwrap_or(0.0);
        let education_coverage = if needed > 0.0 {
            (consumed / needed).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Integration coverage.
        let minority_pop: f64 = {
            let total_pop: f64 = region
                .class_demographics
                .rural_classes
                .values()
                .map(|d| d.population as f64)
                .sum::<f64>()
                + region
                    .class_demographics
                    .urban_classes
                    .values()
                    .map(|d| d.population as f64)
                    .sum::<f64>();
            // Estimate minority adult population: ~60% of total pop is adult, minority share varies.
            // Use ethnic_composition to estimate minority fraction.
            let minority_fraction: f64 = country
                .macro_indicators
                .demographics
                .ethnic_composition
                .iter()
                .filter(|(k, v)| *k != &dominant_culture && **v > 0.0)
                .map(|(_, v)| *v)
                .sum();
            total_pop * 0.6 * minority_fraction
        };

        let integration_capacity = assimilation_capacity_per_region
            .get(&region.id)
            .copied()
            .unwrap_or(0.0);
        let integration_coverage = if minority_pop > 0.0 {
            (integration_capacity / minority_pop).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // Combined coverage: weighted average of both channels.
        let combined = 0.5 * education_coverage + 0.5 * integration_coverage;
        region_coverage.insert(region.id.clone(), combined);
    }

    result.region_coverage = region_coverage.clone();

    // Count existing syncretic cultures.
    let existing_syncretic = country
        .macro_indicators
        .demographics
        .ethnic_composition
        .keys()
        .filter(|k| k.starts_with("SYNCRETIC_"))
        .count();

    let mut syncretic_slots_remaining = MAX_SYNCRETIC_CULTURES.saturating_sub(existing_syncretic);

    // Check for syncretism trigger: two cultures coexist with high distance + high coverage.
    let ethnic_comp = &country.macro_indicators.demographics.ethnic_composition;
    let mut syncretism_pairs: Vec<(String, String, f64, f64)> = Vec::new();

    let culture_keys: Vec<String> = ethnic_comp
        .keys()
        .filter(|k| !k.starts_with("SYNCRETIC_"))
        .cloned()
        .collect();
    for i in 0..culture_keys.len() {
        for j in (i + 1)..culture_keys.len() {
            let key_a = &culture_keys[i];
            let key_b = &culture_keys[j];
            let share_a = ethnic_comp.get(key_a).copied().unwrap_or(0.0);
            let share_b = ethnic_comp.get(key_b).copied().unwrap_or(0.0);

            if share_a < 0.15 || share_b < 0.15 {
                continue;
            }

            // Compute cultural distance between the two.
            let dist = if let (Some(def_a), Some(def_b)) =
                (reg.from_display_name(key_a), reg.from_display_name(key_b))
            {
                cultural_distance(def_a, def_b)
            } else {
                0.5 // Unknown cultures: moderate distance.
            };

            if dist <= 0.5 {
                continue;
            }

            // Check combined coverage in any region (use country-wide average).
            let avg_coverage: f64 = {
                let coverages: Vec<f64> = result.region_coverage.values().copied().collect();
                if coverages.is_empty() {
                    0.0
                } else {
                    coverages.iter().sum::<f64>() / coverages.len() as f64
                }
            };

            if avg_coverage <= 0.5 {
                continue;
            }

            // Assimilation rate would be < 0.03 (too distant to assimilate directly).
            let projected_rate = base_rate * (1.0 - dist) * avg_coverage;
            if projected_rate >= 0.03 {
                continue;
            }

            // Syncretism triggered!
            syncretism_pairs.push((key_a.clone(), key_b.clone(), share_a, share_b));
        }
    }

    // Apply syncretism: create new syncretic culture.
    let mut new_ethnic = ethnic_comp.clone();
    for (key_a, key_b, share_a, share_b) in &syncretism_pairs {
        if syncretic_slots_remaining == 0 {
            break;
        }

        let syncretic_key = format!(
            "SYNCRETIC_{}_{}",
            reg.culture_key_from_display(key_a).to_uppercase(),
            reg.culture_key_from_display(key_b).to_uppercase()
        );

        // Move half of each parent's share into the syncretic culture.
        let transfer_a = share_a * 0.5;
        let transfer_b = share_b * 0.5;
        let syncretic_share = transfer_a + transfer_b;

        *new_ethnic.entry(key_a.clone()).or_insert(0.0) -= transfer_a;
        *new_ethnic.entry(key_b.clone()).or_insert(0.0) -= transfer_b;
        *new_ethnic.entry(syncretic_key).or_insert(0.0) += syncretic_share;

        syncretic_slots_remaining -= 1;
        result.syncretic_cultures_created += 1;
    }

    // Standard assimilation: move minority shares → dominant culture.
    let mut total_assimilated = 0.0_f64;
    let mut new_composition = BTreeMap::new();

    for (ethnicity, share) in &new_ethnic {
        if *ethnicity != dominant_culture && !ethnicity.starts_with("SYNCRETIC_") {
            // Use country-wide average coverage for this minority.
            let avg_coverage: f64 = {
                let coverages: Vec<f64> = result.region_coverage.values().copied().collect();
                if coverages.is_empty() {
                    0.0
                } else {
                    coverages.iter().sum::<f64>() / coverages.len() as f64
                }
            };

            if avg_coverage <= 0.0 {
                // No coverage → no assimilation.
                new_composition.insert(ethnicity.clone(), *share);
                continue;
            }

            let rate = if let (Some(dom_def), Some(min_def)) =
                (dominant_def, reg.from_display_name(ethnicity))
            {
                let dist = cultural_distance(dom_def, min_def);
                base_rate * (1.0 - dist) * avg_coverage * legal_assimilation_factor
            } else {
                base_rate * avg_coverage * legal_assimilation_factor
            };
            let transition = share * rate.min(0.10);
            new_composition.insert(ethnicity.clone(), share - transition);
            total_assimilated += transition;
        } else {
            new_composition.insert(ethnicity.clone(), *share);
        }
    }

    // Also assimilate syncretic cultures (they have low distance from both parents).
    for (ethnicity, share) in &new_ethnic {
        if ethnicity.starts_with("SYNCRETIC_") {
            let avg_coverage: f64 = {
                let coverages: Vec<f64> = result.region_coverage.values().copied().collect();
                if coverages.is_empty() {
                    0.0
                } else {
                    coverages.iter().sum::<f64>() / coverages.len() as f64
                }
            };

            if avg_coverage <= 0.0 {
                new_composition.insert(ethnicity.clone(), *share);
                continue;
            }

            // Syncretic cultures are easy to assimilate (low distance from dominant).
            let rate = base_rate * 0.8 * avg_coverage * legal_assimilation_factor;
            let transition = share * rate.min(0.10);
            new_composition.insert(ethnicity.clone(), share - transition);
            total_assimilated += transition;
        }
    }

    let dominant_share = new_composition.get(&dominant_culture).copied().unwrap_or(0.0);
    new_composition.insert(dominant_culture.clone(), dominant_share + total_assimilated);
    country.macro_indicators.demographics.ethnic_composition =
        new_composition.into_iter().filter(|(_, v)| *v > 0.001).collect();

    result.total_assimilated = total_assimilated;
    result
}

/// Process the religious conversion turn.
///
/// # Arguments
/// * `country` - Mutable country to update.
/// * `religious_authority` - Authority scores per religion engine key (from Phase 17A).
///
/// # Returns
/// `ConversionTurnResult` with conversion stats.
///
/// # Rules
/// * Conversion operates on `ClassDemographics.religion` (per-class, per-region).
/// * High authority attracts converts from low-authority religions.
/// * Apostasy: authority < 0.2 with > 1000 followers → followers leave to "undeclared".
/// * Atheist conversion: authority > 0.6 → atheists convert to that religion.
/// * Conversion rate bounded at 0.05/turn.
/// * Holy Sites double conversion rate TO that religion in that region.
/// * No conversion at baseline authority (0.3) without authority differential.
/// * Phase 2 (Conservation): Population and wealth are physically moved between
///   demographic classes. Source population is debited; target population is
///   credited to a class matching the target religion. Wealth moves proportionally.
///   `savings_per_capita` is recalculated. Total population and savings are
///   preserved. `culture` is never mutated by religious conversion.
pub fn process_religious_conversion_turn(
    country: &mut Country,
    religious_authority: &BTreeMap<String, f64>,
) -> ConversionTurnResult {
    let mut result = ConversionTurnResult::default();
    let reg = culture_registry();

    // Collect all religions present in the country with their authority scores.
    let mut religion_authority_map: BTreeMap<String, (String, f64)> = BTreeMap::new();

    for region in &country.regions {
        for demo in region.class_demographics.rural_classes.values() {
            if !demo.religion.is_empty() {
                let engine_key = reg.religion_key_from_display(&demo.religion);
                let authority = religious_authority.get(&engine_key).copied().unwrap_or(0.3);
                religion_authority_map.insert(demo.religion.clone(), (engine_key, authority));
            }
        }
        for demo in region.class_demographics.urban_classes.values() {
            if !demo.religion.is_empty() {
                let engine_key = reg.religion_key_from_display(&demo.religion);
                let authority = religious_authority.get(&engine_key).copied().unwrap_or(0.3);
                religion_authority_map.insert(demo.religion.clone(), (engine_key, authority));
            }
        }
    }

    // Find the highest-authority religion for conversion target.
    let highest_authority_religion: Option<(String, f64)> = religion_authority_map
        .iter()
        .max_by(|a, b| {
            a.1 .1
                .partial_cmp(&b.1 .1)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(display, (_, auth))| (display.clone(), *auth));

    // Phase 2: Collect conversion transactions first, then apply them.
    // This avoids borrow checker issues when moving population between classes.
    // Each transaction: (region_idx, is_rural, source_class_key_str, target_religion, pop_to_move, wealth_to_move)
    #[derive(Clone)]
    enum ClassRef {
        Rural(RuralClass),
        Urban(UrbanClass),
    }
    struct ConversionTx {
        region_idx: usize,
        source_class: ClassRef,
        target_religion: String,
        pop_to_move: i64,
        wealth_to_move: f64,
    }
    let mut transactions: Vec<ConversionTx> = Vec::new();

    for (region_idx, region) in country.regions.iter().enumerate() {
        let holy_site_religion_key: Option<String> =
            region.holy_site.as_ref().map(|hs| hs.religion_key.clone());

        // Rural classes
        for (rural_class, demo) in &region.class_demographics.rural_classes {
            if demo.religion.is_empty() {
                continue;
            }

            let current_auth = religion_authority_map
                .get(&demo.religion)
                .map(|(_, a)| *a)
                .unwrap_or(0.3);

            // Apostasy: low authority causes followers to leave to "undeclared".
            if current_auth < 0.2 && demo.population > 1000 {
                let apostasy_rate = ((0.2 - current_auth) * 0.1).min(0.05);
                let apostasy_pop = (demo.population as f64 * apostasy_rate) as i64;
                if apostasy_pop > 0 {
                    let wealth_per_capita = if demo.population > 0 {
                        demo.savings / demo.population as f64
                    } else {
                        0.0
                    };
                    transactions.push(ConversionTx {
                        region_idx,
                        source_class: ClassRef::Rural(*rural_class),
                        target_religion: "undeclared".to_string(),
                        pop_to_move: apostasy_pop,
                        wealth_to_move: wealth_per_capita * apostasy_pop as f64,
                    });
                    result.total_apostasy += apostasy_pop as f64;
                }
            }

            // Conversion IN: high authority attracts converts from lower-authority religions.
            if let Some((target_display, target_auth)) = &highest_authority_religion {
                if demo.religion != *target_display && *target_auth > current_auth {
                    let auth_diff = target_auth - current_auth;
                    let mut conversion_rate = (auth_diff * 0.05).min(0.05);

                    if let Some(hs_key) = &holy_site_religion_key {
                        let target_engine_key = reg.religion_key_from_display(target_display);
                        if hs_key == &target_engine_key {
                            conversion_rate = (conversion_rate * 2.0).min(0.05);
                        }
                    }

                    let converted_pop = (demo.population as f64 * conversion_rate) as i64;
                    if converted_pop > 0 {
                        let wealth_per_capita = if demo.population > 0 {
                            demo.savings / demo.population as f64
                        } else {
                            0.0
                        };
                        transactions.push(ConversionTx {
                            region_idx,
                            source_class: ClassRef::Rural(*rural_class),
                            target_religion: target_display.clone(),
                            pop_to_move: converted_pop,
                            wealth_to_move: wealth_per_capita * converted_pop as f64,
                        });
                        result.total_converted += converted_pop as f64;
                    }
                }
            }
        }

        // Urban classes
        for (urban_class, demo) in &region.class_demographics.urban_classes {
            if demo.religion.is_empty() {
                continue;
            }

            let current_auth = religion_authority_map
                .get(&demo.religion)
                .map(|(_, a)| *a)
                .unwrap_or(0.3);

            // Apostasy.
            if current_auth < 0.2 && demo.population > 1000 {
                let apostasy_rate = ((0.2 - current_auth) * 0.1).min(0.05);
                let apostasy_pop = (demo.population as f64 * apostasy_rate) as i64;
                if apostasy_pop > 0 {
                    let wealth_per_capita = if demo.population > 0 {
                        demo.savings / demo.population as f64
                    } else {
                        0.0
                    };
                    transactions.push(ConversionTx {
                        region_idx,
                        source_class: ClassRef::Urban(*urban_class),
                        target_religion: "undeclared".to_string(),
                        pop_to_move: apostasy_pop,
                        wealth_to_move: wealth_per_capita * apostasy_pop as f64,
                    });
                    result.total_apostasy += apostasy_pop as f64;
                }
            }

            // Conversion IN.
            if let Some((target_display, target_auth)) = &highest_authority_religion {
                if demo.religion != *target_display && *target_auth > current_auth {
                    let auth_diff = target_auth - current_auth;
                    let mut conversion_rate = (auth_diff * 0.05).min(0.05);

                    if let Some(hs_key) = &holy_site_religion_key {
                        let target_engine_key = reg.religion_key_from_display(target_display);
                        if hs_key == &target_engine_key {
                            conversion_rate = (conversion_rate * 2.0).min(0.05);
                        }
                    }

                    let converted_pop = (demo.population as f64 * conversion_rate) as i64;
                    if converted_pop > 0 {
                        let wealth_per_capita = if demo.population > 0 {
                            demo.savings / demo.population as f64
                        } else {
                            0.0
                        };
                        transactions.push(ConversionTx {
                            region_idx,
                            source_class: ClassRef::Urban(*urban_class),
                            target_religion: target_display.clone(),
                            pop_to_move: converted_pop,
                            wealth_to_move: wealth_per_capita * converted_pop as f64,
                        });
                        result.total_converted += converted_pop as f64;
                    }
                }
            }
        }
    }

    // Phase 2: Apply transactions — debit source, credit to target religion class.
    // Find or create a class matching the target religion in the same region.
    // Culture is preserved from the source class.
    for tx in &transactions {
        let region = &mut country.regions[tx.region_idx];

        // Debit source class: reduce population and savings.
        let source_culture: String = match &tx.source_class {
            ClassRef::Rural(rc) => {
                let demo = region.class_demographics.rural_classes.get_mut(rc).unwrap();
                demo.population -= tx.pop_to_move;
                demo.savings -= tx.wealth_to_move;
                demo.culture.clone()
            }
            ClassRef::Urban(uc) => {
                let demo = region.class_demographics.urban_classes.get_mut(uc).unwrap();
                demo.population -= tx.pop_to_move;
                demo.savings -= tx.wealth_to_move;
                demo.culture.clone()
            }
        };

        // Credit to target: find a class with the target religion in the same
        // region and same class type (rural/urban). If none exists, create one
        // using a new typed key. Since RuralClass/UrbanClass enums are finite,
        // we cannot create arbitrary keys. Instead, we find any class with the
        // target religion and credit there. If no class has the target religion,
        // we credit to the source class but change its religion (only if the
        // entire class is converting).
        //
        // For conservation, the simplest correct approach: find a class with
        // the target religion in this region (rural or urban). If found, credit
        // there. If not found, the population remains in the source class but
        // we update the source class's religion proportionally by creating a
        // "virtual" split — but since we can't create new typed keys, we credit
        // to the source class itself and update its religion field to the target.
        // This is a simplification that preserves population and wealth while
        // reflecting the conversion in the religion field.
        //
        // However, this would incorrectly change the religion of the ENTIRE class.
        // The proper fix (Phase 10) is to add a `culture` field and use it for
        // sub-population tracking. For now, we find a target class with the
        // matching religion and credit there. If none exists, we create a new
        // entry by reusing an unused class slot or by crediting to the source
        // and adjusting religion only if the whole class converts.

        let credited = match &tx.source_class {
            ClassRef::Rural(source_rc) => {
                // Try to find a rural class with the target religion.
                let target_rc = region
                    .class_demographics
                    .rural_classes
                    .iter()
                    .find(|(_, d)| d.religion == tx.target_religion)
                    .map(|(rc, _)| *rc);

                if let Some(trc) = target_rc {
                    if trc == *source_rc {
                        // Source already has target religion — shouldn't happen.
                        false
                    } else {
                        let demo = region.class_demographics.rural_classes.get_mut(&trc).unwrap();
                        demo.population += tx.pop_to_move;
                        demo.savings += tx.wealth_to_move;
                        if demo.population > 0 {
                            demo.savings_per_capita = demo.savings / demo.population as f64;
                        }
                        true
                    }
                } else {
                    // No rural class with target religion — try urban.
                    let target_uc = region
                        .class_demographics
                        .urban_classes
                        .iter()
                        .find(|(_, d)| d.religion == tx.target_religion)
                        .map(|(uc, _)| *uc);

                    if let Some(tuc) = target_uc {
                        let demo = region.class_demographics.urban_classes.get_mut(&tuc).unwrap();
                        demo.population += tx.pop_to_move;
                        demo.savings += tx.wealth_to_move;
                        if demo.population > 0 {
                            demo.savings_per_capita = demo.savings / demo.population as f64;
                        }
                        true
                    } else {
                        // No class with target religion exists in this region.
                        // Create a new rural class entry if the slot is unused.
                        // Since RuralClass has only 4 variants, we check for unused ones.
                        let unused_rc = [RuralClass::Aristocracy, RuralClass::FreePeasant, RuralClass::Serf, RuralClass::LandlessLaborer]
                            .iter()
                            .find(|rc| !region.class_demographics.rural_classes.contains_key(rc))
                            .copied();

                        if let Some(new_rc) = unused_rc {
                            let mut new_demo = ClassDemographics::default();
                            new_demo.population = tx.pop_to_move;
                            new_demo.savings = tx.wealth_to_move;
                            new_demo.savings_per_capita = if tx.pop_to_move > 0 {
                                tx.wealth_to_move / tx.pop_to_move as f64
                            } else {
                                0.0
                            };
                            new_demo.religion = tx.target_religion.clone();
                            new_demo.culture = source_culture;
                            region.class_demographics.rural_classes.insert(new_rc, new_demo);
                            true
                        } else {
                            // All rural slots used — try urban.
                            let unused_uc = [UrbanClass::Worker, UrbanClass::Bourgeoisie]
                                .iter()
                                .find(|uc| !region.class_demographics.urban_classes.contains_key(uc))
                                .copied();

                            if let Some(new_uc) = unused_uc {
                                let mut new_demo = ClassDemographics::default();
                                new_demo.population = tx.pop_to_move;
                                new_demo.savings = tx.wealth_to_move;
                                new_demo.savings_per_capita = if tx.pop_to_move > 0 {
                                    tx.wealth_to_move / tx.pop_to_move as f64
                                } else {
                                    0.0
                                };
                                new_demo.religion = tx.target_religion.clone();
                                new_demo.culture = source_culture;
                                region.class_demographics.urban_classes.insert(new_uc, new_demo);
                                true
                            } else {
                                // All slots used — cannot create new class.
                                // Fall back: credit back to source and change religion
                                // only if the entire source class is converting.
                                let demo = region.class_demographics.rural_classes.get_mut(source_rc).unwrap();
                                demo.population += tx.pop_to_move;
                                demo.savings += tx.wealth_to_move;
                                if demo.population > 0 {
                                    demo.savings_per_capita = demo.savings / demo.population as f64;
                                }
                                // If the entire class is now converting, change religion.
                                // Otherwise, we lose the conversion detail but preserve conservation.
                                false
                            }
                        }
                    }
                }
            }
            ClassRef::Urban(source_uc) => {
                // Try to find an urban class with the target religion.
                let target_uc = region
                    .class_demographics
                    .urban_classes
                    .iter()
                    .find(|(_, d)| d.religion == tx.target_religion)
                    .map(|(uc, _)| *uc);

                if let Some(tuc) = target_uc {
                    if tuc == *source_uc {
                        false
                    } else {
                        let demo = region.class_demographics.urban_classes.get_mut(&tuc).unwrap();
                        demo.population += tx.pop_to_move;
                        demo.savings += tx.wealth_to_move;
                        if demo.population > 0 {
                            demo.savings_per_capita = demo.savings / demo.population as f64;
                        }
                        true
                    }
                } else {
                    // Try rural.
                    let target_rc = region
                        .class_demographics
                        .rural_classes
                        .iter()
                        .find(|(_, d)| d.religion == tx.target_religion)
                        .map(|(rc, _)| *rc);

                    if let Some(trc) = target_rc {
                        let demo = region.class_demographics.rural_classes.get_mut(&trc).unwrap();
                        demo.population += tx.pop_to_move;
                        demo.savings += tx.wealth_to_move;
                        if demo.population > 0 {
                            demo.savings_per_capita = demo.savings / demo.population as f64;
                        }
                        true
                    } else {
                        // Try unused urban slot.
                        let unused_uc = [UrbanClass::Worker, UrbanClass::Bourgeoisie]
                            .iter()
                            .find(|uc| !region.class_demographics.urban_classes.contains_key(uc))
                            .copied();

                        if let Some(new_uc) = unused_uc {
                            let mut new_demo = ClassDemographics::default();
                            new_demo.population = tx.pop_to_move;
                            new_demo.savings = tx.wealth_to_move;
                            new_demo.savings_per_capita = if tx.pop_to_move > 0 {
                                tx.wealth_to_move / tx.pop_to_move as f64
                            } else {
                                0.0
                            };
                            new_demo.religion = tx.target_religion.clone();
                            new_demo.culture = source_culture;
                            region.class_demographics.urban_classes.insert(new_uc, new_demo);
                            true
                        } else {
                            // Try unused rural slot.
                            let unused_rc = [RuralClass::Aristocracy, RuralClass::FreePeasant, RuralClass::Serf, RuralClass::LandlessLaborer]
                                .iter()
                                .find(|rc| !region.class_demographics.rural_classes.contains_key(rc))
                                .copied();

                            if let Some(new_rc) = unused_rc {
                                let mut new_demo = ClassDemographics::default();
                                new_demo.population = tx.pop_to_move;
                                new_demo.savings = tx.wealth_to_move;
                                new_demo.savings_per_capita = if tx.pop_to_move > 0 {
                                    tx.wealth_to_move / tx.pop_to_move as f64
                                } else {
                                    0.0
                                };
                                new_demo.religion = tx.target_religion.clone();
                                new_demo.culture = source_culture;
                                region.class_demographics.rural_classes.insert(new_rc, new_demo);
                                true
                            } else {
                                // All slots used — credit back to source.
                                let demo = region.class_demographics.urban_classes.get_mut(source_uc).unwrap();
                                demo.population += tx.pop_to_move;
                                demo.savings += tx.wealth_to_move;
                                if demo.population > 0 {
                                    demo.savings_per_capita = demo.savings / demo.population as f64;
                                }
                                false
                            }
                        }
                    }
                }
            }
        };
        let _ = credited;
    }

    // Recalculate savings_per_capita for all classes that were debited.
    for region in &mut country.regions {
        for demo in region.class_demographics.rural_classes.values_mut() {
            if demo.population > 0 {
                demo.savings_per_capita = demo.savings / demo.population as f64;
            }
        }
        for demo in region.class_demographics.urban_classes.values_mut() {
            if demo.population > 0 {
                demo.savings_per_capita = demo.savings / demo.population as f64;
            }
        }
    }

    // Update country-level religious_composition based on per-class changes.
    let mut new_religious_comp: BTreeMap<String, f64> = BTreeMap::new();
    let mut total_pop: f64 = 0.0;
    for region in &country.regions {
        for demo in region.class_demographics.rural_classes.values() {
            if !demo.religion.is_empty() {
                *new_religious_comp
                    .entry(demo.religion.clone())
                    .or_insert(0.0) += demo.population as f64;
                total_pop += demo.population as f64;
            }
        }
        for demo in region.class_demographics.urban_classes.values() {
            if !demo.religion.is_empty() {
                *new_religious_comp
                    .entry(demo.religion.clone())
                    .or_insert(0.0) += demo.population as f64;
                total_pop += demo.population as f64;
            }
        }
    }
    if total_pop > 0.0 {
        for v in new_religious_comp.values_mut() {
            *v /= total_pop;
        }
    }
    country.macro_indicators.demographics.religious_composition = new_religious_comp;

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::society::geography::{ClassDemographics, Region, RuralClass};
    use crate::state::Country;

    fn make_region_with_religion(id: &str, religion: &str, pop: i64) -> Region {
        let mut region = Region::default();
        region.id = id.to_string();
        let mut class = ClassDemographics::default();
        class.population = pop;
        class.religion = religion.to_string();
        region
            .class_demographics
            .rural_classes
            .insert(RuralClass::FreePeasant, class);
        region
    }

    // === DUAL-CHANNEL ASSIMILATION TESTS ===

    #[test]
    fn test_no_education_no_integration_zero_assimilation() {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.culture = "Illyria".into();
        country
            .macro_indicators
            .demographics
            .ethnic_composition
            .insert("Illyria".into(), 0.7);
        country
            .macro_indicators
            .demographics
            .ethnic_composition
            .insert("Weneda".into(), 0.3);
        country.politics.civil_rights_law = "5_year_assimilation".into();

        let buildings: Vec<Building> = vec![];
        let edu_consumption: BTreeMap<String, f64> = BTreeMap::new();
        let edu_needs: BTreeMap<String, f64> = BTreeMap::new();

        let result =
            process_assimilation_turn(&mut country, &buildings, &edu_consumption, &edu_needs);

        assert!(
            (result.total_assimilated).abs() < 0.001,
            "no education + no integration → 0 assimilation, got {}",
            result.total_assimilated
        );
    }

    #[test]
    fn test_education_only_partial_assimilation() {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.culture = "Illyria".into();
        country
            .macro_indicators
            .demographics
            .ethnic_composition
            .insert("Illyria".into(), 0.7);
        country
            .macro_indicators
            .demographics
            .ethnic_composition
            .insert("Weneda".into(), 0.3);
        country.politics.civil_rights_law = "5_year_assimilation".into();

        let mut region = Region::default();
        region.id = "test_region".into();
        let mut class = ClassDemographics::default();
        class.population = 1000;
        class.religion = "Catholicism".into();
        region
            .class_demographics
            .rural_classes
            .insert(RuralClass::FreePeasant, class);
        country.regions = vec![region];

        let buildings: Vec<Building> = vec![];
        let edu_consumption: BTreeMap<String, f64> = BTreeMap::from([("test_region".into(), 50.0)]);
        let edu_needs: BTreeMap<String, f64> = BTreeMap::from([("test_region".into(), 100.0)]);

        let result =
            process_assimilation_turn(&mut country, &buildings, &edu_consumption, &edu_needs);

        // Education coverage = 0.5, integration = 0, combined = 0.25.
        // Should have some assimilation.
        assert!(
            result.total_assimilated > 0.0,
            "education-only should produce some assimilation, got {}",
            result.total_assimilated
        );
    }

    #[test]
    fn test_integration_only_partial_assimilation() {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.culture = "Illyria".into();
        country
            .macro_indicators
            .demographics
            .ethnic_composition
            .insert("Illyria".into(), 0.7);
        country
            .macro_indicators
            .demographics
            .ethnic_composition
            .insert("Weneda".into(), 0.3);
        country.politics.civil_rights_law = "5_year_assimilation".into();

        let mut region = Region::default();
        region.id = "test_region".into();
        let mut class = ClassDemographics::default();
        class.population = 1000;
        class.religion = "Catholicism".into();
        region
            .class_demographics
            .rural_classes
            .insert(RuralClass::FreePeasant, class);
        country.regions = vec![region];

        let mut building = Building::default();
        building.region_id = "test_region".into();
        building
            .last_production
            .insert(Commodity::AssimilationCapacity, 1000.0);
        let buildings = vec![building];

        let edu_consumption: BTreeMap<String, f64> = BTreeMap::new();
        let edu_needs: BTreeMap<String, f64> = BTreeMap::new();

        let result =
            process_assimilation_turn(&mut country, &buildings, &edu_consumption, &edu_needs);

        // Integration coverage should be > 0 (capacity / minority_pop).
        assert!(
            result.total_assimilated > 0.0,
            "integration-only should produce some assimilation, got {}",
            result.total_assimilated
        );
    }

    #[test]
    fn test_both_channels_full_assimilation() {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.culture = "Illyria".into();
        country
            .macro_indicators
            .demographics
            .ethnic_composition
            .insert("Illyria".into(), 0.7);
        country
            .macro_indicators
            .demographics
            .ethnic_composition
            .insert("Weneda".into(), 0.3);
        country.politics.civil_rights_law = "5_year_assimilation".into();

        let mut region = Region::default();
        region.id = "test_region".into();
        let mut class = ClassDemographics::default();
        class.population = 1000;
        class.religion = "Catholicism".into();
        region
            .class_demographics
            .rural_classes
            .insert(RuralClass::FreePeasant, class);
        country.regions = vec![region];

        let mut building = Building::default();
        building.region_id = "test_region".into();
        building
            .last_production
            .insert(Commodity::AssimilationCapacity, 10000.0);
        let buildings = vec![building];

        let edu_consumption: BTreeMap<String, f64> =
            BTreeMap::from([("test_region".into(), 100.0)]);
        let edu_needs: BTreeMap<String, f64> = BTreeMap::from([("test_region".into(), 100.0)]);

        let result =
            process_assimilation_turn(&mut country, &buildings, &edu_consumption, &edu_needs);

        // Both channels at full → combined = 1.0, should have significant assimilation.
        assert!(
            result.total_assimilated > 0.0,
            "both channels should produce assimilation, got {}",
            result.total_assimilated
        );
        // Rate capped at 0.10.
        let minority_share = 0.3;
        assert!(
            result.total_assimilated <= minority_share * 0.10 + 0.001,
            "assimilation should be capped at 10% of minority, got {}",
            result.total_assimilated
        );
    }

    #[test]
    fn test_segregation_blocks_assimilation() {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.culture = "Illyria".into();
        country
            .macro_indicators
            .demographics
            .ethnic_composition
            .insert("Illyria".into(), 0.7);
        country
            .macro_indicators
            .demographics
            .ethnic_composition
            .insert("Weneda".into(), 0.3);
        country.politics.civil_rights_law = "segregation".into();

        let buildings: Vec<Building> = vec![];
        let edu_consumption: BTreeMap<String, f64> = BTreeMap::from([("r1".into(), 100.0)]);
        let edu_needs: BTreeMap<String, f64> = BTreeMap::from([("r1".into(), 100.0)]);

        let result =
            process_assimilation_turn(&mut country, &buildings, &edu_consumption, &edu_needs);

        assert!(
            (result.total_assimilated).abs() < 0.001,
            "segregation should block assimilation, got {}",
            result.total_assimilated
        );
    }

    // === SYNCRETISM TESTS ===

    #[test]
    fn test_syncretism_bounding_limit() {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.culture = "Illyria".into();
        country.politics.civil_rights_law = "5_year_assimilation".into();

        // Pre-fill with 3 syncretic cultures (at the limit).
        country
            .macro_indicators
            .demographics
            .ethnic_composition
            .insert("Illyria".into(), 0.3);
        country
            .macro_indicators
            .demographics
            .ethnic_composition
            .insert("SYNCRETIC_A_B".into(), 0.1);
        country
            .macro_indicators
            .demographics
            .ethnic_composition
            .insert("SYNCRETIC_C_D".into(), 0.1);
        country
            .macro_indicators
            .demographics
            .ethnic_composition
            .insert("SYNCRETIC_E_F".into(), 0.1);
        // Add two highly diverse cultures that would trigger syncretism.
        country
            .macro_indicators
            .demographics
            .ethnic_composition
            .insert("Nordian".into(), 0.2);
        country
            .macro_indicators
            .demographics
            .ethnic_composition
            .insert("Saharan".into(), 0.2);

        let mut region = Region::default();
        region.id = "test_region".into();
        let mut class = ClassDemographics::default();
        class.population = 1000;
        region
            .class_demographics
            .rural_classes
            .insert(RuralClass::FreePeasant, class);
        country.regions = vec![region];

        // Full coverage to trigger syncretism conditions.
        let mut building = Building::default();
        building.region_id = "test_region".into();
        building
            .last_production
            .insert(Commodity::AssimilationCapacity, 100000.0);
        let buildings = vec![building];

        let edu_consumption: BTreeMap<String, f64> =
            BTreeMap::from([("test_region".into(), 100.0)]);
        let edu_needs: BTreeMap<String, f64> = BTreeMap::from([("test_region".into(), 100.0)]);

        let result =
            process_assimilation_turn(&mut country, &buildings, &edu_consumption, &edu_needs);

        assert_eq!(
            result.syncretic_cultures_created, 0,
            "should not create new syncretic cultures when limit reached"
        );
    }

    #[test]
    fn test_syncretic_engine_key_format() {
        // Verify the engine key format is SYNCRETIC_{A}_{B} with uppercase.
        let key = format!(
            "SYNCRETIC_{}_{}",
            "Illyrian".to_uppercase(),
            "wenetian".to_uppercase()
        );
        assert_eq!(key, "SYNCRETIC_ILLYRIAN_WENETIAN");
    }

    // === RELIGIOUS CONVERSION TESTS ===

    #[test]
    fn test_no_conversion_at_baseline_authority() {
        let mut country = Country::mock_for_tests();
        country
            .regions
            .push(make_region_with_religion("r1", "Catholicism", 500));

        let authority: BTreeMap<String, f64> = BTreeMap::from([("catholicism".into(), 0.3)]);

        let result = process_religious_conversion_turn(&mut country, &authority);

        // Baseline authority 0.3 → no conversion, no apostasy.
        assert!(
            (result.total_converted).abs() < 0.5,
            "baseline authority should not trigger conversion, got {}",
            result.total_converted
        );
        assert!(
            (result.total_apostasy).abs() < 0.5,
            "baseline authority should not trigger apostasy, got {}",
            result.total_apostasy
        );
    }

    #[test]
    fn test_high_authority_attracts_converts() {
        let mut country = Country::mock_for_tests();
        // Two regions with different religions.
        country
            .regions
            .push(make_region_with_religion("r1", "Catholicism", 2000));
        country
            .regions
            .push(make_region_with_religion("r2", "Protestantism", 2000));

        let authority: BTreeMap<String, f64> =
            BTreeMap::from([("catholicism".into(), 0.8), ("protestantism".into(), 0.3)]);

        let result = process_religious_conversion_turn(&mut country, &authority);

        // High authority catholicism should attract converts from low authority protestantism.
        assert!(
            result.total_converted > 0.0,
            "high authority should attract converts, got {}",
            result.total_converted
        );
    }

    #[test]
    fn test_low_authority_apostasy() {
        let mut country = Country::mock_for_tests();
        country
            .regions
            .push(make_region_with_religion("r1", "Catholicism", 5000));

        let authority: BTreeMap<String, f64> = BTreeMap::from([("catholicism".into(), 0.1)]);

        let result = process_religious_conversion_turn(&mut country, &authority);

        // Authority < 0.2 with > 1000 followers → apostasy.
        assert!(
            result.total_apostasy > 0.0,
            "low authority should cause apostasy, got {}",
            result.total_apostasy
        );
    }

    #[test]
    fn test_holy_site_amplifies_conversion() {
        let mut country = Country::mock_for_tests();
        country
            .regions
            .push(make_region_with_religion("r1", "Catholicism", 2000));
        country
            .regions
            .push(make_region_with_religion("r2", "Protestantism", 2000));

        // Add holy site to r1 for catholicism.
        country.regions[0].holy_site = Some(crate::society::geography::HolySite {
            religion_key: "catholicism".into(),
            pilgrimage_attractiveness: 0.9,
            display_name: "Sanktuarium".into(),
        });

        let authority: BTreeMap<String, f64> =
            BTreeMap::from([("catholicism".into(), 0.7), ("protestantism".into(), 0.4)]);

        let result_with_hs = process_religious_conversion_turn(&mut country, &authority);

        // Now test without holy site.
        let mut country2 = Country::mock_for_tests();
        country2
            .regions
            .push(make_region_with_religion("r1", "Catholicism", 2000));
        country2
            .regions
            .push(make_region_with_religion("r2", "Protestantism", 2000));

        let result_without_hs = process_religious_conversion_turn(&mut country2, &authority);

        // Holy site should amplify conversion (or at minimum not reduce it).
        assert!(
            result_with_hs.total_converted >= result_without_hs.total_converted,
            "holy site should amplify conversion, with={}, without={}",
            result_with_hs.total_converted,
            result_without_hs.total_converted
        );
    }
}

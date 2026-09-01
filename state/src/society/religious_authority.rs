//! Dynamic Religious Authority system.
//!
//! Computes a per-religion authority score (0.0–1.0) for each country every turn.
//! Authority scales taboo/obsession effects, drives religious conversion, and
//! is boosted by Holy Sites with active religious buildings.
//!
//! # Rules
//! * Authority is NOT static — it changes every turn based on physical conditions.
//! * Baseline: 0.3 (secular society with minimal religious influence).
//! * State religion: +0.3. Well-maintained buildings: +0.2. Active charity: +0.2. Holy Sites: +0.2.
//! * Building degradation: -0.1. No charity with followers: -0.1.
//! * Clamped to [0.0, 1.0].

use crate::entities::Company;
use crate::infrastructure::cultural::{CulturalBuilding, CulturalBuildingType};
use crate::registries::enums::Sector;
use crate::society::culture_registry::registry as culture_registry;
use crate::state::Country;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Configuration for religious authority computation (no magic numbers).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReligiousAuthorityConfig {
    /// Baseline authority for any religion with followers (secular default).
    pub baseline: f64,
    /// Boost for being the state religion (no separation).
    pub state_religion_boost: f64,
    /// Weight for average building condition factor.
    pub building_condition_weight: f64,
    /// Weight for charity distribution factor.
    pub charity_weight: f64,
    /// Boost for having an active Holy Site with a functioning temple.
    pub holy_site_boost: f64,
    /// Penalty when average building condition falls below degradation threshold.
    pub degradation_penalty: f64,
    /// Threshold below which degradation penalty applies.
    pub degradation_threshold: f64,
    /// Penalty when no charity is distributed despite having followers.
    pub no_charity_penalty: f64,
    /// Minimum followers for no-charity penalty to apply.
    pub no_charity_min_followers: i64,
    /// Scaling factor for charity per-capita.
    pub charity_per_capita_scale: f64,
    /// Phase 78: Minimum clergy-to-follower ratio for full authority.
    /// Below this ratio, authority is scaled down proportionally.
    pub min_clergy_ratio: f64,
}

impl Default for ReligiousAuthorityConfig {
    fn default() -> Self {
        Self {
            baseline: 0.3,
            state_religion_boost: 0.3,
            building_condition_weight: 0.2,
            charity_weight: 0.2,
            holy_site_boost: 0.2,
            degradation_penalty: 0.1,
            degradation_threshold: 0.3,
            no_charity_penalty: 0.1,
            no_charity_min_followers: 1000,
            charity_per_capita_scale: 0.001,
            min_clergy_ratio: 0.001, // 1 clergy per 1000 followers for full authority
        }
    }
}

/// Per-country state tracking religious authority scores.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ReligiousAuthorityState {
    /// Maps religion engine key → authority score (0.0–1.0).
    #[serde(default)]
    pub authority: BTreeMap<String, f64>,
}

/// Compute religious authority for all religions present in a country.
///
/// # Arguments
/// * `country` - The country to compute authority for.
/// * `cultural_buildings` - All cultural/religious buildings.
/// * `config` - Authority computation parameters.
///
/// # Returns
/// `BTreeMap<String, f64>` mapping religion engine key → authority score.
///
/// # Rules
/// * Identifies religions from `ClassDemographics.religion` via registry lookup.
/// * State religion gets +0.3 boost.
/// * Building condition, charity, and Holy Sites each contribute up to +0.2.
/// * Degradation and no-charity penalties each subtract 0.1.
/// * Phase 78: Authority is scaled by clergy-to-follower ratio. A religion
///   with insufficient clergy has its authority reduced proportionally.
/// * Result clamped to [0.0, 1.0].
pub fn process_religious_authority_turn(
    country: &Country,
    cultural_buildings: &[CulturalBuilding],
    config: &ReligiousAuthorityConfig,
    companies: &[Company],
) -> BTreeMap<String, f64> {
    let reg = culture_registry();

    // Collect all religions present in the country and count followers per religion.
    let mut religion_followers: BTreeMap<String, i64> = BTreeMap::new();
    let mut religion_charity: BTreeMap<String, f64> = BTreeMap::new();

    // Phase 78: Collect clergy FTE per religion from Religion-sector companies.
    let mut religion_clergy_fte: BTreeMap<String, f64> = BTreeMap::new();
    for company in companies {
        if company.sector != Sector::Religion {
            continue;
        }
        if let crate::entities::legal_form::LegalForm::NonProfit(data) = &company.legal_form {
            if !data.religion.is_empty() {
                let rel_key = reg.religion_key_from_display(&data.religion);
                *religion_clergy_fte.entry(rel_key).or_insert(0.0) += company.fulfilled_fte as f64;
            }
        }
    }

    for region in &country.regions {
        for class in region.class_demographics.rural_classes.values() {
            if !class.religion.is_empty() {
                let rel_key = reg.religion_key_from_display(&class.religion);
                *religion_followers.entry(rel_key).or_insert(0) += class.population;
            }
        }
        for class in region.class_demographics.urban_classes.values() {
            if !class.religion.is_empty() {
                let rel_key = reg.religion_key_from_display(&class.religion);
                *religion_followers.entry(rel_key).or_insert(0) += class.population;
            }
        }
    }

    // Sum charity distributed per religion (from cultural buildings).
    for building in cultural_buildings {
        if building.relief_distributed_this_turn > 0.0 {
            // Buildings don't store their religion directly; we infer from region.
            // For now, attribute charity to all religions present in the building's region.
            if let Some(region) = country.regions.iter().find(|r| r.id == building.region_id) {
                let region_religions: std::collections::BTreeSet<String> = region
                    .class_demographics
                    .rural_classes
                    .values()
                    .chain(region.class_demographics.urban_classes.values())
                    .filter(|c| !c.religion.is_empty())
                    .map(|c| reg.religion_key_from_display(&c.religion))
                    .collect();
                for rel_key in region_religions {
                    *religion_charity.entry(rel_key).or_insert(0.0) +=
                        building.relief_distributed_this_turn;
                }
            }
        }
    }

    // Determine state religion from politics.religious_law.
    let is_state_religion = country.politics.religious_law == "State";
    let state_religion_key = if is_state_religion {
        reg.religion_key_from_display(&country.macro_indicators.religion)
    } else {
        String::new()
    };

    // Check for Holy Sites with active temples.
    let mut holy_site_religions: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    for region in &country.regions {
        if let Some(holy_site) = &region.holy_site {
            let has_active_temple = cultural_buildings.iter().any(|b| {
                b.region_id == region.id
                    && (b.building_type == CulturalBuildingType::Temple
                        || b.building_type == CulturalBuildingType::Monastery)
                    && b.condition > config.degradation_threshold
            });
            if has_active_temple {
                holy_site_religions.insert(holy_site.religion_key.clone());
            }
        }
    }

    // Compute authority for each religion.
    let mut result = BTreeMap::new();
    for (rel_key, followers) in &religion_followers {
        let mut authority = config.baseline;

        // State religion boost.
        if is_state_religion && *rel_key == state_religion_key {
            authority += config.state_religion_boost;
        }

        // Building condition factor.
        let religion_buildings: Vec<&CulturalBuilding> = cultural_buildings
            .iter()
            .filter(|b| {
                b.building_type == CulturalBuildingType::Temple
                    || b.building_type == CulturalBuildingType::Monastery
            })
            .filter(|b| {
                // Check if this building's region has followers of this religion.
                if let Some(region) = country.regions.iter().find(|r| r.id == b.region_id) {
                    region
                        .class_demographics
                        .rural_classes
                        .values()
                        .chain(region.class_demographics.urban_classes.values())
                        .any(|c| {
                            !c.religion.is_empty()
                                && reg.religion_key_from_display(&c.religion) == *rel_key
                        })
                } else {
                    false
                }
            })
            .collect();

        if !religion_buildings.is_empty() {
            let avg_condition: f64 = religion_buildings.iter().map(|b| b.condition).sum::<f64>()
                / religion_buildings.len() as f64;
            authority += avg_condition * config.building_condition_weight;

            // Degradation penalty.
            if avg_condition < config.degradation_threshold {
                authority -= config.degradation_penalty;
            }
        }

        // Charity factor.
        let charity = religion_charity.get(rel_key).copied().unwrap_or(0.0);
        if *followers > 0 && charity > 0.0 {
            let charity_per_capita = charity / *followers as f64;
            let charity_factor = (charity_per_capita * config.charity_per_capita_scale).min(1.0);
            authority += charity_factor * config.charity_weight;
        }

        // No-charity penalty.
        if charity == 0.0 && *followers > config.no_charity_min_followers {
            authority -= config.no_charity_penalty;
        }

        // Holy Site boost.
        if holy_site_religions.contains(rel_key) {
            authority += config.holy_site_boost;
        }

        // Phase 78: Scale authority by clergy-to-follower ratio.
        // A religion with insufficient clergy has reduced authority.
        let clergy_fte = religion_clergy_fte.get(rel_key).copied().unwrap_or(0.0);
        let clergy_ratio = if *followers > 0 {
            clergy_fte / *followers as f64
        } else {
            1.0 // No followers → no clergy needed
        };
        let clergy_coverage = if config.min_clergy_ratio > 0.0 {
            (clergy_ratio / config.min_clergy_ratio).min(1.0)
        } else {
            1.0
        };
        authority *= clergy_coverage;

        // Clamp to [0.0, 1.0].
        authority = authority.clamp(0.0, 1.0);
        result.insert(rel_key.clone(), authority);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::society::geography::{ClassDemographics, Region};
    use crate::state::{macro_data::MacroData, Country};

    fn make_region(id: &str, religion: &str, pop: i64) -> Region {
        let mut region = Region::default();
        region.id = id.to_string();
        let mut class = ClassDemographics::default();
        class.population = pop;
        class.religion = religion.to_string();
        region
            .class_demographics
            .rural_classes
            .insert("peasants".into(), class);
        region
    }

    /// Phase 78: Create a Religion-sector company with given clergy FTE.
    fn make_clergy(religion: &str, fte: u32) -> Company {
        let mut c = Company::default();
        c.id = format!("clergy_{}", religion);
        c.sector = Sector::Religion;
        c.legal_form = crate::entities::legal_form::LegalForm::NonProfit(
            crate::entities::legal_form::NonProfitData {
                religion: religion.to_string(),
                is_religious: true,
            },
        );
        c.fulfilled_fte = fte;
        c.worker_capacity = fte;
        c
    }

    /// Phase 78: Sufficient clergy for the test population (1 per 500 followers).
    fn sufficient_clergy(religion: &str, followers: i64) -> Vec<Company> {
        let fte = ((followers as f64 / 500.0).ceil() as u32).max(1);
        vec![make_clergy(religion, fte)]
    }

    #[test]
    fn test_baseline_authority() {
        let mut country = Country::mock_for_tests();
        country.regions.push(make_region("r1", "Catholicism", 500));
        let buildings: Vec<CulturalBuilding> = vec![];
        let config = ReligiousAuthorityConfig::default();
        let companies = sufficient_clergy("Catholicism", 500);
        let result = process_religious_authority_turn(&country, &buildings, &config, &companies);
        let authority = result.get("catholicism").copied().unwrap_or(-1.0);
        // Baseline = 0.3, no charity + followers < 1000 → no penalty
        assert!(
            (authority - 0.3).abs() < 0.01,
            "baseline authority should be 0.3, got {}",
            authority
        );
    }

    #[test]
    fn test_no_charity_penalty() {
        let mut country = Country::mock_for_tests();
        country.regions.push(make_region("r1", "Catholicism", 2000));
        let buildings: Vec<CulturalBuilding> = vec![];
        let config = ReligiousAuthorityConfig::default();
        let companies = sufficient_clergy("Catholicism", 2000);
        let result = process_religious_authority_turn(&country, &buildings, &config, &companies);
        let authority = result.get("catholicism").copied().unwrap_or(-1.0);
        // Baseline 0.3 - no_charity_penalty 0.1 = 0.2
        assert!(
            (authority - 0.2).abs() < 0.01,
            "no charity with 2000 followers → 0.2, got {}",
            authority
        );
    }

    #[test]
    fn test_state_religion_boost() {
        let mut country = Country::mock_for_tests();
        country.politics.religious_law = "State".into();
        country.macro_indicators = MacroData {
            religion: "Catholicism".into(),
            ..Default::default()
        };
        country.regions.push(make_region("r1", "Catholicism", 500));
        let buildings: Vec<CulturalBuilding> = vec![];
        let config = ReligiousAuthorityConfig::default();
        let companies = sufficient_clergy("Catholicism", 500);
        let result = process_religious_authority_turn(&country, &buildings, &config, &companies);
        let authority = result.get("catholicism").copied().unwrap_or(-1.0);
        // Baseline 0.3 + state_religion_boost 0.3 = 0.6
        assert!(
            (authority - 0.6).abs() < 0.01,
            "state religion → 0.6, got {}",
            authority
        );
    }

    #[test]
    fn test_building_condition_boost() {
        let mut country = Country::mock_for_tests();
        country.regions.push(make_region("r1", "Catholicism", 500));
        let building = CulturalBuilding {
            id: "b1".into(),
            building_type: CulturalBuildingType::Temple,
            region_id: "r1".into(),
            condition: 1.0,
            ..Default::default()
        };
        let config = ReligiousAuthorityConfig::default();
        let companies = sufficient_clergy("Catholicism", 500);
        let result = process_religious_authority_turn(&country, &[building], &config, &companies);
        let authority = result.get("catholicism").copied().unwrap_or(-1.0);
        // Baseline 0.3 + condition 1.0 * 0.2 = 0.5
        assert!(
            (authority - 0.5).abs() < 0.01,
            "perfect building → 0.5, got {}",
            authority
        );
    }

    #[test]
    fn test_degradation_penalty() {
        let mut country = Country::mock_for_tests();
        country.regions.push(make_region("r1", "Catholicism", 500));
        let building = CulturalBuilding {
            id: "b1".into(),
            building_type: CulturalBuildingType::Temple,
            region_id: "r1".into(),
            condition: 0.1,
            ..Default::default()
        };
        let config = ReligiousAuthorityConfig::default();
        let companies = sufficient_clergy("Catholicism", 500);
        let result = process_religious_authority_turn(&country, &[building], &config, &companies);
        let authority = result.get("catholicism").copied().unwrap_or(-1.0);
        // Baseline 0.3 + 0.1*0.2=0.02 - degradation 0.1 = 0.22
        assert!(
            (authority - 0.22).abs() < 0.01,
            "degraded building → 0.22, got {}",
            authority
        );
    }

    #[test]
    fn test_authority_clamped() {
        let mut country = Country::mock_for_tests();
        country.politics.religious_law = "State".into();
        country.macro_indicators = MacroData {
            religion: "Catholicism".into(),
            ..Default::default()
        };
        country.regions.push(make_region("r1", "Catholicism", 500));
        let building = CulturalBuilding {
            id: "b1".into(),
            building_type: CulturalBuildingType::Temple,
            region_id: "r1".into(),
            condition: 1.0,
            relief_distributed_this_turn: 10000.0,
            ..Default::default()
        };
        // Add holy site
        country.regions[0].holy_site = Some(crate::society::geography::HolySite {
            religion_key: "catholicism".into(),
            pilgrimage_attractiveness: 0.9,
            display_name: "Sanktuarium".into(),
        });
        let config = ReligiousAuthorityConfig::default();
        let companies = sufficient_clergy("Catholicism", 500);
        let result = process_religious_authority_turn(&country, &[building], &config, &companies);
        let authority = result.get("catholicism").copied().unwrap_or(-1.0);
        assert!(
            authority <= 1.0,
            "authority should be clamped to 1.0, got {}",
            authority
        );
    }
}

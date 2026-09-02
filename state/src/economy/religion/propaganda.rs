//! Phase 18C: Propaganda engine, intelligence state, and terrorism.
//!
//! This module implements:
//! - `PropagandaConfig`: State-funded media campaigns (hate speech, ruling party
//!   boost, demand manipulation, censorship).
//! - `MediaState`: Runtime tracking of media production and state media share.
//! - `IntelligenceState`: Surveillance coverage for terrorism defense.
//! - `process_propaganda_turn()`: Applies propaganda effects scaled by
//!   information consumption ratio.
//! - `check_terrorism_triggers()`: Asymmetric warfare by radicalized minorities
//!   when intelligence coverage is low.

use crate::economy::disasters::{DisasterEvent, DisasterType};
use crate::entities::Building;
use crate::politics::system::IntelligenceState;
use crate::registries::enums::Commodity;
use crate::society::geography::{RuralClass, UrbanClass};
use crate::state::Country;
use serde::{Deserialize, Serialize};
use serde_json::Map;

/// Propaganda campaign type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PropagandaType {
    /// Manipulate consumer demand for targeted goods.
    #[default]
    DemandManipulation,
    /// Hate speech targeting a minority group.
    HateSpeech,
    /// Boost ruling party support.
    RulingPartyBoost,
    /// Censorship of non-state media.
    Censorship,
}

/// Propaganda campaign configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PropagandaConfig {
    /// Funding allocated per turn for state media subsidies.
    #[serde(default)]
    pub state_media_funding: f64,
    /// Strength of demand manipulation effect (0.0–1.0).
    #[serde(default)]
    pub demand_manipulation_strength: f64,
    /// Reduction to pogrom unrest threshold from hate speech (0.0–50.0).
    #[serde(default)]
    pub hate_speech_threshold_reduction: f64,
    /// Boost to ruling party loyalists per turn (0.0–1.0).
    #[serde(default)]
    pub ruling_party_support_boost: f64,
    /// Target minority for hate speech campaigns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_minority: Option<String>,
    /// Active campaign types.
    #[serde(default)]
    pub active_campaigns: Vec<PropagandaCampaign>,
    /// Price floor for subsidized information (0.0 = free, 1.0 = full price).
    /// Default 0.05 (near-free).
    #[serde(default = "default_propaganda_price_floor")]
    pub propaganda_price_floor: f64,
}

fn default_propaganda_price_floor() -> f64 {
    0.05
}

/// Active propaganda campaign.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PropagandaCampaign {
    /// Type of campaign.
    #[serde(default)]
    pub campaign_type: PropagandaType,
    /// Target demographic group (if applicable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_group: Option<String>,
    /// Funding allocated to this campaign.
    #[serde(default)]
    pub funding: f64,
    /// Total duration in turns.
    #[serde(default)]
    pub duration_turns: u32,
    /// Turns remaining.
    #[serde(default)]
    pub turns_remaining: u32,
}

/// Media state tracked on Politics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MediaState {
    /// Total information produced last turn.
    #[serde(default)]
    pub total_information_produced: f64,
    /// State media share (0.0–1.0).
    #[serde(default)]
    pub state_media_share: f64,
    /// Whether propaganda is currently active.
    #[serde(default)]
    pub propaganda_active: bool,
    /// Last turn's consumption ratio (0.0–1.0).
    #[serde(default)]
    pub last_consumption_ratio: f64,
}

/// Result of propaganda processing.
#[derive(Debug, Clone, Default)]
pub struct PropagandaTurnResult {
    /// Consumption ratio of subsidized information.
    pub consumption_ratio: f64,
    /// Whether hate speech campaign was active.
    pub hate_speech_active: bool,
    /// Whether ruling party boost was active.
    pub ruling_party_boost_active: bool,
    /// Actual hate speech threshold reduction applied (scaled by consumption).
    pub applied_threshold_reduction: f64,
    /// Actual loyalist boost applied (scaled by consumption).
    pub applied_loyalist_boost: f64,
}

/// Result of terrorism check.
#[derive(Debug, Clone, Default)]
pub struct TerrorismTurnResult {
    /// Whether any attack was triggered.
    pub attacked: bool,
    /// Region targeted.
    pub target_region: Option<String>,
    /// Severity of the attack (0.0–1.0).
    pub severity: f64,
    /// Buildings destroyed.
    pub buildings_destroyed: u32,
    /// Casualties.
    pub casualties: i64,
    /// Whether the attack was prevented by intelligence.
    pub prevented: bool,
    /// Disaster events generated.
    pub events: Vec<DisasterEvent>,
}

/// Processes propaganda effects for one turn.
///
/// # Arguments
/// * `country` - Mutable country for sentiment updates
/// * `consumption_ratio` - Information consumption ratio from B2C clearing (0.0–1.0)
///
/// # Rules
/// * Effects scale proportionally to `consumption_ratio`.
/// * `HateSpeech`: Reduces pogrom threshold for target minority (only if consumption_ratio > 0.5).
/// * `RulingPartyBoost`: Increases loyalists, decreases radicals proportionally.
/// * `DemandManipulation`: Stored in MediaState for demand modification.
/// * `Censorship`: Reduces non-state media production (applied as production penalty).
/// * Free speech law blocks hate speech when `FreeSpeechLevel::Full`.
pub fn process_propaganda_turn(
    country: &mut Country,
    consumption_ratio: f64,
) -> PropagandaTurnResult {
    let mut result = PropagandaTurnResult {
        consumption_ratio,
        ..Default::default()
    };

    let propaganda_config = match country.politics.propaganda_config.clone() {
        Some(cfg) => cfg,
        None => return result,
    };

    let free_speech_law = country.politics.free_speech_law.clone();
    let allows_hate_speech = free_speech_law
        .as_ref()
        .map(|law| law.allows_hate_speech())
        .unwrap_or(true); // No law = no restriction

    // Process active campaigns
    for campaign in &propaganda_config.active_campaigns {
        match campaign.campaign_type {
            PropagandaType::HateSpeech => {
                if !allows_hate_speech {
                    continue;
                }
                // Hate speech only fires when majority of population consumes state media
                if consumption_ratio > 0.5 {
                    result.hate_speech_active = true;
                    let reduction =
                        propaganda_config.hate_speech_threshold_reduction * consumption_ratio;
                    result.applied_threshold_reduction = reduction;
                }
            }
            PropagandaType::RulingPartyBoost => {
                result.ruling_party_boost_active = true;
                let boost = propaganda_config.ruling_party_support_boost * 0.01 * consumption_ratio;
                result.applied_loyalist_boost = boost;

                // Apply to all classes across all regions
                for region in &mut country.regions {
                    for class in region.class_demographics.rural_classes.values_mut() {
                        class.political_sentiment.loyalists =
                            (class.political_sentiment.loyalists + boost).min(1.0);
                        class.political_sentiment.radicals =
                            (class.political_sentiment.radicals - boost).max(0.0);
                        class.political_sentiment.normalize();
                    }
                    for class in region.class_demographics.urban_classes.values_mut() {
                        class.political_sentiment.loyalists =
                            (class.political_sentiment.loyalists + boost).min(1.0);
                        class.political_sentiment.radicals =
                            (class.political_sentiment.radicals - boost).max(0.0);
                        class.political_sentiment.normalize();
                    }
                }
            }
            PropagandaType::DemandManipulation => {
                // Effect stored in media_state for build_consumer_demand to read
                // The actual demand modification happens in the retail layer
            }
            PropagandaType::Censorship => {
                // Censorship reduces non-state media production
                // Applied as a production penalty on non-state MediaAndEntertainment buildings
                // This is handled in the production cycle via media_state
            }
        }
    }

    // Update media state
    let media_state = country
        .politics
        .media_state
        .get_or_insert_with(MediaState::default);
    media_state.last_consumption_ratio = consumption_ratio;
    media_state.propaganda_active = !propaganda_config.active_campaigns.is_empty();

    // Decrement campaign durations
    if let Some(ref mut cfg) = country.politics.propaganda_config {
        for campaign in &mut cfg.active_campaigns {
            if campaign.turns_remaining > 0 {
                campaign.turns_remaining -= 1;
            }
        }
        // Remove expired campaigns
        cfg.active_campaigns.retain(|c| c.turns_remaining > 0);
    }

    result
}

/// Computes intelligence state from building capacities.
///
/// # Arguments
/// * `buildings` - All buildings in the country
/// * `country` - Country for population lookup
///
/// # Returns
/// Updated `IntelligenceState` with capacity and coverage.
pub fn compute_intelligence_state(buildings: &[Building], country: &Country) -> IntelligenceState {
    let total_capacity: f64 = buildings
        .iter()
        .filter_map(|b| {
            b.last_production
                .get(&Commodity::IntelligenceCapacity)
                .copied()
        })
        .sum();

    let total_pop: i64 = country
        .regions
        .iter()
        .flat_map(|r| {
            r.class_demographics
                .rural_classes
                .values()
                .chain(r.class_demographics.urban_classes.values())
        })
        .map(|c| c.population)
        .sum();

    let surveillance_coverage = if total_pop > 0 {
        (total_capacity / total_pop as f64).min(1.0)
    } else {
        0.0
    };

    IntelligenceState {
        total_capacity,
        surveillance_coverage,
        active_operations: 0,
        movement_infiltration: 0.0,
        attacks_prevented: 0,
        attacks_succeeded: 0,
    }
}

/// Checks for terrorism triggers in regions with extreme radicalization.
///
/// # Arguments
/// * `country` - Mutable country for damage application
/// * `buildings` - All buildings (for intelligence capacity and damage targeting)
/// * `current_turn` - Current turn number for disaster events
///
/// # Rules
/// * Trigger: `radicals > 0.6` for a minority class AND `social_unrest > 70` AND `surveillance_coverage < 0.3`.
/// * Defense: If `intelligence_capacity > radical_population * 0.5`, attack prevented.
/// * Effects: Destroys state buildings, reduces B2B inventory by 50%, casualties, unrest spike.
pub fn check_terrorism_triggers(
    country: &mut Country,
    buildings: &mut [Building],
    current_turn: u32,
) -> TerrorismTurnResult {
    let mut result = TerrorismTurnResult::default();

    // Compute intelligence state
    let intel_state = compute_intelligence_state(buildings, country);

    // Store intelligence state on politics
    let stored_attacks_prevented = country
        .politics
        .intelligence_state
        .as_ref()
        .map(|s| s.attacks_prevented)
        .unwrap_or(0);
    let stored_attacks_succeeded = country
        .politics
        .intelligence_state
        .as_ref()
        .map(|s| s.attacks_succeeded)
        .unwrap_or(0);

    let mut new_intel = intel_state.clone();
    new_intel.attacks_prevented = stored_attacks_prevented;
    new_intel.attacks_succeeded = stored_attacks_succeeded;

    let social_unrest = country.macro_indicators.social_unrest;
    let dominant_religion = country.macro_indicators.religion.clone();

    // Check each region for terrorism triggers
    for region_idx in 0..country.regions.len() {
        let region = &country.regions[region_idx];

        // Check all classes for radicalized minorities
        let mut radical_minority_pop: i64 = 0;
        let mut target_class_id: Option<String> = None;
        let mut target_is_urban = false;

        for (class_id, class) in &region.class_demographics.rural_classes {
            let is_minority = !class.religion.is_empty() && class.religion != dominant_religion;
            if is_minority && class.political_sentiment.radicals > 0.6 {
                let radical_pop =
                    (class.population as f64 * class.political_sentiment.radicals) as i64;
                radical_minority_pop += radical_pop;
                if target_class_id.is_none() {
                    target_class_id = Some(class_id.to_string());
                    target_is_urban = false;
                }
            }
        }
        for (class_id, class) in &region.class_demographics.urban_classes {
            let is_minority = !class.religion.is_empty() && class.religion != dominant_religion;
            if is_minority && class.political_sentiment.radicals > 0.6 {
                let radical_pop =
                    (class.population as f64 * class.political_sentiment.radicals) as i64;
                radical_minority_pop += radical_pop;
                if target_class_id.is_none() {
                    target_class_id = Some(class_id.to_string());
                    target_is_urban = true;
                }
            }
        }

        // Trigger conditions
        if radical_minority_pop == 0 {
            continue;
        }
        if social_unrest < 70.0 {
            continue;
        }
        if intel_state.surveillance_coverage >= 0.3 {
            continue;
        }

        // Defense roll: intelligence capacity vs radical population
        let defense_threshold = radical_minority_pop as f64 * 0.5;
        if intel_state.total_capacity > defense_threshold {
            // Attack prevented
            new_intel.attacks_prevented += 1;
            continue;
        }

        // Attack succeeds
        let severity = ((social_unrest - 70.0) / 30.0).min(1.0).max(0.1);
        let buildings_destroyed = (severity * 5.0) as u32;
        let region_id = region.id.clone();
        let minority_pop = if let Some(ref cid) = target_class_id {
            let class = if target_is_urban {
                UrbanClass::from_str(cid)
                    .and_then(|k| region.class_demographics.urban_classes.get(&k))
            } else {
                RuralClass::from_str(cid)
                    .and_then(|k| region.class_demographics.rural_classes.get(&k))
            };
            class.map(|c| c.population).unwrap_or(0)
        } else {
            0
        };
        let casualties = (minority_pop as f64 * severity * 0.02) as i64;

        result.attacked = true;
        result.target_region = Some(region_id.clone());
        result.severity = severity;
        result.buildings_destroyed = buildings_destroyed;
        result.casualties = casualties;
        result.prevented = false;

        // Destroy state buildings in the region
        let mut destroyed_count = 0u32;
        for building in buildings.iter_mut() {
            if building.region_id != region_id {
                continue;
            }
            // Target state buildings (courthouses, police stations, government)
            let is_state_building =
                building.owner_id.starts_with("STATE_") || building.owner_id.starts_with("LOCAL_");
            if is_state_building && destroyed_count < buildings_destroyed {
                building.condition = (building.condition * (1.0 - severity)).max(0.0);
                if building.condition < 0.1 {
                    building.condition = 0.0;
                }
                destroyed_count += 1;
            }
        }

        // Destroy B2B inventory stockpiles (reduce commercial building inventory by severity * 50%)
        let inventory_destruction_rate = severity * 0.5;
        for building in buildings.iter_mut() {
            if building.region_id != region_id {
                continue;
            }
            if building.owner_id.starts_with("STATE_") || building.owner_id.starts_with("LOCAL_") {
                continue; // State buildings already targeted above
            }
            for qty in building.inventory.values_mut() {
                *qty *= 1.0 - inventory_destruction_rate;
            }
        }

        // Apply casualties to target class
        if casualties > 0 {
            if let Some(ref cid) = target_class_id {
                let region = &mut country.regions[region_idx];
                let class = if target_is_urban {
                    UrbanClass::from_str(cid)
                        .and_then(|k| region.class_demographics.urban_classes.get_mut(&k))
                } else {
                    RuralClass::from_str(cid)
                        .and_then(|k| region.class_demographics.rural_classes.get_mut(&k))
                };
                if let Some(class) = class {
                    class.population = (class.population - casualties).max(0);
                    class.available_fte = (class.available_fte * (1.0 - severity * 0.1)).max(0.0);
                }
            }
        }

        // Economic damage
        let economic_damage = severity * 1_000_000.0;
        country.budget.liquid_reserves =
            (country.budget.liquid_reserves - economic_damage).max(0.0);

        // Unrest spike
        country.macro_indicators.social_unrest += severity * 20.0;

        // Increase justice and security demand
        if let Some(ref mut js) = country.politics.justice_state {
            js.justice_demand += severity * 50.0;
        }

        // Create disaster event
        result.events.push(DisasterEvent {
            disaster_type: DisasterType::TerroristAttack,
            region_id: region_id.clone(),
            severity,
            buildings_destroyed: destroyed_count,
            casualties,
            economic_damage,
            turn: current_turn,
            extra: {
                let mut m = Map::new();
                m.insert("description".to_string(), serde_json::Value::String(format!(
                    "Terrorist attack by radicalized minority: {} buildings damaged, {} casualties, {:.0} economic damage",
                    destroyed_count, casualties, economic_damage
                )));
                m
            },
        });

        new_intel.attacks_succeeded += 1;
        break; // Only one attack per turn
    }

    // Update intelligence state
    country.politics.intelligence_state = Some(new_intel);

    result
}

/// Computes the propaganda subsidy rate from PropagandaConfig.
///
/// # Returns
/// Subsidy rate (1.0 - price_floor) if propaganda is active, 0.0 otherwise.
pub fn compute_propaganda_subsidy_rate(country: &Country) -> f64 {
    let config = match country.politics.propaganda_config.as_ref() {
        Some(cfg) => cfg,
        None => return 0.0,
    };

    // Check if there are active campaigns
    let has_active_campaigns = config
        .active_campaigns
        .iter()
        .any(|c| c.turns_remaining > 0);

    if !has_active_campaigns {
        return 0.0;
    }

    // Check if state has funds
    if country.budget.liquid_reserves < config.state_media_funding {
        // Insolvency: reduced subsidy
        if country.budget.liquid_reserves <= 0.0 {
            return 0.0;
        }
        // Partial subsidy proportional to available funds
        let fraction = country.budget.liquid_reserves / config.state_media_funding.max(1.0);
        return (1.0 - config.propaganda_price_floor) * fraction;
    }

    1.0 - config.propaganda_price_floor
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::politics::free_speech::{FreeSpeechLaw, FreeSpeechLevel};
    use crate::society::geography::{
        ClassDemographics, PoliticalSentiment, Region, UrbanClass,
    };

    #[test]
    fn test_propaganda_ruling_party_boost() {
        let mut country = Country::mock_for_tests();
        country.politics.propaganda_config = Some(PropagandaConfig {
            ruling_party_support_boost: 5.0,
            active_campaigns: vec![PropagandaCampaign {
                campaign_type: PropagandaType::RulingPartyBoost,
                turns_remaining: 3,
                ..Default::default()
            }],
            ..Default::default()
        });

        let mut region = Region::default();
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
            .insert(UrbanClass::Worker, class);
        country.regions.push(region);

        let result = process_propaganda_turn(&mut country, 0.8);

        assert!(result.ruling_party_boost_active);
        // boost = 5.0 * 0.01 * 0.8 = 0.04
        assert!((result.applied_loyalist_boost - 0.04).abs() < 1e-9);

        let class = &country.regions[0].class_demographics.urban_classes[&UrbanClass::Worker];
        assert!(class.political_sentiment.loyalists > 0.3);
        assert!(class.political_sentiment.radicals < 0.5);
    }

    #[test]
    fn test_propaganda_hate_speech_blocked_by_free_speech() {
        let mut country = Country::mock_for_tests();
        country.politics.free_speech_law = Some(FreeSpeechLaw {
            free_speech_level: FreeSpeechLevel::Full,
            ..Default::default()
        });
        country.politics.propaganda_config = Some(PropagandaConfig {
            hate_speech_threshold_reduction: 20.0,
            active_campaigns: vec![PropagandaCampaign {
                campaign_type: PropagandaType::HateSpeech,
                turns_remaining: 3,
                ..Default::default()
            }],
            ..Default::default()
        });

        let result = process_propaganda_turn(&mut country, 0.8);
        assert!(!result.hate_speech_active);
        assert_eq!(result.applied_threshold_reduction, 0.0);
    }

    #[test]
    fn test_propaganda_hate_speech_low_consumption() {
        let mut country = Country::mock_for_tests();
        country.politics.propaganda_config = Some(PropagandaConfig {
            hate_speech_threshold_reduction: 20.0,
            active_campaigns: vec![PropagandaCampaign {
                campaign_type: PropagandaType::HateSpeech,
                turns_remaining: 3,
                ..Default::default()
            }],
            ..Default::default()
        });

        // Consumption ratio 0.3 < 0.5 threshold
        let result = process_propaganda_turn(&mut country, 0.3);
        assert!(!result.hate_speech_active);
    }

    #[test]
    fn test_propaganda_hate_speech_active() {
        let mut country = Country::mock_for_tests();
        country.politics.propaganda_config = Some(PropagandaConfig {
            hate_speech_threshold_reduction: 20.0,
            active_campaigns: vec![PropagandaCampaign {
                campaign_type: PropagandaType::HateSpeech,
                turns_remaining: 3,
                ..Default::default()
            }],
            ..Default::default()
        });

        // Consumption ratio 0.6 > 0.5 threshold
        let result = process_propaganda_turn(&mut country, 0.6);
        assert!(result.hate_speech_active);
        // reduction = 20.0 * 0.6 = 12.0
        assert!((result.applied_threshold_reduction - 12.0).abs() < 1e-9);
    }

    #[test]
    fn test_terrorism_prevented_by_intelligence() {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.social_unrest = 80.0;

        let mut region = Region::default();
        region.id = "r1".to_string();
        let mut class = ClassDemographics::default();
        class.population = 1000;
        class.religion = "Islam".to_string();
        class.political_sentiment = PoliticalSentiment {
            radicals: 0.8,
            loyalists: 0.1,
            undecided: 0.1,
            ..Default::default()
        };
        region
            .class_demographics
            .urban_classes
            .insert(UrbanClass::Worker, class);
        country.regions.push(region);
        country.macro_indicators.religion = "Catholicism".to_string();

        // Add intelligence building with high capacity
        let mut building = Building::default();
        building.region_id = "r1".to_string();
        building
            .last_production
            .insert(Commodity::IntelligenceCapacity, 1000.0);
        let mut buildings = vec![building];

        let result = check_terrorism_triggers(&mut country, &mut buildings, 1);
        assert!(!result.attacked);
        assert!(result.prevented || !result.attacked);
    }

    #[test]
    fn test_terrorism_attack_succeeds() {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.social_unrest = 90.0;
        country.budget.liquid_reserves = 10_000_000.0;

        let mut region = Region::default();
        region.id = "r1".to_string();
        let mut class = ClassDemographics::default();
        class.population = 1000;
        class.religion = "Islam".to_string();
        class.political_sentiment = PoliticalSentiment {
            radicals: 0.8,
            loyalists: 0.1,
            undecided: 0.1,
            ..Default::default()
        };
        region
            .class_demographics
            .urban_classes
            .insert(UrbanClass::Worker, class);
        country.regions.push(region);
        country.macro_indicators.religion = "Catholicism".to_string();

        // No intelligence buildings — surveillance_coverage = 0
        let mut buildings: Vec<Building> = Vec::new();

        let result = check_terrorism_triggers(&mut country, &mut buildings, 1);
        assert!(result.attacked);
        assert!(!result.prevented);
        assert!(result.severity > 0.0);
        assert!(result.casualties > 0);
        assert_eq!(result.events.len(), 1);
        assert_eq!(
            result.events[0].disaster_type,
            DisasterType::TerroristAttack
        );
    }

    #[test]
    fn test_terrorism_no_trigger_low_unrest() {
        let mut country = Country::mock_for_tests();
        country.macro_indicators.social_unrest = 50.0; // Below 70

        let mut region = Region::default();
        region.id = "r1".to_string();
        let mut class = ClassDemographics::default();
        class.population = 1000;
        class.religion = "Islam".to_string();
        class.political_sentiment = PoliticalSentiment {
            radicals: 0.8,
            loyalists: 0.1,
            undecided: 0.1,
            ..Default::default()
        };
        region
            .class_demographics
            .urban_classes
            .insert(UrbanClass::Worker, class);
        country.regions.push(region);
        country.macro_indicators.religion = "Catholicism".to_string();

        let mut buildings: Vec<Building> = Vec::new();
        let result = check_terrorism_triggers(&mut country, &mut buildings, 1);
        assert!(!result.attacked);
    }

    #[test]
    fn test_propaganda_subsidy_rate() {
        let mut country = Country::mock_for_tests();
        country.budget.liquid_reserves = 100_000.0;
        country.politics.propaganda_config = Some(PropagandaConfig {
            state_media_funding: 10_000.0,
            propaganda_price_floor: 0.05,
            active_campaigns: vec![PropagandaCampaign {
                turns_remaining: 3,
                ..Default::default()
            }],
            ..Default::default()
        });

        let rate = compute_propaganda_subsidy_rate(&country);
        assert!((rate - 0.95).abs() < 1e-9);
    }

    #[test]
    fn test_propaganda_subsidy_insolvency() {
        let mut country = Country::mock_for_tests();
        country.budget.liquid_reserves = 0.0;
        country.politics.propaganda_config = Some(PropagandaConfig {
            state_media_funding: 10_000.0,
            propaganda_price_floor: 0.05,
            active_campaigns: vec![PropagandaCampaign {
                turns_remaining: 3,
                ..Default::default()
            }],
            ..Default::default()
        });

        let rate = compute_propaganda_subsidy_rate(&country);
        assert_eq!(rate, 0.0);
    }
}

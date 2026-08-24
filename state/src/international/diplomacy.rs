#![allow(missing_docs)]

use crate::international::DiplomaticRelation;
use crate::politics::vip_registry::DiplomaticPostType;
use crate::state::GameState;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Generates the full bilateral diplomacy matrix for a set of countries.
pub fn generate_diplomacy(countries: &[String]) -> HashMap<String, HashMap<String, DiplomaticRelation>> {
    let mut rng = rand::thread_rng();
    let mut diplomacy: HashMap<String, HashMap<String, DiplomaticRelation>> = HashMap::new();

    for c1 in countries {
        let mut inner = HashMap::new();
        for c2 in countries {
            if c1 == c2 {
                continue;
            }
            let relations = rng.gen_range(-100..=100);
            let mut rel = DiplomaticRelation {
                relations,
                frozen_turns: 0,
                ban_import: false,
                ban_export: false,
                free_trade: false,
                customs_union: false,
                investment_treaty: false,
                economic_community: false,
                treaty_description: "None".to_string(),
                embargo_penalty: 0.0,
            };
            if relations < -50 && rng.gen::<f64>() < 0.5 {
                rel.ban_export = true;
                rel.ban_import = true;
            } else if relations > 50 && rng.gen::<f64>() < 0.5 {
                rel.free_trade = true;
            }
            inner.insert(c2.clone(), rel);
        }
        diplomacy.insert(c1.clone(), inner);
    }

    diplomacy
}

/// Phase 78: Diplomatic post capacity configuration.
///
/// Defines the maximum number of diplomats of each type that a country
/// can post to a single host country. These are structural limits based
/// on diplomatic protocol, not magic numbers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiplomaticPostCap {
    /// Maximum ambassadors per host country (1 — only one chief of mission).
    pub ambassador: usize,
    /// Maximum consuls per host country (2 — consular representation).
    pub consul: usize,
    /// Maximum spies per host country (3 — higher risk justifies more).
    pub spy: usize,
    /// Maximum military attachés per host country (1 — single defense liaison).
    pub military_attache: usize,
}

impl Default for DiplomaticPostCap {
    fn default() -> Self {
        Self {
            ambassador: 1,
            consul: 2,
            spy: 3,
            military_attache: 1,
        }
    }
}

impl DiplomaticPostCap {
    /// Returns the cap for a given diplomatic post type.
    pub fn for_post_type(&self, post_type: &DiplomaticPostType) -> usize {
        match post_type {
            DiplomaticPostType::Ambassador => self.ambassador,
            DiplomaticPostType::Consul => self.consul,
            DiplomaticPostType::Spy => self.spy,
            DiplomaticPostType::MilitaryAttache => self.military_attache,
        }
    }
}

/// Phase 66: Count diplomats of a given type posted from `home` to `host`.
pub fn count_diplomats(state: &GameState, home: &str, host: &str, post_type: &DiplomaticPostType) -> usize {
    let Some(country) = state.countries.get(home) else {
        return 0;
    };
    let Some(registry) = &country.politics.vip_registry else {
        return 0;
    };
    registry.vips.values().filter(|vip| {
        vip.diplomatic_post.as_ref().is_some_and(|post| {
            post.host_country == host && &post.post_type == post_type
        })
    }).count()
}

/// Phase 66: Compute diplomatic modifiers from VIP traits for a posted diplomat.
///
/// Returns (relation_improvement_bonus, spy_discovery_risk_modifier).
/// - Charismatic → +relation improvement rate
/// - Diplomatic → +relation improvement rate (smaller)
/// - Cautious → -discovery risk for spies
/// - Paranoid → +discovery risk for spies (self-defeating)
pub fn compute_diplomat_modifiers(traits: &[String]) -> (f64, f64) {
    let mut relation_bonus = 0.0;
    let mut discovery_modifier = 1.0;
    for trait_id in traits {
        match trait_id.as_str() {
            "Charismatic" => relation_bonus += 0.5,
            "Diplomatic" => relation_bonus += 0.3,
            "Cautious" => discovery_modifier *= 0.7,
            "Paranoid" => discovery_modifier *= 1.3,
            "Incompetent" => relation_bonus -= 0.3,
            _ => {}
        }
    }
    (relation_bonus, discovery_modifier)
}

/// Updates diplomatic relations dynamically based on physical world events.
///
/// Phase 66 additions:
/// - Ambassador presence boosts relation improvement rate.
/// - Spy activity generates intelligence and runs discovery risk checks.
/// - Caught spies trigger automatic ExpelDiplomat and relation freeze.
///
/// # Arguments
/// * `state` - Immutable game state (reads trade balances, politics, military fronts).
/// * `diplomacy` - Mutable bilateral diplomacy matrix.
/// * `diplomatic_config` - Configuration for spy discovery risk, relation rates.
/// * `current_turn` - Current global turn (for intel timestamping).
/// * `intel_updates` - Output buffer: (observer_country, target_country, new_intel_level) tuples.
/// * `expel_actions` - Output buffer: (spy_home_country, host_country) pairs for caught spies.
pub fn process_diplomacy_turn(
    state: &GameState,
    diplomacy: &mut HashMap<String, HashMap<String, DiplomaticRelation>>,
    diplomatic_config: &crate::international::fog_of_war::DiplomaticConfig,
    _current_turn: u32,
    intel_updates: &mut Vec<(String, String, crate::international::fog_of_war::IntelLevel)>,
    expel_actions: &mut Vec<(String, String)>,
) {
    let mut rng = rand::thread_rng();

    // Collect sorted country names for deterministic iteration
    let mut sorted_names: Vec<&String> = state.countries.keys().collect();
    sorted_names.sort();

    for &c1_name in &sorted_names {
        let c1 = match state.countries.get(c1_name) {
            Some(c) => c,
            None => continue,
        };

        // Gather c1's involved front countries
        let c1_front_countries: Vec<&String> = c1
            .military_fronts
            .iter()
            .flat_map(|f| f.involved_countries.iter())
            .filter(|c| *c != c1_name)
            .collect();

        // Read c1's trade balance from budget.extra
        let c1_trade_balance = c1
            .budget
            .extra
            .get("bilans_handlowy")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let c1_exports = c1
            .budget
            .extra
            .get("exports")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let c1_cultural_group = &c1.macro_indicators.cultural_group;
        let c1_is_democratic = c1.politics.government_form.is_democratic();

        // Phase 66: Count ambassadors posted from c1 to each partner
        // and spies posted from c1 to each partner
        let c1_vip_registry = c1.politics.vip_registry.as_ref();

        for &c2_name in &sorted_names {
            if c1_name == c2_name {
                continue;
            }

            let c2 = match state.countries.get(c2_name) {
                Some(c) => c,
                None => continue,
            };

            // Get or create the relation c1 → c2
            let rel = match diplomacy
                .get_mut(c1_name)
                .and_then(|partners| partners.get_mut(c2_name))
            {
                Some(r) => r,
                None => continue,
            };

            // Frozen relations: skip all changes
            if rel.frozen_turns > 0 {
                rel.frozen_turns -= 1;
                continue;
            }

            let mut delta: i64 = 0;

            // 1. Trade imbalance: if c1 has a large deficit, resentment toward partners
            if c1_trade_balance < -c1.budget.gdp * 0.05 {
                delta -= 2;
            } else if c1_trade_balance > c1.budget.gdp * 0.05 {
                delta += 1;
            }

            // 2. Ideological distance
            let c2_is_democratic = c2.politics.government_form.is_democratic();
            if c1_is_democratic != c2_is_democratic {
                delta -= 1;
            }

            // 3. Border tension: both countries involved in the same front
            if c1_front_countries.contains(&c2_name) {
                delta -= 2;
            }

            // 4. Cultural affinity
            if !c1_cultural_group.is_empty() && c1_cultural_group == &c2.macro_indicators.cultural_group {
                delta += 1;
            }

            // 5. Trade volume: high exports build trust
            if c1_exports > c1.budget.gdp * 0.10 {
                delta += 1;
            }

            // 6. Phase 66: Ambassador presence boosts relation improvement
            let has_ambassador = c1_vip_registry.is_some_and(|reg| {
                reg.vips.values().any(|vip| {
                    vip.diplomatic_post.as_ref().is_some_and(|post| {
                        post.host_country == *c2_name && post.post_type == DiplomaticPostType::Ambassador
                    })
                })
            });
            if has_ambassador {
                // Find the ambassador's traits for modifier computation
                if let Some(reg) = c1_vip_registry {
                    for vip in reg.vips.values() {
                        if vip.diplomatic_post.as_ref().is_some_and(|p| {
                            p.host_country == *c2_name && p.post_type == DiplomaticPostType::Ambassador
                        }) {
                            let (rel_bonus, _) = compute_diplomat_modifiers(&vip.traits);
                            delta += 1 + rel_bonus as i64;
                            break;
                        }
                    }
                }
            }

            // Apply delta and clamp
            rel.relations = (rel.relations + delta).clamp(-100, 100);

            // Threshold triggers
            if rel.relations < -50 && rng.gen::<f64>() < 0.3 {
                rel.ban_export = true;
                rel.ban_import = true;
                rel.free_trade = false;
                rel.customs_union = false;
                rel.treaty_description = "Embargo".to_string();
            } else if rel.relations > 50 && rng.gen::<f64>() < 0.3 {
                rel.ban_export = false;
                rel.ban_import = false;
                rel.free_trade = true;
                rel.treaty_description = "Free Trade".to_string();
            } else if rel.relations > -10 && rel.relations < 10 {
                // Relations normalized — lift embargoes if relations improve
                if rel.relations > 0 {
                    rel.ban_export = false;
                    rel.ban_import = false;
                }
            }

            // 7. Phase 66: Spy activity — intel generation and discovery risk
            let spy_vips: Vec<_> = c1_vip_registry.map_or(Vec::new(), |reg| {
                reg.vips.values().filter(|vip| {
                    vip.diplomatic_post.as_ref().is_some_and(|post| {
                        post.host_country == *c2_name && post.post_type == DiplomaticPostType::Spy
                    })
                }).collect()
            });

            for spy in &spy_vips {
                let (_, discovery_mod) = compute_diplomat_modifiers(&spy.traits);

                // Host country counter-intelligence: justice coverage reduces spy success
                let host_justice = c2.budget.extra
                    .get("justice_coverage")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(50.0) / 100.0;

                // Discovery risk = base_risk * trait_modifier * host_justice_coverage
                let discovery_risk = diplomatic_config.spy_discovery_risk * discovery_mod * host_justice;
                if rng.gen::<f64>() < discovery_risk {
                    // Spy caught! Trigger expulsion and relation freeze
                    expel_actions.push((c1_name.clone(), c2_name.clone()));
                    rel.frozen_turns = diplomatic_config.spy_caught_freeze_turns as i64;
                    rel.relations = (rel.relations - diplomatic_config.spy_caught_relation_penalty).clamp(-100, 100);
                } else {
                    // Spy succeeds — upgrade intel level
                    let new_level = crate::international::fog_of_war::IntelLevel::NarrowRange;
                    intel_updates.push((c1_name.clone(), c2_name.clone(), new_level));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::international::fog_of_war::DiplomaticConfig;

    #[test]
    fn test_generate_diplomacy_creates_matrix() {
        let countries = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let diplomacy = generate_diplomacy(&countries);
        assert_eq!(diplomacy.len(), 3);
        assert!(diplomacy["A"].contains_key("B"));
        assert!(!diplomacy["A"].contains_key("A"));
        // Check renamed fields
        let rel = &diplomacy["A"]["B"];
        assert!(rel.relations >= -100 && rel.relations <= 100);
        assert_eq!(rel.frozen_turns, 0);
        assert_eq!(rel.treaty_description, "None");
    }

    #[test]
    fn test_compute_diplomat_modifiers() {
        let traits = vec!["Charismatic".to_string(), "Cautious".to_string()];
        let (rel_bonus, disc_mod) = compute_diplomat_modifiers(&traits);
        assert!(rel_bonus > 0.0, "Charismatic should boost relations");
        assert!(disc_mod < 1.0, "Cautious should reduce discovery risk");
    }

    #[test]
    fn test_compute_diplomat_modifiers_paranoid() {
        let traits = vec!["Paranoid".to_string()];
        let (_, disc_mod) = compute_diplomat_modifiers(&traits);
        assert!(disc_mod > 1.0, "Paranoid should increase discovery risk");
    }

    #[test]
    fn test_process_diplomacy_turn_with_config() {
        let mut state = GameState::default();
        let mut c1 = crate::state::Country::mock_for_tests();
        c1.name = "TestA".to_string();
        let mut c2 = crate::state::Country::mock_for_tests();
        c2.name = "TestB".to_string();
        state.countries.insert("TestA".to_string(), c1);
        state.countries.insert("TestB".to_string(), c2);

        let mut diplomacy = generate_diplomacy(&["TestA".to_string(), "TestB".to_string()]);
        let config = DiplomaticConfig::default();
        let mut intel_updates = Vec::new();
        let mut expel_actions = Vec::new();

        process_diplomacy_turn(&state, &mut diplomacy, &config, 1, &mut intel_updates, &mut expel_actions);

        // Should not crash, relations should be in valid range
        let rel = &diplomacy["TestA"]["TestB"];
        assert!(rel.relations >= -100 && rel.relations <= 100);
    }
}

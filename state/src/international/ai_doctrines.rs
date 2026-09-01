//! Phase 67: Geopolitical AI Doctrines.
//!
//! AI nations pragmatically assess resource deficits, geographical borders,
//! and ideological differences to determine their geopolitical doctrine.
//! The evaluation is kept lightweight to avoid blocking the parallel
//! processing loop.

use crate::politics::vip_registry::DiplomaticPostType;
use crate::state::diplomatic_actions::DiplomaticAction;
use crate::state::GameState;
use serde::{Deserialize, Serialize};

/// A geopolitical doctrine that guides an AI nation's diplomatic behavior.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum GeopoliticalDoctrine {
    /// Nation seeks to secure a specific commodity it lacks.
    #[default]
    Balanced,
    /// Nation is expansionist — seeks territory and military dominance.
    Expansionist,
    /// Nation is isolationist — minimizes diplomatic engagement.
    Isolationist,
    /// Nation actively seeks alliances and treaties.
    AllianceSeeker,
    /// Nation is focused on a specific resource dependency.
    ResourceDependency {
        /// The commodity the nation desperately needs.
        commodity: String,
    },
}

impl GeopoliticalDoctrine {
    /// Returns a human-readable label for UI display.
    pub fn as_str(&self) -> &'static str {
        match self {
            GeopoliticalDoctrine::Balanced => "Balanced",
            GeopoliticalDoctrine::Expansionist => "Expansionist",
            GeopoliticalDoctrine::Isolationist => "Isolationist",
            GeopoliticalDoctrine::AllianceSeeker => "Alliance Seeker",
            GeopoliticalDoctrine::ResourceDependency { .. } => "Resource Dependent",
        }
    }
}

/// Configuration for AI doctrine evaluation. No magic numbers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DoctrineConfig {
    /// Military strength ratio above which a nation becomes Expansionist.
    pub expansionist_military_ratio: f64,
    /// Trade deficit ratio (deficit/GDP) above which a nation becomes ResourceDependent.
    pub resource_deficit_threshold: f64,
    /// Relations score below which a nation becomes Isolationist.
    pub isolationist_relations_threshold: i64,
    /// Relations score above which a nation becomes AllianceSeeker.
    pub alliance_seeker_relations_threshold: i64,
    /// Probability per turn that an expansionist nation launches a provocation.
    pub expansionist_provocation_chance: f64,
    /// Probability per turn that an alliance seeker proposes a treaty.
    pub alliance_seeker_treaty_chance: f64,
    /// Minimum military strength to consider expansionist actions.
    pub min_military_for_expansion: u32,
}

impl Default for DoctrineConfig {
    fn default() -> Self {
        Self {
            expansionist_military_ratio: 1.5,
            resource_deficit_threshold: 0.10,
            isolationist_relations_threshold: -50,
            alliance_seeker_relations_threshold: 50,
            expansionist_provocation_chance: 0.10,
            alliance_seeker_treaty_chance: 0.15,
            min_military_for_expansion: 5,
        }
    }
}

/// Evaluates the appropriate geopolitical doctrine for an AI nation.
///
/// This function is designed to be lightweight and read-only — it does not
/// mutate state. The resulting doctrine is used by `execute_doctrine()` to
/// generate diplomatic actions.
pub fn evaluate_doctrine(
    state: &GameState,
    country_name: &str,
    config: &DoctrineConfig,
) -> GeopoliticalDoctrine {
    let Some(country) = state.countries.get(country_name) else {
        return GeopoliticalDoctrine::Balanced;
    };

    let military_size = country.order_of_battle.unit_count() as u32;

    // Compute average military size across all countries
    let avg_military: f64 = if state.countries.is_empty() {
        0.0
    } else {
        state
            .countries
            .values()
            .map(|c| c.order_of_battle.unit_count() as f64)
            .sum::<f64>()
            / state.countries.len() as f64
    };

    // Expansionist: military significantly above average
    if military_size >= config.min_military_for_expansion
        && avg_military > 0.0
        && (military_size as f64) / avg_military > config.expansionist_military_ratio
    {
        return GeopoliticalDoctrine::Expansionist;
    }

    // Resource dependency: large trade deficit relative to GDP
    // Agent 4 — Phase 6: Renamed from Polish "bilans_handlowy" to English (Rule 12).
    let trade_balance = country
        .budget
        .extra
        .get("trade_balance")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let gdp = country.budget.gdp.max(1.0);
    let deficit_ratio = (-trade_balance / gdp).max(0.0);
    if deficit_ratio > config.resource_deficit_threshold {
        // Determine which commodity is most deficit
        // For simplicity, use the largest net surplus commodity as the "needed" one
        // In a full implementation, we'd analyze per-commodity deficits
        let commodity = country
            .budget
            .extra
            .get("largest_import_commodity")
            .and_then(|v| v.as_str())
            .unwrap_or("Energy")
            .to_string();
        return GeopoliticalDoctrine::ResourceDependency { commodity };
    }

    // Check average relations with other countries
    // Relations are stored in the diplomacy matrix, not directly accessible here.
    // We approximate using the country's reputation.
    // Use reputation as a proxy if available
    let reputation_score = country
        .budget
        .extra
        .get("global_reputation")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    if reputation_score < config.isolationist_relations_threshold as f64 {
        return GeopoliticalDoctrine::Isolationist;
    }

    if reputation_score > config.alliance_seeker_relations_threshold as f64 {
        return GeopoliticalDoctrine::AllianceSeeker;
    }

    GeopoliticalDoctrine::Balanced
}

/// Executes a nation's geopolitical doctrine, generating diplomatic actions.
///
/// This function is called during parallel per-country processing and returns
/// a list of `DiplomaticAction`s to be drained sequentially.
pub fn execute_doctrine(
    state: &GameState,
    country_name: &str,
    doctrine: &GeopoliticalDoctrine,
    config: &DoctrineConfig,
    current_turn: u32,
    rng: &mut impl rand::Rng,
) -> Vec<DiplomaticAction> {
    let mut actions = Vec::new();

    let Some(country) = state.countries.get(country_name) else {
        return actions;
    };

    match doctrine {
        GeopoliticalDoctrine::Expansionist => {
            // Find weakest neighbor to provoke
            if rng.gen::<f64>() < config.expansionist_provocation_chance {
                let mut weakest: Option<(&String, u32)> = None;
                for (name, other) in &state.countries {
                    if name == country_name {
                        continue;
                    }
                    let mil = other.order_of_battle.unit_count() as u32;
                    if weakest.is_none_or(|(_, m)| mil < m) {
                        weakest = Some((name, mil));
                    }
                }
                if let Some((target, _)) = weakest {
                    actions.push(DiplomaticAction::BorderProvocation {
                        from_country: country_name.to_string(),
                        to_country: target.clone(),
                        intensity: 0.7,
                    });
                }
            }
        }
        GeopoliticalDoctrine::AllianceSeeker => {
            // Propose treaties with friendly nations
            if rng.gen::<f64>() < config.alliance_seeker_treaty_chance {
                // Find a country with positive reputation/relations
                let mut best_partner: Option<&String> = None;
                for name in state.countries.keys() {
                    if name == country_name {
                        continue;
                    }
                    best_partner = Some(name);
                    break; // Simple: pick first available
                }
                if let Some(partner) = best_partner {
                    // Queue an ambassador assignment as a precursor to treaty
                    if let Some(registry) = &country.politics.vip_registry {
                        let available_diplomat = registry
                            .vips
                            .values()
                            .find(|v| {
                                v.diplomatic_post.is_none()
                                    && !v.is_dead
                                    && v.roles.contains(
                                        &crate::politics::vip_registry::VipRoleExtended::Ambassador,
                                    )
                            })
                            .map(|v| v.id.clone());
                        if let Some(vip_id) = available_diplomat {
                            actions.push(DiplomaticAction::AssignDiplomat {
                                vip_id,
                                home_country: country_name.to_string(),
                                host_country: partner.clone(),
                                post_type: DiplomaticPostType::Ambassador,
                                assigned_turn: current_turn,
                            });
                        }
                    }
                }
            }
        }
        GeopoliticalDoctrine::ResourceDependency { commodity: _ } => {
            // Seek trade preference treaties with resource-rich nations
            // For now, send aid to potential partners to improve relations
            if country.budget.liquid_reserves > 500_000.0 && rng.gen::<f64>() < 0.05 {
                for (name, other) in &state.countries {
                    if name == country_name {
                        continue;
                    }
                    // Send aid to countries with high GDP (potential resource suppliers)
                    if other.budget.gdp > country.budget.gdp * 0.8 {
                        actions.push(DiplomaticAction::SendEconomicAid {
                            from_country: country_name.to_string(),
                            to_country: name.clone(),
                            amount: 50_000.0,
                        });
                        break;
                    }
                }
            }
        }
        GeopoliticalDoctrine::Isolationist => {
            // Recall diplomats, minimize engagement
            if let Some(registry) = &country.politics.vip_registry {
                for (vip_id, vip) in &registry.vips {
                    if vip.diplomatic_post.is_some() && rng.gen::<f64>() < 0.10 {
                        actions.push(DiplomaticAction::RecallDiplomat {
                            vip_id: vip_id.clone(),
                            home_country: country_name.to_string(),
                        });
                    }
                }
            }
        }
        GeopoliticalDoctrine::Balanced => {
            // No aggressive actions — maintain status quo
        }
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Country, GameState};

    #[test]
    fn test_doctrine_default_balanced() {
        let state = GameState::default();
        let config = DoctrineConfig::default();
        let doctrine = evaluate_doctrine(&state, "NonExistent", &config);
        assert_eq!(doctrine, GeopoliticalDoctrine::Balanced);
    }

    #[test]
    fn test_doctrine_expansionist() {
        let mut state = GameState::default();
        let mut strong = Country::mock_for_tests();
        strong.name = "Strongland".to_string();
        // Add many military units to the OOB
        use crate::military::oob::{Army, Division, Regiment};
        let mut reg = Regiment::new(
            "REG-test-001".to_string(),
            "Test Regiment".to_string(),
            "home".to_string(),
        );
        for i in 0..20 {
            reg.add_unit(crate::military::MilitaryUnit::new(
                format!("unit-{}", i),
                crate::military::UnitType::Infantry,
                100,
                rustc_hash::FxHashMap::default(),
                "home".to_string(),
            ));
        }
        let mut div = Division::new(
            "DIV-test-001".to_string(),
            "Test Division".to_string(),
            "home".to_string(),
        );
        div.add_regiment(reg);
        let mut army = Army::new(
            "ARMY-test-001".to_string(),
            "Test Army".to_string(),
            "home".to_string(),
        );
        army.add_division(div);
        strong.order_of_battle.add_army(army);
        let mut weak = Country::mock_for_tests();
        weak.name = "Weakland".to_string();
        state.countries.insert("Strongland".to_string(), strong);
        state.countries.insert("Weakland".to_string(), weak);

        let config = DoctrineConfig::default();
        let doctrine = evaluate_doctrine(&state, "Strongland", &config);
        assert_eq!(doctrine, GeopoliticalDoctrine::Expansionist);
    }

    #[test]
    fn test_doctrine_isolationist() {
        let mut state = GameState::default();
        let mut country = Country::mock_for_tests();
        country.name = "Loneland".to_string();
        // Set low reputation via extra
        country.budget.extra.insert(
            "global_reputation".to_string(),
            serde_json::Value::from(-60.0),
        );
        state.countries.insert("Loneland".to_string(), country);

        let config = DoctrineConfig::default();
        let doctrine = evaluate_doctrine(&state, "Loneland", &config);
        assert_eq!(doctrine, GeopoliticalDoctrine::Isolationist);
    }

    #[test]
    fn test_doctrine_alliance_seeker() {
        let mut state = GameState::default();
        let mut country = Country::mock_for_tests();
        country.name = "Friendlyland".to_string();
        country.budget.extra.insert(
            "global_reputation".to_string(),
            serde_json::Value::from(60.0),
        );
        state.countries.insert("Friendlyland".to_string(), country);

        let config = DoctrineConfig::default();
        let doctrine = evaluate_doctrine(&state, "Friendlyland", &config);
        assert_eq!(doctrine, GeopoliticalDoctrine::AllianceSeeker);
    }

    #[test]
    fn test_execute_doctrine_balanced_no_actions() {
        let state = GameState::default();
        let config = DoctrineConfig::default();
        let mut rng = rand::thread_rng();
        let actions = execute_doctrine(
            &state,
            "Test",
            &GeopoliticalDoctrine::Balanced,
            &config,
            1,
            &mut rng,
        );
        assert!(
            actions.is_empty(),
            "Balanced doctrine should produce no actions"
        );
    }

    #[test]
    fn test_execute_doctrine_expansionist_may_provoke() {
        let mut state = GameState::default();
        let mut strong = Country::mock_for_tests();
        strong.name = "Strongland".to_string();
        state.countries.insert("Strongland".to_string(), strong);
        state
            .countries
            .insert("Weakland".to_string(), Country::mock_for_tests());

        let config = DoctrineConfig {
            expansionist_provocation_chance: 1.0, // Always provoke
            ..DoctrineConfig::default()
        };
        let mut rng = rand::thread_rng();
        let actions = execute_doctrine(
            &state,
            "Strongland",
            &GeopoliticalDoctrine::Expansionist,
            &config,
            1,
            &mut rng,
        );
        assert!(
            !actions.is_empty(),
            "Expansionist should generate provocation"
        );
        assert!(matches!(
            actions[0],
            DiplomaticAction::BorderProvocation { .. }
        ));
    }
}

#![allow(missing_docs)]

use crate::international::DiplomaticRelation;
use crate::state::GameState;
use rand::Rng;
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
            let relacje = rng.gen_range(-100..=100);
            let mut rel = DiplomaticRelation {
                relacje,
                zamrozenie: 0,
                ban_import: false,
                ban_export: false,
                free_trade: false,
                customs_union: false,
                investment_treaty: false,
                economic_community: false,
                traktat: "Brak".to_string(),
                embargo_penalty: 0.0,
            };
            if relacje < -50 && rng.gen::<f64>() < 0.5 {
                rel.ban_export = true;
                rel.ban_import = true;
            } else if relacje > 50 && rng.gen::<f64>() < 0.5 {
                rel.free_trade = true;
            }
            inner.insert(c2.clone(), rel);
        }
        diplomacy.insert(c1.clone(), inner);
    }

    diplomacy
}

/// Updates diplomatic relations dynamically based on physical world events (Phase 11).
///
/// # Arguments
/// * `state` - Immutable game state (reads trade balances, politics, military fronts).
/// * `diplomacy` - Mutable bilateral diplomacy matrix.
///
/// # Rules
/// * **Frozen relations**: If `zamrozenie > 0`, skip all changes and decrement by 1.
/// * **Trade imbalance**: Large deficit with partner → -1 to -3/turn.
/// * **Ideological distance**: Different regime types (democracy vs autocracy) → -1/turn.
/// * **Border tension**: Overlapping military fronts → -2/turn.
/// * **Cultural affinity**: Same cultural group → +1/turn.
/// * **Trade volume**: High bilateral trade → +1/turn (trade builds trust).
/// * `relacje` clamped to [-100, 100].
/// * Threshold triggers after update: embargo at < -50, free_trade at > 50.
/// * Sequential (not parallel) — reads all countries, mutates shared matrix.
pub fn process_diplomacy_turn(
    state: &GameState,
    diplomacy: &mut HashMap<String, HashMap<String, DiplomaticRelation>>,
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
            if rel.zamrozenie > 0 {
                rel.zamrozenie -= 1;
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
            if c1_front_countries.iter().any(|&c| c == c2_name) {
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

            // Apply delta and clamp
            rel.relacje = (rel.relacje + delta).clamp(-100, 100);

            // Threshold triggers
            if rel.relacje < -50 && rng.gen::<f64>() < 0.3 {
                rel.ban_export = true;
                rel.ban_import = true;
                rel.free_trade = false;
                rel.customs_union = false;
                rel.traktat = "Embargo".to_string();
            } else if rel.relacje > 50 && rng.gen::<f64>() < 0.3 {
                rel.ban_export = false;
                rel.ban_import = false;
                rel.free_trade = true;
                rel.traktat = "Free Trade".to_string();
            } else if rel.relacje > -10 && rel.relacje < 10 {
                // Relations normalized — lift embargoes if relations improve
                if rel.relacje > 0 {
                    rel.ban_export = false;
                    rel.ban_import = false;
                }
            }
        }
    }
}

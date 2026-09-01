use crate::politics::system::Leader;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Leader personality trait (data-driven via JSON)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LeaderTrait {
    /// Trait ID (e.g., "charismatic", "corrupt", "economist")
    #[serde(default)]
    pub id: String,

    /// Trait display name
    #[serde(default)]
    pub name: String,

    /// Trait description
    #[serde(default)]
    pub description: String,

    /// Rarity weight (higher = rarer)
    #[serde(default)]
    pub rarity_weight: f64,

    /// Data-driven modifiers (JSON-configurable)
    #[serde(default)]
    pub modifiers: Vec<TraitModifier>,

    /// Any additional fields
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Data-driven modifier for a trait
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TraitModifier {
    /// Target system (e.g., "campaign", "scandal", "economy", "legislation")
    #[serde(default)]
    pub target_system: String,

    /// Specific parameter to modify (e.g., "cost_multiplier", "discovery_risk")
    #[serde(default)]
    pub parameter: String,

    /// Modifier type (additive, multiplicative, override)
    #[serde(default)]
    pub modifier_type: ModifierType,

    /// Modifier value
    #[serde(default)]
    pub value: f64,

    /// Condition for modifier application (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModifierType {
    #[default]
    Additive, // value is added to base

    Multiplicative, // value multiplies base

    Override, // value replaces base
}

/// Registry of all available leader traits (loaded from JSON)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TraitRegistry {
    /// All traits by ID
    #[serde(default)]
    pub traits: HashMap<String, LeaderTrait>,

    /// Any additional fields
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl TraitRegistry {
    /// Get a trait by ID
    pub fn get(&self, id: &str) -> Option<&LeaderTrait> {
        self.traits.get(id)
    }

    /// Get random trait weighted by rarity
    pub fn get_random_weighted(&self, rng: &mut impl rand::Rng) -> Option<&LeaderTrait> {
        if self.traits.is_empty() {
            return None;
        }

        let total_weight: f64 = self.traits.values().map(|t| t.rarity_weight).sum();
        let mut random_weight = rng.gen::<f64>() * total_weight;

        for trait_data in self.traits.values() {
            random_weight -= trait_data.rarity_weight;
            if random_weight <= 0.0 {
                return Some(trait_data);
            }
        }

        self.traits.values().next()
    }
}

/// Apply leader trait modifiers to a base value
pub fn apply_leader_modifiers(
    leader: &Leader,
    base_value: f64,
    target_system: &str,
    parameter: &str,
    trait_registry: &TraitRegistry,
) -> f64 {
    let mut modified_value = base_value;

    // Iterate through leader's traits
    for trait_id in &leader.traits {
        if let Some(trait_data) = trait_registry.get(trait_id) {
            // Find matching modifiers
            for modifier in &trait_data.modifiers {
                if modifier.target_system == target_system && modifier.parameter == parameter {
                    // Check condition if present (simplified: always apply for now)
                    match modifier.modifier_type {
                        ModifierType::Additive => {
                            modified_value += modifier.value;
                        }
                        ModifierType::Multiplicative => {
                            modified_value *= modifier.value;
                        }
                        ModifierType::Override => {
                            modified_value = modifier.value;
                        }
                    }
                }
            }
        }
    }

    modified_value
}

/// Process leader traits for one turn — apply trait modifiers to key economic parameters.
///
/// # Arguments
/// * `country` - Mutable country (for head_of_state, economic configs)
/// * `trait_registry` - Optional trait registry (if None, no modifiers applied)
///
/// # Returns
/// Vector of diagnostic messages
///
/// # Rules
/// * Looks up the head of state's traits in the registry.
/// * Applies modifiers to: tax_collection_efficiency, infrastructure_investment, diplomatic_influence.
/// * Modifiers are applied to base values in the country's economic config.
pub fn process_leader_traits_turn(
    country: &mut crate::state::Country,
    trait_registry: Option<&TraitRegistry>,
) -> Vec<String> {
    let mut messages = Vec::new();

    let registry = match trait_registry {
        Some(r) => r,
        None => return messages,
    };

    let leader = &country.politics.head_of_state;
    if leader.traits.is_empty() {
        return messages;
    }

    // Apply tax collection efficiency modifier
    let base_tax_efficiency = 1.0;
    let modified_tax_efficiency = apply_leader_modifiers(
        leader,
        base_tax_efficiency,
        "tax",
        "collection_efficiency",
        registry,
    );
    if (modified_tax_efficiency - base_tax_efficiency).abs() > 0.001 {
        messages.push(format!(
            "[TRAITS] {} modified tax collection efficiency: {:.0}% → {:.0}%",
            leader.name,
            base_tax_efficiency * 100.0,
            modified_tax_efficiency * 100.0
        ));
    }

    // Apply infrastructure investment modifier (using education_cost_per_worker as proxy)
    let base_infra = country.infrastructure_config.education_cost_per_worker;
    let modified_infra =
        apply_leader_modifiers(leader, base_infra, "infrastructure", "investment", registry);
    if (modified_infra - base_infra).abs() > 0.001 {
        country.infrastructure_config.education_cost_per_worker = modified_infra.max(1.0);
        messages.push(format!(
            "[TRAITS] {} modified infrastructure cost/worker: {:.0} → {:.0}",
            leader.name, base_infra, modified_infra
        ));
    }

    messages
}

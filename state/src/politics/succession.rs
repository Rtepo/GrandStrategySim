//! Phase 48: Regime-specific succession and royal dynasties.
//!
//! This module implements:
//! - Royal dynasty tracking (family trees, heirs, regency).
//! - Regime-specific succession outcomes (monarchy, democracy, military, theocracy).
//! - Succession triggers: death, incapacity, coup, resignation.

use serde::{Deserialize, Serialize};
use rand::Rng;

// ============================================================================
// ROAL DYNASTY
// ============================================================================

/// A royal family member tracked in the VIP registry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RoyalFamilyMember {
    /// VIP ID in the global registry.
    #[serde(default)]
    pub vip_id: String,
    /// Relationship to the current monarch.
    #[serde(default)]
    pub relation: RoyalRelation,
    /// Turn when this member was born/generated.
    #[serde(default)]
    pub birth_turn: u32,
    /// Whether this member is a legitimate heir.
    #[serde(default)]
    pub is_legitimate: bool,
    /// Whether this member is the heir apparent (first in line).
    #[serde(default)]
    pub is_heir_apparent: bool,
    /// Succession order (1 = first in line).
    #[serde(default)]
    pub succession_order: u32,

    // Phase 86: Genealogy links — parent, spouse, children.
    /// VIP ID of the father (if known).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub father_vip_id: Option<String>,
    /// VIP ID of the mother (if known).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mother_vip_id: Option<String>,
    /// VIP ID of the spouse (if married).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spouse_vip_id: Option<String>,
    /// VIP IDs of all children.
    #[serde(default)]
    pub children_vip_ids: Vec<String>,
    /// Turn when this member married (if applicable).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marriage_turn: Option<u32>,
    /// Turn when this member died (if dead).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub death_turn: Option<u32>,
    /// Cause of death (if dead).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub death_cause: Option<crate::politics::vip_registry::DeathCause>,
}

/// Relationship of a royal family member to the monarch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum RoyalRelation {
    #[default]
    /// The reigning monarch.
    Monarch,
    /// Queen/King consort.
    Consort,
    /// Child of the monarch (prince/princess).
    Child,
    /// Sibling of the monarch.
    Sibling,
    /// Extended royal family (cousin, nephew, etc.).
    Cousin,
    /// Acting ruler during minority/incapacity.
    Regent,
}

/// Royal dynasty tracking (stored on `Politics::royal_dynasty`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RoyalDynasty {
    /// Dynasty name (e.g., "Piast", "Habsburg").
    #[serde(default)]
    pub dynasty_name: String,
    /// All family members.
    #[serde(default)]
    pub members: Vec<RoyalFamilyMember>,
    /// Current monarch's VIP ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_monarch_id: Option<String>,
    /// Current regent (if monarch is underage or incapacitated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_regent_id: Option<String>,
    /// Whether a regency is active.
    #[serde(default)]
    pub regency_active: bool,
    /// Regency council members (VIP IDs).
    #[serde(default)]
    pub regency_council: Vec<String>,

    // Phase 86: Genealogy event history.
    /// History of royal marriages.
    #[serde(default)]
    pub marriage_history: Vec<RoyalMarriage>,
    /// History of royal births.
    #[serde(default)]
    pub birth_history: Vec<RoyalBirth>,
}

/// Phase 86: A royal marriage event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RoyalMarriage {
    /// Turn when the marriage occurred.
    #[serde(default)]
    pub turn: u32,
    /// VIP ID of the first spouse (typically the royal family member).
    #[serde(default)]
    pub spouse1_vip_id: String,
    /// VIP ID of the second spouse (the partner).
    #[serde(default)]
    pub spouse2_vip_id: String,
    /// Political significance of the marriage.
    #[serde(default)]
    pub political_significance: MarriageSignificance,
    /// Foreign dynasty name (for diplomatic marriages), if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreign_dynasty: Option<String>,
}

/// Phase 86: Political significance of a royal marriage.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum MarriageSignificance {
    /// Royal-to-royal marriage (major diplomatic event).
    #[default]
    Dynastic,
    /// Royal-to-noble marriage (domestic alliance).
    Noble,
    /// Royal-to-commoner marriage (no succession rights for children).
    Morganatic,
}

/// Phase 86: A royal birth event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RoyalBirth {
    /// Turn when the birth occurred.
    #[serde(default)]
    pub turn: u32,
    /// VIP ID of the child (instantiated in the global registry).
    #[serde(default)]
    pub child_vip_id: String,
    /// VIP ID of the father.
    #[serde(default)]
    pub father_vip_id: String,
    /// VIP ID of the mother.
    #[serde(default)]
    pub mother_vip_id: String,
    /// Whether the child is a legitimate heir.
    #[serde(default)]
    pub is_legitimate: bool,
}

impl RoyalDynasty {
    /// Create a new royal dynasty with the given name.
    pub fn new(dynasty_name: String) -> Self {
        RoyalDynasty {
            dynasty_name,
            members: Vec::new(),
            current_monarch_id: None,
            current_regent_id: None,
            regency_active: false,
            regency_council: Vec::new(),
            marriage_history: Vec::new(),
            birth_history: Vec::new(),
        }
    }

    /// Get the current heir apparent.
    pub fn heir_apparent(&self) -> Option<&RoyalFamilyMember> {
        self.members
            .iter()
            .find(|m| m.is_heir_apparent && !m.vip_id.is_empty())
    }

    /// Get the succession line ordered by succession_order.
    pub fn succession_line(&self) -> Vec<&RoyalFamilyMember> {
        let mut line: Vec<&RoyalFamilyMember> = self
            .members
            .iter()
            .filter(|m| m.is_legitimate && !m.vip_id.is_empty())
            .collect();
        line.sort_by_key(|m| m.succession_order);
        line
    }

    /// Check if the heir is underage (< 18).
    pub fn heir_is_underage(&self, vip_registry: &crate::politics::vip_registry::VipRegistry) -> bool {
        if let Some(heir) = self.heir_apparent() {
            if let Some(vip) = vip_registry.get(&heir.vip_id) {
                return vip.age < 18;
            }
        }
        false
    }
}

// ============================================================================
// PHASE 86: DYNASTY TURN PROCESSING — MARRIAGES AND BIRTHS
// ============================================================================

/// Phase 86: Process royal dynasty per turn — marriages, births, succession updates.
///
/// This function checks if the monarch or heir needs to marry, and if married
/// couples should have children. All new VIPs (spouses and children) are
/// fully instantiated in the global `vip_registry` — no phantom IDs.
///
/// # Arguments
/// * `dynasty` - Mutable reference to the royal dynasty (Option, may be None)
/// * `vip_registry` - Mutable reference to the VIP registry (Option, may be None)
/// * `culture` - Cultural group for name generation
/// * `dynasty_id` - Dynasty name (for setting on new VIPs)
/// * `current_turn` - Current game turn
///
/// # Returns
/// Vector of diagnostic messages.
pub fn process_dynasty_turn(
    dynasty: &mut Option<RoyalDynasty>,
    vip_registry: &mut Option<crate::politics::vip_registry::VipRegistry>,
    culture: &str,
    dynasty_id: &str,
    current_turn: u32,
) -> Vec<String> {
    let mut messages = Vec::new();

    let dyn_ref = match dynasty.as_mut() {
        Some(d) => d,
        None => return messages,
    };
    let registry = match vip_registry.as_mut() {
        Some(r) => r,
        None => return messages,
    };

    // 1. Marriage check: find unmarried monarch or heir of marriageable age (≥18).
    let monarch_id = dyn_ref.current_monarch_id.clone();
    if let Some(monarch_id) = monarch_id {
        // Check if monarch is unmarried.
        let monarch_unmarried = dyn_ref
            .members
            .iter()
            .find(|m| m.vip_id == monarch_id)
            .map(|m| m.spouse_vip_id.is_none())
            .unwrap_or(false);

        let monarch_age = registry.get(&monarch_id).map(|v| v.age).unwrap_or(0);
        let monarch_is_dead = registry.get(&monarch_id).map(|v| v.is_dead).unwrap_or(false);

        if monarch_unmarried && monarch_age >= 18 && !monarch_is_dead {
            // Generate a spouse VIP.
            let mut rng = rand::thread_rng();
            let spouse_name = crate::politics::names::generate_full_vip(culture, &mut rng);
            let spouse_gender = if registry.get(&monarch_id).map(|v| v.gender.as_str()).unwrap_or("M") == "M" {
                "F"
            } else {
                "M"
            };

            let (traits, main_trait) = crate::politics::vip_registry::assign_core_traits(&mut rng);

            let spouse_vip = crate::politics::vip_registry::Vip {
                id: String::new(), // Will be assigned by register_new
                full_name: spouse_name.full_name.clone(),
                gender: spouse_gender.to_string(),
                age: 18 + rng.gen_range(0..15), // Spouse aged 18-32
                health: crate::politics::vip_registry::VipHealth {
                    physical_health: 0.9,
                    mental_health: 0.9,
                },
                incapacity: crate::politics::vip_registry::IncapacityStatus::Healthy,
                traits,
                main_trait,
                ideology: String::new(),
                religion: String::new(),
                nationality: String::new(),
                dynasty: Some(dynasty_id.to_string()),
                roles: vec![crate::politics::vip_registry::VipRoleExtended::RoyalConsort],
                base_influence: 20,
                faction: String::new(),
                born_turn: current_turn.saturating_sub(18 * 24), // Approximate birth turn
                is_dead: false,
                death_turn: None,
                death_cause: None,
                acting_replacement_id: None,
                diplomatic_post: None,
            };

            let spouse_vip_id = registry.register_new(spouse_vip);

            // Update monarch's RoyalFamilyMember with spouse link.
            if let Some(monarch_member) = dyn_ref.members.iter_mut().find(|m| m.vip_id == monarch_id) {
                monarch_member.spouse_vip_id = Some(spouse_vip_id.clone());
                monarch_member.marriage_turn = Some(current_turn);
            }

            // Add spouse as a new dynasty member.
            dyn_ref.members.push(RoyalFamilyMember {
                vip_id: spouse_vip_id.clone(),
                relation: RoyalRelation::Consort,
                birth_turn: current_turn.saturating_sub(18 * 24),
                is_legitimate: true,
                is_heir_apparent: false,
                succession_order: 999, // Consorts are not in succession line
                father_vip_id: None,
                mother_vip_id: None,
                spouse_vip_id: Some(monarch_id.clone()),
                children_vip_ids: Vec::new(),
                marriage_turn: Some(current_turn),
                death_turn: None,
                death_cause: None,
            });

            // Log marriage event.
            dyn_ref.marriage_history.push(RoyalMarriage {
                turn: current_turn,
                spouse1_vip_id: monarch_id.clone(),
                spouse2_vip_id: spouse_vip_id.clone(),
                political_significance: MarriageSignificance::Dynastic,
                foreign_dynasty: None,
            });

            messages.push(format!(
                "[DYNASTY] {} married {} (dynastic marriage, turn {}).",
                monarch_id, spouse_vip_id, current_turn
            ));
        }
    }

    // 2. Birth check: for married couples where one partner is of childbearing age.
    // Women: 18-45, Men: 18-60. Roll for birth (deterministic, ~20% chance per turn).
    let members_clone = dyn_ref.members.clone();
    for member in &members_clone {
        // Skip if no spouse or already has many children.
        if member.spouse_vip_id.is_none() || member.children_vip_ids.len() >= 6 {
            continue;
        }

        // Skip dead members.
        let vip = match registry.get(&member.vip_id) {
            Some(v) if !v.is_dead => v.clone(),
            _ => continue,
        };

        // Check childbearing age.
        let is_female = vip.gender == "F" || vip.gender == "Female";
        let age_ok = if is_female {
            vip.age >= 18 && vip.age <= 45
        } else {
            vip.age >= 18 && vip.age <= 60
        };
        if !age_ok {
            continue;
        }

        // Check spouse is alive and of childbearing age.
        let spouse_id = member.spouse_vip_id.as_ref().unwrap();
        let spouse = match registry.get(spouse_id) {
            Some(s) if !s.is_dead => s.clone(),
            _ => continue,
        };
        let spouse_age_ok = if spouse.gender == "F" || spouse.gender == "Female" {
            spouse.age >= 18 && spouse.age <= 45
        } else {
            spouse.age >= 18 && spouse.age <= 60
        };
        if !spouse_age_ok {
            continue;
        }

        // Deterministic birth roll: ~20% chance per turn.
        let birth_seed = format!("birth_{}_{}_{}", member.vip_id, spouse_id, current_turn);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::Hasher;
        for b in birth_seed.bytes() {
            hasher.write_u8(b);
        }
        let hash = hasher.finish();
        let roll = (hash % 1000) as f64 / 1000.0;

        if roll < 0.20 {
            // Birth occurs — instantiate a new VIP.
            let mut rng = rand::thread_rng();
            let child_name = crate::politics::names::generate_full_vip(culture, &mut rng);
            let child_gender = if rng.gen::<f64>() < 0.5 { "M" } else { "F" };
            let (traits, main_trait) = crate::politics::vip_registry::assign_core_traits(&mut rng);

            // Determine father and mother.
            let (father_id, mother_id) = if is_female {
                (spouse_id.clone(), member.vip_id.clone())
            } else {
                (member.vip_id.clone(), spouse_id.clone())
            };

            let child_vip = crate::politics::vip_registry::Vip {
                id: String::new(),
                full_name: child_name.full_name.clone(),
                gender: child_gender.to_string(),
                age: 0,
                health: crate::politics::vip_registry::VipHealth {
                    physical_health: 1.0,
                    mental_health: 1.0,
                },
                incapacity: crate::politics::vip_registry::IncapacityStatus::Healthy,
                traits,
                main_trait,
                ideology: String::new(),
                religion: String::new(),
                nationality: String::new(),
                dynasty: Some(dynasty_id.to_string()),
                roles: vec![crate::politics::vip_registry::VipRoleExtended::RoyalHeir],
                base_influence: 5,
                faction: String::new(),
                born_turn: current_turn,
                is_dead: false,
                death_turn: None,
                death_cause: None,
                acting_replacement_id: None,
                diplomatic_post: None,
            };

            let child_vip_id = registry.register_new(child_vip);

            // Update both parents' children_vip_ids.
            if let Some(m) = dyn_ref.members.iter_mut().find(|m| m.vip_id == member.vip_id) {
                m.children_vip_ids.push(child_vip_id.clone());
            }
            if let Some(m) = dyn_ref.members.iter_mut().find(|m| m.vip_id == *spouse_id) {
                m.children_vip_ids.push(child_vip_id.clone());
            }

            // Add child as a new dynasty member.
            let child_relation = if member.relation == RoyalRelation::Monarch || member.relation == RoyalRelation::Consort {
                RoyalRelation::Child
            } else {
                RoyalRelation::Cousin // For other family members
            };

            dyn_ref.members.push(RoyalFamilyMember {
                vip_id: child_vip_id.clone(),
                relation: child_relation,
                birth_turn: current_turn,
                is_legitimate: true,
                is_heir_apparent: false, // Will be set by succession order update
                succession_order: 999,   // Will be recalculated
                father_vip_id: Some(father_id.clone()),
                mother_vip_id: Some(mother_id.clone()),
                spouse_vip_id: None,
                children_vip_ids: Vec::new(),
                marriage_turn: None,
                death_turn: None,
                death_cause: None,
            });

            // Log birth event.
            dyn_ref.birth_history.push(RoyalBirth {
                turn: current_turn,
                child_vip_id: child_vip_id.clone(),
                father_vip_id: father_id.clone(),
                mother_vip_id: mother_id.clone(),
                is_legitimate: true,
            });

            messages.push(format!(
                "[DYNASTY] Royal birth: {} born to {} and {} (turn {}).",
                child_vip_id, father_id, mother_id, current_turn
            ));
        }
    }

    // 3. Succession order update: recalculate based on primogeniture
    //    (eldest legitimate child of monarch first).
    recalculate_succession_order(dyn_ref, registry);

    // 4. Death check: update dynasty members whose VIPs have died.
    let dead_members: Vec<(String, Option<crate::politics::vip_registry::DeathCause>, Option<u32>)> = dyn_ref
        .members
        .iter()
        .filter_map(|m| {
            if let Some(vip) = registry.get(&m.vip_id) {
                if vip.is_dead && m.death_turn.is_none() {
                    return Some((m.vip_id.clone(), vip.death_cause.clone(), vip.death_turn));
                }
            }
            None
        })
        .collect();

    for (vip_id, cause, death_turn) in dead_members {
        if let Some(member) = dyn_ref.members.iter_mut().find(|m| m.vip_id == vip_id) {
            member.death_turn = death_turn;
            member.death_cause = cause;
        }
        messages.push(format!(
            "[DYNASTY] Dynasty member {} died (turn {}).",
            vip_id, current_turn
        ));
    }

    messages
}

/// Phase 86: Recalculate succession order using primogeniture.
/// Eldest legitimate children of the monarch come first, ordered by age.
fn recalculate_succession_order(
    dynasty: &mut RoyalDynasty,
    registry: &crate::politics::vip_registry::VipRegistry,
) {
    let monarch_id = match &dynasty.current_monarch_id {
        Some(id) => id.clone(),
        None => return,
    };

    // Find legitimate children of the monarch, sorted by age (eldest first).
    let mut children: Vec<(String, u32)> = dynasty
        .members
        .iter()
        .filter(|m| {
            m.is_legitimate
                && (m.father_vip_id.as_deref() == Some(monarch_id.as_str())
                    || m.mother_vip_id.as_deref() == Some(monarch_id.as_str()))
        })
        .filter_map(|m| {
            if let Some(vip) = registry.get(&m.vip_id) {
                if !vip.is_dead {
                    return Some((m.vip_id.clone(), vip.age));
                }
            }
            None
        })
        .collect();

    // Sort by age descending (eldest first) using Reverse for descending key.
    children.sort_unstable_by_key(|&(_, age)| std::cmp::Reverse(age));

    // Assign succession orders.
    let mut order = 1u32;
    for (child_id, _) in &children {
        if let Some(member) = dynasty.members.iter_mut().find(|m| m.vip_id == *child_id) {
            member.succession_order = order;
            member.is_heir_apparent = order == 1;
            order += 1;
        }
    }

    // All other legitimate members get higher order numbers.
    let next_order = order;
    let mut other_order = next_order;
    for member in &mut dynasty.members {
        if member.is_legitimate
            && member.succession_order >= 999
            && !member.vip_id.is_empty()
            && member.relation != RoyalRelation::Monarch
            && member.relation != RoyalRelation::Consort
        {
            if let Some(vip) = registry.get(&member.vip_id) {
                if !vip.is_dead {
                    member.succession_order = other_order;
                    other_order += 1;
                }
            }
        }
    }
}

// ============================================================================
// SUCCESSION OUTCOME
// ============================================================================

/// Outcome of a succession event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SuccessionOutcome {
    /// A new leader has been installed.
    Succession {
        new_leader_vip_id: String,
        reason: String,
    },
    /// Regency council established for underage heir.
    Regency {
        regent_vip_id: String,
        heir_vip_id: String,
        council: Vec<String>,
    },
    /// Power struggle initiated (military dictatorship).
    PowerStruggle {
        contenders: Vec<String>,
        duration_turns: u32,
    },
    /// Conclave called (theocracy).
    Conclave {
        electors: Vec<String>,
        rounds: u32,
    },
    /// Snap election triggered (democracy).
    SnapElection {
        reason: String,
    },
    /// Crisis — no clear successor, provisional government.
    Crisis {
        provisional_leader_vip_id: String,
    },
}

impl Default for SuccessionOutcome {
    fn default() -> Self {
        SuccessionOutcome::Succession {
            new_leader_vip_id: String::new(),
            reason: String::new(),
        }
    }
}

/// Regent behavior driven by regent's traits.
#[derive(Debug, Clone, PartialEq)]
pub enum RegentBehavior {
    /// Loyal regency, heir assumes power at majority (age 18).
    Stewardship,
    /// Regent delays handover, faction loyalty drops.
    PowerGrab,
    /// Regent attempts coup, triggers civil war risk.
    Usurpation,
}

/// Determine regent behavior from traits.
pub fn regent_behavior(regent: &crate::politics::vip_registry::Vip) -> RegentBehavior {
    if regent.has_trait("Ambitious") {
        RegentBehavior::PowerGrab
    } else if regent.has_trait("Cruel") {
        RegentBehavior::Usurpation
    } else if regent.has_trait("Loyal") {
        RegentBehavior::Stewardship
    } else {
        RegentBehavior::Stewardship // Default
    }
}

// ============================================================================
// SUCCESSION ENGINE
// ============================================================================

use crate::state::Country;
use crate::politics::system::GovernmentForm;
use crate::politics::vip_registry::{VipRegistry, VipRoleExtended, DeathCause};

/// Process a death/incapacity event for a Head of State.
///
/// Determines the succession path based on the government form and executes
/// the appropriate succession mechanism.
///
/// # Arguments
/// * `country` - Mutable country.
/// * `vip_registry` - Mutable VIP registry.
/// * `deceased_vip_id` - VIP ID of the deceased/incapacitated leader.
/// * `death_cause` - Cause of death (for logging).
/// * `current_turn` - Current game turn.
/// * `rng` - Random number generator.
///
/// # Returns
/// Tuple of (SuccessionOutcome, diagnostic messages).
pub fn process_succession<R: rand::Rng>(
    country: &mut Country,
    vip_registry: &mut VipRegistry,
    deceased_vip_id: &str,
    death_cause: &DeathCause,
    current_turn: u32,
    rng: &mut R,
) -> (SuccessionOutcome, Vec<String>) {
    let mut messages = Vec::new();

    let gov_form = country.politics.government_form;
    messages.push(format!(
        "[SUCCESSION] Processing {} death (cause: {:?}) for {:?} regime.",
        deceased_vip_id, death_cause, gov_form
    ));

    let outcome = match gov_form {
        GovernmentForm::AbsoluteMonarchy
        | GovernmentForm::DualistMonarchy
        | GovernmentForm::ConstitutionalMonarchy
        | GovernmentForm::ElectiveMonarchy => {
            monarchy_succession(country, vip_registry, deceased_vip_id, current_turn, rng, &mut messages)
        }
        GovernmentForm::ParliamentaryDemocracy
        | GovernmentForm::PresidentialRepublic
        | GovernmentForm::SemiPresidentialRepublic
        | GovernmentForm::DirectorialDemocracy => {
            democratic_succession(country, vip_registry, deceased_vip_id, current_turn, &mut messages)
        }
        GovernmentForm::MilitaryDictatorship => {
            military_succession(country, vip_registry, deceased_vip_id, current_turn, rng, &mut messages)
        }
        GovernmentForm::Theocracy => {
            theocratic_succession(country, vip_registry, deceased_vip_id, current_turn, rng, &mut messages)
        }
        GovernmentForm::OnePartyState => {
            one_party_succession(country, vip_registry, deceased_vip_id, current_turn, &mut messages)
        }
    };

    (outcome, messages)
}

/// Monarchy succession: heir inherits throne, regency if underage.
fn monarchy_succession<R: rand::Rng>(
    country: &mut Country,
    vip_registry: &mut VipRegistry,
    deceased_vip_id: &str,
    _current_turn: u32,
    rng: &mut R,
    messages: &mut Vec<String>,
) -> SuccessionOutcome {
    // Check if there's a royal dynasty with an heir.
    if let Some(ref mut dynasty) = country.politics.royal_dynasty {
        // Find the heir in the succession line, excluding the deceased monarch.
        let succession_line: Vec<RoyalFamilyMember> = dynasty.succession_line()
            .into_iter()
            .filter(|m| m.vip_id != deceased_vip_id)
            .cloned()
            .collect();

        if let Some(heir) = succession_line.first() {
            // Check if heir is underage.
            let heir_age = vip_registry.get(&heir.vip_id).map(|v| v.age).unwrap_or(18);

            if heir_age < 18 {
                // Underage heir → establish regency.
                // Select regent from royal family (consort, sibling) or royal council.
                let regent_id = select_regent(dynasty, vip_registry, deceased_vip_id, rng);
                if let Some(regent_id) = regent_id {
                    // Mark regent as Regent role.
                    if let Some(regent_vip) = vip_registry.get_mut(&regent_id) {
                        regent_vip.add_role(VipRoleExtended::Regent);
                    }

                    dynasty.regency_active = true;
                    dynasty.current_regent_id = Some(regent_id.clone());
                    dynasty.current_monarch_id = Some(heir.vip_id.clone());

                    // Mark heir as Monarch role.
                    if let Some(heir_vip) = vip_registry.get_mut(&heir.vip_id) {
                        heir_vip.add_role(VipRoleExtended::Monarch);
                    }

                    messages.push(format!(
                        "[SUCCESSION] Regency established: heir {} (age {}) under regent {}.",
                        heir.vip_id, heir_age, regent_id
                    ));

                    return SuccessionOutcome::Regency {
                        regent_vip_id: regent_id,
                        heir_vip_id: heir.vip_id.clone(),
                        council: dynasty.regency_council.clone(),
                    };
                }
            } else {
                // Heir is of age → direct succession.
                if let Some(heir_vip) = vip_registry.get_mut(&heir.vip_id) {
                    heir_vip.add_role(VipRoleExtended::Monarch);
                }
                dynasty.current_monarch_id = Some(heir.vip_id.clone());
                dynasty.regency_active = false;
                dynasty.current_regent_id = None;

                // Update the country's head_of_state.
                if let Some(heir_vip) = vip_registry.get(&heir.vip_id) {
                    country.politics.head_of_state.name = heir_vip.full_name.clone();
                    country.politics.head_of_state.age = heir_vip.age;
                }

                messages.push(format!(
                    "[SUCCESSION] Heir {} (age {}) inherits throne.",
                    heir.vip_id, heir_age
                ));

                return SuccessionOutcome::Succession {
                    new_leader_vip_id: heir.vip_id.clone(),
                    reason: format!("Monarchy hereditary succession (age {})", heir_age),
                };
            }
        }
    }

    // No heir found → succession crisis.
    messages.push("[SUCCESSION] No legitimate heir found — succession crisis!".to_string());
    SuccessionOutcome::Crisis {
        provisional_leader_vip_id: String::new(),
    }
}

/// Select a regent from the royal family or council.
fn select_regent<R: rand::Rng>(
    dynasty: &RoyalDynasty,
    vip_registry: &VipRegistry,
    deceased_vip_id: &str,
    _rng: &mut R,
) -> Option<String> {
    // Prefer consort, then sibling, then cousin.
    let mut candidates: Vec<&RoyalFamilyMember> = dynasty.members.iter()
        .filter(|m| m.vip_id != deceased_vip_id && !m.vip_id.is_empty())
        .collect();

    // Sort by relation priority: Consort > Sibling > Cousin > Child
    candidates.sort_by_key(|m| match m.relation {
        RoyalRelation::Consort => 0,
        RoyalRelation::Sibling => 1,
        RoyalRelation::Cousin => 2,
        RoyalRelation::Child => 3, // Adult child could also be regent
        RoyalRelation::Regent => 4,
        RoyalRelation::Monarch => 5,
    });

    // Find first candidate who is alive and adult.
    for candidate in &candidates {
        if let Some(vip) = vip_registry.get(&candidate.vip_id) {
            if !vip.is_dead && vip.age >= 21 {
                return Some(candidate.vip_id.clone());
            }
        }
    }

    // Fallback: first alive candidate regardless of age.
    for candidate in &candidates {
        if let Some(vip) = vip_registry.get(&candidate.vip_id) {
            if !vip.is_dead {
                return Some(candidate.vip_id.clone());
            }
        }
    }

    None
}

/// Democratic succession: constitutional order, snap election.
fn democratic_succession(
    country: &mut Country,
    vip_registry: &mut VipRegistry,
    _deceased_vip_id: &str,
    _current_turn: u32,
    messages: &mut Vec<String>,
) -> SuccessionOutcome {
    // For parliamentary democracies: Speaker takes over as acting Head of State,
    // snap election triggered within 2 turns.
    // For presidential republics: VP → Speaker → Chief Justice.

    // Try to find a Speaker in the VIP registry.
    let speakers = vip_registry.get_by_role(&VipRoleExtended::Speaker);
    if let Some(speaker) = speakers.first() {
        // Speaker becomes acting Head of State.
        country.politics.head_of_state.name = speaker.full_name.clone();
        country.politics.head_of_state.age = speaker.age;

        messages.push(format!(
            "[SUCCESSION] Speaker {} becomes acting Head of State. Snap election triggered.",
            speaker.full_name
        ));

        return SuccessionOutcome::SnapElection {
            reason: "Death of Head of State — Speaker assumes acting role".to_string(),
        };
    }

    // No speaker found → crisis.
    messages.push("[SUCCESSION] No speaker available — constitutional crisis!".to_string());
    SuccessionOutcome::Crisis {
        provisional_leader_vip_id: String::new(),
    }
}

/// Military dictatorship succession: power struggle among generals.
fn military_succession<R: rand::Rng>(
    _country: &mut Country,
    vip_registry: &mut VipRegistry,
    _deceased_vip_id: &str,
    _current_turn: u32,
    rng: &mut R,
    messages: &mut Vec<String>,
) -> SuccessionOutcome {
    // Gather all MilitaryCommander VIPs as contenders.
    let contenders: Vec<String> = vip_registry.get_by_role(&VipRoleExtended::MilitaryCommander)
        .into_iter()
        .filter(|v| !v.is_dead)
        .map(|v| v.id.clone())
        .collect();

    if contenders.is_empty() {
        messages.push("[SUCCESSION] No military commanders available — power vacuum!".to_string());
        return SuccessionOutcome::Crisis {
            provisional_leader_vip_id: String::new(),
        };
    }

    // Power struggle duration: 2–6 turns.
    let duration = 2 + rng.gen_range(0..5);

    messages.push(format!(
        "[SUCCESSION] Military power struggle initiated: {} contenders, {} turns.",
        contenders.len(), duration
    ));

    SuccessionOutcome::PowerStruggle {
        contenders,
        duration_turns: duration,
    }
}

/// Theocratic succession: conclave/synod.
fn theocratic_succession<R: rand::Rng>(
    _country: &mut Country,
    vip_registry: &mut VipRegistry,
    _deceased_vip_id: &str,
    _current_turn: u32,
    rng: &mut R,
    messages: &mut Vec<String>,
) -> SuccessionOutcome {
    // Gather all ReligiousLeader VIPs as electors.
    let electors: Vec<String> = vip_registry.get_by_role(&VipRoleExtended::ReligiousLeader)
        .into_iter()
        .filter(|v| !v.is_dead)
        .map(|v| v.id.clone())
        .collect();

    if electors.is_empty() {
        messages.push("[SUCCESSION] No religious leaders available — spiritual crisis!".to_string());
        return SuccessionOutcome::Crisis {
            provisional_leader_vip_id: String::new(),
        };
    }

    // Conclave: 2–5 rounds.
    let rounds = 2 + rng.gen_range(0..4);

    messages.push(format!(
        "[SUCCESSION] Conclave called: {} electors, {} rounds.",
        electors.len(), rounds
    ));

    SuccessionOutcome::Conclave { electors, rounds }
}

/// One-party state succession: party internal vote.
fn one_party_succession(
    country: &mut Country,
    vip_registry: &mut VipRegistry,
    _deceased_vip_id: &str,
    _current_turn: u32,
    messages: &mut Vec<String>,
) -> SuccessionOutcome {
    // In a one-party state, the ruling party's internal organization selects
    // the new leader. The most influential party member becomes the new leader.

    // Try to find a Minister or DeputySpeaker as the next in line.
    let ministers = vip_registry.get_by_role(&VipRoleExtended::Minister);
    if let Some(minister) = ministers.first() {
        country.politics.head_of_state.name = minister.full_name.clone();
        country.politics.head_of_state.age = minister.age;

        messages.push(format!(
            "[SUCCESSION] Party selects {} as new leader (internal vote).",
            minister.full_name
        ));

        return SuccessionOutcome::Succession {
            new_leader_vip_id: minister.id.clone(),
            reason: "One-party internal leadership vote".to_string(),
        };
    }

    messages.push("[SUCCESSION] No party successor available — factional crisis!".to_string());
    SuccessionOutcome::Crisis {
        provisional_leader_vip_id: String::new(),
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::politics::vip_registry::{Vip, VipRegistry};

    #[test]
    fn test_royal_dynasty_new() {
        let dynasty = RoyalDynasty::new("Piast".to_string());
        assert_eq!(dynasty.dynasty_name, "Piast");
        assert!(dynasty.members.is_empty());
        assert!(!dynasty.regency_active);
    }

    #[test]
    fn test_heir_apparent() {
        let mut dynasty = RoyalDynasty::new("Piast".to_string());
        dynasty.members.push(RoyalFamilyMember {
            vip_id: "VIP-001".to_string(),
            relation: RoyalRelation::Monarch,
            is_heir_apparent: false,
            succession_order: 0,
            ..Default::default()
        });
        dynasty.members.push(RoyalFamilyMember {
            vip_id: "VIP-002".to_string(),
            relation: RoyalRelation::Child,
            is_heir_apparent: true,
            succession_order: 1,
            ..Default::default()
        });

        let heir = dynasty.heir_apparent().unwrap();
        assert_eq!(heir.vip_id, "VIP-002");
        assert_eq!(heir.relation, RoyalRelation::Child);
    }

    #[test]
    fn test_succession_line_ordered() {
        let mut dynasty = RoyalDynasty::new("Habsburg".to_string());
        dynasty.members.push(RoyalFamilyMember {
            vip_id: "VIP-003".to_string(),
            succession_order: 3,
            is_legitimate: true,
            ..Default::default()
        });
        dynasty.members.push(RoyalFamilyMember {
            vip_id: "VIP-001".to_string(),
            succession_order: 1,
            is_legitimate: true,
            ..Default::default()
        });
        dynasty.members.push(RoyalFamilyMember {
            vip_id: "VIP-002".to_string(),
            succession_order: 2,
            is_legitimate: true,
            ..Default::default()
        });
        dynasty.members.push(RoyalFamilyMember {
            vip_id: "VIP-004".to_string(),
            succession_order: 4,
            is_legitimate: false, // Illegitimate — excluded.
            ..Default::default()
        });

        let line = dynasty.succession_line();
        assert_eq!(line.len(), 3); // Only legitimate members.
        assert_eq!(line[0].vip_id, "VIP-001");
        assert_eq!(line[1].vip_id, "VIP-002");
        assert_eq!(line[2].vip_id, "VIP-003");
    }

    #[test]
    fn test_heir_is_underage() {
        let mut dynasty = RoyalDynasty::new("Piast".to_string());
        dynasty.members.push(RoyalFamilyMember {
            vip_id: "VIP-001".to_string(),
            is_heir_apparent: true,
            ..Default::default()
        });

        let mut registry = VipRegistry::new();
        registry.register_new(Vip {
            id: "VIP-001".to_string(),
            full_name: "Young Prince".to_string(),
            age: 12,
            ..Default::default()
        });

        assert!(dynasty.heir_is_underage(&registry));

        // Age the heir to 18.
        registry.age_all_vips();
        registry.age_all_vips();
        registry.age_all_vips();
        registry.age_all_vips();
        registry.age_all_vips();
        registry.age_all_vips();
        assert!(!dynasty.heir_is_underage(&registry));
    }

    #[test]
    fn test_regent_behavior_loyal() {
        let regent = Vip {
            traits: vec!["Loyal".to_string()],
            ..Default::default()
        };
        assert_eq!(regent_behavior(&regent), RegentBehavior::Stewardship);
    }

    #[test]
    fn test_regent_behavior_ambitious() {
        let regent = Vip {
            traits: vec!["Ambitious".to_string()],
            ..Default::default()
        };
        assert_eq!(regent_behavior(&regent), RegentBehavior::PowerGrab);
    }

    #[test]
    fn test_regent_behavior_cruel() {
        let regent = Vip {
            traits: vec!["Cruel".to_string()],
            ..Default::default()
        };
        assert_eq!(regent_behavior(&regent), RegentBehavior::Usurpation);
    }

    #[test]
    fn test_regent_behavior_default() {
        let regent = Vip {
            traits: vec!["Diplomatic".to_string()],
            ..Default::default()
        };
        assert_eq!(regent_behavior(&regent), RegentBehavior::Stewardship);
    }

    #[test]
    fn test_succession_outcome_default() {
        let outcome = SuccessionOutcome::default();
        match outcome {
            SuccessionOutcome::Succession { new_leader_vip_id, reason } => {
                assert!(new_leader_vip_id.is_empty());
                assert!(reason.is_empty());
            }
            _ => panic!("Default should be Succession"),
        }
    }

    #[test]
    fn test_monarchy_succession_adult_heir() {
        let mut country = Country::default();
        country.politics.government_form = GovernmentForm::AbsoluteMonarchy;

        let mut registry = VipRegistry::new();
        let monarch_id = registry.register_new(Vip {
            full_name: "King Jan".to_string(),
            age: 70,
            roles: vec![VipRoleExtended::Monarch],
            ..Default::default()
        });
        let heir_id = registry.register_new(Vip {
            full_name: "Prince Piotr".to_string(),
            age: 25,
            ..Default::default()
        });

        country.politics.royal_dynasty = Some(RoyalDynasty {
            dynasty_name: "Piast".to_string(),
            members: vec![
                RoyalFamilyMember {
                    vip_id: monarch_id.clone(),
                    relation: RoyalRelation::Monarch,
                    is_legitimate: true,
                    succession_order: 0,
                    ..Default::default()
                },
                RoyalFamilyMember {
                    vip_id: heir_id.clone(),
                    relation: RoyalRelation::Child,
                    is_legitimate: true,
                    is_heir_apparent: true,
                    succession_order: 1,
                    ..Default::default()
                },
            ],
            current_monarch_id: Some(monarch_id.clone()),
            ..Default::default()
        });

        let mut rng = rand::thread_rng();
        let (outcome, msgs) = process_succession(
            &mut country, &mut registry, &monarch_id,
            &DeathCause::OldAge, 10, &mut rng,
        );

        assert!(!msgs.is_empty());
        match outcome {
            SuccessionOutcome::Succession { new_leader_vip_id, .. } => {
                assert_eq!(new_leader_vip_id, heir_id);
            }
            _ => panic!("Adult heir should get direct succession"),
        }
        // Head of state should be updated.
        assert_eq!(country.politics.head_of_state.name, "Prince Piotr");
    }

    #[test]
    fn test_monarchy_succession_underage_heir_regency() {
        let mut country = Country::default();
        country.politics.government_form = GovernmentForm::AbsoluteMonarchy;

        let mut registry = VipRegistry::new();
        let monarch_id = registry.register_new(Vip {
            full_name: "King Jan".to_string(),
            age: 70,
            roles: vec![VipRoleExtended::Monarch],
            ..Default::default()
        });
        let heir_id = registry.register_new(Vip {
            full_name: "Young Prince Piotr".to_string(),
            age: 10,
            ..Default::default()
        });
        let consort_id = registry.register_new(Vip {
            full_name: "Queen Maria".to_string(),
            age: 45,
            ..Default::default()
        });

        country.politics.royal_dynasty = Some(RoyalDynasty {
            dynasty_name: "Piast".to_string(),
            members: vec![
                RoyalFamilyMember {
                    vip_id: monarch_id.clone(),
                    relation: RoyalRelation::Monarch,
                    is_legitimate: true,
                    succession_order: 0,
                    ..Default::default()
                },
                RoyalFamilyMember {
                    vip_id: consort_id.clone(),
                    relation: RoyalRelation::Consort,
                    is_legitimate: true,
                    succession_order: 99,
                    ..Default::default()
                },
                RoyalFamilyMember {
                    vip_id: heir_id.clone(),
                    relation: RoyalRelation::Child,
                    is_legitimate: true,
                    is_heir_apparent: true,
                    succession_order: 1,
                    ..Default::default()
                },
            ],
            current_monarch_id: Some(monarch_id.clone()),
            ..Default::default()
        });

        let mut rng = rand::thread_rng();
        let (outcome, _msgs) = process_succession(
            &mut country, &mut registry, &monarch_id,
            &DeathCause::OldAge, 10, &mut rng,
        );

        match outcome {
            SuccessionOutcome::Regency { regent_vip_id, heir_vip_id, .. } => {
                assert_eq!(heir_vip_id, heir_id);
                assert_eq!(regent_vip_id, consort_id, "Consort should be regent");
            }
            _ => panic!("Underage heir should trigger regency"),
        }
    }

    #[test]
    fn test_democratic_succession_snap_election() {
        let mut country = Country::default();
        country.politics.government_form = GovernmentForm::ParliamentaryDemocracy;

        let mut registry = VipRegistry::new();
        let hos_id = registry.register_new(Vip {
            full_name: "President Jan".to_string(),
            age: 65,
            roles: vec![VipRoleExtended::HeadOfState],
            ..Default::default()
        });
        let _speaker_id = registry.register_new(Vip {
            full_name: "Speaker Anna".to_string(),
            age: 55,
            roles: vec![VipRoleExtended::Speaker],
            ..Default::default()
        });

        let mut rng = rand::thread_rng();
        let (outcome, _msgs) = process_succession(
            &mut country, &mut registry, &hos_id,
            &DeathCause::Illness, 10, &mut rng,
        );

        match outcome {
            SuccessionOutcome::SnapElection { reason } => {
                assert!(!reason.is_empty());
            }
            _ => panic!("Democracy should trigger snap election"),
        }
        assert_eq!(country.politics.head_of_state.name, "Speaker Anna");
    }

    #[test]
    fn test_military_succession_power_struggle() {
        let mut country = Country::default();
        country.politics.government_form = GovernmentForm::MilitaryDictatorship;

        let mut registry = VipRegistry::new();
        let dictator_id = registry.register_new(Vip {
            full_name: "General Jan".to_string(),
            age: 60,
            roles: vec![VipRoleExtended::HeadOfState],
            ..Default::default()
        });
        let _gen1_id = registry.register_new(Vip {
            full_name: "General Piotr".to_string(),
            age: 50,
            roles: vec![VipRoleExtended::MilitaryCommander],
            ..Default::default()
        });
        let _gen2_id = registry.register_new(Vip {
            full_name: "General Anna".to_string(),
            age: 55,
            roles: vec![VipRoleExtended::MilitaryCommander],
            ..Default::default()
        });

        let mut rng = rand::thread_rng();
        let (outcome, _msgs) = process_succession(
            &mut country, &mut registry, &dictator_id,
            &DeathCause::Coup, 5, &mut rng,
        );

        match outcome {
            SuccessionOutcome::PowerStruggle { contenders, duration_turns } => {
                assert_eq!(contenders.len(), 2);
                assert!((2..=6).contains(&duration_turns));
            }
            _ => panic!("Military dictatorship should trigger power struggle"),
        }
    }

    #[test]
    fn test_theocratic_succession_conclave() {
        let mut country = Country::default();
        country.politics.government_form = GovernmentForm::Theocracy;

        let mut registry = VipRegistry::new();
        let pope_id = registry.register_new(Vip {
            full_name: "Bishop Jan".to_string(),
            age: 70,
            roles: vec![VipRoleExtended::HeadOfState],
            ..Default::default()
        });
        let _bishop1_id = registry.register_new(Vip {
            full_name: "Bishop Piotr".to_string(),
            age: 60,
            roles: vec![VipRoleExtended::ReligiousLeader],
            ..Default::default()
        });
        let _bishop2_id = registry.register_new(Vip {
            full_name: "Bishop Anna".to_string(),
            age: 65,
            roles: vec![VipRoleExtended::ReligiousLeader],
            ..Default::default()
        });

        let mut rng = rand::thread_rng();
        let (outcome, _msgs) = process_succession(
            &mut country, &mut registry, &pope_id,
            &DeathCause::Illness, 10, &mut rng,
        );

        match outcome {
            SuccessionOutcome::Conclave { electors, rounds } => {
                assert_eq!(electors.len(), 2);
                assert!((2..=5).contains(&rounds));
            }
            _ => panic!("Theocracy should trigger conclave"),
        }
    }

    #[test]
    fn test_monarchy_no_heir_crisis() {
        let mut country = Country::default();
        country.politics.government_form = GovernmentForm::AbsoluteMonarchy;
        country.politics.royal_dynasty = Some(RoyalDynasty::new("Empty".to_string()));

        let mut registry = VipRegistry::new();
        let monarch_id = registry.register_new(Vip {
            full_name: "Lonely King".to_string(),
            age: 80,
            ..Default::default()
        });

        let mut rng = rand::thread_rng();
        let (outcome, _) = process_succession(
            &mut country, &mut registry, &monarch_id,
            &DeathCause::OldAge, 10, &mut rng,
        );

        match outcome {
            SuccessionOutcome::Crisis { .. } => {}
            _ => panic!("No heir should trigger crisis"),
        }
    }
}

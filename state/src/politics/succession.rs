//! Phase 48: Regime-specific succession and royal dynasties.
//!
//! This module implements:
//! - Royal dynasty tracking (family trees, heirs, regency).
//! - Regime-specific succession outcomes (monarchy, democracy, military, theocracy).
//! - Succession triggers: death, incapacity, coup, resignation.

use serde::{Deserialize, Serialize};

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
use crate::politics::vip_registry::{VipRegistry, VipRoleExtended, DeathCause, Vip};

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

    let gov_form = country.politics.government_form.clone();
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
    current_turn: u32,
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
    rng: &mut R,
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
    deceased_vip_id: &str,
    current_turn: u32,
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
    country: &mut Country,
    vip_registry: &mut VipRegistry,
    deceased_vip_id: &str,
    current_turn: u32,
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
    country: &mut Country,
    vip_registry: &mut VipRegistry,
    deceased_vip_id: &str,
    current_turn: u32,
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
    deceased_vip_id: &str,
    current_turn: u32,
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
        let (outcome, msgs) = process_succession(
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
        let speaker_id = registry.register_new(Vip {
            full_name: "Speaker Anna".to_string(),
            age: 55,
            roles: vec![VipRoleExtended::Speaker],
            ..Default::default()
        });

        let mut rng = rand::thread_rng();
        let (outcome, msgs) = process_succession(
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
        let gen1_id = registry.register_new(Vip {
            full_name: "General Piotr".to_string(),
            age: 50,
            roles: vec![VipRoleExtended::MilitaryCommander],
            ..Default::default()
        });
        let gen2_id = registry.register_new(Vip {
            full_name: "General Anna".to_string(),
            age: 55,
            roles: vec![VipRoleExtended::MilitaryCommander],
            ..Default::default()
        });

        let mut rng = rand::thread_rng();
        let (outcome, msgs) = process_succession(
            &mut country, &mut registry, &dictator_id,
            &DeathCause::Coup, 5, &mut rng,
        );

        match outcome {
            SuccessionOutcome::PowerStruggle { contenders, duration_turns } => {
                assert_eq!(contenders.len(), 2);
                assert!(duration_turns >= 2 && duration_turns <= 6);
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
        let bishop1_id = registry.register_new(Vip {
            full_name: "Bishop Piotr".to_string(),
            age: 60,
            roles: vec![VipRoleExtended::ReligiousLeader],
            ..Default::default()
        });
        let bishop2_id = registry.register_new(Vip {
            full_name: "Bishop Anna".to_string(),
            age: 65,
            roles: vec![VipRoleExtended::ReligiousLeader],
            ..Default::default()
        });

        let mut rng = rand::thread_rng();
        let (outcome, msgs) = process_succession(
            &mut country, &mut registry, &pope_id,
            &DeathCause::Illness, 10, &mut rng,
        );

        match outcome {
            SuccessionOutcome::Conclave { electors, rounds } => {
                assert_eq!(electors.len(), 2);
                assert!(rounds >= 2 && rounds <= 5);
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

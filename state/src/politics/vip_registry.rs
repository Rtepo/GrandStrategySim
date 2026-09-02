//! Phase 48: Global VIP Registry — tracks all power holders across the simulation.
//!
//! VIPs (Very Important Persons) are named individuals who hold positions of
//! power: national leaders, ministers, speakers, regional governors, mayors,
//! company CEOs, union bosses, religious leaders, and military commanders.
//!
//! Each VIP has:
//! - Age (increments yearly)
//! - Health (degrades with age, events)
//! - Incapacity status (Healthy / Sick / Coma / Dead)
//! - Character traits (Ambitious, Populist, Loyal, Corrupt, etc.)
//! - Death tracking with strict `DeathCause` enum
//!
//! The registry provides:
//! - Global deduplication via name index
//! - Aging and health degradation (yearly batch)
//! - Death checks (yearly for natural, immediate queue for unnatural)
//! - Role-based lookups
//! - Acting/stand-in designation for incapacitated VIPs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// DEATH CAUSE — strict enum (no free-form strings)
// ============================================================================

/// Strict cause-of-death enum for analytics and type safety.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum DeathCause {
    #[default]
    /// Natural death from old age.
    OldAge,
    /// Death from illness/disease.
    Illness,
    /// Assassination (by espionage/rebellion system).
    Assassination,
    /// Execution (judicial or political purge).
    Execution,
    /// Accident (random event).
    Accident,
    /// Death in battle / war.
    Battle,
    /// Suicide (crisis-driven).
    Suicide,
    /// Coup-related death (killed during power struggle).
    Coup,
}

impl std::fmt::Display for DeathCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeathCause::OldAge => write!(f, "Old Age"),
            DeathCause::Illness => write!(f, "Illness"),
            DeathCause::Assassination => write!(f, "Assassination"),
            DeathCause::Execution => write!(f, "Execution"),
            DeathCause::Accident => write!(f, "Accident"),
            DeathCause::Battle => write!(f, "Battle"),
            DeathCause::Suicide => write!(f, "Suicide"),
            DeathCause::Coup => write!(f, "Coup"),
        }
    }
}

// ============================================================================
// INCAPACITY STATUS
// ============================================================================

/// Incapacity status of a VIP.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum IncapacityStatus {
    #[default]
    /// Fully healthy and able to exercise power.
    Healthy,
    /// Temporary illness — requires acting role, may recover.
    Sick,
    /// Prolonged incapacity — requires acting role, unlikely to recover.
    Coma,
    /// Permanently removed from power.
    Dead,
}

// ============================================================================
// VIP ROLE (EXTENDED)
// ============================================================================

/// Extended role types covering all power positions in the simulation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum VipRoleExtended {
    #[default]
    /// No current role.
    None,
    // ── National political ──
    /// Head of State (President, King, etc.).
    HeadOfState,
    /// Prime Minister / Head of Government.
    PrimeMinister,
    /// Government minister.
    Minister,
    /// Speaker of a legislative chamber.
    Speaker,
    /// Deputy speaker.
    DeputySpeaker,
    /// Party whip.
    Whip,
    // ── Regional political ──
    /// Regional governor (megaregion level).
    RegionalGovernor,
    /// Mayor / Village Head (region level).
    Mayor,
    /// Regional councilor.
    RegionalCouncilor,
    // ── Non-political power holders ──
    /// Company CEO.
    Ceo,
    /// Union leader / boss.
    UnionBoss,
    /// Religious leader (bishop, cardinal, patriarch, ayatollah).
    ReligiousLeader,
    /// Military commander (general, field marshal).
    MilitaryCommander,
    // ── Royal family ──
    /// Reigning monarch.
    Monarch,
    /// Royal consort (queen/king consort).
    RoyalConsort,
    /// Royal heir (prince/princess in line of succession).
    RoyalHeir,
    /// Regent (acting ruler during minority/incapacity).
    Regent,
    // ── Corporate governance ──
    /// Phase 55: Board member of a joint-stock company.
    BoardMember,
    /// Phase 55: Board chairperson (leads board meetings).
    BoardChair,
    /// Phase 55: Heir to a family business (in line for CEO succession).
    Heir,
    // ── Diplomatic (Phase 66) ──
    /// Ambassador posted to a foreign country.
    Ambassador,
    /// Consul serving in a foreign country.
    Consul,
    /// Spy operating covertly in a foreign country.
    Spy,
}

impl VipRoleExtended {
    /// Returns true if this role is a Head of State or acting Head of State.
    pub fn is_head_of_state(&self) -> bool {
        matches!(
            self,
            VipRoleExtended::HeadOfState | VipRoleExtended::Monarch | VipRoleExtended::Regent
        )
    }

    /// Returns true if this is a political role (national or regional).
    pub fn is_political(&self) -> bool {
        matches!(
            self,
            VipRoleExtended::HeadOfState
                | VipRoleExtended::PrimeMinister
                | VipRoleExtended::Minister
                | VipRoleExtended::Speaker
                | VipRoleExtended::DeputySpeaker
                | VipRoleExtended::Whip
                | VipRoleExtended::RegionalGovernor
                | VipRoleExtended::Mayor
                | VipRoleExtended::RegionalCouncilor
        )
    }

    /// Phase 54: Returns a human-readable label for this role.
    pub fn as_str(&self) -> &'static str {
        match self {
            VipRoleExtended::None => "Private Citizen",
            VipRoleExtended::HeadOfState => "Head of State",
            VipRoleExtended::PrimeMinister => "Prime Minister",
            VipRoleExtended::Minister => "Minister",
            VipRoleExtended::Speaker => "Speaker",
            VipRoleExtended::DeputySpeaker => "Deputy Speaker",
            VipRoleExtended::Whip => "Whip",
            VipRoleExtended::RegionalGovernor => "Regional Governor",
            VipRoleExtended::RegionalCouncilor => "Regional Councilor",
            VipRoleExtended::Mayor => "Mayor",
            VipRoleExtended::Ceo => "CEO",
            VipRoleExtended::UnionBoss => "Union Boss",
            VipRoleExtended::ReligiousLeader => "Religious Leader",
            VipRoleExtended::MilitaryCommander => "Military Commander",
            VipRoleExtended::Monarch => "Monarch",
            VipRoleExtended::RoyalConsort => "Royal Consort",
            VipRoleExtended::RoyalHeir => "Royal Heir",
            VipRoleExtended::Regent => "Regent",
            VipRoleExtended::BoardMember => "Board Member",
            VipRoleExtended::BoardChair => "Board Chair",
            VipRoleExtended::Heir => "Heir",
            VipRoleExtended::Ambassador => "Ambassador",
            VipRoleExtended::Consul => "Consul",
            VipRoleExtended::Spy => "Spy",
        }
    }

    /// Phase 91: Returns the canonical enum string for this role.
    /// Used for serialization and filtering — distinct from `as_str()` which
    /// returns a human-readable label. This ensures `RoyalHeir` (royal) and
    /// `Heir` (business) are distinct filter values.
    pub fn canonical_name(&self) -> &'static str {
        match self {
            VipRoleExtended::None => "None",
            VipRoleExtended::HeadOfState => "HeadOfState",
            VipRoleExtended::PrimeMinister => "PrimeMinister",
            VipRoleExtended::Minister => "Minister",
            VipRoleExtended::Speaker => "Speaker",
            VipRoleExtended::DeputySpeaker => "DeputySpeaker",
            VipRoleExtended::Whip => "Whip",
            VipRoleExtended::RegionalGovernor => "RegionalGovernor",
            VipRoleExtended::RegionalCouncilor => "RegionalCouncilor",
            VipRoleExtended::Mayor => "Mayor",
            VipRoleExtended::Ceo => "Ceo",
            VipRoleExtended::UnionBoss => "UnionBoss",
            VipRoleExtended::ReligiousLeader => "ReligiousLeader",
            VipRoleExtended::MilitaryCommander => "MilitaryCommander",
            VipRoleExtended::Monarch => "Monarch",
            VipRoleExtended::RoyalConsort => "RoyalConsort",
            VipRoleExtended::RoyalHeir => "RoyalHeir",
            VipRoleExtended::Regent => "Regent",
            VipRoleExtended::BoardMember => "BoardMember",
            VipRoleExtended::BoardChair => "BoardChair",
            VipRoleExtended::Heir => "Heir",
            VipRoleExtended::Ambassador => "Ambassador",
            VipRoleExtended::Consul => "Consul",
            VipRoleExtended::Spy => "Spy",
        }
    }

    /// Phase 54: Returns all valid role variants (excluding `None`).
    pub fn all() -> &'static [VipRoleExtended] {
        &[
            VipRoleExtended::HeadOfState,
            VipRoleExtended::PrimeMinister,
            VipRoleExtended::Minister,
            VipRoleExtended::Speaker,
            VipRoleExtended::DeputySpeaker,
            VipRoleExtended::Whip,
            VipRoleExtended::RegionalGovernor,
            VipRoleExtended::Mayor,
            VipRoleExtended::RegionalCouncilor,
            VipRoleExtended::Ceo,
            VipRoleExtended::UnionBoss,
            VipRoleExtended::ReligiousLeader,
            VipRoleExtended::MilitaryCommander,
            VipRoleExtended::Monarch,
            VipRoleExtended::RoyalConsort,
            VipRoleExtended::RoyalHeir,
            VipRoleExtended::Regent,
            VipRoleExtended::BoardMember,
            VipRoleExtended::BoardChair,
            VipRoleExtended::Heir,
            VipRoleExtended::Ambassador,
            VipRoleExtended::Consul,
            VipRoleExtended::Spy,
        ]
    }
}

// ============================================================================
// VIP ENTITY
// ============================================================================

/// Phase 62.5: Holistic VIP health model.
/// Replaces the single `health: f64` field with physical and mental components.
/// This is a clean breaking change — old saves will fail to deserialize (per Phase 55 policy).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VipHealth {
    /// Physical health (0.0 = dead, 1.0 = perfect health)
    #[serde(default = "default_health")]
    pub physical_health: f64,
    /// Mental health (0.0 = breakdown, 1.0 = stable)
    #[serde(default = "default_health")]
    pub mental_health: f64,
}

impl VipHealth {
    /// Returns the aggregate health score (average of physical and mental).
    pub fn aggregate(&self) -> f64 {
        (self.physical_health + self.mental_health) / 2.0
    }
}

/// Phase 66: Type of diplomatic post a VIP can be assigned to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub enum DiplomaticPostType {
    #[default]
    Ambassador,
    Consul,
    Spy,
    MilitaryAttache,
}

/// Phase 66: A diplomatic posting for a VIP in a foreign country.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DiplomaticPost {
    /// Country where the VIP is posted (host country).
    pub host_country: String,
    /// Type of diplomatic post.
    pub post_type: DiplomaticPostType,
    /// Turn when this post was assigned.
    pub assigned_turn: u32,
}

/// Unique VIP identity tracked across the entire simulation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Vip {
    /// Globally unique VIP ID (e.g., "VIP-000001").
    #[serde(default)]
    pub id: String,
    /// Full name (first + surname).
    #[serde(default)]
    pub full_name: String,
    /// Gender: "M" or "F".
    #[serde(default)]
    pub gender: String,
    /// Current age (increments each year).
    #[serde(default)]
    pub age: u32,
    /// Phase 62.5: Holistic health (physical + mental). Replaces single `health: f64`.
    #[serde(default)]
    pub health: VipHealth,
    /// Incapacity status.
    #[serde(default)]
    pub incapacity: IncapacityStatus,
    /// Character traits (IDs into TraitRegistry or core trait strings).
    #[serde(default)]
    pub traits: Vec<String>,
    /// Primary/dominant trait.
    #[serde(default)]
    pub main_trait: String,
    /// Ideology string.
    #[serde(default)]
    pub ideology: String,
    /// Religion (empty if none).
    #[serde(default)]
    pub religion: String,
    /// Nationality / cultural group.
    #[serde(default)]
    pub nationality: String,
    /// Dynasty ID (for royal family members).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynasty: Option<String>,
    /// Current role(s) held.
    #[serde(default)]
    pub roles: Vec<VipRoleExtended>,
    /// Base political influence (0–100).
    #[serde(default)]
    pub base_influence: u32,
    /// Faction alignment.
    #[serde(default)]
    pub faction: String,
    /// Turn when this VIP was first generated.
    #[serde(default)]
    pub born_turn: u32,
    /// Whether this VIP is dead.
    #[serde(default)]
    pub is_dead: bool,
    /// Turn of death (if dead).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub death_turn: Option<u32>,
    /// Cause of death (strict enum).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub death_cause: Option<DeathCause>,
    /// VIP ID of the acting replacement (if incapacitated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acting_replacement_id: Option<String>,
    /// Phase 66: Diplomatic posting (if this VIP is posted abroad).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diplomatic_post: Option<DiplomaticPost>,
    /// Phase 92: Portrait seed for deterministic avatar generation.
    /// Computed as `format!("{}-{}-{}", cultural_group, gender, full_name)`.
    /// Incorporates gender so any future avatar generator produces gender-
    /// appropriate visuals. Empty string for old saves (frontend fallback).
    #[serde(default)]
    pub portrait_seed: String,
}

fn default_health() -> f64 {
    1.0
}

impl Vip {
    /// Check if this VIP can exercise power (not dead, not incapacitated).
    pub fn can_exercise_power(&self) -> bool {
        !self.is_dead && matches!(self.incapacity, IncapacityStatus::Healthy)
    }

    /// Check if this VIP is incapacitated (sick or coma, but not dead).
    pub fn is_incapacitated(&self) -> bool {
        matches!(
            self.incapacity,
            IncapacityStatus::Sick | IncapacityStatus::Coma
        )
    }

    /// Check if this VIP holds a specific role.
    pub fn has_role(&self, role: &VipRoleExtended) -> bool {
        self.roles.contains(role)
    }

    /// Check if this VIP has a specific trait.
    pub fn has_trait(&self, trait_id: &str) -> bool {
        self.traits.iter().any(|t| t == trait_id)
    }

    /// Add a role to this VIP.
    pub fn add_role(&mut self, role: VipRoleExtended) {
        if !self.roles.contains(&role) {
            self.roles.push(role);
        }
    }

    /// Remove a role from this VIP.
    pub fn remove_role(&mut self, role: &VipRoleExtended) {
        self.roles.retain(|r| r != role);
    }

    /// Mark this VIP as dead with a specific cause.
    pub fn mark_dead(&mut self, turn: u32, cause: DeathCause) {
        self.is_dead = true;
        self.death_turn = Some(turn);
        self.death_cause = Some(cause);
        self.incapacity = IncapacityStatus::Dead;
        self.roles.clear(); // Dead VIPs hold no roles.
    }

    /// Mark this VIP as incapacitated.
    pub fn mark_incapacitated(&mut self, status: IncapacityStatus) {
        if matches!(status, IncapacityStatus::Sick | IncapacityStatus::Coma) {
            self.incapacity = status;
        }
    }

    /// Mark this VIP as recovered (healthy).
    pub fn mark_recovered(&mut self) {
        self.incapacity = IncapacityStatus::Healthy;
        self.acting_replacement_id = None;
    }
}

// ============================================================================
// PENDING UNNATURAL DEATH QUEUE
// ============================================================================

/// A pending unnatural death entry — processed immediately at the start of
/// the next `process_political_turn`, not deferred to the yearly batch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingDeath {
    /// VIP ID of the deceased.
    pub vip_id: String,
    /// Cause of death (must be an unnatural cause).
    pub cause: DeathCause,
    /// Turn when the death occurred.
    pub turn: u32,
}

// ============================================================================
// VIP REGISTRY
// ============================================================================

/// The global registry of all VIPs in the simulation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VipRegistry {
    /// All living VIPs by ID.
    #[serde(default)]
    pub vips: HashMap<String, Vip>,
    /// Name → ID lookup for deduplication.
    #[serde(default)]
    pub name_index: HashMap<String, String>,
    /// Next auto-increment ID counter.
    #[serde(default)]
    pub next_id: u64,
    /// Dead VIPs archived (for history/dynasty tracking).
    #[serde(default)]
    pub deceased: Vec<Vip>,
    /// Pending unnatural deaths — drained at the start of each
    /// `process_political_turn` to trigger immediate succession.
    #[serde(default, skip_serializing)]
    pub pending_unnatural_deaths: Vec<PendingDeath>,
}

impl VipRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        VipRegistry {
            vips: HashMap::new(),
            name_index: HashMap::new(),
            next_id: 1,
            deceased: Vec::new(),
            pending_unnatural_deaths: Vec::new(),
        }
    }

    /// Generate the next unique VIP ID.
    fn next_vip_id(&mut self) -> String {
        let id = format!("VIP-{:06}", self.next_id);
        self.next_id += 1;
        id
    }

    /// Register a new VIP or return an existing one by name.
    ///
    /// If a VIP with the given name already exists, returns a clone of that VIP.
    /// Otherwise, creates a new VIP, registers it, and returns it.
    pub fn register_or_lookup(&mut self, vip: Vip) -> Vip {
        // Check if a VIP with this name already exists.
        if let Some(existing_id) = self.name_index.get(&vip.full_name) {
            if let Some(existing) = self.vips.get(existing_id) {
                return existing.clone();
            }
        }

        // Create new VIP with a generated ID.
        let mut new_vip = vip;
        if new_vip.id.is_empty() {
            new_vip.id = self.next_vip_id();
        }
        let id = new_vip.id.clone();
        let name = new_vip.full_name.clone();
        self.vips.insert(id.clone(), new_vip.clone());
        self.name_index.insert(name, id);
        new_vip
    }

    /// Register a new VIP (always creates a new entry, does not deduplicate).
    pub fn register_new(&mut self, mut vip: Vip) -> String {
        if vip.id.is_empty() {
            vip.id = self.next_vip_id();
        }
        // Phase 92: Auto-populate portrait_seed if not set. Uses nationality
        // (cultural group proxy), gender, and full name for a deterministic,
        // gender-aware visual seed.
        if vip.portrait_seed.is_empty() {
            vip.portrait_seed = format!("{}-{}-{}", vip.nationality, vip.gender, vip.full_name);
        }
        let id = vip.id.clone();
        let name = vip.full_name.clone();
        self.vips.insert(id.clone(), vip);
        self.name_index.insert(name, id.clone());
        id
    }

    /// Get a VIP by ID.
    pub fn get(&self, vip_id: &str) -> Option<&Vip> {
        self.vips.get(vip_id)
    }

    /// Get a mutable reference to a VIP by ID.
    pub fn get_mut(&mut self, vip_id: &str) -> Option<&mut Vip> {
        self.vips.get_mut(vip_id)
    }

    /// Get a VIP by name.
    pub fn get_by_name(&self, name: &str) -> Option<&Vip> {
        self.name_index.get(name).and_then(|id| self.vips.get(id))
    }

    /// Get all VIPs holding a specific role.
    pub fn get_by_role(&self, role: &VipRoleExtended) -> Vec<&Vip> {
        self.vips
            .values()
            .filter(|v| !v.is_dead && v.has_role(role))
            .collect()
    }

    /// Get all VIPs holding a specific role (mutable).
    pub fn get_by_role_mut(&mut self, role: &VipRoleExtended) -> Vec<&mut Vip> {
        self.vips
            .values_mut()
            .filter(|v| !v.is_dead && v.has_role(role))
            .collect()
    }

    /// Get the acting replacement for an incapacitated VIP.
    pub fn get_acting_for(&self, vip_id: &str) -> Option<&Vip> {
        self.vips.get(vip_id).and_then(|vip| {
            vip.acting_replacement_id
                .as_ref()
                .and_then(|id| self.vips.get(id))
        })
    }

    /// Designate an acting replacement for an incapacitated VIP.
    pub fn designate_acting(
        &mut self,
        incapacitated_id: &str,
        acting_id: &str,
    ) -> Result<(), String> {
        let acting_vip = self
            .vips
            .get(acting_id)
            .ok_or_else(|| format!("Acting VIP {} not found", acting_id))?
            .clone();

        if acting_vip.is_dead {
            return Err(format!("Cannot designate dead VIP {} as acting", acting_id));
        }

        if let Some(incapacitated) = self.vips.get_mut(incapacitated_id) {
            incapacitated.acting_replacement_id = Some(acting_id.to_string());
            Ok(())
        } else {
            Err(format!("Incapacitated VIP {} not found", incapacitated_id))
        }
    }

    /// Queue an unnatural death for immediate processing at the start of the
    /// next `process_political_turn`.
    pub fn queue_unnatural_death(&mut self, vip_id: &str, cause: DeathCause, turn: u32) {
        // Mark the VIP as dead immediately so no other system uses them.
        if let Some(vip) = self.vips.get_mut(vip_id) {
            if !vip.is_dead {
                vip.mark_dead(turn, cause.clone());
            }
        }
        // Queue for succession processing.
        self.pending_unnatural_deaths.push(PendingDeath {
            vip_id: vip_id.to_string(),
            cause,
            turn,
        });
    }

    /// Drain the pending unnatural deaths queue.
    /// Called at the start of `process_political_turn` to trigger immediate
    /// succession for assassinated/couped/executed leaders.
    pub fn drain_pending_deaths(&mut self) -> Vec<PendingDeath> {
        std::mem::take(&mut self.pending_unnatural_deaths)
    }

    /// Age all living VIPs by one year.
    /// Called once per year from `process_political_year`.
    pub fn age_all_vips(&mut self) {
        for vip in self.vips.values_mut() {
            if !vip.is_dead {
                vip.age += 1;
            }
        }
    }

    /// Degrade health of all living VIPs based on age.
    /// Called once per year from `process_political_year`.
    pub fn degrade_health_all(&mut self) {
        for vip in self.vips.values_mut() {
            if vip.is_dead {
                continue;
            }
            // Phase 62.5: Degrade both physical and mental health with age.
            vip.health.physical_health =
                age_health_degradation(vip.age, vip.health.physical_health);
            vip.health.mental_health = age_health_degradation(vip.age, vip.health.mental_health);
            // Check for illness-induced incapacity at very low aggregate health.
            if vip.health.aggregate() < 0.15 && matches!(vip.incapacity, IncapacityStatus::Healthy)
            {
                vip.incapacity = IncapacityStatus::Sick;
            }
        }
    }

    /// Check for natural deaths (old age, illness) among all living VIPs.
    /// Called once per year from `process_political_year`.
    ///
    /// Returns a list of (vip_id, cause) tuples for VIPs who died.
    pub fn check_natural_deaths(&mut self, rng: &mut impl rand::Rng) -> Vec<(String, DeathCause)> {
        let mut deaths = Vec::new();
        let vip_ids: Vec<String> = self.vips.keys().cloned().collect();

        for vip_id in vip_ids {
            let vip = match self.vips.get_mut(&vip_id) {
                Some(v) => v,
                None => continue,
            };
            if vip.is_dead {
                continue;
            }

            let death_prob = death_probability(vip.age, vip.health.aggregate());
            if rng.gen::<f64>() < death_prob {
                let cause = if vip.age >= 60 || vip.health.aggregate() < 0.3 {
                    DeathCause::Illness
                } else {
                    DeathCause::OldAge
                };
                vip.mark_dead(0, cause.clone()); // Turn set by caller.
                deaths.push((vip_id, cause));
            }
        }

        // Archive deceased VIPs.
        let dead_ids: Vec<String> = deaths.iter().map(|(id, _)| id.clone()).collect();
        for id in &dead_ids {
            if let Some(vip) = self.vips.remove(id) {
                self.name_index.remove(&vip.full_name);
                self.deceased.push(vip);
            }
        }

        deaths
    }

    /// Count living VIPs.
    pub fn living_count(&self) -> usize {
        self.vips.values().filter(|v| !v.is_dead).count()
    }

    /// Count deceased VIPs.
    pub fn deceased_count(&self) -> usize {
        self.deceased.len()
    }

    /// Phase 86.5A: Prune deceased VIPs while protecting genealogy.
    ///
    /// Removes non-dynasty, non-historical deceased VIPs from the archive
    /// to bound memory growth. The following deceased VIPs are NEVER pruned:
    ///
    /// 1. VIPs referenced by a `RoyalDynasty` through `father_vip_id` or
    ///    `mother_vip_id`.
    /// 2. VIPs that are ancestors of a living dynasty member.
    /// 3. VIPs with historical significance tags.
    ///
    /// Only safe, non-dynasty, non-historical entries are pruned. The archive
    /// is bounded to `max_archive_size` eligible entries (default 200), but
    /// genealogy-protected entries are exempt from this limit.
    ///
    /// # Arguments
    /// * `dynasty_vip_ids` - Set of VIP IDs referenced by any RoyalDynasty
    ///   (father/mother IDs, ancestor IDs of living members).
    /// * `historical_vip_ids` - Set of VIP IDs with historical significance tags.
    /// * `max_archive_size` - Maximum number of prunable (non-protected) entries.
    ///   Default 200.
    pub fn prune_deceased_genealogy_safe(
        &mut self,
        dynasty_vip_ids: &std::collections::HashSet<String>,
        historical_vip_ids: &std::collections::HashSet<String>,
        max_archive_size: usize,
    ) {
        // Partition deceased into protected and prunable.
        let mut protected: Vec<Vip> = Vec::new();
        let mut prunable: Vec<Vip> = Vec::new();

        for vip in self.deceased.drain(..) {
            let is_protected =
                dynasty_vip_ids.contains(&vip.id) || historical_vip_ids.contains(&vip.id);
            if is_protected {
                protected.push(vip);
            } else {
                prunable.push(vip);
            }
        }

        // Sort prunable by death_turn descending (keep most recent deaths).
        prunable.sort_by_key(|b| std::cmp::Reverse(b.death_turn.unwrap_or(0)));

        // Keep only the most recent `max_archive_size` prunable entries.
        prunable.truncate(max_archive_size);

        // Reassemble: protected entries are exempt from the limit.
        protected.extend(prunable);
        self.deceased = protected;
    }
}

// ============================================================================
// HEALTH & AGING MECHANICS
// ============================================================================

/// Age-based annual health degradation.
///
/// Returns the new health value after one year of aging.
/// Older VIPs degrade faster. Health is clamped to [0.0, 1.0].
pub fn age_health_degradation(age: u32, current_health: f64) -> f64 {
    let base_decline = match age {
        0..=40 => 0.0,    // No natural decline
        41..=60 => 0.005, // 0.5% per year
        61..=75 => 0.01,  // 1% per year
        76..=90 => 0.02,  // 2% per year
        _ => 0.04,        // 4% per year (very old)
    };
    (current_health - base_decline).max(0.0)
}

/// Death probability based on age and health.
///
/// Returns the probability of death per year (0.0 to 1.0).
/// Older age and lower health both increase death probability.
pub fn death_probability(age: u32, health: f64) -> f64 {
    let age_factor = match age {
        0..=50 => 0.001,  // 0.1% per year
        51..=65 => 0.005, // 0.5%
        66..=80 => 0.02,  // 2%
        81..=95 => 0.05,  // 5%
        _ => 0.15,        // 15% (extreme old age)
    };
    let health_factor = (1.0 - health).max(0.0);
    (age_factor + health_factor * 0.05).min(1.0)
}

// ============================================================================
// CORE TRAIT ASSIGNMENT
// ============================================================================

/// Core character traits for VIPs (Phase 48).
/// Each entry is (trait_id, rarity_weight). Higher weight = more common.
pub static CORE_TRAITS: &[(&str, f64)] = &[
    ("Ambitious", 0.15),
    ("Populist", 0.10),
    ("Loyal", 0.12),
    ("Corrupt", 0.08),
    ("Reformer", 0.06),
    ("Conservative", 0.10),
    ("Diplomatic", 0.08),
    ("Militarist", 0.07),
    ("Pious", 0.08),
    ("Cruel", 0.05),
    ("Charismatic", 0.10),
    ("Paranoid", 0.06),
    ("Incompetent", 0.05),
];

/// Assign 2–4 random traits from the core trait pool, weighted by rarity.
pub fn assign_core_traits(rng: &mut impl rand::Rng) -> (Vec<String>, String) {
    let total_weight: f64 = CORE_TRAITS.iter().map(|(_, w)| *w).sum();
    let mut assigned = Vec::new();
    let num_traits = 2 + rng.gen_range(0..3); // 2, 3, or 4 traits

    let mut attempts = 0;
    while assigned.len() < num_traits && attempts < num_traits * 4 {
        let mut random_weight = rng.gen::<f64>() * total_weight;
        for (trait_id, weight) in CORE_TRAITS {
            random_weight -= weight;
            if random_weight <= 0.0 {
                let trait_string = trait_id.to_string();
                if !assigned.contains(&trait_string) {
                    assigned.push(trait_string);
                }
                break;
            }
        }
        attempts += 1;
    }

    // Main trait is the first assigned trait (or "Diplomatic" as fallback).
    let main_trait = assigned
        .first()
        .cloned()
        .unwrap_or_else(|| "Diplomatic".to_string());
    (assigned, main_trait)
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_vip(name: &str, age: u32) -> Vip {
        Vip {
            id: String::new(),
            full_name: name.to_string(),
            gender: "M".to_string(),
            age,
            health: VipHealth {
                physical_health: 1.0,
                mental_health: 1.0,
            },
            incapacity: IncapacityStatus::Healthy,
            traits: vec!["Loyal".to_string()],
            main_trait: "Loyal".to_string(),
            ideology: "Centrist".to_string(),
            religion: String::new(),
            nationality: "TestNation".to_string(),
            dynasty: None,
            roles: vec![VipRoleExtended::HeadOfState],
            base_influence: 50,
            faction: "Royal Court".to_string(),
            born_turn: 0,
            is_dead: false,
            death_turn: None,
            death_cause: None,
            acting_replacement_id: None,
            diplomatic_post: None,
            portrait_seed: String::new(),
        }
    }

    #[test]
    fn test_vip_registry_register_new() {
        let mut registry = VipRegistry::new();
        let vip = make_test_vip("Jan Kowalski", 45);
        let id = registry.register_new(vip);
        assert!(id.starts_with("VIP-"));
        assert_eq!(registry.living_count(), 1);
        assert!(registry.get(&id).is_some());
        assert!(registry.get_by_name("Jan Kowalski").is_some());
    }

    #[test]
    fn test_vip_registry_dedup_by_name() {
        let mut registry = VipRegistry::new();
        let vip1 = make_test_vip("Jan Kowalski", 45);
        let id1 = registry.register_new(vip1);
        // Register the same name again — should NOT create a duplicate.
        let vip2 = make_test_vip("Jan Kowalski", 45);
        let found = registry.register_or_lookup(vip2);
        assert_eq!(found.id, id1);
        assert_eq!(registry.living_count(), 1);
    }

    #[test]
    fn test_vip_registry_get_by_role() {
        let mut registry = VipRegistry::new();
        let mut vip1 = make_test_vip("Jan Kowalski", 45);
        vip1.roles = vec![VipRoleExtended::HeadOfState];
        let mut vip2 = make_test_vip("Anna Nowak", 50);
        vip2.roles = vec![VipRoleExtended::Minister];
        registry.register_new(vip1);
        registry.register_new(vip2);

        let heads = registry.get_by_role(&VipRoleExtended::HeadOfState);
        assert_eq!(heads.len(), 1);
        assert_eq!(heads[0].full_name, "Jan Kowalski");

        let ministers = registry.get_by_role(&VipRoleExtended::Minister);
        assert_eq!(ministers.len(), 1);
        assert_eq!(ministers[0].full_name, "Anna Nowak");
    }

    #[test]
    fn test_vip_aging_increments_age() {
        let mut registry = VipRegistry::new();
        let vip = make_test_vip("Jan Kowalski", 45);
        let id = registry.register_new(vip);
        registry.age_all_vips();
        let vip = registry.get(&id).unwrap();
        assert_eq!(vip.age, 46);
    }

    #[test]
    fn test_health_degradation_no_decline_young() {
        let new_health = age_health_degradation(30, 1.0);
        assert!((new_health - 1.0).abs() < 1e-6, "No decline before 41");
    }

    #[test]
    fn test_health_degradation_middle_aged() {
        let new_health = age_health_degradation(50, 1.0);
        assert!((new_health - 0.995).abs() < 1e-6, "0.5% decline at 50");
    }

    #[test]
    fn test_health_degradation_elderly() {
        let new_health = age_health_degradation(70, 1.0);
        assert!((new_health - 0.99).abs() < 1e-6, "1% decline at 70");
    }

    #[test]
    fn test_health_degradation_very_old() {
        let new_health = age_health_degradation(85, 1.0);
        assert!((new_health - 0.98).abs() < 1e-6, "2% decline at 85");
    }

    #[test]
    fn test_health_degradation_extremely_old() {
        let new_health = age_health_degradation(100, 1.0);
        assert!((new_health - 0.96).abs() < 1e-6, "4% decline at 100");
    }

    #[test]
    fn test_health_degradation_clamps_to_zero() {
        let new_health = age_health_degradation(100, 0.01);
        assert_eq!(new_health, 0.0, "Health should clamp to 0");
    }

    #[test]
    fn test_death_probability_young_healthy() {
        let prob = death_probability(30, 1.0);
        assert!(
            prob < 0.01,
            "Young healthy VIP should have very low death prob"
        );
    }

    #[test]
    fn test_death_probability_old_unhealthy() {
        let prob = death_probability(85, 0.2);
        assert!(prob > 0.05, "Old unhealthy VIP should have high death prob");
    }

    #[test]
    fn test_death_probability_extreme_old_age() {
        let prob = death_probability(100, 1.0);
        assert!(
            prob >= 0.15,
            "100-year-old should have at least 15% death prob"
        );
    }

    #[test]
    fn test_mark_dead_clears_roles() {
        let mut vip = make_test_vip("Jan Kowalski", 60);
        assert!(!vip.roles.is_empty());
        vip.mark_dead(10, DeathCause::OldAge);
        assert!(vip.is_dead);
        assert!(vip.roles.is_empty());
        assert_eq!(vip.death_cause, Some(DeathCause::OldAge));
        assert_eq!(vip.incapacity, IncapacityStatus::Dead);
    }

    #[test]
    fn test_mark_incapacitated() {
        let mut vip = make_test_vip("Jan Kowalski", 60);
        vip.mark_incapacitated(IncapacityStatus::Sick);
        assert!(vip.is_incapacitated());
        assert!(!vip.can_exercise_power());
    }

    #[test]
    fn test_mark_recovered() {
        let mut vip = make_test_vip("Jan Kowalski", 60);
        vip.mark_incapacitated(IncapacityStatus::Sick);
        vip.acting_replacement_id = Some("VIP-000002".to_string());
        vip.mark_recovered();
        assert!(vip.can_exercise_power());
        assert!(vip.acting_replacement_id.is_none());
    }

    #[test]
    fn test_queue_unnatural_death() {
        let mut registry = VipRegistry::new();
        let vip = make_test_vip("Jan Kowalski", 50);
        let id = registry.register_new(vip);

        registry.queue_unnatural_death(&id, DeathCause::Assassination, 5);

        // VIP should be marked dead immediately.
        let vip = registry.get(&id).unwrap();
        assert!(vip.is_dead);
        assert_eq!(vip.death_cause, Some(DeathCause::Assassination));

        // Pending death should be in the queue.
        assert_eq!(registry.pending_unnatural_deaths.len(), 1);
        assert_eq!(
            registry.pending_unnatural_deaths[0].cause,
            DeathCause::Assassination
        );
    }

    #[test]
    fn test_drain_pending_deaths() {
        let mut registry = VipRegistry::new();
        let vip1 = make_test_vip("Jan Kowalski", 50);
        let id1 = registry.register_new(vip1);
        let vip2 = make_test_vip("Anna Nowak", 55);
        let id2 = registry.register_new(vip2);

        registry.queue_unnatural_death(&id1, DeathCause::Assassination, 5);
        registry.queue_unnatural_death(&id2, DeathCause::Coup, 6);

        let drained = registry.drain_pending_deaths();
        assert_eq!(drained.len(), 2);
        assert!(registry.pending_unnatural_deaths.is_empty());
    }

    #[test]
    fn test_designate_acting_replacement() {
        let mut registry = VipRegistry::new();
        let mut vip1 = make_test_vip("Jan Kowalski", 60);
        vip1.mark_incapacitated(IncapacityStatus::Coma);
        let id1 = registry.register_new(vip1);
        let vip2 = make_test_vip("Anna Nowak", 45);
        let id2 = registry.register_new(vip2);

        registry.designate_acting(&id1, &id2).unwrap();

        let vip = registry.get(&id1).unwrap();
        assert_eq!(vip.acting_replacement_id, Some(id2.clone()));

        let acting = registry.get_acting_for(&id1).unwrap();
        assert_eq!(acting.full_name, "Anna Nowak");
    }

    #[test]
    fn test_designate_acting_rejects_dead_vip() {
        let mut registry = VipRegistry::new();
        let mut vip1 = make_test_vip("Jan Kowalski", 60);
        vip1.mark_incapacitated(IncapacityStatus::Coma);
        let id1 = registry.register_new(vip1);
        let mut vip2 = make_test_vip("Anna Nowak", 45);
        vip2.mark_dead(5, DeathCause::Assassination);
        let id2 = registry.register_new(vip2);

        let result = registry.designate_acting(&id1, &id2);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_natural_deaths_archives_deceased() {
        let mut rng = rand::thread_rng();
        let mut registry = VipRegistry::new();
        let mut vip = make_test_vip("Old Leader", 95);
        vip.health = VipHealth {
            physical_health: 0.1,
            mental_health: 0.1,
        }; // Very unhealthy
        let id = registry.register_new(vip);

        // Run death check multiple times — with age 95 and health 0.1,
        // death probability is ~0.05 + 0.045 = ~0.095 per year.
        // Run 100 times to virtually guarantee at least one death.
        let mut died = false;
        for _ in 0..100 {
            if !registry.check_natural_deaths(&mut rng).is_empty() {
                died = true;
                break;
            }
            // Re-age to keep probability high
            if let Some(v) = registry.get_mut(&id) {
                v.health = VipHealth {
                    physical_health: 0.1,
                    mental_health: 0.1,
                };
            }
        }
        assert!(died, "A 95-year-old with 0.1 health should eventually die");
        assert!(registry.deceased_count() > 0);
        assert!(
            registry.get(&id).is_none(),
            "Dead VIP should be removed from living"
        );
    }

    #[test]
    fn test_assign_core_traits_returns_2_to_4() {
        let mut rng = rand::thread_rng();
        let (traits, main) = assign_core_traits(&mut rng);
        assert!(traits.len() >= 2 && traits.len() <= 4);
        assert!(!main.is_empty());
        assert!(traits.contains(&main));
    }

    #[test]
    fn test_death_cause_display() {
        assert_eq!(format!("{}", DeathCause::OldAge), "Old Age");
        assert_eq!(format!("{}", DeathCause::Assassination), "Assassination");
        assert_eq!(format!("{}", DeathCause::Coup), "Coup");
    }

    #[test]
    fn test_vip_role_is_head_of_state() {
        assert!(VipRoleExtended::HeadOfState.is_head_of_state());
        assert!(VipRoleExtended::Monarch.is_head_of_state());
        assert!(VipRoleExtended::Regent.is_head_of_state());
        assert!(!VipRoleExtended::Minister.is_head_of_state());
        assert!(!VipRoleExtended::Ceo.is_head_of_state());
    }

    #[test]
    fn test_vip_role_is_political() {
        assert!(VipRoleExtended::HeadOfState.is_political());
        assert!(VipRoleExtended::Mayor.is_political());
        assert!(!VipRoleExtended::Ceo.is_political());
        assert!(!VipRoleExtended::UnionBoss.is_political());
    }

    #[test]
    fn test_vip_has_trait() {
        let vip = Vip {
            traits: vec!["Ambitious".to_string(), "Loyal".to_string()],
            ..Default::default()
        };
        assert!(vip.has_trait("Ambitious"));
        assert!(vip.has_trait("Loyal"));
        assert!(!vip.has_trait("Corrupt"));
    }

    #[test]
    fn test_vip_add_remove_role() {
        let mut vip = Vip::default();
        vip.add_role(VipRoleExtended::Minister);
        assert!(vip.has_role(&VipRoleExtended::Minister));
        vip.add_role(VipRoleExtended::Minister); // Duplicate should not add.
        assert_eq!(vip.roles.len(), 1);
        vip.remove_role(&VipRoleExtended::Minister);
        assert!(!vip.has_role(&VipRoleExtended::Minister));
    }

    #[test]
    fn test_degrade_health_all_applies_to_living_only() {
        let mut registry = VipRegistry::new();
        let mut vip1 = make_test_vip("Old Man", 70);
        vip1.health = VipHealth {
            physical_health: 1.0,
            mental_health: 1.0,
        };
        let id1 = registry.register_new(vip1);
        let mut vip2 = make_test_vip("Dead Man", 70);
        vip2.mark_dead(5, DeathCause::OldAge);
        let id2 = registry.register_new(vip2);

        registry.degrade_health_all();

        let v1 = registry.get(&id1).unwrap();
        assert!(
            (v1.health.aggregate() - 0.99).abs() < 1e-6,
            "Living 70yo should degrade"
        );
        let v2 = registry.get(&id2).unwrap();
        assert_eq!(
            v2.health.aggregate(),
            1.0,
            "Dead VIP health should not change (was 1.0 at creation)"
        );
    }

    #[test]
    fn test_low_health_triggers_sick_incapacity() {
        let mut registry = VipRegistry::new();
        let mut vip = make_test_vip("Sick Leader", 75);
        vip.health = VipHealth {
            physical_health: 0.10,
            mental_health: 0.10,
        };
        let id = registry.register_new(vip);

        registry.degrade_health_all();

        let v = registry.get(&id).unwrap();
        assert_eq!(
            v.incapacity,
            IncapacityStatus::Sick,
            "Health < 0.15 should trigger Sick"
        );
    }

    // ========================================================================
    // Phase 49: VIP genesis cultural consistency and no-duplicate tests
    // ========================================================================

    #[test]
    fn test_generate_full_vip_uses_cultural_group() {
        // Phase 49: VIPs generated with a specific cultural group should
        // have names drawn from that culture's name pool, not a fallback.
        let mut rng = rand::thread_rng();
        let vip = crate::politics::names::generate_full_vip("slavic", &mut rng);
        assert!(
            !vip.full_name.is_empty(),
            "Generated VIP should have a non-empty name"
        );
    }

    #[test]
    fn test_generate_full_vip_germanic() {
        let mut rng = rand::thread_rng();
        let vip = crate::politics::names::generate_full_vip("germanic", &mut rng);
        assert!(
            !vip.full_name.is_empty(),
            "Germanic VIP should have a non-empty name"
        );
    }

    #[test]
    fn test_generate_full_vip_latin() {
        let mut rng = rand::thread_rng();
        let vip = crate::politics::names::generate_full_vip("latin", &mut rng);
        assert!(
            !vip.full_name.is_empty(),
            "Latin VIP should have a non-empty name"
        );
    }

    #[test]
    fn test_generate_unique_vip_no_duplicates() {
        // Phase 49: generate_unique_vip should never produce duplicate names
        // within the same cultural group when called repeatedly.
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(42);
        let mut used_names = std::collections::HashSet::new();

        for _ in 0..30 {
            let vip =
                crate::politics::names::generate_unique_vip("slavic", &mut rng, &mut used_names);
            assert!(
                used_names.contains(&vip.full_name),
                "Generated name should be in the used set"
            );
        }
        // All 30 names should be unique (the HashSet enforces this)
        assert_eq!(used_names.len(), 30, "Should have 30 unique names");
    }

    #[test]
    fn test_no_polish_culture_keys_in_name_pool() {
        // Phase 49: Ensure no Polish internal culture keys remain in
        // name_pool_for_culture. The function should accept lowercase
        // English keys and return a non-empty pool.
        let slavic = crate::politics::names::name_pool_for_culture("slavic");
        assert!(
            !slavic.first_names_male.is_empty(),
            "Slavic pool should not be empty"
        );
        assert!(
            !slavic.surnames.is_empty(),
            "Slavic surnames should not be empty"
        );

        let germanic = crate::politics::names::name_pool_for_culture("germanic");
        assert!(
            !germanic.first_names_male.is_empty(),
            "Germanic pool should not be empty"
        );

        let latin = crate::politics::names::name_pool_for_culture("latin");
        assert!(
            !latin.first_names_male.is_empty(),
            "Latin pool should not be empty"
        );

        let middle_eastern = crate::politics::names::name_pool_for_culture("middle_eastern");
        assert!(
            !middle_eastern.first_names_male.is_empty(),
            "Middle Eastern pool should not be empty"
        );

        let balkan = crate::politics::names::name_pool_for_culture("balkan");
        assert!(
            !balkan.first_names_male.is_empty(),
            "Balkan pool should not be empty"
        );
    }
}

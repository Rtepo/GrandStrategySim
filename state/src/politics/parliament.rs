//! Phase 32: Parliament structure, chambers, parliamentary clubs, named VIPs,
//! and mid-term faction splintering.
//!
//! The Parliament is composed of 0, 1, or 2 chambers based on the country's
//! `GovernmentForm`. Regular MPs are tracked as anonymized seat pools
//! (`ParliamentaryClub`) for performance. Key VIPs (Head of State, PM, Ministers,
//! Speakers) are named individuals generated via `names.rs`.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use super::ideology::Ideology;
use super::names::{generate_full_vip, generate_unique_vip, VipName};
use super::system::{GovernmentForm, Party, Politics};

// ============================================================================
// VIP STRUCTURES
// ============================================================================

/// Types of VIP political offices.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub enum VipRole {
    #[default]
    HeadOfState,
    PrimeMinister,
    Minister,
    Speaker,
    DeputySpeaker,
    Whip,
}

/// A named VIP holding a political office.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NamedVip {
    /// Full name (first + surname).
    pub full_name: String,
    /// Party ID.
    pub party: String,
    /// Role/office.
    pub role: VipRole,
    /// Ideology string.
    pub ideology: String,
    /// Age.
    pub age: u32,
}

// ============================================================================
// CHAMBER STRUCTURES
// ============================================================================

/// The Speaker and Deputy Speakers controlling the legislative agenda.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ChamberPresidium {
    /// Speaker (Marszałek) — named VIP.
    pub speaker: NamedVip,
    /// Deputy speakers (Wicemarszałkowie).
    pub deputy_speakers: Vec<NamedVip>,
    /// Party/club of the speaker.
    pub speaker_club: String,
    /// Agenda control factor (0.0–1.0): how much the speaker controls what reaches the floor.
    pub agenda_control: f64,
}

/// A recorded floor vote on a bill.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct VoteRecord {
    pub bill_id: String,
    pub bill_title: String,
    pub votes_for: u32,
    pub votes_against: u32,
    pub abstentions: u32,
    pub passed: bool,
    pub turn: u32,
}

/// A single legislative chamber (Lower House or Senate).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Chamber {
    /// Chamber identifier: "lower" or "upper".
    pub id: String,
    /// Display name: "Sejm", "Senate", etc.
    pub name: String,
    /// Total seat count.
    pub total_seats: u32,
    /// Seat distribution by parliamentary club (club_id → seats).
    pub seats: HashMap<String, u32>,
    /// Presidium: Speaker and Deputy Speakers.
    pub presidium: ChamberPresidium,
    /// Active bills in this chamber's queue (bill IDs).
    pub legislative_queue: Vec<String>,
    /// Recently passed/rejected bills with vote tallies (last 20).
    pub recent_votes: Vec<VoteRecord>,
}

// ============================================================================
// PARLIAMENTARY CLUB
// ============================================================================

/// A parliamentary club/faction (anonymized MP seat pool).
///
/// Regular MPs are tracked as seat counts, not individual entities.
/// Clubs can form via mid-term splintering without creating new active_parties.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ParliamentaryClub {
    /// Club identifier (may differ from party name for splinter groups).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Parent party (if affiliated); None for independent clubs.
    pub parent_party: Option<String>,
    /// Seat count in the lower house.
    pub seats: u32,
    /// Ideology string (inherited from parent or declared at splinter).
    pub ideology: String,
    /// Club discipline (0.0–1.0).
    pub discipline: f64,
    /// Whether this club was formed by mid-term splintering.
    pub is_splinter: bool,
    /// Turn when the club was formed.
    pub formation_turn: u32,
    /// Phase 54: Chairperson VIP ID (if assigned).
    #[serde(default)]
    pub chairperson_id: Option<String>,
    /// Phase 54: Chairperson display name.
    #[serde(default)]
    pub chairperson_name: String,
}

// ============================================================================
// STATE OF EMERGENCY
// ============================================================================

/// Constitutional State of Emergency / Martial Law (political, not fiscal).
///
/// Distinct from the fiscal `EmergencyPowers` enum in `state/mod.rs`.
/// When active with `parliament_suspended = true`, the executive can bypass
/// Parliament entirely for ALL decisions.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StateOfEmergency {
    /// Whether currently active.
    pub active: bool,
    /// Turn when activated.
    pub activation_turn: u32,
    /// Reason for activation (crisis severity, war, rebellion).
    pub reason: String,
    /// Maximum duration in turns (constitutional limit, e.g., 24 turns = 1 year).
    pub max_duration: u32,
    /// Turns remaining.
    pub turns_remaining: u32,
    /// Whether Parliament is suspended (full martial law) or just fast-tracked.
    pub parliament_suspended: bool,
    /// Who authorized it (Head of State or PM).
    pub authorized_by: String,
    /// Phase 33: Cooldown turns remaining before a new SoE can be activated.
    /// Set to 12 (half a year) after expiry. Reactivation allowed during cooldown
    /// only if crisis severity > 0.9 (catastrophic).
    #[serde(default)]
    pub cooldown_turns: u32,
}

impl StateOfEmergency {
    /// Check if the executive can bypass Parliament entirely.
    pub fn can_bypass_parliament(&self) -> bool {
        self.active && self.parliament_suspended
    }

    /// Tick down the timer; auto-expire when duration elapses.
    /// Phase 33: On expiry, set a 12-turn cooldown before reactivation.
    pub fn tick(&mut self) {
        if self.active && self.turns_remaining > 0 {
            self.turns_remaining -= 1;
            if self.turns_remaining == 0 {
                self.active = false;
                self.parliament_suspended = false;
                // Phase 33: Impose cooldown to prevent immediate reactivation.
                self.cooldown_turns = 12;
            }
        }
        // Tick down cooldown when not active.
        if !self.active && self.cooldown_turns > 0 {
            self.cooldown_turns -= 1;
        }
    }

    /// Phase 33: Check if a new SoE can be activated.
    /// Returns false if cooldown is active (unless severity is catastrophic).
    pub fn can_reactivate(&self, catastrophic: bool) -> bool {
        if self.active {
            return false; // Already active.
        }
        if self.cooldown_turns > 0 && !catastrophic {
            return false; // In cooldown.
        }
        true
    }

    /// Activate a State of Emergency.
    pub fn activate(
        &mut self,
        turn: u32,
        reason: String,
        max_duration: u32,
        parliament_suspended: bool,
        authorized_by: String,
    ) {
        self.active = true;
        self.activation_turn = turn;
        self.reason = reason;
        self.max_duration = max_duration;
        self.turns_remaining = max_duration;
        self.parliament_suspended = parliament_suspended;
        self.authorized_by = authorized_by;
    }

    /// Deactivate the State of Emergency manually.
    pub fn deactivate(&mut self) {
        self.active = false;
        self.parliament_suspended = false;
        self.turns_remaining = 0;
    }
}

// ============================================================================
// FULL PARLIAMENT
// ============================================================================

/// The full Parliament for a country (0, 1, or 2 chambers).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Parliament {
    /// Chambers present (0, 1, or 2 based on GovernmentForm).
    pub chambers: Vec<Chamber>,
    /// All parliamentary clubs in the lower house.
    pub clubs: Vec<ParliamentaryClub>,
    /// Named VIPs: Head of State, PM, Ministers, Speakers.
    pub vips: Vec<NamedVip>,
    /// Whether parliament is currently suspended (State of Emergency).
    pub suspended: bool,
}

impl Parliament {
    /// Get the lower chamber (first chamber), if any.
    pub fn lower_chamber(&self) -> Option<&Chamber> {
        self.chambers.first()
    }

    /// Get the upper chamber (second chamber), if any.
    pub fn upper_chamber(&self) -> Option<&Chamber> {
        self.chambers.get(1)
    }

    /// Get mutable lower chamber.
    pub fn lower_chamber_mut(&mut self) -> Option<&mut Chamber> {
        self.chambers.first_mut()
    }

    /// Get the total seats in the lower chamber.
    pub fn lower_seats(&self) -> u32 {
        self.lower_chamber().map(|c| c.total_seats).unwrap_or(0)
    }

    /// Get the seat distribution of the lower chamber as a party→seats map.
    /// Maps club IDs to seats (compatible with legacy `politics.parliament`).
    pub fn lower_seat_map(&self) -> HashMap<String, u32> {
        self.lower_chamber()
            .map(|c| c.seats.clone())
            .unwrap_or_default()
    }

    /// Record a vote in the lower chamber's recent votes (keeps last 20).
    pub fn record_vote(&mut self, record: VoteRecord) {
        if let Some(chamber) = self.lower_chamber_mut() {
            chamber.recent_votes.insert(0, record);
            chamber.recent_votes.truncate(20);
        }
    }

    /// Add a bill to the lower chamber's legislative queue.
    pub fn queue_bill(&mut self, bill_id: String) {
        if let Some(chamber) = self.lower_chamber_mut() {
            if !chamber.legislative_queue.contains(&bill_id) {
                chamber.legislative_queue.push(bill_id);
            }
        }
    }

    /// Remove a bill from the lower chamber's legislative queue.
    pub fn dequeue_bill(&mut self, bill_id: &str) {
        if let Some(chamber) = self.lower_chamber_mut() {
            chamber.legislative_queue.retain(|id| id != bill_id);
        }
    }
}

// ============================================================================
// PARLIAMENT INITIALIZATION
// ============================================================================

/// Phase 53: Returns culturally-appropriate chamber names for the lower and
/// upper houses of parliament based on the country's cultural group.
///
/// This preserves cultural flavor in the UI instead of using generic
/// "Lower House"/"Senate" for every country.
pub fn get_cultural_chamber_names(culture: &str) -> (String, String) {
    match culture {
        "anglo" => ("House of Commons".to_string(), "House of Lords".to_string()),
        "germanic" => ("Bundestag".to_string(), "Bundesrat".to_string()),
        "slavic" => ("Sejm".to_string(), "Senate".to_string()),
        "latin" => ("Chamber of Deputies".to_string(), "Senate".to_string()),
        "middle_eastern" => ("Majlis".to_string(), "Shura Council".to_string()),
        "balkan" => ("National Assembly".to_string(), "Senate".to_string()),
        _ => ("National Assembly".to_string(), "Senate".to_string()),
    }
}

/// Initialize a Parliament from the current political state.
///
/// # Arguments
/// * `politics` - Current politics state (for government form, parties, seats)
/// * `cultural_group` - Cultural group for VIP name generation
/// * `current_turn` - Current game turn
/// * `rng` - Random number generator
///
/// # Returns
/// A `Parliament` with chambers, clubs, and VIPs populated.
pub fn initialize_parliament(
    politics: &Politics,
    cultural_group: &str,
    current_turn: u32,
    rng: &mut impl rand::Rng,
    used_names: &mut HashSet<String>,
) -> Parliament {
    let form = politics.government_form;
    let num_chambers = form.chambers();
    let legacy_seats = &politics.parliament;
    let upper_house = &politics.upper_house;

    // Build clubs from the legacy seat map.
    let clubs = build_clubs_from_seats(legacy_seats, &politics.active_parties, current_turn);

    // Build chambers.
    let mut chambers = Vec::new();

    // Phase 45: Use the global used_names set passed from process_political_year.
    // No local speaker_names HashSet — the global set is shared across all VIP generation.

    // Phase 53: Use culturally-appropriate chamber names.
    let (lower_name, upper_name) = get_cultural_chamber_names(cultural_group);

    // Lower chamber (if any chambers exist).
    if num_chambers >= 1 {
        let total_seats: u32 = legacy_seats.values().sum();
        let speaker = generate_speaker(politics, cultural_group, rng, used_names);
        let speaker_club = politics.ruling_party.clone();
        let agenda_control = calculate_agenda_control(politics);

        let lower = Chamber {
            id: "lower".to_string(),
            name: lower_name,
            total_seats,
            seats: legacy_seats.clone(),
            presidium: ChamberPresidium {
                speaker: speaker.clone(),
                deputy_speakers: generate_deputy_speakers(politics, cultural_group, rng, used_names),
                speaker_club,
                agenda_control,
            },
            legislative_queue: Vec::new(),
            recent_votes: Vec::new(),
        };
        chambers.push(lower);
    }

    // Upper chamber (if bicameral).
    if num_chambers >= 2 {
        let total_seats: u32 = upper_house.values().sum();
        let upper_speaker = generate_speaker(politics, cultural_group, rng, used_names);

        let upper = Chamber {
            id: "upper".to_string(),
            name: upper_name,
            total_seats,
            seats: upper_house.clone(),
            presidium: ChamberPresidium {
                speaker: upper_speaker,
                deputy_speakers: Vec::new(),
                speaker_club: politics.ruling_party.clone(),
                agenda_control: 0.5,
            },
            legislative_queue: Vec::new(),
            recent_votes: Vec::new(),
        };
        chambers.push(upper);
    }

    // Build VIPs list.
    let vips = build_vips(politics, cultural_group, rng, used_names);

    Parliament {
        chambers,
        clubs,
        vips,
        suspended: false,
    }
}

/// Phase 54: Generate and assign a Chairperson VIP for each parliamentary club.
/// Each chairperson is registered in the VIP registry with the `Speaker` role
/// and their name/ID is stored on the club.
///
/// # Arguments
/// * `parliament` - Mutable parliament whose clubs will receive chairpersons.
/// * `registry` - Mutable VIP registry to register chairpersons in.
/// * `cultural_group` - Cultural group for name generation.
/// * `country_name` - Country name for nationality field.
/// * `rng` - Random number generator.
pub fn assign_club_chairpersons(
    parliament: &mut Parliament,
    registry: &mut super::vip_registry::VipRegistry,
    cultural_group: &str,
    country_name: &str,
    rng: &mut impl rand::Rng,
) {
    use super::vip_registry::{Vip, VipRoleExtended, assign_core_traits};

    for club in &mut parliament.clubs {
        // Skip if already has a chairperson.
        if club.chairperson_id.is_some() {
            continue;
        }

        let vip_name = generate_full_vip(cultural_group, rng);
        let (traits, main_trait) = assign_core_traits(rng);
        let ideology = if club.ideology.is_empty() {
            "Social Liberalism".to_string()
        } else {
            club.ideology.clone()
        };

        let chairperson = Vip {
            full_name: vip_name.full_name.clone(),
            gender: vip_name.gender,
            age: 40 + rng.gen_range(0..25),
            health: crate::politics::vip_registry::VipHealth { physical_health: 1.0, mental_health: 1.0 },
            traits,
            main_trait,
            ideology,
            nationality: country_name.to_string(),
            roles: vec![VipRoleExtended::Speaker],
            base_influence: 15 + rng.gen_range(0..25),
            ..Default::default()
        };

        let chairperson_id = registry.register_new(chairperson);
        club.chairperson_id = Some(chairperson_id);
        club.chairperson_name = vip_name.full_name;
    }
}

/// Build parliamentary clubs from the legacy seat map.
fn build_clubs_from_seats(
    seats: &HashMap<String, u32>,
    active_parties: &HashMap<String, Party>,
    current_turn: u32,
) -> Vec<ParliamentaryClub> {
    seats
        .iter()
        .map(|(party_id, &seat_count)| {
            let party = active_parties.get(party_id);
            let ideology = party.map(|p| p.ideology.clone()).unwrap_or_default();
            let discipline = party
                .map(|p| p.organization.discipline)
                .unwrap_or(0.5);

            ParliamentaryClub {
                id: party_id.clone(),
                name: party_id.clone(),
                parent_party: Some(party_id.clone()),
                seats: seat_count,
                ideology,
                discipline,
                is_splinter: false,
                formation_turn: current_turn,
                chairperson_id: None,
                chairperson_name: String::new(),
            }
        })
        .collect()
}

/// Calculate the Speaker's agenda control factor (0.0–1.0).
fn calculate_agenda_control(politics: &Politics) -> f64 {
    let total_seats: u32 = politics.parliament.values().sum();
    if total_seats == 0 {
        return 0.0;
    }

    let coalition_seats: u32 = politics
        .coalition
        .iter()
        .filter_map(|p| politics.parliament.get(p))
        .sum::<u32>()
        + politics
            .parliament
            .get(&politics.ruling_party)
            .copied()
            .unwrap_or(0);

    let coalition_share = coalition_seats as f64 / total_seats as f64;

    // Majority → high agenda control; minority → low.
    if politics.minority_government {
        0.3 + coalition_share * 0.2
    } else {
        0.5 + coalition_share * 0.4
    }
}

/// Generate a Speaker VIP from the ruling party.
/// Phase 43: Accepts a `used_names` set to avoid cloning the same party leader
/// as Speaker for both chambers. If the leader's name is already used, a fresh
/// unique VIP name is generated instead.
fn generate_speaker(
    politics: &Politics,
    cultural_group: &str,
    rng: &mut impl rand::Rng,
    used_names: &mut HashSet<String>,
) -> NamedVip {
    let ruling = &politics.ruling_party;
    let party = politics.active_parties.get(ruling);
    let ideology = party.map(|p| p.ideology.clone()).unwrap_or_default();

    // Try to use the party leader's name; otherwise generate one.
    // Phase 43: If the leader's name is already in used_names (e.g., used by
    // the other chamber's Speaker), generate a fresh unique VIP instead.
    let full_name = if let Some(p) = party {
        if !p.leader.name.is_empty() && !used_names.contains(&p.leader.name) {
            p.leader.name.clone()
        } else {
            // Generate a unique VIP name, retrying until we get one not in used_names.
            let mut name;
            loop {
                name = generate_full_vip(cultural_group, rng).full_name;
                if !used_names.contains(&name) {
                    break;
                }
            }
            name
        }
    } else {
        let mut name;
        loop {
            name = generate_full_vip(cultural_group, rng).full_name;
            if !used_names.contains(&name) {
                break;
            }
        }
        name
    };

    used_names.insert(full_name.clone());
    let age = 45 + rng.gen_range(0..25);

    NamedVip {
        full_name,
        party: ruling.clone(),
        role: VipRole::Speaker,
        ideology,
        age,
    }
}

/// Generate Deputy Speakers from coalition partners.
/// Phase 45: Accepts the global used_names set to prevent VIP cloning.
/// Deputy speakers now get unique generated names instead of cloning party leaders.
fn generate_deputy_speakers(
    politics: &Politics,
    cultural_group: &str,
    rng: &mut impl rand::Rng,
    used_names: &mut HashSet<String>,
) -> Vec<NamedVip> {
    politics
        .coalition
        .iter()
        .take(2) // Max 2 deputy speakers
        .map(|party_id| {
            let party = politics.active_parties.get(party_id);
            let ideology = party.map(|p| p.ideology.clone()).unwrap_or_default();
            // Phase 45: Generate a unique VIP name instead of cloning party leader.
            // If the party leader's name is not yet in used_names, use it once;
            // otherwise generate a fresh unique name.
            let full_name = if let Some(p) = party {
                if !p.leader.name.is_empty() && !used_names.contains(&p.leader.name) {
                    used_names.insert(p.leader.name.clone());
                    p.leader.name.clone()
                } else {
                    generate_unique_vip(cultural_group, rng, used_names).full_name
                }
            } else {
                generate_unique_vip(cultural_group, rng, used_names).full_name
            };
            NamedVip {
                full_name,
                party: party_id.clone(),
                role: VipRole::DeputySpeaker,
                ideology,
                age: 40 + rng.gen_range(0..25),
            }
        })
        .collect()
}

/// Build the full VIP list (Head of State, PM, Ministers, Speakers).
fn build_vips(
    politics: &Politics,
    cultural_group: &str,
    rng: &mut impl rand::Rng,
    used_names: &mut HashSet<String>,
) -> Vec<NamedVip> {
    let mut vips = Vec::new();
    // Phase 45: Use the global used_names set passed from initialize_parliament.
    // No local HashSet creation — the global set is pre-populated with all
    // party leader names and shared across government + parliament generation.

    // Pre-populate with minister names from ministry_config.
    if let Some(ref min_config) = politics.ministry_config {
        for ministry in &min_config.ministries {
            if !ministry.minister_name.is_empty() {
                used_names.insert(ministry.minister_name.clone());
            }
        }
    }
    // Pre-populate with all party leader names.
    for party in politics.active_parties.values() {
        if !party.leader.name.is_empty() {
            used_names.insert(party.leader.name.clone());
        }
    }
    // Pre-populate with Head of State name if already set.
    if !politics.head_of_state.name.is_empty() {
        used_names.insert(politics.head_of_state.name.clone());
    }

    // Head of State.
    let hos_name = if !politics.head_of_state.name.is_empty() {
        politics.head_of_state.name.clone()
    } else {
        generate_unique_vip(cultural_group, rng, &mut *used_names).full_name
    };
    used_names.insert(hos_name.clone());
    vips.push(NamedVip {
        full_name: hos_name,
        party: politics.ruling_party.clone(),
        role: VipRole::HeadOfState,
        ideology: politics
            .active_parties
            .get(&politics.ruling_party)
            .map(|p| p.ideology.clone())
            .unwrap_or_default(),
        age: politics.head_of_state.age,
    });

    // Prime Minister.
    let pm_party = politics.ruling_party.clone();
    let pm_ideology = politics
        .active_parties
        .get(&pm_party)
        .map(|p| p.ideology.clone())
        .unwrap_or_default();
    let pm_name = politics
        .active_parties
        .get(&pm_party)
        .map(|p| {
            if !p.leader.name.is_empty() {
                p.leader.name.clone()
            } else {
                generate_unique_vip(cultural_group, rng, &mut *used_names).full_name
            }
        })
        .unwrap_or_else(|| generate_unique_vip(cultural_group, rng, &mut *used_names).full_name);
    used_names.insert(pm_name.clone());
    vips.push(NamedVip {
        full_name: pm_name,
        party: pm_party,
        role: VipRole::PrimeMinister,
        ideology: pm_ideology,
        age: 45 + rng.gen_range(0..20),
    });

    // Ministers from ministry_config.
    if let Some(ref min_config) = politics.ministry_config {
        for ministry in &min_config.ministries {
            let min_name = if !ministry.minister_name.is_empty() {
                ministry.minister_name.clone()
            } else {
                generate_unique_vip(cultural_group, rng, &mut *used_names).full_name
            };
            used_names.insert(min_name.clone());
            let min_ideology = politics
                .active_parties
                .get(&ministry.minister_party)
                .map(|p| p.ideology.clone())
                .unwrap_or_default();
            vips.push(NamedVip {
                full_name: min_name,
                party: ministry.minister_party.clone(),
                role: VipRole::Minister,
                ideology: min_ideology,
                age: 40 + rng.gen_range(0..25),
            });
        }
    }

    vips
}

// ============================================================================
// MID-TERM FACTION SPLINTERING
// ============================================================================

/// A recorded splinter event.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SplinterEvent {
    pub source_club: String,
    pub new_club: String,
    pub seats_defected: u32,
    pub reason: String,
    pub turn: u32,
}

/// Check for mid-term faction splintering.
///
/// MPs defect from their club to a new or existing club based on:
/// - Ideological distance from their party's current position
/// - Unpopular government policies (low approval, high unrest)
/// - Party organization factional_tension
///
/// # Arguments
/// * `parliament` - Mutable parliament (clubs and chamber seats updated in place)
/// * `active_parties` - Active parties (for organization metrics)
/// * `approval_rating` - Government approval rating (0–100)
/// * `unrest` - Social unrest level (0–100)
/// * `current_turn` - Current game turn
///
/// # Returns
/// A list of splinter events for telemetry.
///
/// # Rules
/// * Splinter clubs are ParliamentaryClubs only — no new active_parties entries.
/// * Splinter clubs formalize into full parties only at the next general election.
/// * Defectors = `seats * split_risk * (1.0 - discipline) * 0.5`
pub fn check_faction_splintering(
    parliament: &mut Parliament,
    active_parties: &HashMap<String, Party>,
    approval_rating: f64,
    unrest: f64,
    current_turn: u32,
) -> Vec<SplinterEvent> {
    let mut events = Vec::new();

    if parliament.suspended {
        return events; // No splintering when parliament is suspended.
    }

    // Collect splinter candidates first (avoid mutating while iterating).
    let mut splinters: Vec<(usize, u32, String)> = Vec::new(); // (club_idx, defectors, reason)

    for (idx, club) in parliament.clubs.iter().enumerate() {
        if club.is_splinter || club.seats < 5 {
            continue; // Splinters don't splinter again; tiny clubs can't.
        }

        // Get party organization metrics.
        let party = club.parent_party.as_ref().and_then(|p| active_parties.get(p));
        let (factional_tension, discipline, split_risk) = if let Some(p) = party {
            (
                p.organization.factional_tension,
                p.organization.discipline,
                p.organization.split_risk(),
            )
        } else {
            (0.0, club.discipline, 0.0)
        };

        // Check splinter conditions.
        if split_risk <= 0.4 {
            continue;
        }
        if unrest <= 30.0 && approval_rating >= 40.0 {
            continue; // No splintering when things are stable.
        }

        // Calculate defectors.
        let stress_factor = if approval_rating < 30.0 { 1.5 } else { 1.0 };
        let unrest_factor = 1.0 + (unrest / 100.0).min(0.5);
        let defectors = ((club.seats as f64
            * split_risk
            * (1.0 - discipline)
            * 0.5
            * stress_factor
            * unrest_factor) as u32)
            .max(1)
            .min(club.seats / 2); // Max half the club can defect.

        if defectors == 0 {
            continue;
        }

        let reason = format!(
            "Factional tension {:.2}, approval {:.1}, unrest {:.1}",
            factional_tension, approval_rating, unrest
        );
        splinters.push((idx, defectors, reason));
    }

    // Apply splinters.
    for (club_idx, defectors, reason) in splinters {
        let source_id = parliament.clubs[club_idx].id.clone();
        let source_ideology = parliament.clubs[club_idx].ideology.clone();

        // Reduce source club seats.
        parliament.clubs[club_idx].seats -= defectors;

        // Reduce seats in the lower chamber.
        if let Some(chamber) = parliament.lower_chamber_mut() {
            if let Some(seats) = chamber.seats.get_mut(&source_id) {
                *seats = (*seats).saturating_sub(defectors);
            }
        }

        // Create new splinter club.
        let new_club_id = format!("Splinter_{}_{}", source_id, current_turn);
        let new_club = ParliamentaryClub {
            id: new_club_id.clone(),
            name: format!("Splinter ({})", source_id),
            parent_party: None,
            seats: defectors,
            ideology: source_ideology,
            discipline: 0.3, // Splinter groups are less disciplined.
            is_splinter: true,
            formation_turn: current_turn,
            chairperson_id: None,
            chairperson_name: String::new(),
        };
        parliament.clubs.push(new_club);

        // Add seats to the lower chamber for the new club.
        if let Some(chamber) = parliament.lower_chamber_mut() {
            chamber.seats.insert(new_club_id.clone(), defectors);
        }

        events.push(SplinterEvent {
            source_club: source_id,
            new_club: new_club_id,
            seats_defected: defectors,
            reason,
            turn: current_turn,
        });
    }

    events
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::politics::system::{GovernmentForm, Leader, Party, PartyOrganization, Politics};
    use crate::politics::interest_groups::ClassToGroupMapping;
    use serde_json::Map;

    fn make_test_politics() -> Politics {
        let mut politics = Politics::default();
        politics.government_form = GovernmentForm::ParliamentaryDemocracy;
        politics.ruling_party = "TestParty".to_string();
        politics.coalition = vec!["AllyParty".to_string()];
        politics.minority_government = false;
        politics.parliament.insert("TestParty".to_string(), 60);
        politics.parliament.insert("OppParty".to_string(), 30);
        politics.parliament.insert("AllyParty".to_string(), 10);

        let mut test_party = Party::default();
        test_party.ideology = "Social Democracy".to_string();
        test_party.organization = PartyOrganization {
            discipline: 0.7,
            factional_tension: 0.0,
            ..Default::default()
        };
        test_party.leader = Leader {
            name: "Jan Kowalski".to_string(),
            ..Default::default()
        };
        politics.active_parties.insert("TestParty".to_string(), test_party);

        let mut opp_party = Party::default();
        opp_party.ideology = "Classical Liberalism".to_string();
        opp_party.organization = PartyOrganization {
            discipline: 0.6,
            factional_tension: 0.0,
            ..Default::default()
        };
        politics.active_parties.insert("OppParty".to_string(), opp_party);

        let mut ally_party = Party::default();
        ally_party.ideology = "Social Liberalism".to_string();
        ally_party.organization = PartyOrganization {
            discipline: 0.5,
            factional_tension: 0.0,
            ..Default::default()
        };
        politics.active_parties.insert("AllyParty".to_string(), ally_party);

        politics
    }

    #[test]
    fn test_initialize_parliament_two_chambers() {
        let politics = make_test_politics();
        let mut rng = rand::thread_rng();
        let parliament = initialize_parliament(&politics, "slavic", 1, &mut rng, &mut HashSet::new());
        assert_eq!(parliament.chambers.len(), 2); // ParliamentaryDemocracy → 2 chambers
        assert!(parliament.lower_chamber().is_some());
        assert!(parliament.upper_chamber().is_some());
    }

    #[test]
    fn test_initialize_parliament_zero_chambers() {
        let mut politics = make_test_politics();
        politics.government_form = GovernmentForm::AbsoluteMonarchy;
        let mut rng = rand::thread_rng();
        let parliament = initialize_parliament(&politics, "slavic", 1, &mut rng, &mut HashSet::new());
        assert_eq!(parliament.chambers.len(), 0);
    }

    #[test]
    fn test_initialize_parliament_one_chamber() {
        let mut politics = make_test_politics();
        politics.government_form = GovernmentForm::OnePartyState;
        let mut rng = rand::thread_rng();
        let parliament = initialize_parliament(&politics, "slavic", 1, &mut rng, &mut HashSet::new());
        assert_eq!(parliament.chambers.len(), 1);
    }

    #[test]
    fn test_clubs_built_from_seats() {
        let politics = make_test_politics();
        let mut rng = rand::thread_rng();
        let parliament = initialize_parliament(&politics, "slavic", 1, &mut rng, &mut HashSet::new());
        assert_eq!(parliament.clubs.len(), 3); // TestParty, OppParty, AllyParty
        let test_club = parliament.clubs.iter().find(|c| c.id == "TestParty").unwrap();
        assert_eq!(test_club.seats, 60);
        assert!(!test_club.is_splinter);
    }

    #[test]
    fn test_vips_populated() {
        let politics = make_test_politics();
        let mut rng = rand::thread_rng();
        let parliament = initialize_parliament(&politics, "slavic", 1, &mut rng, &mut HashSet::new());
        assert!(!parliament.vips.is_empty());
        // Should have at least Head of State and PM.
        assert!(parliament.vips.iter().any(|v| v.role == VipRole::HeadOfState));
        assert!(parliament.vips.iter().any(|v| v.role == VipRole::PrimeMinister));
    }

    #[test]
    fn test_speaker_from_ruling_party() {
        let politics = make_test_politics();
        let mut rng = rand::thread_rng();
        let parliament = initialize_parliament(&politics, "slavic", 1, &mut rng, &mut HashSet::new());
        let lower = parliament.lower_chamber().unwrap();
        assert_eq!(lower.presidium.speaker.party, "TestParty");
        assert!(!lower.presidium.speaker.full_name.is_empty());
    }

    #[test]
    fn test_agenda_control_majority() {
        let politics = make_test_politics();
        let control = calculate_agenda_control(&politics);
        // TestParty(60) + AllyParty(10) = 70 of 100 → majority
        assert!(control > 0.5);
    }

    #[test]
    fn test_agenda_control_minority() {
        let mut politics = make_test_politics();
        politics.minority_government = true;
        politics.parliament.clear();
        politics.parliament.insert("TestParty".to_string(), 35);
        politics.parliament.insert("OppParty".to_string(), 65);
        let control = calculate_agenda_control(&politics);
        assert!(control < 0.5);
    }

    #[test]
    fn test_state_of_emergency_bypass() {
        let mut soe = StateOfEmergency::default();
        soe.activate(10, "Crisis".to_string(), 24, true, "President".to_string());
        assert!(soe.can_bypass_parliament());
        assert!(soe.active);
        assert_eq!(soe.turns_remaining, 24);
    }

    #[test]
    fn test_state_of_emergency_no_bypass_when_not_suspended() {
        let mut soe = StateOfEmergency::default();
        soe.activate(10, "Crisis".to_string(), 24, false, "President".to_string());
        assert!(!soe.can_bypass_parliament());
        assert!(soe.active);
    }

    #[test]
    fn test_state_of_emergency_auto_expire() {
        let mut soe = StateOfEmergency::default();
        soe.activate(10, "Crisis".to_string(), 3, true, "President".to_string());
        assert!(soe.active);
        soe.tick();
        assert!(soe.active);
        assert_eq!(soe.turns_remaining, 2);
        soe.tick();
        soe.tick();
        assert!(!soe.active);
        assert!(!soe.parliament_suspended);
    }

    #[test]
    fn test_state_of_emergency_deactivate() {
        let mut soe = StateOfEmergency::default();
        soe.activate(10, "Crisis".to_string(), 24, true, "President".to_string());
        soe.deactivate();
        assert!(!soe.active);
        assert!(!soe.parliament_suspended);
    }

    #[test]
    fn test_splintering_no_trigger_when_stable() {
        let politics = make_test_politics();
        let mut rng = rand::thread_rng();
        let mut parliament = initialize_parliament(&politics, "slavic", 1, &mut rng, &mut HashSet::new());
        let events = check_faction_splintering(&mut parliament, &politics.active_parties, 60.0, 10.0, 5);
        assert!(events.is_empty()); // High approval, low unrest → no splinter
    }

    #[test]
    fn test_splintering_triggers_with_high_tension() {
        let mut politics = make_test_politics();
        // Set high factional tension on TestParty.
        politics.active_parties.get_mut("TestParty").unwrap().organization.factional_tension = 0.9;
        politics.active_parties.get_mut("TestParty").unwrap().organization.cohesion = 0.2;

        let mut rng = rand::thread_rng();
        let mut parliament = initialize_parliament(&politics, "slavic", 1, &mut rng, &mut HashSet::new());
        let events = check_faction_splintering(&mut parliament, &politics.active_parties, 20.0, 60.0, 5);
        assert!(!events.is_empty()); // Should splinter
        let event = &events[0];
        assert_eq!(event.source_club, "TestParty");
        assert!(event.seats_defected > 0);
        assert!(event.seats_defected <= 30); // Max half of 60
    }

    #[test]
    fn test_splintering_creates_new_club() {
        let mut politics = make_test_politics();
        politics.active_parties.get_mut("TestParty").unwrap().organization.factional_tension = 0.9;
        politics.active_parties.get_mut("TestParty").unwrap().organization.cohesion = 0.2;

        let mut rng = rand::thread_rng();
        let mut parliament = initialize_parliament(&politics, "slavic", 1, &mut rng, &mut HashSet::new());
        let initial_clubs = parliament.clubs.len();
        check_faction_splintering(&mut parliament, &politics.active_parties, 20.0, 60.0, 5);
        assert!(parliament.clubs.len() > initial_clubs);
        let splinter = parliament.clubs.iter().find(|c| c.is_splinter).unwrap();
        assert!(splinter.seats > 0);
    }

    #[test]
    fn test_splintering_reallocates_seats() {
        let mut politics = make_test_politics();
        politics.active_parties.get_mut("TestParty").unwrap().organization.factional_tension = 0.9;
        politics.active_parties.get_mut("TestParty").unwrap().organization.cohesion = 0.2;

        let mut rng = rand::thread_rng();
        let mut parliament = initialize_parliament(&politics, "slavic", 1, &mut rng, &mut HashSet::new());
        let initial_test_seats = parliament.lower_chamber().unwrap().seats.get("TestParty").copied().unwrap_or(0);
        check_faction_splintering(&mut parliament, &politics.active_parties, 20.0, 60.0, 5);
        let final_test_seats = parliament.lower_chamber().unwrap().seats.get("TestParty").copied().unwrap_or(0);
        assert!(final_test_seats < initial_test_seats); // Seats reduced
    }

    #[test]
    fn test_splintering_skipped_when_suspended() {
        let mut politics = make_test_politics();
        politics.active_parties.get_mut("TestParty").unwrap().organization.factional_tension = 0.9;
        politics.active_parties.get_mut("TestParty").unwrap().organization.cohesion = 0.2;

        let mut rng = rand::thread_rng();
        let mut parliament = initialize_parliament(&politics, "slavic", 1, &mut rng, &mut HashSet::new());
        parliament.suspended = true;
        let events = check_faction_splintering(&mut parliament, &politics.active_parties, 20.0, 60.0, 5);
        assert!(events.is_empty());
    }

    #[test]
    fn test_record_vote_keeps_last_20() {
        let mut parliament = Parliament::default();
        parliament.chambers.push(Chamber::default());
        for i in 0..25 {
            parliament.record_vote(VoteRecord {
                bill_id: format!("bill_{}", i),
                turn: i,
                ..Default::default()
            });
        }
        assert_eq!(parliament.lower_chamber().unwrap().recent_votes.len(), 20);
        // Most recent should be first.
        assert_eq!(parliament.lower_chamber().unwrap().recent_votes[0].bill_id, "bill_24");
    }

    #[test]
    fn test_queue_and_dequeue_bill() {
        let mut parliament = Parliament::default();
        parliament.chambers.push(Chamber::default());
        parliament.queue_bill("BILL-001".to_string());
        parliament.queue_bill("BILL-002".to_string());
        assert_eq!(parliament.lower_chamber().unwrap().legislative_queue.len(), 2);
        // Queueing same bill again should not duplicate.
        parliament.queue_bill("BILL-001".to_string());
        assert_eq!(parliament.lower_chamber().unwrap().legislative_queue.len(), 2);
        parliament.dequeue_bill("BILL-001");
        assert_eq!(parliament.lower_chamber().unwrap().legislative_queue.len(), 1);
    }
}

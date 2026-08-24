//! Local council structures and election systems for regional governance

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Local council structure (regional legislative body)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct LocalCouncil {
    /// Unique identifier for the council
    #[serde(default)]
    pub id: String,
    
    /// Total number of seats (dynamic, scales with population)
    #[serde(default)]
    pub total_seats: u32,
    
    /// Councilors by class and faction
    #[serde(default)]
    pub councilors: Vec<Councilor>,
    
    /// Faction distribution (Populares, Moderates, Optimates)
    #[serde(default)]
    pub faction_distribution: FactionDistribution,
    
    /// Election system in use
    #[serde(default)]
    pub election_system: LocalElectionSystem,
    
    /// Election configuration (varies by system)
    #[serde(default)]
    pub election_config: ElectionConfig,
    
    /// Last election year
    #[serde(default)]
    pub last_election_year: u32,
    
    /// Years until next election
    #[serde(default)]
    pub years_to_next_election: u32,
    
    /// Council approval rating (0-100)
    #[serde(default)]
    pub approval_rating: f64,
    
    /// Any additional council fields
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Individual councilor
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Councilor {
    /// Councilor identifier
    #[serde(default)]
    pub id: String,
    
    /// Name
    #[serde(default)]
    pub name: String,
    
    /// Class represented (Aristocracy, Burghers, Peasants, etc.)
    #[serde(default)]
    pub represented_class: String,
    
    /// Faction alignment
    #[serde(default)]
    pub faction: Faction,
    
    /// Years in office
    #[serde(default)]
    pub years_in_office: u32,
    
    /// Political influence (0-100)
    #[serde(default)]
    pub political_influence: f64,
    
    /// Hidden/active trait affecting voting behavior
    #[serde(default)]
    pub hidden_trait: CouncilorTrait,
    
    /// Whether trait is revealed to other actors
    #[serde(default)]
    pub trait_revealed: bool,
    
    /// Blackmail material (if corrupt)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blackmail_material: Option<String>,
    
    /// Party affiliation (for discipline and wealth effects in voting)
    #[serde(default)]
    pub party: String,
    
    // PHASE 4: Corruption risk (0-1) - for tracking bribery exposure
    #[serde(default)]
    pub corruption_risk: f64,
}

/// Faction alignment for political actors
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Faction {
    #[default]
    /// Populares - populist, reformist, pro-plebeian
    Populares,
    /// Moderates - centrist, compromise-seeking
    Moderates,
    /// Optimates - conservative, aristocratic, pro-status quo
    Optimates,
}

impl std::fmt::Display for Faction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Faction::Populares => write!(f, "Populares"),
            Faction::Moderates => write!(f, "Moderates"),
            Faction::Optimates => write!(f, "Optimates"),
        }
    }
}

/// Hidden/active trait affecting councilor voting behavior
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CouncilorTrait {
    #[default]
    /// Always votes party line, cannot be swayed
    Loyalist,
    /// Can be swayed by concessions/horse-trading
    Undecided,
    /// Hidden by default, can be bribed or blackmailed
    Corrupt,
    /// Randomly votes based on personal ideology
    Maverick,
}

/// Faction distribution across the council
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct FactionDistribution {
    /// Populares seat count
    #[serde(default)]
    pub populares_count: u32,
    
    /// Moderates seat count
    #[serde(default)]
    pub moderates_count: u32,
    
    /// Optimates seat count
    #[serde(default)]
    pub optimates_count: u32,
    
    /// Faction stability (0-1, higher = less faction switching)
    #[serde(default)]
    pub faction_stability: f64,
}

impl FactionDistribution {
    /// Total seats
    pub fn total(&self) -> u32 {
        self.populares_count + self.moderates_count + self.optimates_count
    }
    
    /// Populares share (0-1)
    pub fn populares_share(&self) -> f64 {
        let total = self.total();
        if total == 0 { 0.0 } else { self.populares_count as f64 / total as f64 }
    }
    
    /// Optimates share (0-1)
    pub fn optimates_share(&self) -> f64 {
        let total = self.total();
        if total == 0 { 0.0 } else { self.optimates_count as f64 / total as f64 }
    }
}

/// Local election system type
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LocalElectionSystem {
    #[default]
    /// Curial system - seats allocated by class, hereditary/appointed
    Curial,
    /// Census system - weighted voting based on wealth/tax contribution
    Census,
    /// Democratic system - universal suffrage, secret ballot
    Democratic,
}

/// Election configuration (varies by system)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "system", rename_all = "snake_case")]
pub enum ElectionConfig {
    Curial(CurialConfiguration),
    Census(CensusConfiguration),
    Democratic(DemocraticConfiguration),
}

impl Default for ElectionConfig {
    fn default() -> Self {
        ElectionConfig::Curial(CurialConfiguration::default())
    }
}

/// Curial election configuration
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct CurialConfiguration {
    /// Seat allocation by class (class_name -> seat_count)
    #[serde(default)]
    pub seat_allocation: BTreeMap<String, u32>,
    
    /// Hereditary vs appointed ratio (0 = all appointed, 1 = all hereditary)
    #[serde(default)]
    pub hereditary_ratio: f64,
    
    /// Aristocratic veto power (true = aristocracy can veto decisions)
    #[serde(default)]
    pub aristocratic_veto: bool,
}

/// Census election configuration
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct CensusConfiguration {
    /// Minimum tax contribution to qualify for voting
    #[serde(default)]
    pub tax_threshold: f64,
    
    /// Vote weight multiplier based on wealth
    #[serde(default)]
    pub wealth_weight_multiplier: f64,
    
    /// Property qualification for candidacy
    #[serde(default)]
    pub property_qualification: f64,
}

/// Democratic election configuration
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct DemocraticConfiguration {
    /// Voting age
    #[serde(default)]
    pub voting_age: u32,
    
    /// Candidacy age
    #[serde(default)]
    pub candidacy_age: u32,
    
    /// Term length in years
    #[serde(default)]
    pub term_length: u32,
    
    /// Term limits (0 = no limit)
    #[serde(default)]
    pub term_limits: u32,
    
    /// Proportional representation (true) vs first-past-the-post (false)
    #[serde(default)]
    pub proportional_representation: bool,
}

/// Calculate faction alignment for Curial councils
/// 
/// # Arguments
/// * `council` - The local council to calculate alignment for
/// * `revolt_risk` - Regional revolt risk (0-1), affects faction defection
/// * `economic_stability` - Regional economic stability (0-1)
/// 
/// # Returns
/// Updated faction distribution based on class interests and conditions
/// 
/// # Rules
/// * Curial councils shift continuously (not just at elections)
/// * High revolt_risk causes defection from Optimates to Populares
/// * Economic instability causes defection from Moderates to Populares
/// * Aristocratic veto power increases Optimates stability
pub fn calculate_curial_faction_alignment(
    council: &mut LocalCouncil,
    revolt_risk: f64,
    economic_stability: f64,
) {
    let config = match &council.election_config {
        ElectionConfig::Curial(cfg) => cfg,
        _ => return, // Only applies to Curial systems
    };
    
    let total_seats = council.total_seats as f64;
    if total_seats == 0.0 {
        return;
    }
    
    let mut defection_rate = 0.0;
    
    // High revolt risk causes Optimates -> Populares defection
    if revolt_risk > 0.5 {
        defection_rate += (revolt_risk - 0.5) * 0.3;
    }
    
    // Economic instability causes Moderates -> Populares defection
    if economic_stability < 0.5 {
        defection_rate += (0.5 - economic_stability) * 0.2;
    }
    
    // Aristocratic veto reduces defection from Optimates
    if config.aristocratic_veto {
        defection_rate *= 0.5;
    }
    
    // Apply faction stability modifier
    defection_rate *= 1.0 - council.faction_distribution.faction_stability;
    
    // Calculate seat transfers
    let optimates_defecting = (council.faction_distribution.optimates_count as f64 * defection_rate) as u32;
    let moderates_defecting = (council.faction_distribution.moderates_count as f64 * defection_rate * 0.5) as u32;
    
    // Transfer seats
    council.faction_distribution.optimates_count = council.faction_distribution.optimates_count.saturating_sub(optimates_defecting);
    council.faction_distribution.moderates_count = council.faction_distribution.moderates_count.saturating_sub(moderates_defecting);
    council.faction_distribution.populares_count += optimates_defecting + moderates_defecting;
    
    // Update individual councilor factions
    update_councilor_factions(council, defection_rate);
}

/// Update individual councilor faction alignments based on defection rate
fn update_councilor_factions(council: &mut LocalCouncil, defection_rate: f64) {
    for councilor in &mut council.councilors {
        // Random chance to switch faction based on defection rate
        if rand::random::<f64>() < defection_rate {
            match councilor.faction {
                Faction::Optimates => {
                    councilor.faction = Faction::Populares;
                }
                Faction::Moderates => {
                    councilor.faction = Faction::Populares;
                }
                Faction::Populares => {
                    // Rarely switch back to Moderates if conditions improve
                    if rand::random::<f64>() < 0.1 {
                        councilor.faction = Faction::Moderates;
                    }
                }
            }
        }
    }
}

/// Calculate dynamic seat count based on regional population
/// 
/// # Arguments
/// * `population` - Regional population
/// 
/// # Returns
/// Number of council seats (typically 15-50, scaling with population)
pub fn calculate_seat_count(population: i64) -> u32 {
    if population < 10_000 {
        15
    } else if population < 100_000 {
        20
    } else if population < 500_000 {
        30
    } else if population < 1_000_000 {
        40
    } else {
        50
    }
}

/// Calculate councilor vote probability based on trait and conditions
/// 
/// # Arguments
/// * `councilor` - The councilor to calculate vote probability for
/// * `concession_offered` - Whether a concession was offered to sway the vote
/// * `ideological_alignment` - Ideological alignment score (0-1, higher = more aligned)
/// * `bribed` - Whether the councilor was bribed
/// * `blackmailed` - Whether the councilor was blackmailed
/// 
/// # Returns
/// Vote probability (0-1, where 0.5 is neutral)
/// 
/// # Rules
/// * Loyalist: Always votes party line (0.9+ probability)
/// * Undecided: Base 50% party line, +30% if concession offered, +20% if ideological alignment
/// * Corrupt: Base 40% party line, +40% if bribed, +20% if blackmailed
/// * Maverick: Votes based on ideological alignment with randomness
pub fn calculate_vote_probability(
    councilor: &Councilor,
    concession_offered: bool,
    ideological_alignment: f64,
    bribed: bool,
    blackmailed: bool,
    party_discipline: f64,
    party_wealth: f64,
) -> f64 {
    match councilor.hidden_trait {
        CouncilorTrait::Loyalist => {
            // Loyalists are naturally bound to the party line
            // Discipline has minimal effect on them
            let base_probability = 0.9 + rand::random::<f64>() * 0.1;
            base_probability + (party_discipline * 0.05)  // Small discipline boost
        }
        CouncilorTrait::Undecided => {
            // Undecided councilors are significantly affected by discipline
            let mut probability: f64 = 0.5;
            if concession_offered {
                probability += 0.3;
            }
            probability += ideological_alignment * 0.2;
            probability += party_discipline * 0.3;  // Significant discipline effect
            probability.min(1.0)
        }
        CouncilorTrait::Corrupt => {
            // Corrupt councilors only care about discipline if party is wealthy
            let mut probability: f64 = 0.4;
            if bribed {
                probability += 0.4;
            }
            if blackmailed {
                probability += 0.2;
            }
            // Only apply discipline if party has sufficient wealth (> 10,000)
            if party_wealth > 10000.0 {
                probability += party_discipline * 0.2;
            }
            probability.min(1.0)
        }
        CouncilorTrait::Maverick => {
            // Votes based on ideological alignment with randomness
            ideological_alignment + (rand::random::<f64>() - 0.5) * 0.3
        }
    }
}

//! Front-based combat system for war management

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::society::geography::RuralClass;

/// Control status of a region
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RegionControl {
    /// Controlled by original owner
    Owner,
    /// Occupied by enemy
    Occupied(String),
    /// Contested territory
    Contested,
    /// Rebel control
    RebelControl,
}

/// Casualty breakdown by category
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Casualties {
    /// Dead soldiers
    #[serde(rename = "polegli", default)]
    pub dead: i64,
    
    /// Wounded soldiers
    #[serde(rename = "ranni", default)]
    pub wounded: i64,
    
    /// Deserters
    #[serde(rename = "dezerterzy", default)]
    pub deserters: i64,
    
    /// Demographic breakdown of casualties (for routing back to classes)
    #[serde(rename = "podział_demograficzny", default)]
    pub demographic_breakdown: HashMap<RuralClass, i64>,
}

impl Casualties {
    /// Create new casualties record
    /// 
    /// # Arguments
    /// * `dead` - Number of dead
    /// * `wounded` - Number of wounded
    /// * `deserters` - Number of deserters
    /// 
    /// # Returns
    /// New Casualties instance
    pub fn new(dead: i64, wounded: i64, deserters: i64) -> Self {
        Casualties {
            dead,
            wounded,
            deserters,
            demographic_breakdown: HashMap::new(),
        }
    }
    
    /// Set demographic breakdown
    /// 
    /// # Arguments
    /// * `breakdown` - HashMap of RuralClass to casualty count
    pub fn set_demographic_breakdown(&mut self, breakdown: HashMap<RuralClass, i64>) {
        self.demographic_breakdown = breakdown;
    }
    
    /// Get total casualties
    /// 
    /// # Returns
    /// Sum of dead, wounded, and deserters
    pub fn total(&self) -> i64 {
        self.dead + self.wounded + self.deserters
    }
}

/// Battle record
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Battle {
    /// Unique battle ID
    #[serde(rename = "id_bitwy", default)]
    pub id: String,
    
    /// Battle location (region ID)
    #[serde(rename = "lokalizacja")]
    pub location: String,
    
    /// Attacking country
    #[serde(rename = "atakujący")]
    pub attacker: String,
    
    /// Defending country
    #[serde(rename = "obrońca")]
    pub defender: String,
    
    /// Turn when battle occurred
    #[serde(rename = "tura")]
    pub turn: u32,
    
    /// Attacker units involved
    #[serde(rename = "jednostki_atakujące", default)]
    pub attacker_units: Vec<String>,
    
    /// Defender units involved
    #[serde(rename = "jednostki_obronne", default)]
    pub defender_units: Vec<String>,
    
    /// Attacker casualties
    #[serde(rename = "straty_atakujące")]
    pub attacker_casualties: Casualties,
    
    /// Defender casualties
    #[serde(rename = "straty_obronne")]
    pub defender_casualties: Casualties,
    
    /// Battle result
    #[serde(rename = "wynik")]
    pub result: BattleResult,
}

/// Battle outcome
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BattleResult {
    /// Attacker victory
    AttackerVictory,
    /// Defender victory
    DefenderVictory,
    /// Stalemate
    Stalemate,
    /// Pyrrhic victory (attacker won but with heavy losses)
    PyrrhicVictory,
}

/// Military front (collection of battles in a region)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Front {
    /// Unique front ID
    #[serde(rename = "id_frontu", default)]
    pub id: String,
    
    /// Front name
    #[serde(rename = "nazwa")]
    pub name: String,
    
    /// Regions involved in this front
    #[serde(rename = "regiony", default)]
    pub regions: Vec<String>,
    
    /// Control status of each region
    #[serde(rename = "kontrola_regionów", default)]
    pub region_control: HashMap<String, RegionControl>,
    
    /// Active battles in this front
    #[serde(rename = "bitwy", default)]
    pub battles: Vec<Battle>,
    
    /// Countries involved in this front
    #[serde(rename = "kraj_zaangażowane", default)]
    pub involved_countries: Vec<String>,
    
    /// War exhaustion for each country
    #[serde(rename = "zmęczenie_wojenne", default)]
    pub war_exhaustion: HashMap<String, f64>,
}

impl Front {
    /// Create a new military front
    /// 
    /// # Arguments
    /// * `id` - Unique front identifier
    /// * `name` - Front name
    /// * `regions` - Regions in this front
    /// * `countries` - Countries involved
    /// 
    /// # Returns
    /// New Front instance
    pub fn new(
        id: String,
        name: String,
        regions: Vec<String>,
        countries: Vec<String>,
    ) -> Self {
        let mut region_control = HashMap::new();
        for region in &regions {
            region_control.insert(region.clone(), RegionControl::Owner);
        }
        
        let mut war_exhaustion = HashMap::new();
        for country in &countries {
            war_exhaustion.insert(country.clone(), 0.0);
        }
        
        Front {
            id,
            name,
            regions,
            region_control,
            battles: Vec::new(),
            involved_countries: countries,
            war_exhaustion,
        }
    }
    
    /// Add a battle to this front
    /// 
    /// # Arguments
    /// * `battle` - Battle to add
    pub fn add_battle(&mut self, battle: Battle) {
        self.battles.push(battle);
    }
    
    /// Update region control
    /// 
    /// # Arguments
    /// * `region` - Region to update
    /// * `control` - New control status
    pub fn update_region_control(&mut self, region: String, control: RegionControl) {
        self.region_control.insert(region, control);
    }
    
    /// Increase war exhaustion for a country
    /// 
    /// # Arguments
    /// * `country` - Country to affect
    /// * `amount` - Amount to increase
    pub fn increase_war_exhaustion(&mut self, country: String, amount: f64) {
        *self.war_exhaustion.entry(country).or_insert(0.0) += amount;
    }
    
    /// Decay war exhaustion for all countries
    /// 
    /// # Arguments
    /// * `decay_rate` - Rate to decay (0-1)
    pub fn decay_war_exhaustion(&mut self, decay_rate: f64) {
        for exhaustion in self.war_exhaustion.values_mut() {
            *exhaustion *= (1.0 - decay_rate).max(0.0);
        }
    }
    
    /// Get total casualties for a country
    /// 
    /// # Arguments
    /// * `country` - Country to query
    /// 
    /// # Returns
    /// Total casualties (dead + wounded + deserters)
    pub fn get_country_casualties(&self, country: &str) -> i64 {
        let mut total = 0;
        for battle in &self.battles {
            if battle.attacker == country {
                total += battle.attacker_casualties.total();
            }
            if battle.defender == country {
                total += battle.defender_casualties.total();
            }
        }
        total
    }
    
    /// Check if front is active (has recent battles)
    /// 
    /// # Arguments
    /// * `current_turn` - Current game turn
    /// * `active_turns` - Number of turns to consider active
    /// 
    /// # Returns
    /// True if front has battles within active_turns
    pub fn is_active(&self, current_turn: u32, active_turns: u32) -> bool {
        self.battles.iter()
            .any(|b| current_turn - b.turn <= active_turns)
    }
}

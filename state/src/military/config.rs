//! Military combat and supply configuration parameters.
//!
//! All combat multipliers, terrain modifiers, casualty ratios, base ammo burn
//! rates, and supply attrition thresholds live here.  No magic numbers in
//! logic files — everything configurable via `MilitaryCombatConfig`.

use serde::{Deserialize, Serialize};

/// All combat and supply parameters.  Stored on `Country`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MilitaryCombatConfig {
    // ── Terrain modifiers (defender bonus) ──
    /// Defender multiplier in mountain terrain.
    #[serde(default = "default_terrain_mountain")]
    pub terrain_mountain_defense_bonus: f64,
    /// Defender multiplier in forest terrain.
    #[serde(default = "default_terrain_forest")]
    pub terrain_forest_defense_bonus: f64,
    /// Defender multiplier on plains.
    #[serde(default = "default_terrain_plains")]
    pub terrain_plains_defense_bonus: f64,

    // ── Battle resolution thresholds ──
    /// Attacker power > defender power * this → AttackerVictory.
    #[serde(default = "default_decisive_victory")]
    pub decisive_victory_ratio: f64,
    /// Attacker power > defender power * this → PyrrhicVictory.
    #[serde(default = "default_pyrrhic_victory")]
    pub pyrrhic_victory_ratio: f64,

    // ── Casualty ratios ──
    /// Maximum fraction of loser's manpower that becomes casualties.
    #[serde(default = "default_max_loser_casualty")]
    pub max_loser_casualty_ratio: f64,
    /// Winner casualties as a fraction of loser casualties.
    #[serde(default = "default_winner_casualty_mult")]
    pub winner_casualty_multiplier: f64,
    /// Casualty ratio for both sides in a stalemate.
    #[serde(default = "default_stalemate_casualty")]
    pub stalemate_casualty_ratio: f64,
    /// Fraction of casualties that are dead.
    #[serde(default = "default_dead_ratio")]
    pub dead_ratio: f64,
    /// Fraction of casualties that are wounded.
    #[serde(default = "default_wounded_ratio")]
    pub wounded_ratio: f64,
    /// Fraction of casualties that are deserters.
    #[serde(default = "default_deserters_ratio")]
    pub deserters_ratio: f64,

    // ── Combat commodity burn rates (per 1000 soldiers per battle) ──
    /// Base ammunition burned per 1000 soldiers in a decisive battle.
    #[serde(default = "default_ammo_burn")]
    pub base_ammo_burn_per_battle: f64,
    /// Base fuel burned per 1000 soldiers in a decisive battle.
    #[serde(default = "default_fuel_burn")]
    pub base_fuel_burn_per_battle: f64,
    /// Combat intensity multiplier for stalemates (reduced burn).
    #[serde(default = "default_stalemate_intensity")]
    pub stalemate_combat_intensity: f64,
    /// Combat intensity multiplier for decisive battles (full burn).
    #[serde(default = "default_decisive_intensity")]
    pub decisive_combat_intensity: f64,

    // ── Supply & attrition ──
    /// Supply level at or above which a unit fights at full power.
    #[serde(default = "default_supply_full")]
    pub supply_full_threshold: f64,
    /// Combat power multiplier when supply is zero.
    #[serde(default = "default_supply_zero_penalty")]
    pub supply_zero_penalty: f64,
    /// Supply level below which attrition occurs.
    #[serde(default = "default_attrition_threshold")]
    pub attrition_supply_threshold: f64,
    /// Fraction of manpower lost per turn below attrition threshold.
    #[serde(default = "default_attrition_loss")]
    pub attrition_manpower_loss_ratio: f64,
    /// Organization lost per turn when fully unsupplied.
    #[serde(default = "default_org_loss")]
    pub organization_loss_unsupplied: f64,

    // ── Upkeep ──
    /// Food consumed per 1000 soldiers per turn.
    #[serde(default = "default_food_upkeep")]
    pub food_upkeep_per_1000: f64,
    /// Number of turns of upkeep a unit's field stockpile can hold.
    #[serde(default = "default_supply_capacity")]
    pub unit_supply_capacity_turns: f64,

    // ── War exhaustion ──
    /// War exhaustion decay rate per turn (fraction).
    #[serde(default = "default_we_decay")]
    pub war_exhaustion_decay_rate: f64,
    /// War exhaustion gained per 1000 casualties.
    #[serde(default = "default_we_per_casualty")]
    pub war_exhaustion_per_casualty: f64,

    // ── Peasant devastation ──
    /// Multiplier: foraging_intensity * this = GDP damage fraction.
    #[serde(default = "default_peasant_devastation")]
    pub peasant_devastation_multiplier: f64,

    // ── Multi-domain combat (Phase 70.4) ──
    /// Land combat power multiplier for the side with air superiority (offensive).
    /// Derived from close-air-support effectiveness (CAS multiplier).
    #[serde(default = "default_air_superiority_offensive")]
    pub air_superiority_offensive_bonus: f64,
    /// Land combat power multiplier for the side with air superiority (defensive).
    /// Derived from air interdiction reducing attacker effectiveness.
    #[serde(default = "default_air_superiority_defensive")]
    pub air_superiority_defensive_bonus: f64,
    /// Land combat power multiplier for the side with naval control (coastal).
    /// Derived from naval gunfire support effectiveness.
    #[serde(default = "default_naval_bombardment")]
    pub naval_bombardment_bonus: f64,

    // ── Strategic retreat (Phase 70.4b) ──
    /// Combat power ratio below which retreat is evaluated (e.g., 0.333 = 1:3).
    /// If one side's power is less than this fraction of the other's, retreat
    /// is considered (subject to commander traits).
    #[serde(default = "default_catastrophic_power_ratio")]
    pub catastrophic_power_ratio: f64,
    /// Casualty ratio for the retreating side (e.g., 0.05 = 5% losses).
    /// Much lower than a decisive defeat (30%) — the army escapes.
    #[serde(default = "default_retreat_casualty_ratio")]
    pub retreat_casualty_ratio: f64,
    /// Casualty ratio for the opposing side during a retreat (e.g., 0.02).
    /// Minimal — the retreating side fled, so the victor takes few losses.
    #[serde(default = "default_retreat_attacker_casualty_ratio")]
    pub retreat_attacker_casualty_ratio: f64,
    /// Fraction of ToE equipment lost during retreat (e.g., 0.15 = 15%).
    /// Abandoned equipment flows to the victor as captured materiel.
    #[serde(default = "default_retreat_equipment_loss_rate")]
    pub retreat_equipment_loss_rate: f64,

    // ── Phase 71: Devastation Index ──
    /// Devastation added per decisive battle on combat-zone parcels.
    #[serde(default = "default_devastation_per_decisive_battle")]
    pub devastation_per_decisive_battle: f64,
    /// Devastation added per stalemate turn on combat-zone parcels.
    #[serde(default = "default_devastation_per_stalemate_turn")]
    pub devastation_per_stalemate_turn: f64,
    /// Natural devastation decay rate per turn when no combat occurs.
    #[serde(default = "default_devastation_decay_rate")]
    pub devastation_decay_rate: f64,
    /// Rate at which devastation spreads to adjacent parcels via the
    /// topological adjacency graph (fraction of excess devastation spread).
    #[serde(default = "default_devastation_spread_rate")]
    pub devastation_spread_rate: f64,
    /// Damage threshold: below this, buildings are damaged (reduced output)
    /// but remain operational.
    #[serde(default = "default_damage_threshold")]
    pub damage_threshold: f64,
    /// Destruction threshold: above this, buildings are destroyed and must
    /// be rebuilt by the Construction sector.
    #[serde(default = "default_destruction_threshold")]
    pub destruction_threshold: f64,
    /// Factor by which devastation reduces building condition when damaged.
    #[serde(default = "default_damage_factor")]
    pub damage_factor: f64,
}

// ── Default value functions ──

fn default_terrain_mountain() -> f64 {
    1.3
}
fn default_terrain_forest() -> f64 {
    1.2
}
fn default_terrain_plains() -> f64 {
    1.0
}
fn default_decisive_victory() -> f64 {
    1.5
}
fn default_pyrrhic_victory() -> f64 {
    1.0
}
fn default_max_loser_casualty() -> f64 {
    0.3
}
fn default_winner_casualty_mult() -> f64 {
    0.4
}
fn default_stalemate_casualty() -> f64 {
    0.1
}
fn default_dead_ratio() -> f64 {
    0.5
}
fn default_wounded_ratio() -> f64 {
    0.35
}
fn default_deserters_ratio() -> f64 {
    0.15
}
fn default_ammo_burn() -> f64 {
    10.0
}
fn default_fuel_burn() -> f64 {
    5.0
}
fn default_stalemate_intensity() -> f64 {
    0.5
}
fn default_decisive_intensity() -> f64 {
    1.0
}
fn default_supply_full() -> f64 {
    80.0
}
fn default_supply_zero_penalty() -> f64 {
    0.3
}
fn default_attrition_threshold() -> f64 {
    25.0
}
fn default_attrition_loss() -> f64 {
    0.01
}
fn default_org_loss() -> f64 {
    5.0
}
fn default_food_upkeep() -> f64 {
    2.0
}
fn default_supply_capacity() -> f64 {
    3.0
}
fn default_we_decay() -> f64 {
    0.05
}
fn default_we_per_casualty() -> f64 {
    0.1
}
fn default_peasant_devastation() -> f64 {
    0.3
}
fn default_air_superiority_offensive() -> f64 {
    1.25
}
fn default_air_superiority_defensive() -> f64 {
    1.15
}
fn default_naval_bombardment() -> f64 {
    1.20
}
fn default_catastrophic_power_ratio() -> f64 {
    0.333
}
fn default_retreat_casualty_ratio() -> f64 {
    0.05
}
fn default_retreat_attacker_casualty_ratio() -> f64 {
    0.02
}
fn default_retreat_equipment_loss_rate() -> f64 {
    0.15
}
fn default_devastation_per_decisive_battle() -> f64 {
    0.15
}
fn default_devastation_per_stalemate_turn() -> f64 {
    0.05
}
fn default_devastation_decay_rate() -> f64 {
    0.02
}
fn default_devastation_spread_rate() -> f64 {
    0.10
}
fn default_damage_threshold() -> f64 {
    0.3
}
fn default_destruction_threshold() -> f64 {
    0.7
}
fn default_damage_factor() -> f64 {
    0.5
}

impl Default for MilitaryCombatConfig {
    fn default() -> Self {
        MilitaryCombatConfig {
            terrain_mountain_defense_bonus: default_terrain_mountain(),
            terrain_forest_defense_bonus: default_terrain_forest(),
            terrain_plains_defense_bonus: default_terrain_plains(),
            decisive_victory_ratio: default_decisive_victory(),
            pyrrhic_victory_ratio: default_pyrrhic_victory(),
            max_loser_casualty_ratio: default_max_loser_casualty(),
            winner_casualty_multiplier: default_winner_casualty_mult(),
            stalemate_casualty_ratio: default_stalemate_casualty(),
            dead_ratio: default_dead_ratio(),
            wounded_ratio: default_wounded_ratio(),
            deserters_ratio: default_deserters_ratio(),
            base_ammo_burn_per_battle: default_ammo_burn(),
            base_fuel_burn_per_battle: default_fuel_burn(),
            stalemate_combat_intensity: default_stalemate_intensity(),
            decisive_combat_intensity: default_decisive_intensity(),
            supply_full_threshold: default_supply_full(),
            supply_zero_penalty: default_supply_zero_penalty(),
            attrition_supply_threshold: default_attrition_threshold(),
            attrition_manpower_loss_ratio: default_attrition_loss(),
            organization_loss_unsupplied: default_org_loss(),
            food_upkeep_per_1000: default_food_upkeep(),
            unit_supply_capacity_turns: default_supply_capacity(),
            war_exhaustion_decay_rate: default_we_decay(),
            war_exhaustion_per_casualty: default_we_per_casualty(),
            peasant_devastation_multiplier: default_peasant_devastation(),
            air_superiority_offensive_bonus: default_air_superiority_offensive(),
            air_superiority_defensive_bonus: default_air_superiority_defensive(),
            naval_bombardment_bonus: default_naval_bombardment(),
            catastrophic_power_ratio: default_catastrophic_power_ratio(),
            retreat_casualty_ratio: default_retreat_casualty_ratio(),
            retreat_attacker_casualty_ratio: default_retreat_attacker_casualty_ratio(),
            retreat_equipment_loss_rate: default_retreat_equipment_loss_rate(),
            devastation_per_decisive_battle: default_devastation_per_decisive_battle(),
            devastation_per_stalemate_turn: default_devastation_per_stalemate_turn(),
            devastation_decay_rate: default_devastation_decay_rate(),
            devastation_spread_rate: default_devastation_spread_rate(),
            damage_threshold: default_damage_threshold(),
            destruction_threshold: default_destruction_threshold(),
            damage_factor: default_damage_factor(),
        }
    }
}

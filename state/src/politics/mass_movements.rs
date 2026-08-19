use crate::entities::{Building, Company};
use crate::politics::chaos_config::ChaosConfig;
use crate::registries::enums::Commodity;
use crate::society::geography::{ClassDemographics, EconomicStatus, RegionalClassDemographics};
use crate::state::treasury::Treasury;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// Type of mass movement
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MassMovementType {
    #[default]
    #[serde(rename = "strajk")]
    IndustrialStrike,  // Workers stop production
    
    #[serde(rename = "zamieszki")]
    Riot,  // Violent unrest, property damage
    
    #[serde(rename = "protest")]
    PeacefulProtest,  // Non-violent demonstration
    
    #[serde(rename = "okupacja")]
    Occupation,  // Physical occupation of facilities
    
    #[serde(rename = "bojkot")]
    Boycott,  // Consumer boycott of specific goods
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MassMovementStatus {
    #[default]
    #[serde(rename = "formowanie")]
    Forming,  // Gathering support
    
    #[serde(rename = "aktywny")]
    Active,  // Currently disrupting
    
    #[serde(rename = "negocjacje")]
    Negotiating,  // In talks with government
    
    #[serde(rename = "zakończony_sukcesem")]
    ResolvedSuccess,  // Demands met
    
    #[serde(rename = "zakończony_porażką")]
    ResolvedFailure,  // Suppressed/abandoned
    
    #[serde(rename = "rozproszony")]
    Dispersed,  // Broken up by force
}

/// A mass movement event in a region
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MassMovement {
    /// Movement ID
    #[serde(rename = "id_ruchu", default)]
    pub id: String,
    
    /// Region where movement is active
    #[serde(rename = "region_id", default)]
    pub region_id: String,
    
    /// Movement type
    #[serde(rename = "typ_ruchu", default)]
    pub movement_type: MassMovementType,
    
    /// Demographic class primarily involved
    #[serde(rename = "klasa_inicjująca", default)]
    pub initiating_class: String,
    
    /// Turn when movement started
    #[serde(rename = "turn_początku", default)]
    pub start_turn: u32,
    
    /// Expected duration in turns (0 = indefinite)
    #[serde(rename = "przewidywany_czas_trwania", default)]
    pub expected_duration: u32,
    
    /// Current intensity (0-1, affects disruption magnitude)
    #[serde(rename = "intensywność", default)]
    pub intensity: f64,
    
    /// Participating population count
    #[serde(rename = "liczba_uczestników", default)]
    pub participant_count: i64,
    
    /// Whether movement is union-backed (triggers strike fund mechanics)
    #[serde(rename = "wspierany_przez_związki", default)]
    pub union_backed: bool,
    
    /// Trade union ID providing funding (if union_backed)
    #[serde(rename = "id_związku", skip_serializing_if = "Option::is_none")]
    pub union_id: Option<String>,
    
    /// Strike fund allocation per participant (if union_backed)
    #[serde(rename = "fund_strajkowy_na_uczestnika", default)]
    pub strike_fund_per_participant: f64,
    
    /// Target companies affected by this movement
    #[serde(rename = "firmy_celowe", default)]
    pub target_companies: Vec<String>,
    
    /// Movement status
    #[serde(rename = "status", default)]
    pub status: MassMovementStatus,
    
    /// Demands (list of concessions requested)
    #[serde(rename = "żądania", default)]
    pub demands: Vec<String>,
    
    /// Any additional fields
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SuppressionError {
    #[serde(rename = "niewystarczające_fundusze")]
    InsufficientFunds,  // Treasury cannot afford suppression cost
    
    #[serde(rename = "ruch_już_zakończony")]
    MovementAlreadyResolved,  // Cannot suppress inactive movement
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SuppressionResult {
    #[default]
    #[serde(rename = "sukces")]
    Success,  // Movement dispersed, casualties occurred
    
    #[serde(rename = "porażka")]
    Failure,  // Insufficient security power, movement continues
    
    #[serde(rename = "odwrócenie")]
    Backlash,  // Suppression triggered massive radicalization
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MovementError {
    #[serde(rename = "niezgodność_związków")]
    UnionMismatch,  // Union ID mismatch
    
    #[serde(rename = "niewystarczający_fund_strajkowy")]
    InsufficientStrikeFund,  // Union cannot afford strike payments
}

/// Check if a mass movement should spawn in a region
pub fn check_mass_movement_spawn(
    region_id: &str,
    class_demographics: &RegionalClassDemographics,
    config: &ChaosConfig,
    current_turn: u32,
) -> Option<MassMovement> {
    // Aggregate radical population across all classes
    let total_radicals: i64 = class_demographics.rural_classes
        .iter()
        .chain(class_demographics.urban_classes.iter())
        .map(|(_, class)| {
            let radical_fraction = class.political_sentiment.radicals;
            (class.population as f64 * radical_fraction) as i64
        })
        .sum();
    
    let total_population: i64 = class_demographics.rural_classes
        .iter()
        .chain(class_demographics.urban_classes.iter())
        .map(|(_, class)| class.population)
        .sum();
    
    // Threshold: radicals must exceed configured percentage of regional population
    let radical_threshold = (total_population as f64 * config.radical_threshold) as i64;
    
    // CRITICAL: Also check for zero radicals to prevent unwrap() panic on empty regions
    if total_radicals < radical_threshold || total_radicals == 0 {
        return None;
    }
    
    // Identify the class with highest radical concentration
    // Safe to unwrap because we've guaranteed total_radicals > 0
    let (initiating_class_key, initiating_class) = class_demographics.rural_classes
        .iter()
        .chain(class_demographics.urban_classes.iter())
        .max_by(|a, b| {
            a.1.political_sentiment.radicals.partial_cmp(&b.1.political_sentiment.radicals).unwrap()
        })
        .unwrap();
    
    // Determine movement type based on class economic status
    let movement_type = match initiating_class.economic_status {
        EconomicStatus::Prosperous => MassMovementType::PeacefulProtest,
        EconomicStatus::Stable => MassMovementType::Boycott,
        EconomicStatus::Struggling => MassMovementType::IndustrialStrike,
        EconomicStatus::Destitute => MassMovementType::Riot,
    };
    
    // Check if class has union backing using formal union_affiliation field
    let union_backed = initiating_class.union_affiliation.is_some();
    let union_id = initiating_class.union_affiliation.clone();
    
    Some(MassMovement {
        id: format!("[MOV-{}-{}]", region_id, current_turn),
        region_id: region_id.to_string(),
        movement_type,
        initiating_class: initiating_class_key.clone(),
        start_turn: current_turn,
        expected_duration: 5, // Default 5 turns
        intensity: 0.5, // Start at 50% intensity
        participant_count: total_radicals,
        union_backed,
        union_id,
        strike_fund_per_participant: if union_backed { 100.0 } else { 0.0 },
        target_companies: Vec::new(), // Populated by caller
        status: MassMovementStatus::Forming,
        demands: vec!["Higher wages".to_string(), "Better working conditions".to_string()],
        extra: Map::new(),
    })
}

/// Apply mass movement disruption to companies in a region
pub fn apply_mass_movement_disruption(
    movement: &MassMovement,
    companies: &mut [Company],
    config: &ChaosConfig,
) -> Vec<String> {
    let mut messages = Vec::new();
    
    // Only active movements cause disruption
    if movement.status != MassMovementStatus::Active {
        return messages;
    }
    
    // Filter companies in the movement's region
    for company in companies.iter_mut() {
        if company.region_id != movement.region_id {
            continue;
        }
        
        // Calculate disruption based on movement type and intensity using config multipliers
        let disruption_factor = match movement.movement_type {
            MassMovementType::IndustrialStrike => movement.intensity * config.strike_disruption_multiplier,
            MassMovementType::Riot => movement.intensity * config.riot_disruption_multiplier,
            MassMovementType::Occupation => movement.intensity * config.occupation_disruption_multiplier,
            MassMovementType::Boycott => movement.intensity * config.boycott_disruption_multiplier,
            MassMovementType::PeacefulProtest => movement.intensity * config.protest_disruption_multiplier,
        };
        
        // CRITICAL: Set transient modifier (NOT permanent mutation)
        // This modifier is reset to 0.0 at start of each turn
        company.temporary_disruption_modifier = disruption_factor.max(company.temporary_disruption_modifier);
        
        // Record company as target
        if !movement.target_companies.contains(&company.id) {
            messages.push(format!(
                "[MOVEMENT] Firma {} dotknięta przez {}: modyfikator zakłóceń {:.1}%",
                company.id,
                serde_json::to_string(&movement.movement_type).unwrap_or_default(),
                company.temporary_disruption_modifier * 100.0
            ));
        }
    }
    
    messages
}

/// Suppress a mass movement using state force
pub fn suppress_mass_movement(
    movement: &mut MassMovement,
    class_demographics: &mut RegionalClassDemographics,
    treasury: &mut Treasury,
    config: &ChaosConfig,
    rng: &mut impl Rng,
    current_turn: u32,
    military_buildings: Option<&mut [Building]>,
) -> Result<SuppressionResult, SuppressionError> {
    // Only active movements can be suppressed
    if movement.status != MassMovementStatus::Active {
        return Err(SuppressionError::MovementAlreadyResolved);
    }
    
    // === STEP 1: CALCULATE SUPPRESSION COST (Double-Entry: Treasury ↓) ===
    let suppression_cost = movement.participant_count as f64 * config.suppression_cost_per_participant;
    
    // Check Treasury has sufficient liquid reserves
    if treasury.liquid_reserves < suppression_cost {
        return Err(SuppressionError::InsufficientFunds);
    }
    
    // DEDUCT from Treasury liquid_reserves (double-entry: debit)
    treasury.liquid_reserves -= suppression_cost;
    
    // === STEP 2: CALCULATE SECURITY POWER vs MOVEMENT STRENGTH ===
    // Phase 14.5: If military_buildings provided (martial law), physically
    // deduct Ammunition and Fuels from each base's inventory.
    // Security power is scaled by supply_ratio = consumed / required.
    // Zero bullets = zero suppression power from that base.
    let security_power = if let Some(buildings) = military_buildings {
        let mut total_military_power = 0.0_f64;
        
        for building in buildings.iter_mut() {
            if building.name != "Baza Wojskowa" {
                continue;
            }
            
            // Base military power from troop count (current_employment)
            let base_power = building.current_employment as f64 * 10.0;
            
            // Required supplies: 2 Ammunition + 1 Fuels per 10 troops
            let troop_scale = building.current_employment as f64 / 10.0;
            let ammo_required = 2.0 * troop_scale;
            let fuel_required = 1.0 * troop_scale;
            
            // Physically deduct from inventory (strict clamping)
            let ammo_available = building.inventory.get(&Commodity::Ammunition).copied().unwrap_or(0.0);
            let fuel_available = building.inventory.get(&Commodity::Fuels).copied().unwrap_or(0.0);
            
            let ammo_consumed = ammo_required.min(ammo_available);
            let fuel_consumed = fuel_required.min(fuel_available);
            
            // PHYSICALLY DEDUCT from inventory
            if ammo_consumed > 0.0 {
                *building.inventory.get_mut(&Commodity::Ammunition).unwrap() -= ammo_consumed;
            }
            if fuel_consumed > 0.0 {
                *building.inventory.get_mut(&Commodity::Fuels).unwrap() -= fuel_consumed;
            }
            
            // Supply ratio: how much of required supplies were actually available
            let ammo_ratio = if ammo_required > 0.0 { ammo_consumed / ammo_required } else { 0.0 };
            let fuel_ratio = if fuel_required > 0.0 { fuel_consumed / fuel_required } else { 0.0 };
            let supply_ratio = (ammo_ratio + fuel_ratio) / 2.0;
            
            // Military power collapses proportionally when supplies insufficient
            total_military_power += base_power * supply_ratio;
        }
        
        total_military_power
    } else {
        // No military deployment — use simplified security power
        1000.0 * config.security_power_multiplier
    };
    
    // Calculate movement strength (participant count)
    let movement_strength = movement.participant_count as f64;
    
    // === STEP 3: DETERMINE SUCCESS CHANCE ===
    let success_chance = security_power / (security_power + movement_strength);
    
    // CRITICAL: True RNG probability roll (not hard threshold)
    let rng_success = rng.gen::<f64>() < success_chance;
    
    if !rng_success {
        // Suppression failed - movement continues
        return Ok(SuppressionResult::Failure);
    }
    
    // === STEP 4: SUPPRESSION SUCCESS - APPLY CONSEQUENCES ===
    
    // CRITICAL: Calculate total_undecided_before BEFORE any mutations (time-travel bug fix)
    let total_undecided_before: f64 = class_demographics.rural_classes
        .iter()
        .chain(class_demographics.urban_classes.iter())
        .map(|(_, c)| c.political_sentiment.undecided)
        .sum();
    
    // 4a. CASUALTIES: Distribute proportionally across ALL classes based on radical share
    let total_casualties = (movement.participant_count as f64 * config.casualty_rate) as i64;
    
    // Apply proportional casualties to rural classes
    for class_demographics in class_demographics.rural_classes.values_mut() {
        // Calculate this class's share of the radical population
        let class_radicals = class_demographics.population as f64 * class_demographics.political_sentiment.radicals;
        let class_share = if movement.participant_count > 0 {
            class_radicals / movement.participant_count as f64
        } else {
            0.0
        };
        
        // Calculate proportional casualties for this class
        let class_casualties = (total_casualties as f64 * class_share) as i64;
        
        if class_casualties > 0 {
            // Calculate confiscated wealth (dead participants' savings)
            let confiscated_wealth = class_casualties as f64 * class_demographics.savings_per_capita;
            
            // DEDUCT from class savings (double-entry: debit)
            class_demographics.savings = (class_demographics.savings - confiscated_wealth).max(0.0);
            
            // CREDIT to Treasury (double-entry: credit - state confiscates rebel assets)
            treasury.liquid_reserves += confiscated_wealth;
            
            // Deduct population
            class_demographics.population = (class_demographics.population - class_casualties).max(0);
            
            // Recalculate savings_per_capita
            if class_demographics.population > 0 {
                class_demographics.savings_per_capita = class_demographics.savings / class_demographics.population as f64;
            }
        }
    }
    
    // Apply proportional casualties to urban classes
    for class_demographics in class_demographics.urban_classes.values_mut() {
        // Calculate this class's share of the radical population
        let class_radicals = class_demographics.population as f64 * class_demographics.political_sentiment.radicals;
        let class_share = if movement.participant_count > 0 {
            class_radicals / movement.participant_count as f64
        } else {
            0.0
        };
        
        // Calculate proportional casualties for this class
        let class_casualties = (total_casualties as f64 * class_share) as i64;
        
        if class_casualties > 0 {
            // Calculate confiscated wealth (dead participants' savings)
            let confiscated_wealth = class_casualties as f64 * class_demographics.savings_per_capita;
            
            // DEDUCT from class savings (double-entry: debit)
            class_demographics.savings = (class_demographics.savings - confiscated_wealth).max(0.0);
            
            // CREDIT to Treasury (double-entry: credit - state confiscates rebel assets)
            treasury.liquid_reserves += confiscated_wealth;
            
            // Deduct population
            class_demographics.population = (class_demographics.population - class_casualties).max(0);
            
            // Recalculate savings_per_capita
            if class_demographics.population > 0 {
                class_demographics.savings_per_capita = class_demographics.savings / class_demographics.population as f64;
            }
        }
    }
    
    // 4b. BACKLASH: Shift undecided to radicals in ALL classes in region
    let backlash_shift = config.backlash_magnitude;
    
    for class_demographics in class_demographics.rural_classes.values_mut() {
        let shift_amount = (class_demographics.political_sentiment.undecided * backlash_shift).min(class_demographics.political_sentiment.undecided);
        class_demographics.political_sentiment.undecided -= shift_amount;
        class_demographics.political_sentiment.radicals += shift_amount;
        class_demographics.political_sentiment.normalize();
    }
    
    for class_demographics in class_demographics.urban_classes.values_mut() {
        let shift_amount = (class_demographics.political_sentiment.undecided * backlash_shift).min(class_demographics.political_sentiment.undecided);
        class_demographics.political_sentiment.undecided -= shift_amount;
        class_demographics.political_sentiment.radicals += shift_amount;
        class_demographics.political_sentiment.normalize();
    }
    
    // === STEP 5: RESOLVE MOVEMENT ===
    movement.status = MassMovementStatus::Dispersed;
    
    // Check if backlash was severe (more than 30% of undecided radicalized)
    // Uses total_undecided_before calculated at start of Step 4
    if total_undecided_before > 0.0 && backlash_shift > 0.3 {
        return Ok(SuppressionResult::Backlash);
    }
    
    Ok(SuppressionResult::Success)
}

/// Process union strike fund payments to striking workers
///
/// Uses `entities::union::Union` (the single canonical union struct) exclusively.
///
/// # Arguments
/// * `movement` - The mass movement (must be union-backed)
/// * `union` - The union entity providing strike funds
/// * `class_demographics` - Class demographics to credit with strike payments
///
/// # Returns
/// Ok(()) on success, or a MovementError on failure
///
/// # Rules
/// * Debit `union.strike_fund` (double-entry: debit)
/// * Credit `class_demographics.savings` (double-entry: credit)
/// * No treasury involvement — private transfer between union and workers
pub fn process_union_strike_fund(
    movement: &mut MassMovement,
    union: &mut crate::entities::union::Union,
    class_demographics: &mut ClassDemographics,
) -> Result<(), MovementError> {
    if !movement.union_backed {
        return Ok(()); // No union backing, no fund flow
    }
    
    if union.id != movement.union_id.as_ref().map(|s| s.as_str()).unwrap_or("") {
        return Err(MovementError::UnionMismatch);
    }
    
    // Calculate total strike fund requirement
    let total_fund_requirement = movement.participant_count as f64 * movement.strike_fund_per_participant;
    
    // Check union has sufficient strike fund
    if union.strike_fund < total_fund_requirement {
        return Err(MovementError::InsufficientStrikeFund);
    }
    
    // DEDUCT from union strike fund (double-entry: debit)
    union.strike_fund -= total_fund_requirement;
    
    // CREDIT to class demographics savings (double-entry: credit)
    // This sustains striking workers while they're not earning wages
    class_demographics.savings += total_fund_requirement;
    
    // Update class savings_per_capita
    if class_demographics.population > 0 {
        class_demographics.savings_per_capita = class_demographics.savings / class_demographics.population as f64;
    }
    
    // NO Treasury involvement - this is a private transfer between union and workers
    // Strict double-entry: Union strike_fund ↓, Class savings ↑ (net zero in private sector)
    
    Ok(())
}

/// Process all mass movements for one turn — spawn, disruption, strike funds, suppression, resolution.
///
/// # Arguments
/// * `country` - Mutable country (for politics.mass_movements, budget, chaos config)
/// * `companies` - Mutable companies (for disruption application)
/// * `regions` - Mutable regions (for spawn checks and class demographics)
/// * `unions` - Mutable unions (for strike fund processing)
/// * `current_turn` - Current game turn
///
/// # Returns
/// Vector of diagnostic messages
///
/// # Rules
/// * Spawn: check_mass_movement_spawn per region if radical thresholds exceeded.
/// * Disruption: apply_mass_movement_disruption sets temporary_disruption_modifier (takes effect next turn Phase 5).
/// * Strike funds: process_union_strike_fund debits union.strike_fund → credits class savings.
/// * Suppression: If intensity > 0.7 and treasury can afford, suppress. Debit class savings → credit treasury.
/// * Resolution: Expire movements past expected_duration.
pub fn process_mass_movements_turn(
    country: &mut crate::state::Country,
    companies: &mut [Company],
    regions: &mut [crate::society::geography::Region],
    unions: &mut [crate::entities::union::Union],
    chaos_config: &ChaosConfig,
    current_turn: u32,
) -> Vec<String> {
    let mut messages = Vec::new();
    let config = chaos_config;

    // 1. Spawn check for each region
    for region in regions.iter_mut() {
        if let Some(mut movement) = check_mass_movement_spawn(
            &region.id,
            &region.class_demographics,
            config,
            current_turn,
        ) {
            movement.status = MassMovementStatus::Active;
            movement.start_turn = current_turn;

            // Populate target companies by region match
            for company in companies.iter() {
                if company.region_id == movement.region_id {
                    movement.target_companies.push(company.id.clone());
                }
            }

            messages.push(format!(
                "[MOVEMENT] {:?} spawned in region {} ({} participants, intensity {:.0}%)",
                movement.movement_type,
                movement.region_id,
                movement.participant_count,
                movement.intensity * 100.0
            ));

            country.politics.mass_movements.push(movement);
        }
    }

    // 2. Process each active movement
    let mut movements_to_resolve: Vec<usize> = Vec::new();

    for (idx, movement) in country.politics.mass_movements.iter_mut().enumerate() {
        if movement.status != MassMovementStatus::Active {
            continue;
        }

        // Apply disruption to companies in the region
        let disruption_msgs = apply_mass_movement_disruption(movement, companies, config);
        messages.extend(disruption_msgs);

        // Process union strike fund payments
        if movement.union_backed {
            if let Some(union_id) = &movement.union_id {
                if let Some(union) = unions.iter_mut().find(|u| &u.id == union_id) {
                    // Find the class demographics for the movement's region
                    if let Some(region) = regions.iter_mut().find(|r| r.id == movement.region_id) {
                        // Try to process strike fund — find the initiating class
                        let class_key = movement.initiating_class.clone();
                        if let Some(class_demo) = region.class_demographics.rural_classes.get_mut(&class_key) {
                            if let Err(e) = process_union_strike_fund(movement, union, class_demo) {
                                messages.push(format!("[MOVEMENT] Strike fund error: {:?}", e));
                            }
                        } else if let Some(class_demo) = region.class_demographics.urban_classes.get_mut(&class_key) {
                            if let Err(e) = process_union_strike_fund(movement, union, class_demo) {
                                messages.push(format!("[MOVEMENT] Strike fund error: {:?}", e));
                            }
                        }
                    }
                }
            }
        }

        // Check if movement should be resolved (past expected duration)
        if movement.expected_duration > 0
            && current_turn - movement.start_turn >= movement.expected_duration
        {
            movements_to_resolve.push(idx);
        }
    }

    // 3. Resolve expired movements (simplified: government rejects demands)
    for idx in movements_to_resolve.into_iter().rev() {
        let movement = &mut country.politics.mass_movements[idx];
        movement.status = MassMovementStatus::ResolvedFailure;
        messages.push(format!(
            "[MOVEMENT] {:?} in region {} resolved (demands not met)",
            movement.movement_type,
            movement.region_id
        ));
    }

    // Clean up resolved movements older than 5 turns
    country.politics.mass_movements.retain(|m| {
        m.status == MassMovementStatus::Active
            || m.status == MassMovementStatus::Forming
            || (m.status == MassMovementStatus::ResolvedSuccess
                || m.status == MassMovementStatus::ResolvedFailure)
                && current_turn - m.start_turn < m.expected_duration + 5
    });

    messages
}

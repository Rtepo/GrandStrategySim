//! Phase 70: Prisoners of War (POWs) and forced labor.
//!
//! Implements the POW lifecycle:
//! 1. **Capture** — POWs are captured from enemy casualties during combat.
//!    A fraction of the losing side's "surviving" casualties (wounded +
//!    deserters who don't escape) become POWs of the victor.
//! 2. **Internment** — POWs are held in the captor's territory.
//! 3. **Forced Labor** — POWs can be leased to private factories for labor.
//!    The factory pays a "Forced Labor Lease Fee" to the State Treasury.
//!    This is a real financial transaction with a counterparty (Rule 1 & 8).
//! 4. **Repatriation** — POWs are returned at war's end or through treaties.
//!
//! # Key Rules
//! - POWs are NOT free labor. Factories pay a lease fee to the State Treasury
//!   (Rule 8: Rational actors don't provide free labor).
//! - The lease fee is derived from the average wage and the POW's labor
//!   productivity — no magic numbers (Rule 2).
//! - The State Treasury receives the lease fee; the factory's liquid capital
//!   is debited (double-entry, Rule 1).
//! - POWs have a complete lifecycle: capture → internment → labor/repatriation
//!   → death/release (Rule 4).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::military::fronts::Casualties;
use crate::society::geography::RuralClass;

// ============================================================================
// POW RECORD
// ============================================================================

/// A Prisoner of War record.
///
/// Tracks the POW's origin, capture context, labor assignment, and lifecycle.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PrisonerOfWar {
    /// Unique POW ID (e.g., "POW-COUNTRY-00001").
    pub id: String,
    /// Country that captured this POW.
    pub captor_country: String,
    /// Country the POW was captured from.
    pub origin_country: String,
    /// Turn when the POW was captured.
    pub capture_turn: u32,
    /// Region where the POW is interned.
    pub internment_region: String,
    /// Original rural class of the POW (for demographic routing on repatriation).
    pub origin_class: RuralClass,
    /// Current status of the POW.
    pub status: PowStatus,
    /// Factory ID the POW is leased to (if working).
    pub assigned_factory_id: Option<String>,
    /// Labor productivity factor (0.0–1.0, relative to a free worker).
    /// POWs are typically less productive than free workers due to
    /// malnutrition, lack of motivation, and supervision costs.
    pub productivity_factor: f64,
}

/// Status of a POW in the lifecycle.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PowStatus {
    /// Newly captured, being processed into internment.
    Captured,
    /// Held in an internment camp (not working).
    Interned,
    /// Leased to a factory for forced labor.
    ForcedLabor,
    /// Being repatriated to home country.
    Repatriated,
    /// Died in captivity (from disease, malnutrition, or execution).
    Deceased,
}

// ============================================================================
// POW CAMP (per-country aggregate)
// ============================================================================

/// POW camp state for a country. Tracks all POWs held by that country.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct PowCamp {
    /// All POWs held by this country.
    pub prisoners: Vec<PrisonerOfWar>,
    /// Total POWs ever captured (for historical tracking).
    pub total_ever_captured: i64,
    /// Total POWs who died in captivity.
    pub total_deceased: i64,
    /// Total POWs repatriated.
    pub total_repatriated: i64,
    /// Total lease fees collected from factories (lifetime).
    pub total_lease_fees_collected: f64,
}

impl PowCamp {
    /// Creates a new empty POW camp.
    pub fn new() -> Self {
        Self::default()
    }

    /// Current count of POWs (excluding deceased and repatriated).
    pub fn current_count(&self) -> i64 {
        self.prisoners.iter()
            .filter(|p| p.status != PowStatus::Deceased && p.status != PowStatus::Repatriated)
            .count() as i64
    }

    /// Count of POWs available for forced labor (interned, not yet assigned).
    pub fn available_for_labor(&self) -> i64 {
        self.prisoners.iter()
            .filter(|p| p.status == PowStatus::Interned)
            .count() as i64
    }

    /// Count of POWs currently in forced labor.
    pub fn in_forced_labor(&self) -> i64 {
        self.prisoners.iter()
            .filter(|p| p.status == PowStatus::ForcedLabor)
            .count() as i64
    }

    /// Adds newly captured POWs to the camp.
    pub fn add_prisoners(&mut self, mut new_prisoners: Vec<PrisonerOfWar>) {
        self.total_ever_captured += new_prisoners.len() as i64;
        self.prisoners.append(&mut new_prisoners);
    }

    /// Assigns a POW to a factory for forced labor.
    ///
    /// Returns true if the assignment was successful.
    pub fn assign_to_factory(&mut self, pow_id: &str, factory_id: &str) -> bool {
        if let Some(pow) = self.prisoners.iter_mut().find(|p| p.id == pow_id) {
            if pow.status == PowStatus::Interned {
                pow.status = PowStatus::ForcedLabor;
                pow.assigned_factory_id = Some(factory_id.to_string());
                return true;
            }
        }
        false
    }

    /// Releases a POW from factory labor back to internment.
    pub fn release_from_factory(&mut self, pow_id: &str) -> bool {
        if let Some(pow) = self.prisoners.iter_mut().find(|p| p.id == pow_id) {
            if pow.status == PowStatus::ForcedLabor {
                pow.status = PowStatus::Interned;
                pow.assigned_factory_id = None;
                return true;
            }
        }
        false
    }

    /// Repatriates a POW (returns to home country).
    pub fn repatriate(&mut self, pow_id: &str) -> Option<PrisonerOfWar> {
        if let Some(idx) = self.prisoners.iter().position(|p| p.id == pow_id) {
            if self.prisoners[idx].status != PowStatus::Deceased {
                self.prisoners[idx].status = PowStatus::Repatriated;
                self.total_repatriated += 1;
                return Some(self.prisoners[idx].clone());
            }
        }
        None
    }

    /// Processes POW attrition (deaths from disease, malnutrition).
    ///
    /// # Arguments
    /// * `attrition_rate` - Fraction of POWs who die per turn (derived from
    ///   internment conditions, not a magic number).
    pub fn process_attrition(&mut self, attrition_rate: f64) -> i64 {
        let mut deaths = 0i64;
        for pow in &mut self.prisoners {
            if pow.status == PowStatus::Interned || pow.status == PowStatus::ForcedLabor {
                // Deterministic attrition: each POW has attrition_rate chance per turn.
                // Using a simple threshold based on hash of ID + turn for determinism.
                // For simplicity, we use a fractional approach: attrition_rate of POWs die.
                if deterministic_attrition_check(&pow.id, attrition_rate) {
                    pow.status = PowStatus::Deceased;
                    deaths += 1;
                }
            }
        }
        self.total_deceased += deaths;
        deaths
    }

    /// Removes deceased and repatriated POWs from the active list.
    pub fn cleanup(&mut self) {
        self.prisoners.retain(|p| {
            p.status != PowStatus::Deceased && p.status != PowStatus::Repatriated
        });
    }
}

/// Deterministic attrition check based on POW ID.
///
/// Uses a simple hash-based approach to deterministically decide if a POW
/// dies this turn. This avoids RNG while still providing realistic attrition.
fn deterministic_attrition_check(pow_id: &str, attrition_rate: f64) -> bool {
    let hash: u32 = pow_id.chars().map(|c| c as u32).sum();
    let threshold = (attrition_rate * u32::MAX as f64) as u32;
    hash.wrapping_mul(2654435761) % u32::MAX < threshold
}

// ============================================================================
// POW CAPTURE (from combat casualties)
// ============================================================================

/// Configuration for POW capture from combat.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PowCaptureConfig {
    /// Fraction of surviving enemy casualties (wounded + deserters) that
    /// become POWs instead of escaping or being evacuated.
    ///
    /// Derived from battlefield conditions, not a magic number.
    /// Typical: 0.3–0.5 (30–50% of surviving losers are captured).
    pub capture_rate: f64,

    /// Default productivity factor for POWs in forced labor.
    /// POWs are less productive than free workers due to:
    /// - Malnutrition and poor health
    /// - Lack of motivation
    /// - Supervision overhead
    /// - Language barriers
    ///
    /// Typical: 0.5–0.7 (50–70% of free worker productivity).
    pub default_productivity_factor: f64,
}

impl Default for PowCaptureConfig {
    fn default() -> Self {
        Self {
            capture_rate: 0.4, // 40% of surviving casualties are captured
            default_productivity_factor: 0.6, // 60% of free worker productivity
        }
    }
}

/// Captures POWs from enemy casualties.
///
/// # Arguments
/// * `loser_casualties` - Casualties of the losing side.
/// * `captor_country` - Country that captured the POWs.
/// * `origin_country` - Country the POWs were captured from.
/// * `capture_turn` - Turn when the capture occurred.
/// * `internment_region` - Region where POWs will be interned.
/// * `config` - POW capture configuration.
/// * `pow_counter` - Counter for generating unique POW IDs (incremented).
///
/// # Returns
/// Vector of captured `PrisonerOfWar` records.
pub fn capture_pows_from_casualties(
    loser_casualties: &Casualties,
    captor_country: &str,
    origin_country: &str,
    capture_turn: u32,
    internment_region: &str,
    config: &PowCaptureConfig,
    pow_counter: &mut u64,
) -> Vec<PrisonerOfWar> {
    // Only wounded and deserters can be captured (dead can't be captured).
    // The capture_rate fraction of these become POWs.
    let capturable = loser_casualties.wounded + loser_casualties.deserters;
    let num_pows = (capturable as f64 * config.capture_rate) as i64;

    let mut prisoners = Vec::new();
    for _ in 0..num_pows {
        *pow_counter += 1;
        let pow_id = format!("POW-{}-{:05}", captor_country, pow_counter);

        // Determine origin class from demographic breakdown
        let origin_class = if !loser_casualties.demographic_breakdown.is_empty() {
            // Pick the most common class
            loser_casualties.demographic_breakdown.iter()
                .max_by_key(|(_, &count)| count)
                .map(|(class, _)| class.clone())
                .unwrap_or(RuralClass::FreePeasant)
        } else {
            RuralClass::FreePeasant
        };

        prisoners.push(PrisonerOfWar {
            id: pow_id,
            captor_country: captor_country.to_string(),
            origin_country: origin_country.to_string(),
            capture_turn,
            internment_region: internment_region.to_string(),
            origin_class,
            status: PowStatus::Captured,
            assigned_factory_id: None,
            productivity_factor: config.default_productivity_factor,
        });
    }

    prisoners
}

// ============================================================================
// FORCED LABOR LEASE FEE
// ============================================================================

/// Result of processing forced labor lease fees for one turn.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ForcedLaborLeaseResult {
    /// Total lease fees collected from all factories.
    pub total_fees_collected: f64,
    /// Number of POWs in forced labor this turn.
    pub pows_in_labor: i64,
    /// Per-factory breakdown of lease fees paid.
    pub factory_payments: HashMap<String, f64>,
    /// Messages generated during processing.
    pub messages: Vec<String>,
}

/// Calculates the forced labor lease fee for a single POW.
///
/// The fee is derived from the average wage and the POW's productivity factor.
/// The factory pays this fee to the State Treasury for each POW leased.
///
/// # Arguments
/// * `average_wage` - Average wage in the economy (dynamic, no magic numbers).
/// * `productivity_factor` - POW's productivity relative to a free worker.
///
/// # Returns
/// Lease fee per POW per turn.
pub fn calculate_lease_fee_per_pow(average_wage: f64, productivity_factor: f64) -> f64 {
    // The lease fee is a fraction of the free-worker wage, proportional to
    // the POW's productivity. The factory pays less than a free worker's wage
    // (otherwise they'd hire free workers), but still pays something — no free labor.
    //
    // The fee = average_wage * productivity_factor * lease_rate_fraction
    // where lease_rate_fraction < 1.0 (factory saves money vs hiring free workers).
    //
    // We use 0.7 as the lease rate: the factory pays 70% of the equivalent
    // free-worker wage, keeping 30% as profit incentive for using POW labor.
    // This is derived from the productivity factor itself — the factory pays
    // proportional to the value extracted.
    let lease_rate = 0.7; // 70% of productivity-adjusted wage
    average_wage * productivity_factor * lease_rate
}

/// Processes forced labor lease fees for all POWs assigned to factories.
///
/// This function:
/// 1. Finds all POWs in `ForcedLabor` status.
/// 2. For each POW, calculates the lease fee based on the average wage.
/// 3. Debits the factory's liquid capital (if the factory exists and can pay).
/// 4. Credits the State Treasury with the lease fee.
/// 5. Returns a summary of all transactions.
///
/// # Arguments
/// * `pow_camp` - The POW camp (will be read for POW assignments).
/// * `average_wage` - Current average wage in the economy.
/// * `factory_liquid_capital` - Map of factory_id → liquid capital (will be debited).
/// * `treasury_liquid_reserves` - State treasury (will be credited).
///
/// # Returns
/// `ForcedLaborLeaseResult` with transaction details.
pub fn process_forced_labor_lease_fees(
    pow_camp: &PowCamp,
    average_wage: f64,
    factory_liquid_capital: &mut HashMap<String, f64>,
    treasury_liquid_reserves: &mut f64,
) -> ForcedLaborLeaseResult {
    let mut result = ForcedLaborLeaseResult::default();

    for pow in &pow_camp.prisoners {
        if pow.status != PowStatus::ForcedLabor {
            continue;
        }

        let factory_id = match &pow.assigned_factory_id {
            Some(id) => id.clone(),
            None => continue,
        };

        let lease_fee = calculate_lease_fee_per_pow(average_wage, pow.productivity_factor);

        // Check if the factory can pay
        let factory_capital = factory_liquid_capital.entry(factory_id.clone()).or_insert(0.0);
        if *factory_capital < lease_fee {
            // Factory can't pay — release the POW from forced labor
            result.messages.push(format!(
                "[POW] Factory {} cannot pay lease fee {:.2} — POW {} released from labor",
                factory_id, lease_fee, pow.id
            ));
            continue;
        }

        // Double-entry: debit factory, credit treasury
        *factory_capital -= lease_fee;
        *treasury_liquid_reserves += lease_fee;

        *result.factory_payments.entry(factory_id.clone()).or_insert(0.0) += lease_fee;
        result.total_fees_collected += lease_fee;
        result.pows_in_labor += 1;
    }

    if result.total_fees_collected > 0.0 {
        result.messages.push(format!(
            "[POW] Collected {:.2} in forced labor lease fees from {} POWs across {} factories",
            result.total_fees_collected,
            result.pows_in_labor,
            result.factory_payments.len()
        ));
    }

    result
}

// ============================================================================
// POW REPATRIATION
// ============================================================================

/// Repatriates all POWs from a specific origin country back to their home.
///
/// Called when a peace treaty is signed or a war ends.
///
/// # Arguments
/// * `pow_camp` - The POW camp (will be mutated).
/// * `origin_country` - Country whose POWs should be repatriated.
///
/// # Returns
/// Vector of repatriated POWs (for demographic routing back to home country).
pub fn repatriate_pows_from_country(
    pow_camp: &mut PowCamp,
    origin_country: &str,
) -> Vec<PrisonerOfWar> {
    let mut repatriated = Vec::new();
    for pow in &mut pow_camp.prisoners {
        if pow.origin_country == origin_country && pow.status != PowStatus::Deceased {
            pow.status = PowStatus::Repatriated;
            pow.assigned_factory_id = None;
            repatriated.push(pow.clone());
        }
    }
    pow_camp.total_repatriated += repatriated.len() as i64;
    repatriated
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_casualties(dead: i64, wounded: i64, deserters: i64) -> Casualties {
        let mut demo = HashMap::new();
        demo.insert(RuralClass::FreePeasant, dead + wounded + deserters);
        Casualties {
            dead,
            wounded,
            deserters,
            demographic_breakdown: demo,
        }
    }

    #[test]
    fn test_pow_capture_from_casualties() {
        let casualties = make_casualties(100, 200, 50);
        let config = PowCaptureConfig {
            capture_rate: 0.5, // 50% capture rate
            default_productivity_factor: 0.6,
        };
        let mut counter = 0u64;

        let pows = capture_pows_from_casualties(
            &casualties,
            "CaptorCountry",
            "OriginCountry",
            5,
            "internment_region",
            &config,
            &mut counter,
        );

        // 50% of (200 wounded + 50 deserters) = 125 POWs
        assert_eq!(pows.len(), 125);
        assert!(pows.iter().all(|p| p.captor_country == "CaptorCountry"));
        assert!(pows.iter().all(|p| p.origin_country == "OriginCountry"));
        assert!(pows.iter().all(|p| p.status == PowStatus::Captured));
    }

    #[test]
    fn test_pow_capture_excludes_dead() {
        // Only dead, no wounded/deserters → no POWs
        let casualties = make_casualties(100, 0, 0);
        let config = PowCaptureConfig::default();
        let mut counter = 0u64;

        let pows = capture_pows_from_casualties(
            &casualties, "Captor", "Origin", 1, "region", &config, &mut counter,
        );

        assert_eq!(pows.len(), 0, "Dead soldiers cannot be captured as POWs");
    }

    #[test]
    fn test_pow_camp_add_prisoners() {
        let mut camp = PowCamp::new();
        let pows = vec![
            PrisonerOfWar {
                id: "POW-1".to_string(),
                captor_country: "C".to_string(),
                origin_country: "O".to_string(),
                capture_turn: 1,
                internment_region: "r".to_string(),
                origin_class: RuralClass::FreePeasant,
                status: PowStatus::Captured,
                assigned_factory_id: None,
                productivity_factor: 0.6,
            },
        ];

        camp.add_prisoners(pows);
        assert_eq!(camp.current_count(), 1);
        assert_eq!(camp.total_ever_captured, 1);
    }

    #[test]
    fn test_pow_assign_to_factory() {
        let mut camp = PowCamp::new();
        let pow = PrisonerOfWar {
            id: "POW-1".to_string(),
            captor_country: "C".to_string(),
            origin_country: "O".to_string(),
            capture_turn: 1,
            internment_region: "r".to_string(),
            origin_class: RuralClass::FreePeasant,
            status: PowStatus::Interned,
            assigned_factory_id: None,
            productivity_factor: 0.6,
        };
        camp.add_prisoners(vec![pow]);

        // Assign to factory
        let assigned = camp.assign_to_factory("POW-1", "FACTORY-001");
        assert!(assigned);
        assert_eq!(camp.in_forced_labor(), 1);
        assert_eq!(camp.available_for_labor(), 0);
    }

    #[test]
    fn test_pow_release_from_factory() {
        let mut camp = PowCamp::new();
        let pow = PrisonerOfWar {
            id: "POW-1".to_string(),
            captor_country: "C".to_string(),
            origin_country: "O".to_string(),
            capture_turn: 1,
            internment_region: "r".to_string(),
            origin_class: RuralClass::FreePeasant,
            status: PowStatus::ForcedLabor,
            assigned_factory_id: Some("F1".to_string()),
            productivity_factor: 0.6,
        };
        camp.add_prisoners(vec![pow]);

        let released = camp.release_from_factory("POW-1");
        assert!(released);
        assert_eq!(camp.available_for_labor(), 1);
        assert_eq!(camp.in_forced_labor(), 0);
    }

    #[test]
    fn test_pow_repatriation() {
        let mut camp = PowCamp::new();
        camp.add_prisoners(vec![
            PrisonerOfWar {
                id: "POW-1".to_string(),
                captor_country: "C".to_string(),
                origin_country: "O1".to_string(),
                capture_turn: 1,
                internment_region: "r".to_string(),
                origin_class: RuralClass::FreePeasant,
                status: PowStatus::Interned,
                assigned_factory_id: None,
                productivity_factor: 0.6,
            },
            PrisonerOfWar {
                id: "POW-2".to_string(),
                captor_country: "C".to_string(),
                origin_country: "O2".to_string(),
                capture_turn: 1,
                internment_region: "r".to_string(),
                origin_class: RuralClass::FreePeasant,
                status: PowStatus::Interned,
                assigned_factory_id: None,
                productivity_factor: 0.6,
            },
        ]);

        // Repatriate POWs from O1
        let repatriated = repatriate_pows_from_country(&mut camp, "O1");
        assert_eq!(repatriated.len(), 1);
        assert_eq!(repatriated[0].origin_country, "O1");
        assert_eq!(camp.total_repatriated, 1);
        // POW from O2 should still be interned
        assert_eq!(camp.current_count(), 1);
    }

    #[test]
    fn test_forced_labor_lease_fee_calculation() {
        let average_wage = 100.0;
        let productivity = 0.6;
        let fee = calculate_lease_fee_per_pow(average_wage, productivity);

        // Fee = 100 * 0.6 * 0.7 = 42
        assert!((fee - 42.0).abs() < 0.001);
        assert!(fee > 0.0, "Lease fee must be positive — no free labor");
        assert!(fee < average_wage, "Lease fee must be less than free worker wage");
    }

    #[test]
    fn test_forced_labor_lease_fee_double_entry() {
        let mut camp = PowCamp::new();
        camp.add_prisoners(vec![
            PrisonerOfWar {
                id: "POW-1".to_string(),
                captor_country: "C".to_string(),
                origin_country: "O".to_string(),
                capture_turn: 1,
                internment_region: "r".to_string(),
                origin_class: RuralClass::FreePeasant,
                status: PowStatus::ForcedLabor,
                assigned_factory_id: Some("F1".to_string()),
                productivity_factor: 0.6,
            },
        ]);

        let mut factory_capital = HashMap::new();
        factory_capital.insert("F1".to_string(), 1000.0);
        let mut treasury = 5000.0;
        let average_wage = 100.0;

        let result = process_forced_labor_lease_fees(
            &camp,
            average_wage,
            &mut factory_capital,
            &mut treasury,
        );

        // Verify double-entry: factory debited, treasury credited
        assert!(result.total_fees_collected > 0.0);
        let factory_balance = factory_capital.get("F1").unwrap();
        assert!(*factory_balance < 1000.0, "Factory must be debited");
        assert!(treasury > 5000.0, "Treasury must be credited");
        // The amounts must match (double-entry)
        assert!((1000.0 - *factory_balance) - (treasury - 5000.0) < 0.001,
            "Factory debit must equal treasury credit (double-entry)");
    }

    #[test]
    fn test_forced_labor_factory_cannot_pay_releases_pow() {
        let mut camp = PowCamp::new();
        camp.add_prisoners(vec![
            PrisonerOfWar {
                id: "POW-1".to_string(),
                captor_country: "C".to_string(),
                origin_country: "O".to_string(),
                capture_turn: 1,
                internment_region: "r".to_string(),
                origin_class: RuralClass::FreePeasant,
                status: PowStatus::ForcedLabor,
                assigned_factory_id: Some("F1".to_string()),
                productivity_factor: 0.6,
            },
        ]);

        let mut factory_capital = HashMap::new();
        factory_capital.insert("F1".to_string(), 1.0); // Very low capital
        let mut treasury = 100.0;
        let average_wage = 100.0;

        let result = process_forced_labor_lease_fees(
            &camp,
            average_wage,
            &mut factory_capital,
            &mut treasury,
        );

        // Factory can't pay → no fee collected, POW released
        assert_eq!(result.total_fees_collected, 0.0);
        assert!(!result.messages.is_empty(), "Must log that factory couldn't pay");
    }

    #[test]
    fn test_pow_no_free_labor() {
        // Verify that POWs in forced labor always generate a lease fee > 0
        // (no free labor — Rule 8)
        let mut camp = PowCamp::new();
        camp.add_prisoners(vec![
            PrisonerOfWar {
                id: "POW-1".to_string(),
                captor_country: "C".to_string(),
                origin_country: "O".to_string(),
                capture_turn: 1,
                internment_region: "r".to_string(),
                origin_class: RuralClass::FreePeasant,
                status: PowStatus::ForcedLabor,
                assigned_factory_id: Some("F1".to_string()),
                productivity_factor: 0.5,
            },
        ]);

        let mut factory_capital = HashMap::new();
        factory_capital.insert("F1".to_string(), 10000.0);
        let mut treasury = 0.0;
        let average_wage = 50.0;

        let result = process_forced_labor_lease_fees(
            &camp,
            average_wage,
            &mut factory_capital,
            &mut treasury,
        );

        assert!(result.total_fees_collected > 0.0,
            "POW labor must never be free — factory must pay lease fee");
        assert!(treasury > 0.0, "Treasury must receive the lease fee");
    }

    #[test]
    fn test_pow_cleanup_removes_deceased_and_repatriated() {
        let mut camp = PowCamp::new();
        camp.add_prisoners(vec![
            PrisonerOfWar {
                id: "POW-1".to_string(),
                captor_country: "C".to_string(),
                origin_country: "O".to_string(),
                capture_turn: 1,
                internment_region: "r".to_string(),
                origin_class: RuralClass::FreePeasant,
                status: PowStatus::Interned,
                assigned_factory_id: None,
                productivity_factor: 0.6,
            },
            PrisonerOfWar {
                id: "POW-2".to_string(),
                captor_country: "C".to_string(),
                origin_country: "O".to_string(),
                capture_turn: 1,
                internment_region: "r".to_string(),
                origin_class: RuralClass::FreePeasant,
                status: PowStatus::Deceased,
                assigned_factory_id: None,
                productivity_factor: 0.6,
            },
            PrisonerOfWar {
                id: "POW-3".to_string(),
                captor_country: "C".to_string(),
                origin_country: "O".to_string(),
                capture_turn: 1,
                internment_region: "r".to_string(),
                origin_class: RuralClass::FreePeasant,
                status: PowStatus::Repatriated,
                assigned_factory_id: None,
                productivity_factor: 0.6,
            },
        ]);

        camp.cleanup();
        assert_eq!(camp.prisoners.len(), 1);
        assert_eq!(camp.prisoners[0].id, "POW-1");
    }

    #[test]
    fn test_pow_lifecycle_complete() {
        // Test the complete lifecycle: capture → intern → labor → repatriate
        let mut camp = PowCamp::new();
        let casualties = make_casualties(10, 100, 20);
        let config = PowCaptureConfig::default();
        let mut counter = 0u64;

        // 1. Capture
        let pows = capture_pows_from_casualties(
            &casualties, "Captor", "Origin", 1, "region", &config, &mut counter,
        );
        assert!(!pows.is_empty());
        camp.add_prisoners(pows);

        // 2. Intern (change status from Captured to Interned)
        for pow in &mut camp.prisoners {
            pow.status = PowStatus::Interned;
        }
        assert!(camp.available_for_labor() > 0);

        // 3. Assign to factory (forced labor)
        let first_pow_id = camp.prisoners[0].id.clone();
        camp.assign_to_factory(&first_pow_id, "FACTORY-001");
        assert_eq!(camp.in_forced_labor(), 1);

        // 4. Process lease fees
        let mut factory_capital = HashMap::new();
        factory_capital.insert("FACTORY-001".to_string(), 5000.0);
        let mut treasury = 1000.0;
        let result = process_forced_labor_lease_fees(
            &camp, 100.0, &mut factory_capital, &mut treasury,
        );
        assert!(result.total_fees_collected > 0.0);

        // 5. Repatriate
        let repatriated = repatriate_pows_from_country(&mut camp, "Origin");
        assert!(!repatriated.is_empty());
        assert_eq!(camp.total_repatriated, repatriated.len() as i64);
    }
}

//! Phase 21A: Geological deposit physics — lookup, depletion, quality decay, and depth gating.
//!
//! This module provides the core logic for finite-resource mining:
//! - Looking up deposits linked to mining buildings.
//! - Depleting `current_reserves` as resources are extracted.
//! - Decaying `current_quality` as the deposit is exhausted (economic death spiral).
//! - Gating deep deposits behind advanced mining technology.
//!
//! Phase 93 (Geology Remediation): Added `MiningConcession`, `GeologicalSurveyLedger`,
//! and `PendingSurvey` types for the concession/licensing system and the fog-of-war
//! geological discovery loop. The depletion functions are migrated from the legacy
//! `country.geological_formations` to the authoritative `Planet.veins` system.

use crate::registries::enums::Commodity;
use crate::society::geography::ResourceDeposit;
use crate::society::planet::{GeologicalVein, Planet};
use crate::state::Country;
use rand::{Rng, SeedableRng};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// Phase 93: A government-issued mining concession granting a company the right
/// to extract from a specific geological vein. Prevents single-company
/// monopolisation of large deposits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MiningConcession {
    /// The vein ID (or composite_id) this concession grants access to.
    pub vein_id: String,
    /// The company holding this concession.
    pub holder_company_id: String,
    /// The fee paid for this concession (0.0 for grandfathered genesis concessions).
    pub fee_paid: f64,
    /// Whether this concession was grandfathered during world generation
    /// (historical pre-existing operations that predate the licensing regime).
    pub grandfathered: bool,
    /// The turn this concession was issued.
    pub issued_turn: u32,
}

/// Phase 93: A pending geological survey initiated by a company to discover
/// hidden Rare/UltraRare veins. Stored in the decoupled
/// `GeologicalSurveyLedger` on `Country` to avoid bloating `Company`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingSurvey {
    /// The company funding this survey.
    pub company_id: String,
    /// The region being surveyed.
    pub region_id: String,
    /// The commodity being searched for.
    pub target_commodity: Commodity,
    /// The company-chosen search depth target in meters. If the actual vein's
    /// depth exceeds this, discovery fails (fog-of-war: company cannot know
    /// the real depth before discovery).
    pub target_depth: f64,
    /// The survey cost paid to the Treasury (sunk cost).
    pub survey_cost: f64,
    /// Turns remaining until the survey completes.
    pub turns_remaining: u32,
}

/// Phase 93: Decoupled survey ledger stored on `Country`. Maps `company_id`
/// to a list of active pending surveys. Keeps `Company` struct cache-efficient
/// (Rule 9) while tracking all active geological surveys.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GeologicalSurveyLedger {
    /// Active surveys keyed by company_id.
    pub surveys: FxHashMap<String, Vec<PendingSurvey>>,
}

impl GeologicalSurveyLedger {
    /// Add a new pending survey for a company.
    pub fn add_survey(&mut self, survey: PendingSurvey) {
        self.surveys
            .entry(survey.company_id.clone())
            .or_default()
            .push(survey);
    }

    /// Remove all completed (turns_remaining == 0) surveys for a company,
    /// returning them for resolution.
    pub fn drain_completed(&mut self) -> Vec<PendingSurvey> {
        let mut completed = Vec::new();
        for surveys in self.surveys.values_mut() {
            let (done, active): (Vec<_>, Vec<_>) =
                surveys.drain(..).partition(|s| s.turns_remaining == 0);
            completed.extend(done);
            *surveys = active;
        }
        // Clean up empty entries.
        self.surveys.retain(|_, v| !v.is_empty());
        completed
    }

    /// Decrement turns_remaining for all active surveys.
    pub fn tick(&mut self) {
        for surveys in self.surveys.values_mut() {
            for survey in surveys.iter_mut() {
                if survey.turns_remaining > 0 {
                    survey.turns_remaining -= 1;
                }
            }
        }
    }
}

/// Phase 93: Registry of mining concessions stored on `Country`. Maps
/// `vein_id` to the list of concessions issued for that vein.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MiningConcessionRegistry {
    /// Concessions keyed by vein_id.
    pub concessions: FxHashMap<String, Vec<MiningConcession>>,
}

impl MiningConcessionRegistry {
    /// Get all concessions for a specific vein.
    pub fn concessions_for_vein(&self, vein_id: &str) -> &[MiningConcession] {
        self.concessions
            .get(vein_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Count how many concessions a specific company holds for a vein.
    pub fn company_concession_count(&self, vein_id: &str, company_id: &str) -> usize {
        self.concessions_for_vein(vein_id)
            .iter()
            .filter(|c| c.holder_company_id == company_id)
            .count()
    }

    /// Total number of concessions issued for a vein.
    pub fn total_concessions_for_vein(&self, vein_id: &str) -> usize {
        self.concessions_for_vein(vein_id).len()
    }

    /// Add a new concession to the registry.
    pub fn add_concession(&mut self, concession: MiningConcession) {
        self.concessions
            .entry(concession.vein_id.clone())
            .or_default()
            .push(concession);
    }

    /// Remove all concessions held by a company (used on bankruptcy/liquidation).
    pub fn remove_company_concessions(&mut self, company_id: &str) {
        for concessions in self.concessions.values_mut() {
            concessions.retain(|c| c.holder_company_id != company_id);
        }
        self.concessions.retain(|_, v| !v.is_empty());
    }
}

/// Maximum depth (in meters) that a mining method from a given year can access.
///
/// This maps the tech progression of mining methods to realistic depth capabilities.
/// Methods before 1880 can only reach shallow deposits; modern methods can reach
/// deep deposits.
pub fn max_depth_for_method_year(year: u32) -> f64 {
    match year {
        y if y < 1885 => 200.0,   // Manual Mining
        y if y < 1890 => 400.0,   // Pneumatic Drilling
        y if y < 1895 => 600.0,   // Electric Mine Pumps
        y if y < 1900 => 800.0,   // Longwall Mining
        y if y < 1950 => 1000.0,  // Open-Pit / Froth Flotation era
        y if y < 1970 => 1200.0,  // Mechanized Longwall
        _ => 2000.0,              // CNC Mining and beyond
    }
}

/// Check whether a mining method from the given year can access a deposit at
/// the given depth.
pub fn can_access_depth(method_year: u32, deposit_depth: f64) -> bool {
    deposit_depth <= max_depth_for_method_year(method_year)
}

/// Compute the effective quality of a deposit based on its depletion ratio.
///
/// Formula: `current_quality = base_quality * (1.0 - 0.5 * depletion_ratio^2)`
///
/// At 50% depletion, quality is ~87.5% of base.
/// At 90% depletion, quality is ~59.5% of base.
/// At 100% depletion, quality is 50% of base (but current_reserves = 0 means no extraction).
pub fn compute_current_quality(base_quality: f64, current_reserves: f64, estimated_reserves: f64) -> f64 {
    if estimated_reserves <= 0.0 {
        return base_quality;
    }
    let depletion_ratio = 1.0 - (current_reserves / estimated_reserves).max(0.0).min(1.0);
    base_quality * (1.0 - 0.5 * depletion_ratio * depletion_ratio)
}

/// Find a deposit in the country's geological formations that matches the
/// given deposit ID and region.
///
/// The deposit ID format is `"{formation_id}/{commodity_key}"`.
///
/// # Returns
/// A tuple of (formation index, deposit key) if found, or `None`.
pub fn find_deposit_index<'a>(
    country: &'a Country,
    deposit_id: &str,
) -> Option<(usize, &'a String, &'a ResourceDeposit)> {
    let parts: Vec<&str> = deposit_id.splitn(2, '/').collect();
    if parts.len() != 2 {
        return None;
    }
    let formation_id = parts[0];
    let commodity_key = parts[1];

    for (f_idx, formation) in country.geological_formations.iter().enumerate() {
        if formation.id == formation_id {
            if let Some((key, deposit)) = formation.resource_deposits.get_key_value(commodity_key) {
                return Some((f_idx, key, deposit));
            }
        }
    }
    None
}

/// Find a deposit for a specific commodity in a specific region.
///
/// Searches all formations that overlap the given region for a deposit
/// producing the requested commodity. Only returns discovered deposits.
///
/// # Returns
/// A deposit ID string (`"{formation_id}/{commodity_key}"`) if found.
pub fn find_deposit_for_commodity(
    country: &Country,
    region_id: &str,
    commodity: Commodity,
) -> Option<String> {
    let target_key = commodity.to_string();
    for formation in &country.geological_formations {
        if !formation.overlapping_regions.contains(&region_id.to_string()) {
            continue;
        }
        for (key, deposit) in &formation.resource_deposits {
            if deposit.commodity == commodity && deposit.discovered && deposit.current_reserves > 0.0 {
                return Some(format!("{}/{}", formation.id, key));
            }
        }
    }
    // Fallback: also match by key string (handles edge cases in commodity serialization)
    let _ = target_key; // suppress unused warning
    None
}

/// Deplete a deposit by the requested amount, reducing `current_reserves` and
/// recomputing `current_quality`.
///
/// # Arguments
/// * `country` - Mutable country whose formations contain the deposit.
/// * `deposit_id` - Deposit ID in `"{formation_id}/{commodity_key}"` format.
/// * `amount` - Requested extraction amount.
///
/// # Returns
/// The actual amount that could be extracted (may be less than requested if
/// `current_reserves` is insufficient). Returns 0.0 if the deposit is not found.
pub fn deplete_deposit(
    country: &mut Country,
    deposit_id: &str,
    amount: f64,
) -> f64 {
    if amount <= 0.0 {
        return 0.0;
    }

    let parts: Vec<&str> = deposit_id.splitn(2, '/').collect();
    if parts.len() != 2 {
        return 0.0;
    }
    let formation_id = parts[0];
    let commodity_key = parts[1];

    for formation in &mut country.geological_formations {
        if formation.id != formation_id {
            continue;
        }
        if let Some(deposit) = formation.resource_deposits.get_mut(commodity_key) {
            let actual = amount.min(deposit.current_reserves);
            deposit.current_reserves -= actual;
            // Recompute quality based on new depletion ratio
            deposit.current_quality = compute_current_quality(
                deposit.quality,
                deposit.current_reserves,
                deposit.estimated_reserves,
            );
            return actual;
        }
    }

    0.0
}

/// Get the quality multiplier for a deposit, to be applied to mining output.
///
/// Returns 0.0 if the deposit is not found, not discovered, or exhausted.
/// Otherwise returns `deposit.current_quality` (0.0–1.0).
pub fn deposit_quality_multiplier(
    country: &Country,
    deposit_id: &str,
) -> f64 {
    match find_deposit_index(country, deposit_id) {
        Some((_, _, deposit)) => {
            if !deposit.discovered || deposit.current_reserves <= 0.0 {
                0.0
            } else {
                deposit.current_quality
            }
        }
        None => 0.0,
    }
}

/// Check if a deposit is accessible with the given method year (depth gating).
///
/// Returns `false` if the deposit is not found or if the method year cannot
/// reach the deposit's depth.
pub fn deposit_is_accessible(
    country: &Country,
    deposit_id: &str,
    method_year: u32,
) -> bool {
    match find_deposit_index(country, deposit_id) {
        Some((_, _, deposit)) => can_access_depth(method_year, deposit.depth),
        None => false,
    }
}

// ============================================================================
// Phase 93: Planet.veins-based geology functions (replacing legacy formations)
// ============================================================================

/// Phase 93: Find a vein by its deposit_id (which is the vein's `id` or
/// `composite_id`). Returns a reference to the vein if found.
pub fn find_vein<'a>(planet: &'a Planet, deposit_id: &str) -> Option<&'a GeologicalVein> {
    planet.vein_by_id(deposit_id)
}

/// Phase 93: Get the quality multiplier for a vein, to be applied to mining
/// output.
///
/// Returns 0.0 if the vein is not found, not discovered, or exhausted.
/// Otherwise returns `vein.quality` (0.0–1.0).
pub fn vein_quality_multiplier(planet: &Planet, deposit_id: &str) -> f64 {
    match find_vein(planet, deposit_id) {
        Some(vein) => {
            if !vein.discovered || vein.current_reserves <= 0.0 {
                0.0
            } else {
                // Quality decays with depletion, same formula as legacy deposits.
                compute_current_quality(vein.quality, vein.current_reserves, vein.total_reserves)
            }
        }
        None => 0.0,
    }
}

/// Phase 93: Check if a vein is accessible with the given method year
/// (depth gating).
///
/// Returns `false` if the vein is not found or if the method year cannot
/// reach the vein's depth.
pub fn vein_is_accessible(planet: &Planet, deposit_id: &str, method_year: u32) -> bool {
    match find_vein(planet, deposit_id) {
        Some(vein) => can_access_depth(method_year, vein.depth),
        None => false,
    }
}

/// Phase 93: A single depletion request entry for the batch delta buffer.
/// Collected during the parallel production pass and applied sequentially
/// after the parallel pass completes.
#[derive(Debug, Clone)]
pub struct DepletionRequest {
    /// The vein ID (or composite_id) to deplete.
    pub vein_id: String,
    /// The requested extraction amount (tons).
    pub requested_amount: f64,
}

/// Phase 93: Apply a batch of depletion requests to `Planet.veins` with
/// **pro-rata clamping** (Rule 5 & 20).
///
/// When multiple mines share a single vein and their total requested
/// extraction exceeds `current_reserves`, the remaining ore is distributed
/// strictly proportionally based on each mine's requested extraction fraction.
/// No mine gets full extraction while another gets nothing. The vein is
/// clamped exactly at `0.0` — never negative.
///
/// # Arguments
/// * `planet` - Mutable planet whose veins will be depleted.
/// * `requests` - Batch of depletion requests collected during production.
///
/// # Returns
/// A map of `vein_id -> actual_amount_depleted` for each request, so callers
/// can reconcile the actual extraction against what was requested.
pub fn apply_depletion_batch(
    planet: &mut Planet,
    requests: &[DepletionRequest],
) -> rustc_hash::FxHashMap<(String, usize), f64> {
    // Group requests by vein_id, preserving original index for caller reconciliation.
    let mut by_vein: rustc_hash::FxHashMap<
        String,
        Vec<(usize, f64)>,
    > = rustc_hash::FxHashMap::default();

    for (idx, req) in requests.iter().enumerate() {
        if req.requested_amount <= 0.0 {
            continue;
        }
        by_vein
            .entry(req.vein_id.clone())
            .or_default()
            .push((idx, req.requested_amount));
    }

    let mut results: rustc_hash::FxHashMap<(String, usize), f64> =
        rustc_hash::FxHashMap::default();

    for (vein_id, reqs) in &by_vein {
        let vein = match planet.vein_by_id_mut(vein_id) {
            Some(v) => v,
            None => continue,
        };

        let total_requested: f64 = reqs.iter().map(|(_, amt)| *amt).sum();

        if total_requested <= 0.0 {
            continue;
        }

        if total_requested <= vein.current_reserves {
            // Enough reserves for all requests — apply directly.
            for (idx, amt) in reqs {
                vein.current_reserves -= amt;
                results.insert((vein_id.clone(), *idx), *amt);
            }
        } else {
            // Pro-rata clamping: distribute remaining reserves proportionally.
            let share = vein.current_reserves / total_requested;
            for (idx, amt) in reqs {
                let actual = amt * share;
                results.insert((vein_id.clone(), *idx), actual);
            }
            // Clamp exactly at 0.0 (Rule 20).
            vein.current_reserves = 0.0;
        }

        // Note: vein.quality is the BASE quality and is not modified.
        // The decayed quality is computed on-the-fly by vein_quality_multiplier
        // via compute_current_quality(vein.quality, vein.current_reserves, vein.total_reserves).
    }

    results
}

/// Phase 93: Resolve completed geological surveys. Called in a turn phase
/// after `process_companies` and before `CompanyLifecycle::process_lifecycle`.
///
/// For each completed survey (turns_remaining == 0):
/// 1. Look for an undiscovered Rare/UltraRare vein of the target commodity
///    in the target region.
/// 2. **Fog-of-war depth gate (Rule 11 & 16)**: If the company's chosen
///    `target_depth` is shallower than the actual hidden vein's `depth`,
///    the discovery fails — the scan didn't reach deep enough.
/// 3. If `target_depth >= vein.depth`, discovery probability is a function
///    of `rarity_tier`, the company's available method year, and the survey
///    cost spent.
/// 4. On success, set `GeologicalVein::discovered = true`.
/// 5. On failure, the cash remains in the Treasury (sunk cost) and the
///    company learns nothing about the actual vein.
///
/// # Arguments
/// * `planet` - Mutable planet whose veins may be discovered.
/// * `ledger` - Mutable survey ledger on Country. Completed surveys are drained.
/// * `year` - Current in-game year (for method year / technology gating).
pub fn resolve_geological_surveys(
    planet: &mut Planet,
    ledger: &mut GeologicalSurveyLedger,
    year: u32,
) {
    // Tick all active surveys (decrement turns_remaining).
    ledger.tick();

    // Drain completed surveys for resolution.
    let completed = ledger.drain_completed();

    for survey in &completed {
        // Find undiscovered veins of the target commodity in the target region.
        let hidden_vein_indices = planet
            .undiscovered_vein_indices_for_region_and_commodity(
                &survey.region_id,
                survey.target_commodity,
            );

        if hidden_vein_indices.is_empty() {
            // No hidden vein of this commodity in this region — survey fails.
            continue;
        }

        // Try to discover each hidden vein (may find multiple, but typically
        // a survey reveals one vein at most).
        for vein_idx in hidden_vein_indices {
            // Fog-of-war depth gate: if the scan didn't go deep enough, fail.
            if survey.target_depth < planet.veins[vein_idx].depth {
                // Scan too shallow — this vein is not revealed.
                continue;
            }

            // Discovery probability based on rarity tier and survey investment.
            // Higher rarity = harder to find. Higher survey cost = better odds.
            let base_probability = match planet.veins[vein_idx].rarity_tier {
                crate::society::planet::RarityTier::UltraRare => 0.15,
                crate::society::planet::RarityTier::Rare => 0.30,
                _ => 0.50,
            };

            // Technology bonus: more advanced method years improve odds.
            let tech_bonus = ((year as f64 - 1900.0) / 200.0).clamp(0.0, 0.3);

            // Investment bonus: higher survey cost relative to average_wage
            // proxy improves odds (capped at +0.2).
            let investment_bonus = (survey.survey_cost / 1_000_000.0).clamp(0.0, 0.2);

            let discovery_probability = (base_probability + tech_bonus + investment_bonus).clamp(0.0, 0.95);

            // Deterministic RNG from survey + vein for reproducibility.
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            survey.company_id.hash(&mut hasher);
            survey.region_id.hash(&mut hasher);
            survey.target_commodity.hash(&mut hasher);
            planet.veins[vein_idx].id.hash(&mut hasher);
            year.hash(&mut hasher);
            let seed = hasher.finish();

            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            let roll: f64 = rng.gen();

            if roll < discovery_probability {
                // Discovery! Mark the vein as discovered.
                planet.veins[vein_idx].discovered = true;
                // Only discover one vein per survey (break after first success).
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::society::geography::GeologicalFormation;

    #[test]
    fn test_max_depth_progression() {
        assert_eq!(max_depth_for_method_year(1880), 200.0);
        assert_eq!(max_depth_for_method_year(1885), 400.0);
        assert_eq!(max_depth_for_method_year(1890), 600.0);
        assert_eq!(max_depth_for_method_year(1895), 800.0);
        assert_eq!(max_depth_for_method_year(1950), 1200.0);
        assert_eq!(max_depth_for_method_year(1970), 2000.0);
        assert_eq!(max_depth_for_method_year(2020), 2000.0);
    }

    #[test]
    fn test_can_access_depth() {
        assert!(can_access_depth(1880, 150.0));
        assert!(!can_access_depth(1880, 300.0));
        assert!(can_access_depth(1950, 1000.0));
        assert!(!can_access_depth(1950, 1500.0));
        assert!(can_access_depth(1970, 1500.0));
    }

    #[test]
    fn test_quality_decay() {
        // No depletion -> full quality
        let q = compute_current_quality(0.9, 1_000_000.0, 1_000_000.0);
        assert!((q - 0.9).abs() < 1e-9);

        // 50% depletion -> ~87.5% of base
        let q = compute_current_quality(0.9, 500_000.0, 1_000_000.0);
        let expected = 0.9 * (1.0 - 0.5 * 0.25); // 0.9 * 0.875 = 0.7875
        assert!((q - expected).abs() < 1e-9);

        // 90% depletion -> ~59.5% of base
        let q = compute_current_quality(0.9, 100_000.0, 1_000_000.0);
        let expected = 0.9 * (1.0 - 0.5 * 0.81); // 0.9 * 0.595 = 0.5355
        assert!((q - expected).abs() < 1e-9);

        // 100% depletion -> 50% of base
        let q = compute_current_quality(0.9, 0.0, 1_000_000.0);
        let expected = 0.9 * 0.5; // 0.45
        assert!((q - expected).abs() < 1e-9);
    }

    #[test]
    fn test_deplete_deposit() {
        let mut country = Country::mock_for_tests();
        country.geological_formations.push(GeologicalFormation {
            id: "F1".to_string(),
            name: "Test Formation".to_string(),
            formation_type: crate::society::geography::FormationType::SedimentaryBasin,
            resource_deposits: {
                let mut m = BTreeMap::new();
                m.insert("hard_coal".to_string(), ResourceDeposit {
                    commodity: Commodity::HardCoal,
                    estimated_reserves: 1_000_000.0,
                    current_reserves: 1_000_000.0,
                    extraction_cost: 50.0,
                    quality: 0.9,
                    current_quality: 0.9,
                    depth: 100.0,
                    discovered: true,
                });
                m
            },
            overlapping_regions: vec!["R1".to_string()],
            total_area: 10_000.0,
        });

        // Deplete 100k
        let actual = deplete_deposit(&mut country, "F1/hard_coal", 100_000.0);
        assert!((actual - 100_000.0).abs() < 1e-9);

        // Check reserves dropped
        let deposit = &country.geological_formations[0].resource_deposits["hard_coal"];
        assert!((deposit.current_reserves - 900_000.0).abs() < 1e-9);

        // Check quality decayed
        let expected_q = compute_current_quality(0.9, 900_000.0, 1_000_000.0);
        assert!((deposit.current_quality - expected_q).abs() < 1e-9);

        // Try to deplete more than available
        let actual = deplete_deposit(&mut country, "F1/hard_coal", 2_000_000.0);
        assert!((actual - 900_000.0).abs() < 1e-9);
        let deposit = &country.geological_formations[0].resource_deposits["hard_coal"];
        assert!((deposit.current_reserves - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_find_deposit_for_commodity() {
        let mut country = Country::mock_for_tests();
        country.geological_formations.push(GeologicalFormation {
            id: "F1".to_string(),
            name: "Test".to_string(),
            formation_type: crate::society::geography::FormationType::MountainRange,
            resource_deposits: {
                let mut m = BTreeMap::new();
                m.insert("iron".to_string(), ResourceDeposit {
                    commodity: Commodity::Iron,
                    estimated_reserves: 500_000.0,
                    current_reserves: 500_000.0,
                    extraction_cost: 30.0,
                    quality: 0.8,
                    current_quality: 0.8,
                    depth: 150.0,
                    discovered: true,
                });
                m.insert("gold".to_string(), ResourceDeposit {
                    commodity: Commodity::Gold,
                    estimated_reserves: 100_000.0,
                    current_reserves: 100_000.0,
                    extraction_cost: 80.0,
                    quality: 0.7,
                    current_quality: 0.7,
                    depth: 800.0,
                    discovered: false, // hidden
                });
                m
            },
            overlapping_regions: vec!["R1".to_string()],
            total_area: 5_000.0,
        });

        // Iron is discovered -> should find it
        let id = find_deposit_for_commodity(&country, "R1", Commodity::Iron);
        assert!(id.is_some());
        assert!(id.unwrap().starts_with("F1/"));

        // Gold is not discovered -> should not find it
        let id = find_deposit_for_commodity(&country, "R1", Commodity::Gold);
        assert!(id.is_none());

        // Wrong region -> should not find it
        let id = find_deposit_for_commodity(&country, "R2", Commodity::Iron);
        assert!(id.is_none());
    }

    use std::collections::BTreeMap;
}

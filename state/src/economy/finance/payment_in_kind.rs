//! Payment in kind (Wynagrodzenie w naturze) for agricultural workers (Phase 6.5).
//!
//! Implements the deduction of harvest commodities to satisfy subsistence needs
//! of agricultural workers before crops are sent to warehouses or sold on B2B markets.

use crate::data::{consumption_registry, substitution_matrix, subsistence_config, ConsumptionBasket, NeedTier};
use crate::economy::labor_market::LaborAllocationMatrix;
use crate::registries::enums::Commodity;
use crate::society::geography::{DemographyType, Region};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Ledger tracking in-kind payments per company (Phase 6.5)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct InKindLedger {
    /// Company ID → (Commodity → units deducted as in-kind payment)

    pub deductions: BTreeMap<String, BTreeMap<Commodity, f64>>,

    /// Company ID → cash wage offset (for FreePeasant/LandlessLaborer)

    pub cash_offsets: BTreeMap<String, f64>,

    /// Phase 44: Per-class deductions for B2C demand netting.
    /// (region_id, demography_type, class_id) → (Commodity → units deducted)
    #[serde(default)]
    pub deductions_by_class: BTreeMap<(String, DemographyType, String), BTreeMap<Commodity, f64>>,
}

/// Nutritional deficit tracking for demographic classes (Phase 6.5)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct NutritionalDeficit {
    /// (region_id, demography_type, class_id) → (Commodity → deficit units)

    pub deficits: BTreeMap<(String, DemographyType, String), BTreeMap<Commodity, f64>>,
    
    /// (region_id, demography_type, class_id) → quality penalty (0-1)
    /// Applied when subsistence is met via substitution

    pub quality_penalties: BTreeMap<(String, DemographyType, String), f64>,
}

/// Apply payment in kind to agricultural harvest (Phase 6.5, Phase D.5).
///
/// # Arguments
/// * `region` - Mutable reference to the region
/// * `labor_allocation` - LaborAllocationMatrix from resolve_regional_labor_market
/// * `harvest_bundle` - Harvest bundle (company_id → commodity → units)
/// * `current_turn` - Current turn number
///
/// # Returns
/// * `(InKindLedger, NutritionalDeficit)` - Tracking structures for accounting
///
/// # Rules
/// * Serfs: in-kind INSTEAD of wages (no cash, savings forced to 0)
/// * FreePeasant/LandlessLaborer: in-kind as VWAP wage offset
/// * Aristocracy: no in-kind at all
/// * Intersection + capped substitution: only deduct commodities actually harvested
/// * Surplus of produced commodity may substitute for missing needs up to substitution_cap
/// * Remaining harvest after deduction flows to D.6 (deposit to warehouses)
pub fn apply_payment_in_kind(
    region: &mut Region,
    labor_allocation: &LaborAllocationMatrix,
    harvest_bundle: &mut BTreeMap<String, BTreeMap<Commodity, f64>>,
    current_turn: u32,
) -> (InKindLedger, NutritionalDeficit) {
    let mut in_kind_ledger = InKindLedger::default();
    let mut nutritional_deficit = NutritionalDeficit::default();
    
    let config = subsistence_config();
    let consumption = consumption_registry();
    let substitution = substitution_matrix();

    // Process each agricultural company in the region
    for (company_id, harvest) in harvest_bundle.iter_mut() {
        // Get labor allocation for this company
        let company_fte: BTreeMap<(DemographyType, String), f64> = labor_allocation
            .fte
            .iter()
            .filter(|((cid, _, _), _)| cid == company_id)
            .map(|((_, dt, cid), fte)| ((*dt, cid.clone()), *fte))
            .collect();
        
        let company_wages: BTreeMap<(DemographyType, String), f64> = labor_allocation
            .wages
            .iter()
            .filter(|((cid, _, _), _)| cid == company_id)
            .map(|((_, dt, cid), wages)| ((*dt, cid.clone()), *wages))
            .collect();

        if company_fte.is_empty() {
            continue; // No workers, no in-kind payment
        }

        // Calculate total subsistence need for all workers
        let mut total_subsistence_need: BTreeMap<Commodity, f64> = BTreeMap::new();
        
        for ((demography_type, class_id), fte) in &company_fte {
            if let Some(basket) = consumption.get(class_id) {
                if let Some(subsistence_tier) = basket.tiers.get(&NeedTier::Subsistence) {
                    for (commodity, per_capita) in subsistence_tier {
                        let need = per_capita * fte;
                        *total_subsistence_need.entry(*commodity).or_insert(0.0) += need;
                    }
                }
            }
        }

        // Class-dependent accounting rules
        let mut deductions: BTreeMap<Commodity, f64> = BTreeMap::new();
        let mut cash_offset = 0.0;

        for ((demography_type, class_id), fte) in &company_fte {
            let class_wages = company_wages.get(&(*demography_type, class_id.clone())).copied().unwrap_or(0.0);
            
            // Serfs: in-kind INSTEAD of wages (no cash offset)
            if class_id == "Serf" {
                // Full in-kind, no cash
                if let Some(basket) = consumption.get(class_id) {
                    if let Some(subsistence_tier) = basket.tiers.get(&NeedTier::Subsistence) {
                        for (commodity, per_capita) in subsistence_tier {
                            let need = per_capita * fte;
                            *deductions.entry(*commodity).or_insert(0.0) += need;
                        }
                    }
                }
            }
            // FreePeasant/LandlessLaborer: in-kind as VWAP wage offset
            else if class_id == "FreePeasant" || class_id == "LandlessLaborer" {
                if config.vwap_wage_offset {
                    // Calculate in-kind value and offset cash wages
                    let in_kind_value = calculate_in_kind_value(class_id, *fte, &consumption);
                    cash_offset += in_kind_value.min(class_wages);
                    
                    // Deduct from harvest
                    if let Some(basket) = consumption.get(class_id) {
                        if let Some(subsistence_tier) = basket.tiers.get(&NeedTier::Subsistence) {
                            for (commodity, per_capita) in subsistence_tier {
                                let need = per_capita * fte;
                                *deductions.entry(*commodity).or_insert(0.0) += need;
                            }
                        }
                    }
                }
            }
            // Aristocracy: no in-kind at all
            else if class_id == "Aristocracy" {
                // No deductions, full cash wages
            }
        }

        // Intersection: only deduct commodities actually harvested
        let mut actual_deductions: BTreeMap<Commodity, f64> = BTreeMap::new();
        for (commodity, needed) in &deductions {
            if let Some(&available) = harvest.get(commodity) {
                let deductible = needed.min(available);
                if deductible > 0.0 {
                    actual_deductions.insert(*commodity, deductible);
                    *harvest.get_mut(commodity).unwrap() -= deductible;
                }
            }
        }

        // Substitution: surplus of produced commodity may cover deficit
        for (commodity, deficit) in &total_subsistence_need {
            let already_covered = actual_deductions.get(commodity).copied().unwrap_or(0.0);
            let remaining_deficit = deficit - already_covered;
            
            if remaining_deficit > 0.0 {
                // Look for substitution candidates
                if let Some(candidates) = substitution.get(commodity) {
                    for sub in candidates {
                        if let Some(&surplus) = harvest.get(&sub.donor) {
                            let substitution_cap = config.substitution_cap * remaining_deficit;
                            let donor_needed = substitution_cap * sub.ratio;
                            
                            if donor_needed <= surplus {
                                // Apply substitution
                                let donor_used = donor_needed;
                                *harvest.get_mut(&sub.donor).unwrap() -= donor_used;
                                *actual_deductions.entry(sub.donor).or_insert(0.0) += donor_used;
                                
                                // Track quality penalty
                                let key = (region.id.clone(), DemographyType::Rural, "Serf".to_string());
                                *nutritional_deficit.quality_penalties.entry(key).or_insert(0.0) += config.nutritional_penalty;
                            }
                        }
                    }
                }
            }
        }

        // Record in-kind ledger
        if !actual_deductions.is_empty() {
            in_kind_ledger.deductions.insert(company_id.clone(), actual_deductions.clone());
            if cash_offset > 0.0 {
                in_kind_ledger.cash_offsets.insert(company_id.clone(), cash_offset);
            }
            // Phase 44: Track per-class deductions for B2C demand netting.
            for ((demography_type, class_id), fte) in &company_fte {
                if class_id == "Serf" || class_id == "FreePeasant" || class_id == "LandlessLaborer" {
                    let key = (region.id.clone(), *demography_type, class_id.clone());
                    let class_entry = in_kind_ledger.deductions_by_class.entry(key).or_default();
                    for (&commodity, &qty) in &actual_deductions {
                        // Proportionally assign deductions based on FTE share
                        let total_fte: f64 = company_fte.values().sum();
                        if total_fte > 0.0 {
                            let share = fte / total_fte;
                            *class_entry.entry(commodity).or_insert(0.0) += qty * share;
                        }
                    }
                }
            }
        }
    }

    // Net out B2C demand for agricultural classes
    // (This will be used in Phase 6.5 Section 4.1 Build Demand)
    for ((_company_id, demography_type, class_id), fte) in &labor_allocation.fte {
        if let Some(basket) = consumption.get(class_id) {
            if let Some(subsistence_tier) = basket.tiers.get(&NeedTier::Subsistence) {
                for (commodity, per_capita) in subsistence_tier {
                    let need = per_capita * *fte;
                    let key = (region.id.clone(), *demography_type, class_id.clone());
                    *nutritional_deficit.deficits.entry(key).or_insert_with(BTreeMap::new)
                        .entry(*commodity).or_insert(0.0) += need;
                }
            }
        }
    }

    (in_kind_ledger, nutritional_deficit)
}

/// Calculate the monetary value of in-kind payment for a class.
///
/// # Arguments
/// * `class_id` - Demographic class identifier
/// * `fte` - Full-time equivalent workers
/// * `consumption` - Consumption registry
///
/// # Returns
/// * Monetary value of in-kind payment (using VWAP pricing placeholder)
///
/// # Rules
/// Phase 44: Uses base_price fallback of 100.0 per unit (replaces old 1.0 placeholder).
/// The actual imputed GDP valuation happens in turn.rs using VWAP from market history.
fn calculate_in_kind_value(
    class_id: &str,
    fte: f64,
    consumption: &BTreeMap<String, ConsumptionBasket>,
) -> f64 {
    if let Some(basket) = consumption.get(class_id) {
        if let Some(subsistence_tier) = basket.tiers.get(&NeedTier::Subsistence) {
            let total_units: f64 = subsistence_tier.values().sum();
            // Phase 44: Use 100.0 as base price fallback (was 1.0 placeholder).
            // The actual imputed GDP valuation in turn.rs uses VWAP from market history.
            total_units * fte * 100.0
        } else {
            0.0
        }
    } else {
        0.0
    }
}

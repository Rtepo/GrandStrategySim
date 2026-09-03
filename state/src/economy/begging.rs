//! Phase D8: Begging & Vagrancy Mechanics
//!
//! Implements begging as an informal, zero-sum wealth transfer from donor
//! classes (those with positive savings) to destitute recipients (disabled,
//! unemployed, or impoverished classes whose savings have been exhausted).
//!
//! # Rules (per user corrections)
//! * Total transfers are capped by the actual positive savings of donor classes.
//! * If the entire region is impoverished, begging produces zero.
//! * Donor savings must never become negative.
//! * No fiat is created or destroyed (Rule 1).
//! * Recipient need is capped by the poverty gap (subsistence minus current
//!   per-capita savings).
//!
//! # Donors
//! * Aristocracy and Bourgeoisie are primary donors (wealthy classes).
//! * FreePeasants are secondary donors (only if they have surplus savings).
//! * Serfs, LandlessLaborers, and Workers are not donors — they are
//!   potential recipients.
//!
//! # Recipients
//! * Classes with per-capita savings below the subsistence threshold.
//! * Disabled citizens (active_disabled) are prioritized.
//! * The transfer is pro-rata based on recipient need and donor capacity.

use crate::society::geography::{ClassDemographics, PoorLaws, RuralClass, UrbanClass};
use serde::{Deserialize, Serialize};

/// Phase D8: Configuration for begging and vagrancy mechanics.
///
/// All nominal values are scaled by `average_wage` to remain inflation-proof
/// (Rule 2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BeggingConfig {
    /// Subsistence threshold as a fraction of average_wage.
    /// Classes with per-capita savings below this are potential recipients.
    pub subsistence_threshold_fraction: f64,
    /// Maximum fraction of a donor class's positive savings that can be
    /// extracted via begging per turn (e.g., 0.01 = 1%).
    pub max_donor_extraction_rate: f64,
    /// Unrest increase per turn when begging occurs (social tension from
    /// visible destitution).
    pub unrest_per_begging_incident: f64,
}

impl Default for BeggingConfig {
    fn default() -> Self {
        BeggingConfig {
            subsistence_threshold_fraction: 0.5,
            max_donor_extraction_rate: 0.01,
            unrest_per_begging_incident: 0.001,
        }
    }
}

/// Phase D8: Result of one turn of begging processing for a region.
#[derive(Debug, Default, Clone)]
pub struct BeggingTurnResult {
    /// Total wealth transferred from donors to recipients.
    pub total_transferred: f64,
    /// Number of recipients who received begging transfers.
    pub recipient_count: i64,
    /// Number of donors who contributed.
    pub donor_count: i64,
    /// Whether begging occurred (false = region is impoverished, zero transfer).
    pub begging_occurred: bool,
    /// Unrest increase from begging.
    pub unrest_increase: f64,
}

/// Phase D8: Process begging for a single region.
///
/// This is a zero-sum informal wealth transfer. It runs after social welfare
/// and charity distribution, catching those who fell through the safety net.
///
/// # Flow
/// 1. Identify recipient classes (per-capita savings < subsistence threshold).
/// 2. Compute recipient need (subsistence - current per-capita savings) × population.
/// 3. Identify donor classes (Aristocracy, Bourgeoisie, FreePeasant with surplus).
/// 4. Cap total transfer by min(total_need, total_donor_capacity).
/// 5. Transfer pro-rata: donors contribute proportionally to their surplus;
///    recipients receive proportionally to their need.
/// 6. Donor savings are debited (never below zero).
/// 7. Recipient savings are credited.
/// 8. Unrest increases from visible begging.
pub fn process_begging_turn(
    region: &mut crate::society::geography::Region,
    config: &BeggingConfig,
    average_wage: f64,
    poor_laws: &PoorLaws,
) -> BeggingTurnResult {
    let mut result = BeggingTurnResult::default();
    let avg_wage = average_wage.max(1.0);
    let subsistence_threshold = config.subsistence_threshold_fraction * avg_wage;

    // Phase D9: If begging is repressed, reduce the effective transfer by
    // the repression rate. This doesn't delete population — it suppresses
    // the informal transfer, leaving recipients destitute (which increases
    // unrest). Arrested beggars are routed to workhouses if capacity exists.
    let repression_factor = if poor_laws.begging_repressed {
        1.0 - poor_laws.begging_repression_rate
    } else {
        1.0
    };

    // ── Step 1 & 2: Identify recipients and compute need ─────────────
    // Recipients are classes with per-capita savings below subsistence.
    // Need = (subsistence - per_capita_savings) × population, clamped to >= 0.
    #[derive(Clone, Copy)]
    enum ClassRef {
        Rural(RuralClass),
        Urban(UrbanClass),
    }

    let mut recipients: Vec<(ClassRef, f64)> = Vec::new(); // (class, need)
    let mut total_need: f64 = 0.0;
    let mut recipient_count: i64 = 0;

    for (class, demo) in &region.class_demographics.rural_classes {
        if demo.population <= 0 {
            continue;
        }
        let per_capita = demo.savings / demo.population as f64;
        if per_capita < subsistence_threshold {
            let need = (subsistence_threshold - per_capita) * demo.population as f64;
            if need > 0.0 {
                recipients.push((ClassRef::Rural(*class), need));
                total_need += need;
                recipient_count += demo.population;
            }
        }
    }
    for (class, demo) in &region.class_demographics.urban_classes {
        if demo.population <= 0 {
            continue;
        }
        let per_capita = demo.savings / demo.population as f64;
        if per_capita < subsistence_threshold {
            let need = (subsistence_threshold - per_capita) * demo.population as f64;
            if need > 0.0 {
                recipients.push((ClassRef::Urban(*class), need));
                total_need += need;
                recipient_count += demo.population;
            }
        }
    }

    if total_need <= 0.0 || recipients.is_empty() {
        return result; // No recipients need begging.
    }

    // ── Step 3: Identify donors and compute capacity ─────────────────
    // Donors: Aristocracy, Bourgeoisie, FreePeasant with positive savings.
    // Capacity = min(savings × max_extraction_rate, savings) — never negative.
    let mut donors: Vec<(ClassRef, f64)> = Vec::new(); // (class, capacity)
    let mut total_capacity: f64 = 0.0;
    let mut donor_count: i64 = 0;

    let extract_capacity = |demo: &ClassDemographics| -> f64 {
        if demo.savings <= 0.0 {
            return 0.0;
        }
        let capacity = demo.savings * config.max_donor_extraction_rate;
        capacity.min(demo.savings) // Never extract more than available
    };

    if let Some(demo) = region.class_demographics.rural_classes.get(&RuralClass::Aristocracy) {
        let cap = extract_capacity(demo);
        if cap > 0.0 {
            donors.push((ClassRef::Rural(RuralClass::Aristocracy), cap));
            total_capacity += cap;
            donor_count += demo.population;
        }
    }
    if let Some(demo) = region.class_demographics.rural_classes.get(&RuralClass::FreePeasant) {
        let cap = extract_capacity(demo);
        if cap > 0.0 {
            donors.push((ClassRef::Rural(RuralClass::FreePeasant), cap));
            total_capacity += cap;
            donor_count += demo.population;
        }
    }
    if let Some(demo) = region.class_demographics.urban_classes.get(&UrbanClass::Bourgeoisie) {
        let cap = extract_capacity(demo);
        if cap > 0.0 {
            donors.push((ClassRef::Urban(UrbanClass::Bourgeoisie), cap));
            total_capacity += cap;
            donor_count += demo.population;
        }
    }

    if total_capacity <= 0.0 || donors.is_empty() {
        // Entire region is impoverished — begging produces zero.
        // Track destitution unrest.
        result.unrest_increase = config.unrest_per_begging_incident * recipient_count as f64;
        return result;
    }

    // ── Step 4: Cap total transfer ───────────────────────────────────
    // Apply repression factor (Phase D9): repressed begging reduces the
    // effective transfer, leaving recipients destitute.
    let raw_transfer = total_need.min(total_capacity);
    let total_transfer = raw_transfer * repression_factor;
    result.begging_occurred = total_transfer > 0.0;
    result.total_transferred = total_transfer;
    result.recipient_count = recipient_count;
    result.donor_count = donor_count;

    // ── Step 5: Transfer pro-rata ────────────────────────────────────
    // Donors contribute proportionally to their capacity.
    // Recipients receive proportionally to their need.
    let donor_fraction = total_transfer / total_capacity;
    let recipient_fraction = total_transfer / total_need;

    // Debit donors
    for (class_ref, capacity) in &donors {
        let debit = capacity * donor_fraction;
        match class_ref {
            ClassRef::Rural(rc) => {
                if let Some(demo) = region.class_demographics.rural_classes.get_mut(rc) {
                    // Never go below zero — clamp to savings
                    let actual_debit = debit.min(demo.savings.max(0.0));
                    demo.savings -= actual_debit;
                }
            }
            ClassRef::Urban(uc) => {
                if let Some(demo) = region.class_demographics.urban_classes.get_mut(uc) {
                    let actual_debit = debit.min(demo.savings.max(0.0));
                    demo.savings -= actual_debit;
                }
            }
        }
    }

    // Credit recipients
    for (class_ref, need) in &recipients {
        let credit = need * recipient_fraction;
        match class_ref {
            ClassRef::Rural(rc) => {
                if let Some(demo) = region.class_demographics.rural_classes.get_mut(rc) {
                    demo.savings += credit;
                }
            }
            ClassRef::Urban(uc) => {
                if let Some(demo) = region.class_demographics.urban_classes.get_mut(uc) {
                    demo.savings += credit;
                }
            }
        }
    }

    // ── Step 6: Unrest from visible begging ──────────────────────────
    // Repression increases unrest: suppressed begging leaves recipients
    // destitute and creates social tension from enforcement.
    let base_unrest = config.unrest_per_begging_incident * recipient_count as f64;
    let repression_unrest = if poor_laws.begging_repressed {
        base_unrest * poor_laws.begging_repression_rate * 2.0
    } else {
        0.0
    };
    result.unrest_increase = base_unrest + repression_unrest;

    result
}

/// Phase D9: Process local disability relief from a micro-region budget.
///
/// This is a separate financial flow from the national disability pension.
/// It debits the micro-region's `sub_budget.liquid_reserves` and credits
/// the savings of disabled citizens in the parent region. If the micro-region
/// budget is insolvent, the relief fails — no fiat is created.
///
/// # Flow
/// 1. Compute relief per disabled person: rate × average_wage.
/// 2. Count disabled citizens in the parent region.
/// 3. Cap total relief by the micro-region's liquid_reserves.
/// 4. Debit micro-region budget, credit class savings pro-rata.
pub fn process_local_disability_relief(
    region: &mut crate::society::geography::Region,
    micro_region_id: &str,
    relief_rate: f64,
    average_wage: f64,
) -> f64 {
    if relief_rate <= 0.0 {
        return 0.0;
    }

    let micro_region = match region.micro_regions.get_mut(micro_region_id) {
        Some(mr) if mr.local_laws.poor_laws.local_disability_relief_rate > 0.0 => mr,
        _ => return 0.0,
    };

    let avg_wage = average_wage.max(1.0);
    let relief_per_person = relief_rate * avg_wage;

    // Count disabled citizens in the region.
    let mut disabled_by_class: Vec<(String, i64)> = Vec::new(); // (class_key, count)
    let mut total_disabled: i64 = 0;

    for (rc, demo) in &region.class_demographics.rural_classes {
        if demo.active_disabled > 0 {
            let key = format!("Rural:{:?}", rc);
            disabled_by_class.push((key, demo.active_disabled));
            total_disabled += demo.active_disabled;
        }
    }
    for (uc, demo) in &region.class_demographics.urban_classes {
        if demo.active_disabled > 0 {
            let key = format!("Urban:{:?}", uc);
            disabled_by_class.push((key, demo.active_disabled));
            total_disabled += demo.active_disabled;
        }
    }

    if total_disabled <= 0 {
        return 0.0;
    }

    let total_relief_due = relief_per_person * total_disabled as f64;
    let available = micro_region.sub_budget.liquid_reserves;
    let actual_relief = total_relief_due.min(available.max(0.0));

    if actual_relief <= 0.0 {
        return 0.0; // Micro-region is insolvent — relief fails.
    }

    // Debit micro-region budget.
    micro_region.sub_budget.liquid_reserves -= actual_relief;

    // Credit class savings pro-rata.
    let relief_fraction = actual_relief / total_relief_due;
    for (class_key, count) in &disabled_by_class {
        let class_relief = relief_per_person * *count as f64 * relief_fraction;
        let parts: Vec<&str> = class_key.splitn(2, ':').collect();
        if parts.len() != 2 {
            continue;
        }
        let category = parts[0];
        let class_name = parts[1];
        if category == "Rural" {
            for (rc, demo) in &mut region.class_demographics.rural_classes {
                if format!("{:?}", rc) == class_name {
                    demo.savings += class_relief;
                    break;
                }
            }
        } else if category == "Urban" {
            for (uc, demo) in &mut region.class_demographics.urban_classes {
                if format!("{:?}", uc) == class_name {
                    demo.savings += class_relief;
                    break;
                }
            }
        }
    }

    actual_relief
}

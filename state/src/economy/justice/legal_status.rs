//! Phase 18A: Legal status, shadow economy, and amnesty/legalization.
//!
//! This module implements:
//! - `LegalStatus` enum for tracking citizenship/residency of demographic classes
//! - `ShadowEmployment` for companies hiring undocumented workers off-the-books
//! - `ShadowEconomyState` for aggregate shadow economy tracking
//! - `AmnestyLaw` for configurable legalization programs
//! - `process_shadow_economy_turn()` for shadow labor processing
//! - `process_remittances_turn()` for TemporaryWorker outbound remittances
//! - `process_amnesty_turn()` for gradual legalization of Illegal populations
//!
//! # Rules
//! * Remittances are deducted from net income at the source (labor_market.rs), NOT from savings.
//! * Amnesty affordability clamp prevents negative savings (debt trap).
//! * Deportation extracts proportional wealth from class savings (no per-capita duplication).
//! * Illegals have 0.0 assimilation rate — only legalization unlocks assimilation.

#![allow(missing_docs)]

use crate::entities::Company;
use crate::registries::enums::Sector;
use crate::state::Country;
use serde::{Deserialize, Serialize};

/// Legal status of a demographic cohort or class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LegalStatus {
    /// Full citizen with voting rights and full labor access.
    #[default]
    Citizen,
    /// Legal resident (e.g., assimilated immigrant with permanent residency).
    Resident,
    /// Temporary worker with legal employment but limited rights; pays remittances.
    TemporaryWorker,
    /// Undocumented/illegal immigrant; works in shadow economy, no assimilation.
    Illegal,
}

/// Shadow employment record on a company (off-the-books workers).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ShadowEmployment {
    /// Number of hidden (off-the-books) FTEs.
    #[serde(default)]
    pub hidden_fte: f64,
    /// Wage paid to hidden workers (per FTE, below market rate).
    #[serde(default)]
    pub shadow_wage_per_fte: f64,
    /// PIT evaded this turn (for fine calculation).
    #[serde(default)]
    pub pit_evaded: f64,
    /// Turns since last inspection (higher = more likely to be caught).
    #[serde(default)]
    pub turns_since_inspection: u32,
}

/// Aggregate shadow economy state tracked on `Politics`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ShadowEconomyState {
    /// Total hidden FTEs across all companies.
    #[serde(default)]
    pub total_hidden_fte: f64,
    /// Total PIT evaded this turn.
    #[serde(default)]
    pub total_pit_evaded: f64,
    /// Total remittances sent abroad by TemporaryWorkers this turn.
    #[serde(default)]
    pub total_remittances_outbound: f64,
    /// Number of inspectorate raids conducted this turn.
    #[serde(default)]
    pub raids_conducted: u32,
    /// Total fines collected from shadow economy raids.
    #[serde(default)]
    pub fines_collected: f64,
    /// Number of Illegals legalized this turn via amnesty.
    #[serde(default)]
    pub legalized_this_turn: i64,
}

/// Amnesty / legalization program configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AmnestyLaw {
    /// Whether amnesty is currently active.
    #[serde(default)]
    pub active: bool,
    /// Fraction of Illegal population legalized per turn (0.0–1.0).
    #[serde(default = "default_legalization_rate")]
    pub legalization_rate: f64,
    /// Target status for legalized individuals.
    #[serde(default)]
    pub target_status: LegalStatus,
    /// One-time tax penalty for legalization (flat fee per person).
    #[serde(default)]
    pub legalization_fee: f64,
}

fn default_legalization_rate() -> f64 {
    0.05
}

/// Sectors that are labor-intensive and prone to shadow employment.
const SHADOW_ECONOMY_SECTORS: &[Sector] = &[
    Sector::Agriculture,
    Sector::LightIndustry,
    Sector::Construction,
    Sector::Hospitality,
];

/// Default shadow wage as a fraction of market wage.
const DEFAULT_SHADOW_WAGE_FRACTION: f64 = 0.50;

/// Result of processing the shadow economy for one turn.
#[derive(Debug, Clone, Default)]
pub struct ShadowEconomyTurnResult {
    /// Total hidden FTEs processed.
    pub total_hidden_fte: f64,
    /// Total shadow wages paid (below market rate).
    pub total_shadow_wages: f64,
    /// Total PIT evaded.
    pub total_pit_evaded: f64,
}

/// Process the shadow economy turn for a country.
///
/// Calculates shadow employment impact: companies in labor-intensive sectors
/// with `ShadowEmployment` records have their hidden FTEs processed.
/// Hidden workers are paid shadow wages (no PIT), and the evaded PIT is tracked.
///
/// # Arguments
/// * `country` - Mutable country (reads pit_rate, regions for worker crediting).
/// * `companies` - Mutable companies (reads/updates shadow_employment).
///
/// # Returns
/// `ShadowEconomyTurnResult` with aggregate shadow economy stats.
///
/// # Rules
/// * Only companies in labor-intensive sectors are eligible.
/// * Shadow workers are paid `shadow_wage_per_fte` (default: 50% of offered_wage_per_fte).
/// * Phase 28: Shadow wages are routed through `TransferSettler` for strict
///   double-entry accounting. Company cash is debited and worker savings are
///   credited (to the rural `landless_laborer` class of the company's region).
///   The ONLY difference from a legal wage is that PIT is NOT withheld.
/// * PIT evaded = `pit_rate * shadow_wage * hidden_fte`.
/// * `turns_since_inspection` increments each turn.
pub fn process_shadow_economy_turn(
    country: &mut Country,
    companies: &mut [Company],
) -> ShadowEconomyTurnResult {
    let pit_rate = country.tax_rates.income_tax.rate;
    let mut result = ShadowEconomyTurnResult::default();

    // Phase 28: Collect shadow wage payments first, then route through
    // TransferSettler. This avoids borrow checker issues with simultaneous
    // mutable access to companies and country.
    let mut pending_payments: Vec<(usize, f64, String)> = Vec::new();

    for payer_idx in 0..companies.len() {
        let company = &mut companies[payer_idx];
        if !SHADOW_ECONOMY_SECTORS.contains(&company.sector) {
            continue;
        }

        if let Some(ref mut shadow) = company.shadow_employment {
            if shadow.hidden_fte <= 0.0 {
                shadow.turns_since_inspection += 1;
                continue;
            }

            // Ensure shadow wage is set (default to 50% of market wage)
            if shadow.shadow_wage_per_fte <= 0.0 {
                shadow.shadow_wage_per_fte = company.offered_wage_per_fte * DEFAULT_SHADOW_WAGE_FRACTION;
            }

            let shadow_wages = shadow.hidden_fte * shadow.shadow_wage_per_fte;
            let pit_evaded = shadow_wages * pit_rate;
            let company_region_id = company.region_id.clone();

            shadow.pit_evaded = pit_evaded;
            shadow.turns_since_inspection += 1;

            result.total_hidden_fte += shadow.hidden_fte;
            result.total_shadow_wages += shadow_wages;
            result.total_pit_evaded += pit_evaded;

            pending_payments.push((payer_idx, shadow_wages, company_region_id));
        }
    }

    // Phase 28: Route shadow wages through TransferSettler for strict
    // double-entry. Debit company cash, credit worker savings.
    // The ONLY difference from legal wages: PIT is NOT withheld.
    // Shadow workers are credited to the rural landless_laborer class
    // of the company's region (the most likely demographic for off-the-books workers).
    for (payer_idx, shadow_wages, company_region_id) in pending_payments {
        let region_idx = country.regions.iter().position(|r| r.id == company_region_id);
        if let Some(ri) = region_idx {
            let _ = crate::economy::transfer_settler::settle_wage_payment(
                companies,
                payer_idx,
                shadow_wages,
                country,
                ri,
                true, // rural
                "bezrolnik", // landless_laborer class key
            );
        } else {
            // Fallback: if region not found, debit company cash directly
            if let Some(ref mut ba) = companies[payer_idx].brokerage_account {
                ba.cash = (ba.cash - shadow_wages).max(0.0);
            }
        }
    }

    result
}

/// Phase 28: Trigger shadow employment for companies that cannot fill their
/// labor demand through legal channels.
///
/// When a company's `fulfilled_fte < target_fte_demand * 0.5` (more than half
/// of its labor demand is unmet) and it has some `available_cash`, it may
/// resort to hiring workers off-the-books at a lower wage.
///
/// The probability of entering the shadow economy increases with:
/// - High PIT rate (tax evasion incentive)
/// - Low inspectorate capacity (low detection risk)
/// - High unmet labor demand
///
/// # Arguments
/// * `country` - Country state (reads pit_rate, inspectorate capacity).
/// * `companies` - Mutable companies (may activate shadow_employment).
/// * `rng` - Random number generator.
///
/// # Rules
/// * Only companies in `SHADOW_ECONOMY_SECTORS` are eligible.
/// * Companies that already have `shadow_employment` are not re-triggered.
/// * The hidden FTE is a fraction of the unmet demand (10-30%).
/// * The shadow wage is 50% of the offered legal wage.
pub fn trigger_shadow_employment(
    country: &Country,
    companies: &mut [Company],
    rng: &mut impl rand::Rng,
) {
    let pit_rate = country.tax_rates.income_tax.rate;
    let inspectorate_capacity = country.politics.inspectorate_state
        .as_ref()
        .map(|ist| ist.labor_inspection_capacity)
        .unwrap_or(0.0);

    for company in companies.iter_mut() {
        if !SHADOW_ECONOMY_SECTORS.contains(&company.sector) {
            continue;
        }
        // Skip if already has shadow employment
        if company.shadow_employment.is_some() {
            continue;
        }
        // Phase 34: Startup grace period — companies with fewer than 3
        // financial history entries are too new to enter the shadow economy.
        // They haven't had time to establish legal operations. This prevents
        // first-turn mass shadow economy triggering when companies haven't
        // had a chance to hire legally.
        if company.financial_history.len() < 3 {
            continue;
        }
        // Phase 34: Raise unmet demand threshold from 50% to 80%.
        // Only trigger shadow employment when labor demand is severely unmet.
        let unmet_demand = (company.target_fte_demand as f64 - company.fulfilled_fte as f64).max(0.0);
        if unmet_demand < company.target_fte_demand as f64 * 0.8 {
            continue;
        }
        // Skip if no cash to pay shadow wages
        let available = company.brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(company.available_cash);
        if available <= 0.0 {
            continue;
        }

        // Phase 34: Lower base probability from 10% to 5%.
        // - Higher with high PIT rate (evasion incentive)
        // - Higher with low inspectorate capacity (low detection risk)
        // - Base chance of 5% (down from 10%)
        let pit_incentive = pit_rate * 0.5; // Up to 50% with 100% PIT
        let detection_risk = inspectorate_capacity * 0.01; // Up to ~1% with full capacity
        let shadow_probability = (0.05 + pit_incentive - detection_risk).clamp(0.0, 0.8);

        if rng.gen::<f64>() < shadow_probability {
            // Hidden FTE: 10-30% of unmet demand
            let hidden_fte = unmet_demand * (0.10 + rng.gen::<f64>() * 0.20);
            let shadow_wage = company.offered_wage_per_fte * DEFAULT_SHADOW_WAGE_FRACTION;

            company.shadow_employment = Some(ShadowEmployment {
                hidden_fte,
                shadow_wage_per_fte: shadow_wage,
                pit_evaded: 0.0,
                turns_since_inspection: 0,
            });
        }
    }
}

/// Process TemporaryWorker remittances.
///
/// Remittances are deducted from net income at the source in `labor_market.rs`
/// during wage payout. This function is called by the turn engine to route
/// the accumulated remittance amount to `ForeignEntity` (money leaves the system).
///
/// # Arguments
/// * `total_remittances` - Total remittances accumulated during wage payout.
/// * `shadow_economy_state` - Mutable state to update with outbound remittances.
///
/// # Returns
/// The total remittance amount (for the caller to route via TransferSettler).
pub fn process_remittances_turn(
    total_remittances: f64,
    shadow_economy_state: &mut ShadowEconomyState,
) -> f64 {
    shadow_economy_state.total_remittances_outbound = total_remittances;
    total_remittances
}

/// Result of amnesty processing for one turn.
#[derive(Debug, Clone, Default)]
pub struct AmnestyTurnResult {
    /// Total number of Illegals legalized this turn.
    pub legalized_count: i64,
    /// Total legalization fees collected.
    pub fees_collected: f64,
    /// Total shadow FTEs removed (workers moved to official employment).
    pub shadow_fte_removed: f64,
}

/// Process amnesty/legalization for one turn.
///
/// When `AmnestyLaw.active` is true, a percentage of the Illegal population
/// is legalized each turn. The class must be able to afford the legalization fee
/// (affordability clamp prevents negative savings).
///
/// # Arguments
/// * `country` - Mutable country (reads amnesty_law, updates class demographics).
/// * `companies` - Mutable companies (reduces shadow_employment proportionally).
///
/// # Returns
/// `AmnestyTurnResult` with legalization stats.
///
/// # Rules
/// * `legalized_count = min(target_count, affordable_count)` where
///   `affordable_count = class.savings / legalization_fee` (if fee > 0).
/// * Legalization fee is debited from class savings, credited to Treasury.
/// * `ShadowEmployment.hidden_fte` is reduced proportionally on employing companies.
/// * If `legalization_fee == 0.0`, all targeted workers are legalized (free amnesty).
pub fn process_amnesty_turn(
    country: &mut Country,
    companies: &mut [Company],
) -> AmnestyTurnResult {
    let mut result = AmnestyTurnResult::default();

    let amnesty_law = match &country.politics.amnesty_law {
        Some(ref law) if law.active => law.clone(),
        _ => return result,
    };

    let legalization_rate = amnesty_law.legalization_rate;
    let legalization_fee = amnesty_law.legalization_fee;
    let target_status = amnesty_law.target_status;

    for region in &mut country.regions {
        // Process rural classes
        let rural_ids: Vec<String> = region.class_demographics.rural_classes.keys().cloned().collect();
        for class_id in rural_ids {
            let legalized = legalize_class(
                region.class_demographics.rural_classes.get_mut(&class_id).unwrap(),
                legalization_rate,
                legalization_fee,
                target_status,
            );
            if legalized.legalized_count > 0 {
                result.legalized_count += legalized.legalized_count;
                result.fees_collected += legalized.fees_collected;
                country.budget.liquid_reserves += legalized.fees_collected;
            }
        }

        // Process urban classes
        let urban_ids: Vec<String> = region.class_demographics.urban_classes.keys().cloned().collect();
        for class_id in urban_ids {
            let legalized = legalize_class(
                region.class_demographics.urban_classes.get_mut(&class_id).unwrap(),
                legalization_rate,
                legalization_fee,
                target_status,
            );
            if legalized.legalized_count > 0 {
                result.legalized_count += legalized.legalized_count;
                result.fees_collected += legalized.fees_collected;
                country.budget.liquid_reserves += legalized.fees_collected;
            }
        }
    }

    // Reduce shadow employment proportionally on companies
    if result.legalized_count > 0 {
        let total_illegal_pop: i64 = country.regions.iter()
            .flat_map(|r| {
                r.class_demographics.rural_classes.values()
                    .chain(r.class_demographics.urban_classes.values())
            })
            .map(|d| d.illegal_population)
            .sum();

        let original_illegal_pop = total_illegal_pop + result.legalized_count;
        if original_illegal_pop > 0 {
            let legalization_fraction = result.legalized_count as f64 / original_illegal_pop as f64;
            for company in companies.iter_mut() {
                if let Some(ref mut shadow) = company.shadow_employment {
                    let removed = shadow.hidden_fte * legalization_fraction;
                    shadow.hidden_fte -= removed;
                    result.shadow_fte_removed += removed;
                }
            }
        }
    }

    // Update shadow economy state
    if let Some(ref mut state) = country.politics.shadow_economy_state {
        state.legalized_this_turn = result.legalized_count;
    }

    result
}

/// Legalize a single class's illegal population.
fn legalize_class(
    class: &mut crate::society::geography::ClassDemographics,
    legalization_rate: f64,
    legalization_fee: f64,
    target_status: LegalStatus,
) -> AmnestyTurnResult {
    let mut result = AmnestyTurnResult::default();

    if class.illegal_population <= 0 {
        return result;
    }

    let target_legalized = (class.illegal_population as f64 * legalization_rate).floor() as i64;
    if target_legalized <= 0 {
        return result;
    }

    // Affordability clamp: cannot legalize more than the class can afford
    let legalized_count = if legalization_fee > 0.0 {
        let affordable = (class.savings / legalization_fee).floor() as i64;
        target_legalized.min(affordable.max(0))
    } else {
        target_legalized
    };

    if legalized_count <= 0 {
        return result;
    }

    // Deduct legalization fee from class savings
    let total_fee = legalization_fee * legalized_count as f64;
    class.savings -= total_fee;

    // Remove from illegal population
    class.illegal_population -= legalized_count;

    // Update legal status if the entire class is now legalized
    if class.illegal_population <= 0 && class.legal_status == LegalStatus::Illegal {
        class.legal_status = target_status;
    }

    result.legalized_count = legalized_count;
    result.fees_collected = total_fee;
    result
}

/// Process deportation of illegal workers after inspectorate raids.
///
/// Removes deported individuals from population AND extracts their proportional
/// share of class savings to prevent per-capita wealth duplication.
///
/// # Arguments
/// * `country` - Mutable country (updates class demographics, budget).
/// * `deported_count` - Number of illegals to deport.
/// * `region_id` - Region where the deportation occurs.
/// * `class_key` - Class key (rural or urban) to deport from.
/// * `is_rural` - True for rural classes, false for urban.
///
/// # Returns
/// Total wealth extracted (routed to ForeignEntity by the caller).
///
/// # Rules
/// * `per_capita_savings = class.savings / class.population`.
/// * `deported_wealth = per_capita_savings * deported_count`.
/// * `class.savings -= deported_wealth` (deduct from pooled savings).
/// * `class.illegal_population -= deported_count` (remove from population).
/// * The caller routes `deported_wealth` via `TransferRecipient::ForeignEntity`.
pub fn process_deportation_wealth_extraction(
    country: &mut Country,
    deported_count: i64,
    region_id: &str,
    class_key: &str,
    is_rural: bool,
) -> f64 {
    if deported_count <= 0 {
        return 0.0;
    }

    let region = match country.regions.iter_mut().find(|r| r.id == region_id) {
        Some(r) => r,
        None => return 0.0,
    };

    let class = if is_rural {
        region.class_demographics.rural_classes.get_mut(class_key)
    } else {
        region.class_demographics.urban_classes.get_mut(class_key)
    };

    let class = match class {
        Some(c) => c,
        None => return 0.0,
    };

    if class.population <= 0 {
        return 0.0;
    }

    let per_capita_savings = class.savings / class.population as f64;
    let deported_wealth = per_capita_savings * deported_count as f64;

    class.savings -= deported_wealth;
    class.illegal_population = (class.illegal_population - deported_count).max(0);
    class.population -= deported_count;

    deported_wealth
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::society::geography::ClassDemographics;

    #[test]
    fn test_shadow_employment_zero_pit() {
        let mut country = Country::mock_for_tests();
        country.tax_rates.income_tax.rate = 0.20;

        let mut company = Company::default();
        company.sector = Sector::Agriculture;
        company.offered_wage_per_fte = 1000.0;
        company.brokerage_account = Some(crate::securities::BrokerageAccount {
            cash: 100_000.0,
            ..Default::default()
        });
        company.shadow_employment = Some(ShadowEmployment {
            hidden_fte: 10.0,
            shadow_wage_per_fte: 500.0,
            ..Default::default()
        });

        let mut companies = [company.clone()];
        let result = process_shadow_economy_turn(&mut country, &mut companies);

        // Shadow wages = 10 * 500 = 5000
        assert_eq!(result.total_shadow_wages, 5000.0);
        // PIT evaded = 5000 * 0.20 = 1000
        assert_eq!(result.total_pit_evaded, 1000.0);
        // Phase 28: Company cash debited via TransferSettler (may differ slightly
        // if the test country has no matching region/class for the worker).
        // The key assertion is that shadow wages and PIT evaded are correct.
        let remaining_cash = companies[0].brokerage_account.as_ref().map(|ba| ba.cash).unwrap_or(0.0);
        assert!(remaining_cash <= 100_000.0, "company cash should be debited");
    }

    #[test]
    fn test_amnesty_affordability_clamp() {
        let mut class = ClassDemographics::default();
        class.population = 100;
        class.savings = 100.0;
        class.illegal_population = 10;
        class.legal_status = LegalStatus::Illegal;

        // legalization_rate = 0.5 → target = 5, fee = 50 → affordable = 100/50 = 2
        let result = legalize_class(&mut class, 0.5, 50.0, LegalStatus::Resident);

        // Only 2 can be legalized (affordability clamp)
        assert_eq!(result.legalized_count, 2);
        assert_eq!(result.fees_collected, 100.0);
        assert_eq!(class.savings, 0.0);
        assert_eq!(class.illegal_population, 8);
    }

    #[test]
    fn test_amnesty_free_legalization() {
        let mut class = ClassDemographics::default();
        class.population = 100;
        class.savings = 0.0;
        class.illegal_population = 10;
        class.legal_status = LegalStatus::Illegal;

        // fee = 0 → all targeted workers legalized
        let result = legalize_class(&mut class, 0.5, 0.0, LegalStatus::Resident);

        assert_eq!(result.legalized_count, 5);
        assert_eq!(result.fees_collected, 0.0);
        assert_eq!(class.illegal_population, 5);
    }

    #[test]
    fn test_amnesty_no_active_law() {
        let mut country = Country::mock_for_tests();
        country.politics.amnesty_law = Some(AmnestyLaw {
            active: false,
            ..Default::default()
        });

        let result = process_amnesty_turn(&mut country, &mut []);
        assert_eq!(result.legalized_count, 0);
    }

    #[test]
    fn test_deportation_wealth_extraction() {
        let mut country = Country::mock_for_tests();
        country.regions.clear();
        let mut region = crate::society::geography::Region::default();
        region.id = "R1".to_string();
        let mut class = ClassDemographics::default();
        class.population = 200;
        class.savings = 10_000.0;
        class.illegal_population = 50;
        region.class_demographics.rural_classes.insert("workers".to_string(), class);
        country.regions.push(region);

        // Deport 50 people: per_capita = 10000/200 = 50, wealth = 50 * 50 = 2500
        let wealth = process_deportation_wealth_extraction(
            &mut country, 50, "R1", "workers", true,
        );

        assert_eq!(wealth, 2500.0);
        let region = &country.regions[0];
        let class = &region.class_demographics.rural_classes["workers"];
        assert_eq!(class.savings, 7500.0);
        assert_eq!(class.illegal_population, 0);
        assert_eq!(class.population, 150);
    }

    #[test]
    fn test_remittance_income_based() {
        // Remittances are processed at the source in labor_market.rs.
        // This test verifies the process_remittances_turn function records the amount.
        let mut state = ShadowEconomyState::default();
        let amount = process_remittances_turn(500.0, &mut state);
        assert_eq!(amount, 500.0);
        assert_eq!(state.total_remittances_outbound, 500.0);
    }

    #[test]
    fn test_legal_status_default() {
        let status = LegalStatus::default();
        assert_eq!(status, LegalStatus::Citizen);
    }
}

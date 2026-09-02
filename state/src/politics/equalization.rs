//! Phase D.9: Centralized Equalization Payments (Janosikowe)
//!
//! Equalization payments redistribute tax revenue from wealthy regions to
//! poor regions through the central Treasury as a mandatory clearinghouse.
//! Direct peer-to-peer transfers between regions are prohibited.
//!
//! ## Three-Step Clearinghouse Flow
//!
//! 1. **Collection**: Debit rich regions → credit `Treasury.equalization_fund`.
//! 2. **Distribution**: Debit `equalization_fund` → credit poor regions
//!    (pro-rata if entitlements exceed the fund).
//! 3. **Remainder Sweep**: Any unallocated balance in the fund is swept to
//!    `Treasury.liquid_reserves` (central state retains surplus as
//!    administrative fee). This guarantees `equalization_fund == 0.0` after
//!    distribution.
//!
//! ## Conservation Invariant
//!
//! After all three steps:
//! - `sum(all regional liquid_reserves) + country.budget.liquid_reserves
//!    + country.budget.equalization_fund == total_currency_before`
//! - `country.budget.equalization_fund == 0.0` (guaranteed by Step 3).
//! - `sum(rich_debits) == sum(poor_credits) + remainder_swept`.

use crate::state::Country;
use serde::{Deserialize, Serialize};

/// Configuration for equalization payments.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EqualizationConfig {
    /// Per-capita revenue threshold (fraction of national average) below
    /// which a region qualifies for equalization. E.g., 0.8 means regions
    /// below 80% of the national average per-capita revenue receive transfers.
    #[serde(default = "default_equalization_threshold")]
    pub equalization_threshold: f64,

    /// Fraction of above-average surplus extracted from rich regions.
    #[serde(default = "default_equalization_rate")]
    pub equalization_rate: f64,
}

fn default_equalization_threshold() -> f64 {
    0.8
}
fn default_equalization_rate() -> f64 {
    0.3
}

impl Default for EqualizationConfig {
    fn default() -> Self {
        EqualizationConfig {
            equalization_threshold: default_equalization_threshold(),
            equalization_rate: default_equalization_rate(),
        }
    }
}

/// Process equalization payments through the central Treasury clearinghouse.
///
/// Three-step flow:
/// 1. Collection: debit rich regions, credit `equalization_fund`.
/// 2. Distribution: debit `equalization_fund`, credit poor regions (pro-rata).
/// 3. Remainder sweep: sweep any unallocated fund to `liquid_reserves`.
///
/// Guarantees `equalization_fund == 0.0` after execution.
pub fn process_equalization(country: &mut Country, config: &EqualizationConfig) {
    // Compute per-capita revenue for all regions.
    let region_revenues: Vec<(String, f64, f64)> = country
        .regions
        .iter()
        .filter_map(|r| {
            let gov = r.governance.as_ref()?;
            let pop = r.population as f64;
            if pop <= 0.0 {
                return None;
            }
            let per_capita = gov.budget.tax_revenue / pop;
            Some((r.id.clone(), per_capita, pop))
        })
        .collect();

    if region_revenues.is_empty() {
        return;
    }

    // Compute national average per-capita revenue.
    let total_pop: f64 = region_revenues.iter().map(|(_, _, pop)| *pop).sum();
    let total_revenue: f64 = country
        .regions
        .iter()
        .filter_map(|r| r.governance.as_ref().map(|g| g.budget.tax_revenue))
        .sum();

    if total_pop <= 0.0 {
        return;
    }
    let national_avg = total_revenue / total_pop;
    if national_avg <= 0.0 {
        return;
    }

    // Step 1: Collection — debit rich regions, credit equalization_fund.
    let mut total_collected = 0.0_f64;
    for (region_id, per_capita, _pop) in &region_revenues {
        if *per_capita <= national_avg {
            continue;
        }
        let surplus = (per_capita - national_avg)
            * country
                .regions
                .iter()
                .find(|r| &r.id == region_id)
                .map(|r| r.population as f64)
                .unwrap_or(0.0);
        let debit = surplus * config.equalization_rate;

        if debit <= 0.0 {
            continue;
        }

        // Clamp to available reserves.
        let available = country
            .regions
            .iter()
            .find(|r| &r.id == region_id)
            .and_then(|r| r.governance.as_ref())
            .map(|g| g.budget.liquid_reserves.max(0.0))
            .unwrap_or(0.0);
        let actual_debit = debit.min(available);
        if actual_debit <= 0.0 {
            continue;
        }

        // Debit rich region.
        if let Some(region) = country.regions.iter_mut().find(|r| &r.id == region_id) {
            if let Some(governance) = region.governance.as_mut() {
                governance.budget.liquid_reserves -= actual_debit;
            }
        }
        total_collected += actual_debit;
    }

    // Credit equalization_fund.
    country.budget.equalization_fund += total_collected;

    // Step 2: Distribution — debit equalization_fund, credit poor regions.
    let threshold = national_avg * config.equalization_threshold;
    let mut entitlements: Vec<(String, f64)> = Vec::new();
    let mut total_entitlements = 0.0_f64;

    for (region_id, per_capita, pop) in &region_revenues {
        if *per_capita >= threshold {
            continue;
        }
        // Entitlement proportional to shortfall below threshold, scaled by pop.
        let shortfall = threshold - per_capita;
        let entitlement = shortfall * pop;
        if entitlement > 0.0 {
            entitlements.push((region_id.clone(), entitlement));
            total_entitlements += entitlement;
        }
    }

    let fund_available = country.budget.equalization_fund;
    let collection_ratio = if total_entitlements > 0.0 {
        (fund_available / total_entitlements).min(1.0)
    } else {
        0.0
    };

    let mut total_distributed = 0.0_f64;
    for (region_id, entitlement) in &entitlements {
        let actual_transfer = entitlement * collection_ratio;
        if actual_transfer <= 0.0 {
            continue;
        }
        if let Some(region) = country.regions.iter_mut().find(|r| &r.id == region_id) {
            if let Some(governance) = region.governance.as_mut() {
                governance.budget.liquid_reserves += actual_transfer;
            }
        }
        total_distributed += actual_transfer;
    }

    // Debit equalization_fund by the distributed amount.
    country.budget.equalization_fund -= total_distributed;

    // Step 3: Remainder sweep — sweep any unallocated fund to liquid_reserves.
    if country.budget.equalization_fund > 0.0 {
        let remainder = country.budget.equalization_fund;
        country.budget.liquid_reserves += remainder;
        country.budget.equalization_fund = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::politics::local_government::RegionalGovernance;
    use crate::society::geography::Region;

    fn make_region(id: &str, pop: i64, tax_revenue: f64, reserves: f64) -> Region {
        let mut region = Region::default();
        region.id = id.to_string();
        region.population = pop;
        let mut gov = RegionalGovernance::default();
        gov.budget.tax_revenue = tax_revenue;
        gov.budget.liquid_reserves = reserves;
        region.governance = Some(gov);
        region
    }

    #[test]
    fn test_equalization_surplus_scenario() {
        // Rich region: per-capita = 1000, Poor region: per-capita = 200
        // National avg = 600. Threshold = 600 * 0.8 = 480.
        // Rich surplus = (1000 - 600) * 1000 = 400000. Debit = 400000 * 0.3 = 120000.
        // Poor shortfall = (480 - 200) * 1000 = 280000. Entitlement = 280000.
        // Fund = 120000. Collection ratio = 120000 / 280000 = 0.4286.
        // Poor receives 280000 * 0.4286 = 120000. Fund = 0. No remainder.
        let mut country = Country::default();
        country.regions.push(make_region("RICH", 1000, 1_000_000.0, 500_000.0));
        country.regions.push(make_region("POOR", 1000, 200_000.0, 10_000.0));

        let config = EqualizationConfig::default();
        process_equalization(&mut country, &config);

        // Fund must be zero.
        assert!((country.budget.equalization_fund).abs() < 1e-6);

        // Rich region should have been debited.
        let rich_reserves = country.regions[0].governance.as_ref().unwrap().budget.liquid_reserves;
        assert!(rich_reserves < 500_000.0);

        // Poor region should have been credited.
        let poor_reserves = country.regions[1].governance.as_ref().unwrap().budget.liquid_reserves;
        assert!(poor_reserves > 10_000.0);
    }

    #[test]
    fn test_equalization_remainder_sweep() {
        // Rich region with huge surplus, poor region with tiny shortfall.
        // Collection > entitlements → remainder swept to liquid_reserves.
        let mut country = Country::default();
        country.budget.liquid_reserves = 1_000_000.0;
        country.regions.push(make_region("RICH", 10000, 10_000_000.0, 5_000_000.0));
        country.regions.push(make_region("POOR", 100, 480_000.0, 1_000.0));
        // Per-capita: RICH=1000, POOR=4800. National avg = (10M + 480K) / 10100 = 1037.6
        // Threshold = 1037.6 * 0.8 = 830.1. POOR (4800) is above threshold!
        // So POOR doesn't qualify. All collected goes to remainder.
        // Actually let me recalculate: POOR per_capita = 480_000 / 100 = 4800.
        // That's above national avg (1037.6), so POOR is also rich!
        // Let me fix: make POOR actually poor.
        country.regions[1].governance.as_mut().unwrap().budget.tax_revenue = 10_000.0;
        // Now POOR per_capita = 10000/100 = 100. National avg = (10M + 10K) / 10100 = 991.1
        // Threshold = 991.1 * 0.8 = 792.9. POOR (100) < 792.9 → qualifies.
        // RICH surplus = (991.1 - 991.1) * 10000 = 0... wait, RICH per_capita = 1000.
        // RICH surplus = (1000 - 991.1) * 10000 = 89000. Debit = 89000 * 0.3 = 26700.
        // POOR shortfall = (792.9 - 100) * 100 = 69290. Entitlement = 69290.
        // Fund = 26700. Collection ratio = 26700 / 69290 = 0.385.
        // Poor receives 69290 * 0.385 = 26700. Fund = 0. No remainder.

        // To test remainder sweep, I need collection > entitlements.
        // Make POOR have very small shortfall.
        country.regions[1].governance.as_mut().unwrap().budget.tax_revenue = 78_000.0;
        // POOR per_capita = 780. Threshold = 991.1 * 0.8 = 792.9.
        // Shortfall = (792.9 - 780) * 100 = 1290. Entitlement = 1290.
        // Fund = 26700. Collection ratio = min(26700/1290, 1.0) = 1.0.
        // Poor receives 1290. Remainder = 26700 - 1290 = 25410 → swept to liquid_reserves.

        let initial_treasury = country.budget.liquid_reserves;
        let config = EqualizationConfig::default();
        process_equalization(&mut country, &config);

        // Fund must be zero.
        assert!((country.budget.equalization_fund).abs() < 1e-6);

        // Treasury should have received the remainder.
        assert!(country.budget.liquid_reserves > initial_treasury);
    }

    #[test]
    fn test_equalization_no_regions() {
        let mut country = Country::default();
        let config = EqualizationConfig::default();
        process_equalization(&mut country, &config);
        assert!((country.budget.equalization_fund).abs() < 1e-6);
    }

    #[test]
    fn test_equalization_all_equal_regions() {
        // All regions have same per-capita revenue — no equalization.
        let mut country = Country::default();
        country.regions.push(make_region("A", 1000, 500_000.0, 100_000.0));
        country.regions.push(make_region("B", 1000, 500_000.0, 100_000.0));

        let config = EqualizationConfig::default();
        let initial_a = country.regions[0].governance.as_ref().unwrap().budget.liquid_reserves;
        let initial_b = country.regions[1].governance.as_ref().unwrap().budget.liquid_reserves;

        process_equalization(&mut country, &config);

        // No transfers should occur.
        let final_a = country.regions[0].governance.as_ref().unwrap().budget.liquid_reserves;
        let final_b = country.regions[1].governance.as_ref().unwrap().budget.liquid_reserves;
        assert!((final_a - initial_a).abs() < 1e-6);
        assert!((final_b - initial_b).abs() < 1e-6);
        assert!((country.budget.equalization_fund).abs() < 1e-6);
    }
}

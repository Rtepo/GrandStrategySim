//! Phase D.8: JST (Local Government Unit) B2B Procurement Spending
//!
//! Regional governments (JSTs) submit formal Buy Orders to the B2B marketplace
//! for `ConstructionMachinery` (infrastructure maintenance) and
//! `AdministrativeServices` (local administration operations).
//!
//! ## Strict Market Clearing
//!
//! JSTs cannot simply debit reserves and credit providers. They must submit
//! formal `Bid` entries to the `OrderBook`. If the market cannot fulfill the
//! order, the JST cash remains unspent (refunded), and local infrastructure
//! degrades physically.
//!
//! ## Double-Entry Compliance
//!
//! 1. **Bid submission**: Debit `governance.budget.liquid_reserves` (encumber).
//! 2. **On fill**: Seller credited via `settle_trades` (which handles bank
//!    balance sheet sync). JST encumbrance was already debited.
//! 3. **On unfill**: Refund encumbrance back to `liquid_reserves`.
//! 4. **Infrastructure degradation**: If procurement is insufficient,
//!    `region.infrastructure_level` decays. `development_level` is NOT
//!    affected — it is a deep socioeconomic indicator (HDI-like).

use crate::economy::order_book::{Bid, OrderBook};
use crate::entities::Company;
use crate::registries::enums::Commodity;
use crate::state::Country;
use serde::{Deserialize, Serialize};

/// Configuration for JST spending behavior.
///
/// All values are configurable and intended to be overridden at runtime
/// from macroeconomic aggregates to ensure inflation-proof scaling.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JstSpendingConfig {
    /// Fraction of `local_expenditures` allocated to infrastructure maintenance
    /// (the rest goes to administrative services).
    #[serde(default = "default_infrastructure_fraction")]
    pub infrastructure_maintenance_fraction: f64,

    /// Markup over last known market price for JST bids. JSTs are
    /// price-insensitive buyers but must still respect market clearing.
    #[serde(default = "default_bid_price_markup")]
    pub bid_price_markup: f64,

    /// Rate at which `infrastructure_level` decays per turn if procurement
    /// is unfilled. Represents physical wear and tear.
    #[serde(default = "default_infrastructure_decay_rate")]
    pub infrastructure_decay_rate: f64,

    /// Maintenance requirement coefficient: how many units of
    /// ConstructionMachinery are needed per unit of infrastructure_level.
    /// Derived from physical scaling (Rule 15).
    #[serde(default = "default_maintenance_coefficient")]
    pub maintenance_coefficient: f64,

    /// Administrative services requirement per capita (scaled by average_wage).
    #[serde(default = "default_admin_per_capita_coefficient")]
    pub admin_per_capita_coefficient: f64,
}

fn default_infrastructure_fraction() -> f64 {
    0.7
}
fn default_bid_price_markup() -> f64 {
    0.05
}
fn default_infrastructure_decay_rate() -> f64 {
    0.02
}
fn default_maintenance_coefficient() -> f64 {
    0.1
}
fn default_admin_per_capita_coefficient() -> f64 {
    0.001
}

impl Default for JstSpendingConfig {
    fn default() -> Self {
        JstSpendingConfig {
            infrastructure_maintenance_fraction: default_infrastructure_fraction(),
            bid_price_markup: default_bid_price_markup(),
            infrastructure_decay_rate: default_infrastructure_decay_rate(),
            maintenance_coefficient: default_maintenance_coefficient(),
            admin_per_capita_coefficient: default_admin_per_capita_coefficient(),
        }
    }
}

/// Submit JST B2B procurement bids to the local order book.
///
/// For each region with `local_expenditures > 0`:
/// 1. Compute the procurement budget (clamped to available reserves).
/// 2. Split into `ConstructionMachinery` and `AdministrativeServices` demands.
/// 3. Submit formal `Bid` entries to the `OrderBook`.
/// 4. Encumber (debit) `liquid_reserves` by the bid value.
///
/// Returns a map of `region_id -> total_encumbered` for later refund processing.
pub fn submit_jst_procurement_bids(
    country: &mut Country,
    companies: &[Company],
    order_book: &mut OrderBook,
    config: &JstSpendingConfig,
) -> std::collections::HashMap<String, f64> {
    let mut encumbrances: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();

    let avg_wage = country.macro_indicators.average_wage.max(1.0);

    for region in &mut country.regions {
        let region_id = region.id.clone();
        let Some(governance) = region.governance.as_mut() else {
            continue;
        };

        let local_expenditures = governance.budget.local_expenditures;
        if local_expenditures <= 0.0 {
            continue;
        }

        // Clamp procurement budget to available reserves.
        let available = governance.budget.liquid_reserves.max(0.0);
        let procurement_budget = local_expenditures.min(available);
        if procurement_budget <= 0.0 {
            continue;
        }

        // Split budget into infrastructure and administrative portions.
        let infra_budget = procurement_budget * config.infrastructure_maintenance_fraction;
        let admin_budget = procurement_budget * (1.0 - config.infrastructure_maintenance_fraction);

        // Determine limit prices from last known market prices (or fallback
        // to average_wage-based estimates — Rule 2: no magic numbers).
        let infra_price = get_reference_price(Commodity::ConstructionMachinery, companies)
            .max(avg_wage * 10.0);
        let admin_price = get_reference_price(Commodity::AdministrativeServices, companies)
            .max(avg_wage);

        // Apply markup.
        let infra_limit = infra_price * (1.0 + config.bid_price_markup);
        let admin_limit = admin_price * (1.0 + config.bid_price_markup);

        let mut total_encumbered = 0.0_f64;

        // Submit ConstructionMachinery bid.
        if infra_limit > 0.0 && infra_budget > 0.0 {
            let quantity = infra_budget / infra_limit;
            if quantity > 0.0 {
                total_encumbered += infra_budget;
                order_book.bids.entry(Commodity::ConstructionMachinery).or_default().push(Bid {
                    buyer_id: format!("JST-{}", region_id),
                    commodity: Commodity::ConstructionMachinery,
                    quantity,
                    limit_price: infra_limit,
                    blueprint_id: None,
                    min_quality: None,
                });
            }
        }

        // Submit AdministrativeServices bid.
        if admin_limit > 0.0 && admin_budget > 0.0 {
            let quantity = admin_budget / admin_limit;
            if quantity > 0.0 {
                total_encumbered += admin_budget;
                order_book.bids.entry(Commodity::AdministrativeServices).or_default().push(Bid {
                    buyer_id: format!("JST-{}", region_id),
                    commodity: Commodity::AdministrativeServices,
                    quantity,
                    limit_price: admin_limit,
                    blueprint_id: None,
                    min_quality: None,
                });
            }
        }

        // Encumber (debit) liquid_reserves.
        if total_encumbered > 0.0 {
            governance.budget.liquid_reserves -= total_encumbered;
            encumbrances.insert(region_id, total_encumbered);
        }
    }

    encumbrances
}

/// Get a reference price for a commodity from company unfilled bid prices
/// or building inventories. Falls back to 0.0 if no data available.
fn get_reference_price(commodity: Commodity, companies: &[Company]) -> f64 {
    // Try to find a company that has an unfilled bid price for this commodity.
    for company in companies {
        if let Some(&price) = company.unfilled_bid_prices.get(&commodity) {
            if price > 0.0 {
                return price;
            }
        }
    }
    0.0
}

/// Refund unfilled JST bids after order matching.
///
/// Scans the `OrderBook` for remaining unfilled bids where `buyer_id` starts
/// with `"JST-"`. Refunds the encumbered amount back to the corresponding
/// region's `liquid_reserves`.
pub fn refund_unfilled_jst_bids(
    order_book: &OrderBook,
    country: &mut Country,
) {
    for bids in order_book.bids.values() {
        for bid in bids {
            if !bid.buyer_id.starts_with("JST-") || bid.quantity <= 0.0 {
                continue;
            }
            let refund = bid.quantity * bid.limit_price;
            if refund <= 0.0 {
                continue;
            }
            // Extract region_id from buyer_id "JST-{region_id}".
            let region_id = &bid.buyer_id[4..];
            if let Some(region) = country.regions.iter_mut().find(|r| r.id == region_id) {
                if let Some(governance) = region.governance.as_mut() {
                    governance.budget.liquid_reserves += refund;
                }
            }
        }
    }
}

/// Settle JST procurement trades after order matching.
///
/// For each executed `Trade` where `buyer_id` starts with `"JST-"`:
/// - Credit the seller via `TransferSettler::credit_company_by_id` (handles
///   bank balance sheet sync atomically — Black Hole 1.19 pattern).
/// - The JST's encumbrance was already debited at bid submission.
/// - Record the procured quantity for the infrastructure update phase.
///
/// `settle_trades` may also credit the seller via its fallback path, but
/// that path does not sync the bank balance sheet. This function ensures
/// proper double-entry by using the TransferSettler for the full credit.
///
/// Returns a map of `(region_id, commodity) -> procured_quantity`.
pub fn settle_jst_trades(
    trades: &[crate::economy::order_book::Trade],
    companies: &mut [Company],
) -> std::collections::HashMap<(String, Commodity), f64> {
    let mut procured: std::collections::HashMap<(String, Commodity), f64> =
        std::collections::HashMap::new();

    for trade in trades {
        if !trade.buyer_id.starts_with("JST-") {
            continue;
        }
        let region_id = trade.buyer_id[4..].to_string();
        let key = (region_id, trade.commodity);
        *procured.entry(key).or_insert(0.0) += trade.quantity;

        // Credit seller via TransferSettler for proper bank balance sheet sync.
        let trade_value = trade.quantity * trade.execution_price;
        if trade_value > 0.0 {
            crate::economy::transfer_settler::credit_company_by_id(
                companies,
                &trade.seller_id,
                trade_value,
            );
        }
    }

    procured
}

/// Update regional infrastructure based on procured ConstructionMachinery.
///
/// For each region:
/// - Compare procured `ConstructionMachinery` against the maintenance
///   requirement (derived from `infrastructure_level * decay_rate`).
/// - If procurement is insufficient, reduce **only** `infrastructure_level`
///   by the shortfall fraction. `development_level` is NOT affected.
/// - If procurement meets or exceeds the requirement, maintain
///   `infrastructure_level` (prevent decay).
pub fn update_jst_infrastructure(
    country: &mut Country,
    procured: &std::collections::HashMap<(String, Commodity), f64>,
    config: &JstSpendingConfig,
) {
    for region in &mut country.regions {
        let region_id = region.id.clone();
        let infra_procured = procured
            .get(&(region_id.clone(), Commodity::ConstructionMachinery))
            .copied()
            .unwrap_or(0.0);

        // Use the regional treasury's infrastructure_level as the physical
        // infrastructure metric. This reflects physical wear and tear on
        // local utilities, roads, and municipal infrastructure.
        let infra_level = region.treasury.infrastructure_level;

        // Maintenance requirement: how much ConstructionMachinery is needed
        // to prevent decay. Scaled by infrastructure_level (Rule 15).
        let maintenance_requirement = infra_level * config.maintenance_coefficient;

        if infra_procured >= maintenance_requirement {
            // Procurement meets requirement — no decay.
            // Slight improvement if significantly exceeds.
            if maintenance_requirement > 0.0 && infra_procured > maintenance_requirement * 1.5 {
                region.treasury.infrastructure_level += 0.01;
            }
        } else if maintenance_requirement > 0.0 {
            // Procurement insufficient — decay infrastructure_level.
            // Only infrastructure_level is affected. development_level is
            // a deep socioeconomic indicator (HDI-like) and must NOT decay
            // from a single missed maintenance turn.
            let shortfall_ratio =
                (maintenance_requirement - infra_procured) / maintenance_requirement;
            let decay = infra_level * config.infrastructure_decay_rate * shortfall_ratio;
            region.treasury.infrastructure_level = (infra_level - decay).max(0.0);
        }
        // If maintenance_requirement is 0 (no infrastructure), nothing to decay.
    }
}

/// Collect local fees from citizens and companies for JST-provided services.
///
/// Debits from citizen savings and company `available_cash` (clamped to
/// available). Credits to `governance.budget.liquid_reserves`.
/// Fee rates derived from `average_wage` (Rule 2 — no magic numbers).
pub fn collect_local_fees(
    country: &mut Country,
    companies: &mut [Company],
    config: &JstSpendingConfig,
) {
    let avg_wage = country.macro_indicators.average_wage.max(1.0);
    let fee_per_capita = avg_wage * config.admin_per_capita_coefficient;

    for region in &mut country.regions {
        let population = region.population as f64;
        let total_fees = fee_per_capita * population;
        if total_fees <= 0.0 {
            continue;
        }

        // Debit from citizen class savings (proportional to class population).
        let total_class_pop: i64 = region
            .class_demographics
            .rural_classes
            .values()
            .chain(region.class_demographics.urban_classes.values())
            .map(|d| d.population)
            .sum();

        if total_class_pop <= 0 {
            continue;
        }

        let mut total_collected = 0.0_f64;
        let fee_per_person = total_fees / total_class_pop as f64;

        // Collect from rural classes.
        for demo in region.class_demographics.rural_classes.values_mut() {
            let class_fee = fee_per_person * demo.population as f64;
            let actual = class_fee.min(demo.savings);
            demo.savings -= actual;
            if demo.population > 0 {
                demo.savings_per_capita = demo.savings / demo.population as f64;
            }
            total_collected += actual;
        }

        // Collect from urban classes.
        for demo in region.class_demographics.urban_classes.values_mut() {
            let class_fee = fee_per_person * demo.population as f64;
            let actual = class_fee.min(demo.savings);
            demo.savings -= actual;
            if demo.population > 0 {
                demo.savings_per_capita = demo.savings / demo.population as f64;
            }
            total_collected += actual;
        }

        // Credit collected fees to JST reserves.
        if total_collected > 0.0 {
            if let Some(governance) = region.governance.as_mut() {
                governance.budget.local_fees = total_collected;
                governance.budget.liquid_reserves += total_collected;
            }
        }
    }

    // Note: Company fee collection would require iterating companies by region.
    // For now, citizen fees are the primary source. Company fees can be added
    // when company-to-region mapping is more robustly tracked.
    let _ = companies;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::politics::local_government::RegionalGovernance;
    use crate::society::geography::Region;

    #[test]
    fn test_jst_spending_config_default() {
        let config = JstSpendingConfig::default();
        assert!(config.infrastructure_maintenance_fraction > 0.0);
        assert!(config.infrastructure_maintenance_fraction < 1.0);
        assert!(config.bid_price_markup >= 0.0);
        assert!(config.infrastructure_decay_rate > 0.0);
    }

    #[test]
    fn test_submit_jst_bids_encumbers_reserves() {
        let mut country = Country::default();
        country.macro_indicators.average_wage = 100.0;
        let mut region = Region::default();
        region.id = "REG-001".to_string();
        region.population = 10000;
        region.treasury.infrastructure_level = 50.0;
        let mut gov = RegionalGovernance::default();
        gov.budget.local_expenditures = 5000.0;
        gov.budget.liquid_reserves = 10000.0;
        region.governance = Some(gov);
        country.regions.push(region);

        let companies: Vec<Company> = vec![];
        let mut order_book = OrderBook::default();
        let config = JstSpendingConfig::default();

        let encumbrances = submit_jst_procurement_bids(&mut country, &companies, &mut order_book, &config);

        // Should have encumbered the full local_expenditures (clamped to reserves).
        assert!(encumbrances.contains_key("REG-001"));
        let encumbered = encumbrances["REG-001"];
        assert!(encumbered > 0.0);
        assert!(encumbered <= 5000.0);

        // Reserves should be reduced by the encumbered amount.
        let remaining = country.regions[0].governance.as_ref().unwrap().budget.liquid_reserves;
        assert!((remaining - (10000.0 - encumbered)).abs() < 1e-6);

        // Order book should have bids for ConstructionMachinery and AdministrativeServices.
        assert!(order_book.bids.contains_key(&Commodity::ConstructionMachinery));
        assert!(order_book.bids.contains_key(&Commodity::AdministrativeServices));
    }

    #[test]
    fn test_submit_jst_bids_skips_zero_expenditures() {
        let mut country = Country::default();
        country.macro_indicators.average_wage = 100.0;
        let mut region = Region::default();
        region.id = "REG-002".to_string();
        let mut gov = RegionalGovernance::default();
        gov.budget.local_expenditures = 0.0;
        gov.budget.liquid_reserves = 10000.0;
        region.governance = Some(gov);
        country.regions.push(region);

        let companies: Vec<Company> = vec![];
        let mut order_book = OrderBook::default();
        let config = JstSpendingConfig::default();

        let encumbrances = submit_jst_procurement_bids(&mut country, &companies, &mut order_book, &config);
        assert!(encumbrances.is_empty());
        assert!(order_book.bids.is_empty());
    }

    #[test]
    fn test_refund_unfilled_jst_bids() {
        let mut country = Country::default();
        let mut region = Region::default();
        region.id = "REG-003".to_string();
        let mut gov = RegionalGovernance::default();
        gov.budget.liquid_reserves = 1000.0;
        region.governance = Some(gov);
        country.regions.push(region);

        let mut order_book = OrderBook::default();
        order_book.bids.entry(Commodity::ConstructionMachinery).or_default().push(Bid {
            buyer_id: "JST-REG-003".to_string(),
            commodity: Commodity::ConstructionMachinery,
            quantity: 10.0,
            limit_price: 50.0,
            blueprint_id: None,
            min_quality: None,
        });

        refund_unfilled_jst_bids(&order_book, &mut country);

        // Reserves should be refunded by 10 * 50 = 500.
        let reserves = country.regions[0].governance.as_ref().unwrap().budget.liquid_reserves;
        assert!((reserves - 1500.0).abs() < 1e-6);
    }

    #[test]
    fn test_infrastructure_decay_on_unfilled_procurement() {
        let mut country = Country::default();
        let mut region = Region::default();
        region.id = "REG-004".to_string();
        region.treasury.infrastructure_level = 100.0;
        country.regions.push(region);

        let procured: std::collections::HashMap<(String, Commodity), f64> =
            std::collections::HashMap::new();
        // No procurement for this region — infrastructure should decay.
        let config = JstSpendingConfig::default();

        update_jst_infrastructure(&mut country, &procured, &config);

        // infrastructure_level should have decreased.
        assert!(country.regions[0].treasury.infrastructure_level < 100.0);
        assert!(country.regions[0].treasury.infrastructure_level > 0.0);
    }

    #[test]
    fn test_infrastructure_maintained_on_filled_procurement() {
        let mut country = Country::default();
        let mut region = Region::default();
        region.id = "REG-005".to_string();
        region.treasury.infrastructure_level = 100.0;
        country.regions.push(region);

        let config = JstSpendingConfig::default();
        let maintenance_requirement = 100.0 * config.maintenance_coefficient;

        let mut procured: std::collections::HashMap<(String, Commodity), f64> =
            std::collections::HashMap::new();
        // Provide enough to meet the requirement.
        procured.insert(
            ("REG-005".to_string(), Commodity::ConstructionMachinery),
            maintenance_requirement,
        );

        update_jst_infrastructure(&mut country, &procured, &config);

        // infrastructure_level should be maintained (no decay).
        assert!((country.regions[0].treasury.infrastructure_level - 100.0).abs() < 1e-6);
    }

    #[test]
    fn test_development_level_not_affected_by_procurement() {
        let mut country = Country::default();
        let mut region = Region::default();
        region.id = "REG-006".to_string();
        region.treasury.infrastructure_level = 100.0;
        region.development_level = 50.0;
        country.regions.push(region);

        let procured: std::collections::HashMap<(String, Commodity), f64> =
            std::collections::HashMap::new();
        let config = JstSpendingConfig::default();

        update_jst_infrastructure(&mut country, &procured, &config);

        // development_level must NOT change from procurement shortfall.
        assert!((country.regions[0].development_level - 50.0).abs() < 1e-6);
    }
}

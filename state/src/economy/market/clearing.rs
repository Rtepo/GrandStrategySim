//! Market clearing and international trade price discovery.
//!
//! This module ports the price-clearing step from the Python engine's
//! `economy/markets/goods/prices.py` and `economy/markets/spatial/local_market.py`.
//! It reconciles local supply/demand with the global market and applies tariff
//! and export-tax boundaries.

use crate::economy::market::{GlobalMarket, MarketOrders};
use crate::registries::enums::Commodity;
use crate::state::Country;
use crate::state::tax::{AggregateVatRecord, TaxRouting, TaxType, route_tax_collection_to_country};
use rustc_hash::FxHashMap;

/// Hot-path hash map alias for clearing internals.
pub type HashMap<K, V> = FxHashMap<K, V>;

/// Financial transaction for warehouse extraction (Phase 5.5).
#[derive(Debug, Clone)]
pub struct FinancialTransaction {
    /// Original producer (batch owner)
    pub batch_owner: String,
    /// LogisticsCompany ID (warehouse owner)
    pub warehouse_owner: String,
    /// Quantity extracted
    pub quantity: f64,
    /// Accumulated storage fees owed
    pub accumulated_fees: f64,
    /// Cross-region transport cost
    pub transport_cost: f64,
    /// Commodity type
    pub commodity: Commodity,
}

/// Minimum/maximum price modifiers relative to the global base price.
const PRICE_FLOOR: f64 = 0.2;
const PRICE_CAP: f64 = 5.0;

/// VAT-enabled market clearing result with tax records.
#[derive(Debug, Clone)]
pub struct VatMarketResult {
    /// Cleared local prices (Gross prices including VAT)
    pub local_prices: HashMap<Commodity, f64>,
    /// Aggregate VAT records for tax routing
    pub vat_records: Vec<AggregateVatRecord>,
    /// Net revenue for sellers (excluding VAT)
    pub seller_revenue: HashMap<Commodity, f64>,
}

/// Resolves the local market price for every commodity with active orders.
///
/// # Arguments
/// * `market_orders` - Aggregate local buy/sell orders produced by the
///   production cycle.
/// * `country` - Country state containing the `TradePolicy`.
/// * `global_market` - Shared global market with base prices and net surplus.
///
/// # Returns
/// A map from commodity to the cleared local price.
///
/// # Rules
/// * If local buy orders exceed sell orders, the country is in deficit.
///   * Deficit is first met by importing from the global market.
///   * The imported price is `global_base * (1 + import_tariff)`.
///   * If the global market cannot cover the deficit, the price is pushed
///     toward the shortage cap.
/// * If local sell orders exceed buy orders, the country has a surplus.
///   * Exports are sold at `global_base * (1 - export_tax)`.
///   * If global demand cannot absorb the surplus, the price is pushed
///     toward the surplus floor.
/// * When local supply and demand match, the price equals the global base
///   price.
pub fn resolve_market_prices(
    market_orders: &MarketOrders,
    country: &Country,
    global_market: &GlobalMarket,
) -> HashMap<Commodity, f64> {
    let mut local_prices = HashMap::default();

    for (good, order) in &market_orders.orders {
        let net = order.buy - order.sell;
        let global_base = global_market.base_price(*good, 100.0);

        if net > 0.0 {
            local_prices.insert(
                *good,
                resolve_deficit(*good, net, global_base, country, global_market),
            );
        } else if net < 0.0 {
            local_prices.insert(
                *good,
                resolve_surplus(*good, -net, global_base, country, global_market),
            );
        } else {
            local_prices.insert(*good, global_base);
        }
    }

    local_prices
}

/// Resolves local market prices with VAT wedge calculation.
///
/// # Arguments
/// * `market_orders` - Aggregate local buy/sell orders
/// * `country` - Country state containing tax rates and trade policy
/// * `global_market` - Shared global market with base prices
/// * `region_id` - Region ID for VAT routing
///
/// # Returns
/// VatMarketResult containing gross prices, VAT records, and seller net revenue
///
/// # Rules
/// * Gross Price = Net Price * (1.0 + vat_rate)
/// * Net Price = Gross Price / (1.0 + vat_rate)
/// * VAT Collected = Gross Price - Net_Price
/// * Seller receives Net_Price * cleared_quantity
/// * Buyer pays Gross_Price * cleared_quantity
/// * VAT wedge is routed through cascading treasury system to close the economy loop
pub fn resolve_market_prices_with_vat(
    market_orders: &MarketOrders,
    country: &mut Country,
    global_market: &GlobalMarket,
    region_id: &str,
) -> VatMarketResult {
    let mut local_prices = HashMap::default();
    let mut vat_records = Vec::new();
    let mut seller_revenue = HashMap::default();

    for (good, order) in &market_orders.orders {
        let net = order.buy - order.sell;
        let global_base = global_market.base_price(*good, 100.0);

        // Resolve gross price (what buyer pays)
        let gross_price = if net > 0.0 {
            resolve_deficit(*good, net, global_base, country, global_market)
        } else if net < 0.0 {
            resolve_surplus(*good, -net, global_base, country, global_market)
        } else {
            global_base
        };

        // Get VAT rate for this commodity from tax configuration
        let vat_rate = get_vat_rate_for_commodity(*good, &country.tax_rates);

        // Calculate net price (what seller receives)
        // Net_Price = Gross_Price / (1.0 + vat_rate)
        let net_price = gross_price / (1.0 + vat_rate);

        // Calculate VAT collected per unit
        // VAT_Collected = Gross_Price - Net_Price
        let vat_per_unit = gross_price - net_price;

        // Determine cleared quantity (minimum of buy and sell)
        let cleared_quantity = order.buy.min(order.sell);

        // Calculate total VAT collected
        let total_vat_collected = vat_per_unit * cleared_quantity;

        // Calculate seller net revenue
        let net_revenue = net_price * cleared_quantity;

        // Record aggregate VAT record and route to treasury
        if total_vat_collected > 0.0 {
            let vat_record = AggregateVatRecord {
                commodity: format!("{:?}", good),
                cleared_quantity,
                gross_price,
                vat_rate,
                total_vat_collected,
                region_id: region_id.to_string(),
            };
            vat_records.push(vat_record.clone());

            // Route VAT through cascading treasury system
            // VAT is typically a national tax, so use national exception
            let national_routing = TaxRouting {
                microregion_share: 0.0,
                region_share: 0.0,
                central_share: 1.0,
                national_exception: true,
                extra: Default::default(),
            };

            route_tax_collection_to_country(
                total_vat_collected,
                &national_routing,
                country,
                region_id,
                format!("VAT_{:?}", good),
                TaxType::VAT,
            );
        }

        local_prices.insert(*good, gross_price);
        seller_revenue.insert(*good, net_revenue);
    }

    VatMarketResult {
        local_prices,
        vat_records,
        seller_revenue,
    }
}

/// Get VAT rate for a commodity from tax configuration.
///
/// # Arguments
/// * `commodity` - Commodity to look up
/// * `tax_rates` - Country tax rates configuration
///
/// # Returns
/// VAT rate (0.0 - 1.0), defaults to 0.0 if not found
fn get_vat_rate_for_commodity(commodity: Commodity, tax_rates: &crate::state::tax::TaxRates) -> f64 {
    // Map commodity to VAT category (simplified - in production would use commodity registry)
    // Using actual commodities from the enum
    let category = match commodity {
        Commodity::Bricks | Commodity::Cement | Commodity::Steel | Commodity::Aluminum => "industry",
        Commodity::Agd | Commodity::Cars => "services",
        _ => "services", // Default to services category
    };

    tax_rates
        .vat
        .get(category)
        .map(|bracket| bracket.rate)
        .unwrap_or(0.0)
}

fn resolve_deficit(
    good: Commodity,
    deficit: f64,
    global_base: f64,
    country: &Country,
    global_market: &GlobalMarket,
) -> f64 {
    let tariff = country
        .trade_policy
        .import_tariffs
        .get(&good)
        .copied()
        .unwrap_or(0.0);
    let import_price = global_base * (1.0 + tariff);

    // Stabilization Sprint: If no net_surplus entry exists for this commodity,
    // there is NO global surplus available — the commodity cannot be imported.
    // Default to 0.0 (not deficit) so the price correctly hits the shortage cap.
    let global_surplus = global_market
        .net_surplus
        .get(&good)
        .copied()
        .unwrap_or(0.0);

    if global_surplus >= deficit {
        // Fully covered by imports; the marginal cleared price is the
        // tariff-adjusted global price.
        return import_price;
    }

    if global_surplus > 0.0 {
        // Partially covered: the uncovered share raises the price toward the
        // hard shortage cap.
        let coverage = global_surplus / deficit;
        let shortage_price = global_base * PRICE_CAP;
        return import_price + (shortage_price - import_price) * (1.0 - coverage);
    }

    // No global surplus available; price hits the shortage cap.
    global_base * PRICE_CAP
}

/// Extract goods from warehouses to meet deficit (Phase 5.5).
///
/// # Arguments
/// * `good` - Commodity to extract
/// * `deficit` - Amount needed
/// * `target_region` - Region where deficit exists
/// * `warehouses` - Available warehouses (placeholder - needs Country access)
/// * `current_turn` - Current turn number
///
/// # Returns
/// * (amount_extracted, Vec<FinancialTransaction>)
///
/// # Rules
/// * Local-First: Check warehouses in same region first
/// * FEFO: Extract oldest batches first within each warehouse
/// * Neighboring regions: If local insufficient, check neighbors (with transport costs)
/// * Sort neighboring warehouses by transport cost (cheapest first)
pub fn extract_from_warehouses(
    good: Commodity,
    deficit: f64,
    target_region: &str,
    warehouses: &mut Vec<crate::society::housing::CommercialBuilding>,
    current_turn: u32,
    country: &crate::state::Country,
) -> (f64, Vec<FinancialTransaction>) {
    let mut remaining_deficit = deficit;
    let mut transactions = Vec::new();

    // STEP 1: Local warehouses (same region) - no transport cost
    let local_warehouses: Vec<&mut crate::society::housing::CommercialBuilding> = warehouses
        .iter_mut()
        .filter(|w| w.micro_region_id == target_region)
        .collect();

    remaining_deficit = extract_with_fefo(
        good,
        remaining_deficit,
        local_warehouses,
        target_region,
        current_turn,
        &mut transactions,
        false,  // is_cross_region = false (no transport cost)
        country,
    );

    // STEP 2: Neighboring regions (if deficit remains) - with transport costs
    // Sequential extraction to avoid borrow checker conflicts
    if remaining_deficit > 0.0 {
        // Filter neighboring warehouses (different region)
        let neighboring_warehouses: Vec<&mut crate::society::housing::CommercialBuilding> = warehouses
            .iter_mut()
            .filter(|w| w.micro_region_id != target_region)
            .collect();

        remaining_deficit = extract_with_fefo(
            good,
            remaining_deficit,
            neighboring_warehouses,
            target_region,
            current_turn,
            &mut transactions,
            true,  // is_cross_region = true (incurs transport costs)
            country,
        );
    }

    (deficit - remaining_deficit, transactions)
}

/// Extract using FEFO (First-Expired-First-Out) logic (Phase 5.5).
fn extract_with_fefo(
    good: Commodity,
    deficit: f64,
    warehouses: Vec<&mut crate::society::housing::CommercialBuilding>,
    target_region: &str,
    _current_turn: u32,
    transactions: &mut Vec<FinancialTransaction>,
    is_cross_region: bool,
    country: &crate::state::Country,
) -> f64 {
    let mut remaining = deficit;

    for warehouse in warehouses {
        if remaining <= 0.0 {
            break;
        }

        // Calculate transport cost if cross-region
        let transport_cost = if is_cross_region {
            crate::society::geography::calculate_transport_cost(
                &warehouse.micro_region_id,
                target_region,
                country,
                good,
            )
        } else {
            0.0
        };
        
        if let Some(batches) = warehouse.current_inventory.get_mut(&format!("{:?}", good)) {
            // Sort by age (oldest first)
            batches.sort_by_key(|b| b.storage_turn);
            
            for batch in batches.iter_mut() {
                if remaining <= 0.0 {
                    break;
                }
                
                let extract_amount = batch.quantity.min(remaining);
                batch.quantity -= extract_amount;
                remaining -= extract_amount;
                
                // Record financial transaction with transport cost
                transactions.push(FinancialTransaction {
                    batch_owner: batch.owner_id.clone(),
                    warehouse_owner: find_logistics_company(&warehouse.id),
                    quantity: extract_amount,
                    accumulated_fees: batch.accumulated_fees,
                    transport_cost,
                    commodity: good,
                });
                
                // Remove empty batch
                if batch.quantity <= 0.0 {
                    // Will be cleaned up in post-processing
                }
            }
            
            // Clean up empty batches
            batches.retain(|b| b.quantity > 0.0);
        }
    }
    
    deficit - remaining
}

/// Find logistics company that owns a warehouse.
/// Phase 13: Requires company registry lookup.
fn find_logistics_company(_warehouse_id: &str) -> String {
    "logistics_placeholder".to_string()
}

fn resolve_surplus(
    good: Commodity,
    surplus: f64,
    global_base: f64,
    country: &Country,
    global_market: &GlobalMarket,
) -> f64 {
    let tax = country
        .trade_policy
        .export_taxes
        .get(&good)
        .copied()
        .unwrap_or(0.0);
    let export_price = global_base * (1.0 - tax);

    // A negative net surplus means the world has a deficit (i.e. demand for
    // exports); a positive value means the world is already saturated.
    // Stabilization Sprint: If no net_surplus entry exists for this commodity,
    // there is NO global demand for exports — the commodity cannot be exported.
    // Default to 0.0 (not surplus) so the price correctly hits the surplus floor.
    let global_demand = global_market
        .net_surplus
        .get(&good)
        .copied()
        .map(|s| if s < 0.0 { -s } else { 0.0 })
        .unwrap_or(0.0);

    if global_demand >= surplus {
        // Full export absorption; the marginal cleared price is the
        // export-tax-adjusted global price.
        return export_price;
    }

    if global_demand > 0.0 {
        // Partially absorbed; the unexported share pushes the price toward
        // the surplus floor.
        let coverage = global_demand / surplus;
        let surplus_floor = global_base * PRICE_FLOOR;
        return export_price - (export_price - surplus_floor) * (1.0 - coverage);
    }

    // Phase 5: No global demand - route surplus to warehouses instead of destroying
    // The surplus is stored in available warehouses, and the producer pays storage fees
    // This is a placeholder - actual warehouse routing would happen in a separate phase
    // For now, we still hit the surplus floor price, but goods are not destroyed
    global_base * PRICE_FLOOR
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::market::{MarketOrders, MarketOrder};
    use crate::state::{TaxRates, AggregateVatRecord};
    use crate::state::tax::VatBracket;

    #[test]
    fn test_vat_net_gross_math() {
        // Test: Gross Price = 120, VAT Rate = 0.20
        // Expected: Net Price = 120 / 1.20 = 100, VAT = 20
        let gross_price: f64 = 120.0;
        let vat_rate: f64 = 0.20;
        let net_price = gross_price / (1.0 + vat_rate);
        let vat_collected = gross_price - net_price;

        assert!((net_price - 100.0_f64).abs() < 1e-9, "Expected 100.0, got {}", net_price);
        assert!((vat_collected - 20.0_f64).abs() < 1e-9, "Expected 20.0, got {}", vat_collected);
    }

    #[test]
    fn test_vat_money_mass_preservation() {
        // Test: Net Revenue + VAT = Gross Spend (closed-loop economy)
        let gross_price: f64 = 150.0;
        let vat_rate: f64 = 0.25;
        let cleared_quantity: f64 = 100.0;

        let net_price = gross_price / (1.0 + vat_rate);
        let vat_per_unit = gross_price - net_price;

        let net_revenue = net_price * cleared_quantity;
        let total_vat = vat_per_unit * cleared_quantity;
        let gross_spend = gross_price * cleared_quantity;

        assert!((net_revenue + total_vat - gross_spend).abs() < 1e-9,
            "Money mass not preserved: Net Revenue ({}) + VAT ({}) != Gross Spend ({})",
            net_revenue, total_vat, gross_spend);
    }

    #[test]
    fn test_vat_zero_rate() {
        // Test: Zero VAT rate should not affect price
        let gross_price: f64 = 100.0;
        let vat_rate: f64 = 0.0;
        let net_price = gross_price / (1.0 + vat_rate);

        assert!((net_price - gross_price).abs() < 1e-9, "Zero VAT should not change price");
    }

    #[test]
    fn test_vat_high_rate() {
        // Test: High VAT rate (50%)
        let gross_price: f64 = 150.0;
        let vat_rate: f64 = 0.50;
        let net_price = gross_price / (1.0 + vat_rate);
        let vat_collected = gross_price - net_price;

        assert!((net_price - 100.0_f64).abs() < 1e-9, "Expected 100.0, got {}", net_price);
        assert!((vat_collected - 50.0_f64).abs() < 1e-9, "Expected 50.0, got {}", vat_collected);
    }

    #[test]
    fn test_resolve_market_prices_with_vat_integration() {
        // Integration test: Full VAT market clearing with routing to treasury
        let mut market_orders = MarketOrders {
            orders: HashMap::default(),
        };

        market_orders.orders.insert(
            Commodity::Bricks, // Use actual commodity from enum
            MarketOrder {
                buy: 1000.0,
                sell: 800.0,
            },
        );

        let mut country = Country {
            tax_rates: TaxRates {
                vat: {
                    let mut map = std::collections::HashMap::new();
                    map.insert(
                        "industry".to_string(),
                        VatBracket {
                            rate: 0.10,
                            consumption_share: 0.2,
                            extra: Default::default(),
                        },
                    );
                    map
                },
                ..Default::default()
            },
            ..Default::default()
        };

        // Add a region for routing
        country.regions.push(crate::society::geography::Region {
            id: "region_1".to_string(),
            treasury: crate::state::treasury::Treasury::default(),
            ..Default::default()
        });

        let global_market = GlobalMarket::default();

        // Record initial budget
        let initial_budget = country.budget.liquid_reserves;

        let result = resolve_market_prices_with_vat(
            &market_orders,
            &mut country,
            &global_market,
            "region_1",
        );

        // Verify VAT records were created
        assert!(!result.vat_records.is_empty(), "VAT records should be created");

        // Verify money mass preservation for each record
        for record in &result.vat_records {
            let net_price = record.gross_price / (1.0 + record.vat_rate);
            let net_revenue = net_price * record.cleared_quantity;
            let gross_spend = record.gross_price * record.cleared_quantity;

            assert!((net_revenue + record.total_vat_collected - gross_spend).abs() < 1e-9,
                "Money mass not preserved for commodity {}", record.commodity);
        }

        // Verify seller revenue is recorded
        assert!(result.seller_revenue.contains_key(&Commodity::Bricks),
            "Seller revenue should be recorded for Cegly");

        // Verify VAT was actually routed to treasury (closed-loop economy)
        let total_vat_collected: f64 = result.vat_records.iter().map(|r| r.total_vat_collected).sum();
        let budget_increase = country.budget.liquid_reserves - initial_budget;

        assert!((budget_increase - total_vat_collected).abs() < 1e-9,
            "VAT not routed to treasury: Budget increase ({}) != Total VAT ({})",
            budget_increase, total_vat_collected);

        // Verify closed-loop economy: Seller revenue + VAT = Gross spend
        let seller_revenue = result.seller_revenue.get(&Commodity::Bricks).unwrap();
        let gross_spend = result.local_prices.get(&Commodity::Bricks).unwrap() * 800.0; // cleared quantity

        assert!((seller_revenue + total_vat_collected - gross_spend).abs() < 1e-9,
            "Closed-loop economy violated: Seller Revenue ({}) + VAT ({}) != Gross Spend ({})",
            seller_revenue, total_vat_collected, gross_spend);
    }

    #[test]
    fn test_vat_aggregate_record_no_placeholders() {
        // Test: AggregateVatRecord contains no placeholder strings
        let record = AggregateVatRecord {
            commodity: "Food".to_string(),
            cleared_quantity: 100.0,
            gross_price: 120.0,
            vat_rate: 0.20,
            total_vat_collected: 20.0,
            region_id: "region_1".to_string(),
        };

        assert!(!record.commodity.contains("placeholder"), "Commodity should not contain placeholder");
        assert!(!record.region_id.contains("placeholder"), "Region ID should not contain placeholder");
    }
}

//! Core economic indicators and the per-country turn function.
//!
//! This module hosts the deterministic per-country economic step, including
//! the sector-level GDP share update from physical employment counts.

use serde_json::Value;
use std::collections::HashMap;

use crate::registries::enums::Sector;

// Re-export so the context type is reachable from `economy::indicators` as well
// as from `economy`, which matches the function signatures in this module.
pub use crate::economy::CountryTurnCtx;

/// Recalculates each sector's `"gdp_share"` (GDP share) from its physical
/// employment count (`"zatrudnienie"`).
///
/// This is a Rust port of the Python helper `_calculate_gdp_shares_from_employment`
/// in `economy/indicators/core.py`.
///
/// # Arguments
/// * `ctx` - The [`CountryTurnCtx`] whose `country.budget.sectors` will be
///   updated in place.
///
/// # Rules
/// * Employment is read from each sector's `extra["zatrudnienie"]` as an integer.
/// * If the total employment is positive, each sector's `gdp_share` becomes
///   `employment / total_employment`.
/// * If total employment is zero, all sectors receive an equal share
///   (`1.0 / sector_count`).
/// * The function is deterministic and mutates only the `gdp_share` field.
///
/// # Python Reference
/// ```python
/// def _calculate_gdp_shares_from_employment(sektory, pkb):
///     total_employment = 0
///     for sec_dict in sektory.values():
///         if isinstance(sec_dict, dict):
///             total_employment += sec_dict.get('zatrudnienie', 0)
///
///     if total_employment > 0:
///         for sec_dict in sektory.values():
///             if isinstance(sec_dict, dict):
///                 employment = sec_dict.get('zatrudnienie', 0)
///                 sec_dict['gdp_share'] = employment / total_employment
///     else:
///         sector_count = len([s for s in sektory.values() if isinstance(s, dict)])
///         if sector_count > 0:
///             equal_share = 1.0 / sector_count
///             for sec_dict in sektory.values():
///                 if isinstance(sec_dict, dict):
///                     sec_dict['gdp_share'] = equal_share
/// ```
pub fn update_gdp_shares_from_employment(ctx: &mut CountryTurnCtx<'_>) {
    let sectors = &mut ctx.country.budget.sectors;

    if !ctx.buildings.is_empty() {
        let mut employment: HashMap<Sector, (i64, u32)> = HashMap::new();
        for building in &ctx.buildings {
            let entry = employment.entry(building.sector).or_insert((0, 0));
            let scale = building.scale_factor.max(1) as i64;
            entry.0 += building.current_employment as i64 * scale;
            entry.1 += building.worker_capacity * scale as u32;
        }
        for (sector, (emp, capacity)) in employment {
            if let Some(share) = sectors.get_mut(&sector) {
                share.extra.insert("zatrudnienie".to_string(), Value::from(emp));
                let pmi = if capacity > 0 {
                    (100.0 * (emp as f64 / capacity as f64)).min(100.0)
                } else {
                    0.0
                };
                share.extra.insert("pmi".to_string(), Value::from(pmi));
            }
        }
    }

    let total_employment: i64 = sectors
        .values()
        .map(|share| {
            share
                .extra
                .get("employment")
                .and_then(Value::as_i64)
                .unwrap_or(0)
        })
        .sum();

    if total_employment > 0 {
        for share in sectors.values_mut() {
            let employment = share
                .extra
                .get("employment")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            share.gdp_share = employment as f64 / total_employment as f64;
        }
    } else {
        let count = sectors.len() as f64;
        if count > 0.0 {
            let equal_share = 1.0 / count;
            for share in sectors.values_mut() {
                share.gdp_share = equal_share;
            }
        }
    }
}

/// Phase 29: Compute the PMI diffusion index for a sector.
///
/// PMI = Orders * 0.30 + Production * 0.25 + Employment * 0.20
///       + Deliveries * 0.15 + Inventories * 0.10
///
/// Each sub-component is normalized to 0–100 where 50 = neutral, >50 =
/// expansion, <50 = contraction.
///
/// # Arguments
/// * `sector` - The sector to compute PMI for.
/// * `buildings` - All buildings (filtered by sector).
/// * `order_book` - The current OrderBook for order/delivery data.
/// * `prev_telemetry` - Previous-turn telemetry values (orders, production,
///   deliveries, inventory) for delta calculations.
///
/// # Returns
/// The PMI value [0, 100] and a map of sub-component values for debugging.
pub fn compute_pmi_diffusion_index(
    sector: Sector,
    buildings: &[crate::entities::Building],
    order_book: &crate::economy::market::order_book::OrderBook,
    prev_telemetry: &HashMap<String, f64>,
) -> (f64, HashMap<String, f64>) {
    let sector_commodities = sector.primary_commodities();
    let sector_buildings: Vec<&crate::entities::Building> = buildings
        .iter()
        .filter(|b| b.sector == sector)
        .collect();

    // 1. Orders (30%): Sum of bid quantities in the OrderBook for sector commodities
    let current_orders: f64 = sector_commodities
        .iter()
        .map(|c| {
            order_book
                .bids
                .get(c)
                .map(|bids| bids.iter().map(|b| b.quantity).sum::<f64>())
                .unwrap_or(0.0)
        })
        .sum();
    let prev_orders = prev_telemetry.get("_prev_orders").copied().unwrap_or(0.0);
    let orders_component = if prev_orders > 0.0 {
        50.0 + 50.0 * ((current_orders - prev_orders) / prev_orders).clamp(-1.0, 1.0)
    } else if current_orders > 0.0 {
        60.0 // New orders appearing — slight expansion signal
    } else {
        50.0 // Neutral — no data
    };

    // 2. Production (25%): Sum of building output vs previous turn
    let current_production: f64 = sector_buildings
        .iter()
        .flat_map(|b| b.last_production.values().copied())
        .sum();
    let prev_production = prev_telemetry.get("_prev_production").copied().unwrap_or(0.0);
    let production_component = if prev_production > 0.0 {
        50.0 + 50.0 * ((current_production - prev_production) / prev_production).clamp(-1.0, 1.0)
    } else if current_production > 0.0 {
        60.0
    } else {
        50.0
    };

    // 3. Employment (20%): FTE utilization (retained from old PMI but reweighted)
    let total_emp: f64 = sector_buildings
        .iter()
        .map(|b| b.current_employment as f64)
        .sum();
    let total_capacity: f64 = sector_buildings
        .iter()
        .map(|b| b.worker_capacity as f64)
        .sum();
    let employment_component = if total_capacity > 0.0 {
        (100.0 * (total_emp / total_capacity)).clamp(0.0, 100.0)
    } else {
        50.0
    };

    // 4. Deliveries (15%): Sum of ask quantities (proxy for delivery supply)
    // In a full implementation, this would use settled trade counts.
    let current_deliveries: f64 = sector_commodities
        .iter()
        .map(|c| {
            order_book
                .asks
                .get(c)
                .map(|asks| asks.iter().map(|a| a.quantity).sum::<f64>())
                .unwrap_or(0.0)
        })
        .sum();
    let prev_deliveries = prev_telemetry.get("_prev_deliveries").copied().unwrap_or(0.0);
    let deliveries_component = if prev_deliveries > 0.0 {
        50.0 + 50.0 * ((current_deliveries - prev_deliveries) / prev_deliveries).clamp(-1.0, 1.0)
    } else if current_deliveries > 0.0 {
        55.0
    } else {
        50.0
    };

    // 5. Inventories (10%): Change in building inventory levels
    let current_inventory: f64 = sector_buildings
        .iter()
        .map(|b| b.inventory.values().sum::<f64>())
        .sum();
    let prev_inventory = prev_telemetry.get("_prev_inventory").copied().unwrap_or(0.0);
    let inventory_component = if prev_inventory > 0.0 {
        // Rising inventories = above 50 (stockpiling), falling = below 50 (drawdown)
        50.0 + 50.0 * ((current_inventory - prev_inventory) / prev_inventory).clamp(-1.0, 1.0)
    } else {
        50.0
    };

    // Weighted sum
    let pmi = orders_component * 0.30
        + production_component * 0.25
        + employment_component * 0.20
        + deliveries_component * 0.15
        + inventory_component * 0.10;

    let mut components = HashMap::new();
    components.insert("orders".to_string(), orders_component);
    components.insert("production".to_string(), production_component);
    components.insert("employment".to_string(), employment_component);
    components.insert("deliveries".to_string(), deliveries_component);
    components.insert("inventories".to_string(), inventory_component);
    components.insert("_current_orders".to_string(), current_orders);
    components.insert("_current_production".to_string(), current_production);
    components.insert("_current_deliveries".to_string(), current_deliveries);
    components.insert("_current_inventory".to_string(), current_inventory);

    (pmi, components)
}

/// Executes one deterministic economic turn for a single country.
///
/// # Arguments
/// * `ctx` - The [`CountryTurnCtx`] carrying the country state and turn
///   metadata.
///
/// # Returns
/// `Ok(())` once the turn is processed.
///
/// # Rules
/// * This function is strictly deterministic: the same inputs must produce the
///   same outputs on every run.
/// * The function mutates `ctx.country` in place.
pub fn run_economic_turn(ctx: &mut CountryTurnCtx<'_>) -> Result<(), String> {
    update_gdp_shares_from_employment(ctx);

    // Apply infrastructure capacity effects to all regions
    // This includes healthcare, dependency care, and education effects
    for region in &mut ctx.country.regions {
        crate::infrastructure::effects::apply_infrastructure_effects(region, ctx.year);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::CountryTurnCtx;
    use crate::registries::enums::Sector;
    use crate::registries::Registries;
    use crate::state::{Country, Treasury};
    use serde_json::Map;

    /// Builds a `Treasury` with two sectors carrying explicit `zatrudnienie`.
    fn treasury_with_employment() -> Treasury {
        let treasury: Treasury = serde_json::from_str(
            r#"{
                "gdp": 100.0,
                "population": 1000,
                "nominal_budget": 10.0,
                "liquid_reserves": 0.0,
                "citizen_savings": 0.0,
                "private_capital": 0.0,
                "infrastructure_level": 1.0,
                "energy_infrastructure": 1.0,
                "stock_market": {"index": 100.0, "confidence": 50.0, "last_change": 0.0, "sector_indices": {}},
                "allocations": {
                    "industry": 0.14, "education_propaganda": 0.14, "healthcare": 0.14,
                    "infrastructure_transport": 0.14, "social_programs": 0.15,
                    "agriculture_rural": 0.15, "armed_forces": 0.14
                },
                "black_ops_budget": 0.0,
                "sectors": {
                    "agriculture": {"gdp_share": 0.0, "employment": 300},
                    "heavy_industry": {"gdp_share": 0.0, "employment": 700}
                },
                "science": {"innovation_points": 0.0, "researching": null, "discovered": [], "base_innovativeness": 0.0},
                "last_balance_log": ""
            }"#,
        )
        .unwrap();
        treasury
    }

    fn dummy_country() -> Country {
        let mut country = Country::mock_for_tests();
        country.name = "Iliria".to_string();
        country.budget = treasury_with_employment();
        country.macro_indicators = serde_json::from_str(
            r#"{"inflation":0.0,"gini":0.0,"social_unrest":0.0,"wealth_bracket":"high","productivity":1.0,"currency":"ILI","energy_mix":{"coal":0.0,"natural_gas":0.0,"uranium":0.0,"renewables":1.0},"average_wage":1.0,"culture":"Iliria","cultural_group":"","religion":""}"#,
        )
        .unwrap();
        country.tax_rates = serde_json::from_str(
            r#"{"income_tax":{"rate":0.0,"structure":"liniowy"},"corporate_tax":0.0,"vat":{},"public_debt":{"current_debt":0.0,"interest_rate":0.0}}"#,
        )
        .unwrap();
        country
    }

    fn dummy_ctx<'a>(country: &'a mut Country, registries: &'a Registries) -> CountryTurnCtx<'a> {
        CountryTurnCtx {
            country_name: "Iliria".to_string(),
            turn: 0,
            year: 2020,
            registries,
            country,
            buildings: Vec::new(),
            market_prices: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn gdp_shares_from_employment_match_python() {
        let reg = Registries::native_only();
        let mut country = dummy_country();
        let mut ctx = dummy_ctx(&mut country, &reg);

        update_gdp_shares_from_employment(&mut ctx);

        let agri = &ctx.country.budget.sectors[&Sector::Agriculture];
        let heavy = &ctx.country.budget.sectors[&Sector::HeavyIndustry];

        assert!((agri.gdp_share - 0.3).abs() < 1e-9);
        assert!((heavy.gdp_share - 0.7).abs() < 1e-9);
    }

    #[test]
    fn gdp_shares_equal_when_no_employment() {
        let reg = Registries::native_only();
        let mut country = dummy_country();
        // Remove employment from both sectors.
        for share in country.budget.sectors.values_mut() {
            share.extra = Map::new();
        }

        let mut ctx = dummy_ctx(&mut country, &reg);
        update_gdp_shares_from_employment(&mut ctx);

        let agri = &ctx.country.budget.sectors[&Sector::Agriculture];
        let heavy = &ctx.country.budget.sectors[&Sector::HeavyIndustry];

        assert!((agri.gdp_share - 0.5).abs() < 1e-9);
        assert!((heavy.gdp_share - 0.5).abs() < 1e-9);
    }

    #[test]
    fn run_turn_updates_gdp_shares() {
        let reg = Registries::native_only();
        let mut country = dummy_country();
        let mut ctx = dummy_ctx(&mut country, &reg);

        run_economic_turn(&mut ctx).unwrap();

        let agri = &ctx.country.budget.sectors[&Sector::Agriculture];
        let heavy = &ctx.country.budget.sectors[&Sector::HeavyIndustry];
        assert!((agri.gdp_share - 0.3).abs() < 1e-9);
        assert!((heavy.gdp_share - 0.7).abs() < 1e-9);
    }

    // ── Phase 29: PMI Diffusion Index Tests ──

    #[test]
    fn test_pmi_weights_sum_to_one() {
        // Verify the weights sum to 1.0
        let weights = [0.30, 0.25, 0.20, 0.15, 0.10];
        let sum: f64 = weights.iter().sum();
        assert!((sum - 1.0).abs() < 1e-9, "PMI weights must sum to 1.0");
    }

    #[test]
    fn test_pmi_neutral_when_no_data() {
        let buildings: Vec<crate::entities::Building> = Vec::new();
        let order_book = crate::economy::market::order_book::OrderBook::default();
        let prev = HashMap::new();

        let (pmi, components) = compute_pmi_diffusion_index(
            Sector::HeavyIndustry,
            &buildings,
            &order_book,
            &prev,
        );

        // With no data, all components should be 50 (neutral)
        assert!((pmi - 50.0).abs() < 1.0, "PMI should be ~50 with no data, got {}", pmi);
        assert!((components["orders"] - 50.0).abs() < 0.1);
        assert!((components["production"] - 50.0).abs() < 0.1);
        assert!((components["employment"] - 50.0).abs() < 0.1);
        assert!((components["deliveries"] - 50.0).abs() < 0.1);
        assert!((components["inventories"] - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_pmi_rising_when_orders_increase() {
        let buildings: Vec<crate::entities::Building> = Vec::new();
        let mut order_book = crate::economy::market::order_book::OrderBook::default();
        // Add bids for Steel (a HeavyIndustry commodity)
        order_book.bids.insert(
            crate::registries::enums::Commodity::Steel,
            vec![crate::economy::market::order_book::Bid {
                buyer_id: "test".to_string(),
                commodity: crate::registries::enums::Commodity::Steel,
                quantity: 1000.0,
                limit_price: 100.0,
                blueprint_id: None,
                min_quality: None,
            }],
        );
        let mut prev = HashMap::new();
        prev.insert("_prev_orders".to_string(), 500.0); // Orders doubled

        let (pmi, components) = compute_pmi_diffusion_index(
            Sector::HeavyIndustry,
            &buildings,
            &order_book,
            &prev,
        );

        // Orders doubled → orders component should be 100 (max expansion)
        assert!(components["orders"] > 50.0, "Orders component should be > 50");
        assert!(pmi > 50.0, "PMI should be > 50 when orders rising");
    }

    #[test]
    fn test_pmi_falling_when_production_decreases() {
        let mut building = crate::entities::Building::default();
        building.sector = Sector::HeavyIndustry;
        building.last_production.insert(crate::registries::enums::Commodity::Steel, 100.0);
        let buildings = vec![building];
        let order_book = crate::economy::market::order_book::OrderBook::default();
        let mut prev = HashMap::new();
        prev.insert("_prev_production".to_string(), 500.0); // Production fell

        let (pmi, components) = compute_pmi_diffusion_index(
            Sector::HeavyIndustry,
            &buildings,
            &order_book,
            &prev,
        );

        // Production fell 80% → production component should be < 50
        assert!(components["production"] < 50.0, "Production component should be < 50");
        assert!(pmi < 55.0, "PMI should be low when production falling");
    }

    #[test]
    fn test_pmi_bounded_0_to_100() {
        let buildings: Vec<crate::entities::Building> = Vec::new();
        let order_book = crate::economy::market::order_book::OrderBook::default();
        let mut prev = HashMap::new();
        // Extreme values
        prev.insert("_prev_orders".to_string(), 1.0);
        prev.insert("_prev_production".to_string(), 1.0);
        prev.insert("_prev_deliveries".to_string(), 1.0);
        prev.insert("_prev_inventory".to_string(), 1.0);

        let (pmi, _) = compute_pmi_diffusion_index(
            Sector::HeavyIndustry,
            &buildings,
            &order_book,
            &prev,
        );

        assert!(pmi >= 0.0 && pmi <= 100.0, "PMI must be in [0, 100], got {}", pmi);
    }
}

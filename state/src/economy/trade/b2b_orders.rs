//! B2B order submission, trade settlement, and production execution.
//!
//! This module implements the core Phase 6.3 (Production Planning),
//! Phase 6.4a (Double-Entry Settlement), and Phase 6.4b (Production Execution)
//! of the real economy turn loop.
//!
//! # Key Invariants
//! * Inventory lives ONLY on `Building.inventory`. There is no `Company.inventory` field.
//! * Company aggregate inventory is computed dynamically via `compute_company_inventory`.
//! * Goods in trades route directly to the specific `Building.inventory` that requested them.
//! * Wages are NOT paid here — they are settled in Phase W1 by `resolve_regional_labor_market`.
//! * Overflow that cannot be warehoused perishes immediately (no next-turn buffer).

use crate::economy::b2b_config::B2bOrderConfig;
use crate::economy::fixed_assets::draft_animal_maintenance_needed;
use crate::economy::generative_goods_config::GenerativeGoodsConfig;
use crate::economy::market_history::{get_reference_price, MarketHistory};
use crate::economy::order_book::{Ask, Bid, OrderBook, Trade};
use crate::economy::production::ProductionResult;
use crate::economy::transfer_settler::{credit_company_by_id, debit_company_by_id};
use crate::entities::{Building, Company};
use crate::registries::enums::{Commodity, Sector};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

type HashMap<K, V> = FxHashMap<K, V>;

/// Dynamically compute aggregate inventory for a company by iterating its buildings.
///
/// # Arguments
/// * `company` - The company to compute inventory for.
/// * `buildings` - All buildings (filtered by `owner_id == company.id`).
///
/// # Returns
/// A `BTreeMap<Commodity, f64>` of total inventory quantities.
///
/// # Rules
/// * Read-only computation — NO state is stored on `Company`.
/// * Prevents data duplication and duping bugs.
pub fn compute_company_inventory(
    company: &Company,
    buildings: &[Building],
) -> BTreeMap<Commodity, f64> {
    let mut total: BTreeMap<Commodity, f64> = BTreeMap::new();
    for b in buildings.iter().filter(|b| b.owner_id == company.id) {
        for (&commodity, &qty) in &b.inventory {
            *total.entry(commodity).or_insert(0.0) += qty;
        }
    }
    total
}

/// Calculate the total inventory capacity for a company's buildings.
fn compute_company_inventory_capacity(
    company: &Company,
    buildings: &[Building],
) -> f64 {
    buildings
        .iter()
        .filter(|b| b.owner_id == company.id)
        .map(|b| b.inventory_capacity)
        .sum()
}

/// Calculate the utilization ratio of a company's inventory.
///
/// # Returns
/// A value in `[0.0, 1.0+]` representing current inventory / max capacity.
fn compute_inventory_utilization(
    company: &Company,
    buildings: &[Building],
) -> f64 {
    let capacity = compute_company_inventory_capacity(company, buildings);
    if capacity <= 0.0 {
        return 0.0;
    }
    let total_qty: f64 = compute_company_inventory(company, buildings)
        .values()
        .sum();
    total_qty / capacity
}

/// Calculate dynamic markup based on inventory utilization.
///
/// # Arguments
/// * `utilization` - Current inventory utilization (0.0 to 1.0+).
/// * `config` - B2B order configuration with thresholds.
///
/// # Returns
/// Markup ratio to apply to unit cost.
///
/// # Rules
/// * `utilization >= fire_sale_threshold` → `min_markup_ratio` (fire sale pricing).
/// * `utilization <= scarcity_threshold` → `max_markup_ratio` (scarcity premium).
/// * Between thresholds → linear interpolation.
pub fn calculate_dynamic_markup(utilization: f64, config: &B2bOrderConfig) -> f64 {
    if utilization >= config.fire_sale_threshold {
        return config.min_markup_ratio;
    }
    if utilization <= config.scarcity_threshold {
        return config.max_markup_ratio;
    }
    // Linear interpolation between scarcity (high markup) and fire sale (low markup)
    let range = config.fire_sale_threshold - config.scarcity_threshold;
    if range <= 0.0 {
        return config.min_markup_ratio;
    }
    let t = (utilization - config.scarcity_threshold) / range;
    config.max_markup_ratio * (1.0 - t) + config.min_markup_ratio * t
}

/// Calculate the unit cost of production for a building.
///
/// # Arguments
/// * `building` - Building with active production method.
/// * `reference_prices` - Market reference prices for input commodities.
/// * `base_wage` - Average wage per FTE.
///
/// # Returns
/// Unit cost per output unit.
///
/// # Rules
/// * Unit Cost = (sum of input_qty × reference_price + wage_cost) / output_volume.
/// * If output_volume is zero, returns 0.0.
pub fn calculate_unit_cost(
    building: &Building,
    reference_prices: &HashMap<Commodity, f64>,
    base_wage: f64,
) -> f64 {
    let method = &building.active_method;
    let production_scale = building.current_employment as f64 / 1000.0;

    let mut input_cost = 0.0;
    for (&commodity, &qty_per_1k) in &method.inputs {
        let price = reference_prices.get(&commodity).copied().unwrap_or(0.0);
        input_cost += qty_per_1k * price * production_scale;
    }

    let wage_cost = building.current_employment as f64 * base_wage;

    let mut output_volume = 0.0;
    for (&_commodity, &qty_per_1k) in &method.outputs {
        output_volume += qty_per_1k * production_scale;
    }

    if output_volume > 0.0 {
        (input_cost + wage_cost) / output_volume
    } else {
        0.0
    }
}

/// Submit B2B Buy Bids and Sell Asks for all companies based on their buildings' BOMs.
///
/// # Arguments
/// * `companies` - Mutable slice of all companies (cash is encumbered).
/// * `buildings` - Slice of all buildings (read-only for BOM and inventory lookup).
/// * `order_book` - Mutable order book to submit bids and asks into.
/// * `market_history` - Historical price data for reference prices.
/// * `config` - B2B order configuration.
///
/// # Returns
/// A vector of diagnostic messages.
///
/// # Rules
/// * For each company, iterate its buildings to compute aggregate inventory.
/// * Submit Buy Bids for each input commodity in each building's BOM.
/// * Submit Sell Asks for each output commodity in each building's BOM.
/// * Cash is encumbered: `available_cash -= encumbrance`, `debit_cash += encumbrance`.
/// * Skip bids if `available_cash` is insufficient.
/// * Sell Ask limit price = unit_cost × (1 + dynamic_markup).
/// * Buy Bid limit price = reference_price × (1 + buy_premium_ratio).
pub fn submit_company_b2b_orders(
    companies: &mut [Company],
    buildings: &[Building],
    order_book: &mut OrderBook,
    market_history: &MarketHistory,
    config: &B2bOrderConfig,
    gen_config: &GenerativeGoodsConfig,
) -> Vec<String> {
    let mut messages = Vec::new();

    for company in companies.iter_mut() {
        // Sync available_cash from brokerage account
        let liquid = company.computed_liquid_capital();
        company.available_cash = liquid;

        // Compute inventory utilization for dynamic pricing
        let utilization = compute_inventory_utilization(company, buildings);
        let dynamic_markup = calculate_dynamic_markup(utilization, config);

        // Phase 76: Bootstrap pricing when no VWAP exists (Turn 0 condition).
        // When the market has no transaction history, the dynamic markup would
        // set seller asks at 3× reference (scarcity pricing for empty inventory),
        // while buyers bid at 1.05× reference. This spread never crosses, so no
        // trades execute and no VWAP is ever established — a deadlock.
        // Bootstrap: use min_markup_ratio (0.0) so sellers ask at ref×1.0 while
        // buyers bid at ref×1.05, guaranteeing spread crossing on Turn 0.
        let is_bootstrap = market_history.vwap_per_commodity.is_empty()
            && market_history.last_trade_price.is_empty();
        let markup = if is_bootstrap {
            config.min_markup_ratio
        } else {
            dynamic_markup
        };

        // Maximum cash to encumber for input purchases
        let max_encumber = liquid * config.max_cash_encumbrance_ratio;
        let mut total_encumbered = 0.0;

        // Collect company's buildings
        let company_buildings: Vec<&Building> = buildings
            .iter()
            .filter(|b| b.owner_id == company.id)
            .collect();

        if company_buildings.is_empty() {
            continue;
        }

        // Submit Buy Bids for inputs
        for building in &company_buildings {
            let method = &building.active_method;
            let production_scale = building.current_employment as f64 / 1000.0;

            for (&commodity, &qty_per_1k) in &method.inputs {
                // Phase 19B: Fixed-asset commodities (machinery/vehicles) are no
                // longer consumed per-turn — they're installed as FixedAssetCohorts.
                // Skip them here; they're procured via separate asset-purchase bids.
                if commodity.is_fixed_asset() {
                    continue;
                }

                let desired_qty = qty_per_1k * production_scale;
                if desired_qty <= 0.0 {
                    continue;
                }

                let ref_price = get_reference_price(&commodity, market_history);
                if ref_price.is_none() {
                    messages.push(format!(
                        "Company {}: No reference price for {:?}, skipping buy bid",
                        company.id, commodity
                    ));
                    continue;
                }

                // Phase 45: Dynamic buyer pricing with unfilled-order feedback.
                // If this commodity had an unfilled bid last turn, raise the bid
                // price by 10% above the last unfilled price. The ONLY ceiling
                // is the buyer's max_affordable_budget (cash encumbrance limit).
                // NO profitability ceiling is applied.
                let base_price = ref_price.unwrap();
                let last_unfilled = company.unfilled_bid_prices.get(&commodity).copied().unwrap_or(0.0);
                let limit_price = if last_unfilled > 0.0 {
                    // Raise bid by 10% above last unfilled price
                    (last_unfilled * 1.10).max(base_price * (1.0 + config.buy_premium_ratio))
                } else {
                    base_price * (1.0 + config.buy_premium_ratio)
                };
                // Phase 25: Include a freight cost reserve in the encumbrance so
                // buyers have enough cash to pay for transport when the trade settles.
                // Without this, buyers encumber only the commodity cost and then
                // can't afford freight, causing trades to fail with UnaffordableFreight.
                let commodity_cost = desired_qty * limit_price;
                let freight_reserve = commodity_cost * config.freight_cost_reserve_ratio;
                let encumbrance = commodity_cost + freight_reserve;

                // Check if we can afford this bid
                if total_encumbered + encumbrance > max_encumber {
                    let remaining = max_encumber - total_encumbered;
                    if remaining <= 0.0 || limit_price <= 0.0 {
                        continue;
                    }
                    // Phase 25: affordable_qty must account for the freight reserve too.
                    let total_per_unit = limit_price * (1.0 + config.freight_cost_reserve_ratio);
                    let affordable_qty = remaining / total_per_unit;
                    if affordable_qty <= 0.0 {
                        continue;
                    }
                    // Submit partial bid (encumber commodity cost + freight reserve)
                    let partial_encumbrance = affordable_qty * total_per_unit;
                    company.available_cash -= partial_encumbrance;
                    company.debit_cash += partial_encumbrance;
                    total_encumbered += partial_encumbrance;

                    order_book
                        .bids
                        .entry(commodity)
                        .or_insert_with(Vec::new)
                        .push(Bid {
                            buyer_id: company.id.clone(),
                            commodity,
                            quantity: affordable_qty,
                            limit_price,
                            blueprint_id: None,
                            min_quality: None,
                        });
                } else {
                    company.available_cash -= encumbrance;
                    company.debit_cash += encumbrance;
                    total_encumbered += encumbrance;

                    order_book
                        .bids
                        .entry(commodity)
                        .or_insert_with(Vec::new)
                        .push(Bid {
                            buyer_id: company.id.clone(),
                            commodity,
                            quantity: desired_qty,
                            limit_price,
                            blueprint_id: None,
                            min_quality: None,
                        });
                }
            }
        }

        // Phase 23A: Submit Buy Bids for Draft Animal maintenance (Fodder + Water).
        // Buildings with DraftAnimals cohorts need animal feed, distinct from
        // the MaintenanceServices consumed by machinery cohorts.
        for building in &company_buildings {
            let needed = draft_animal_maintenance_needed(&building.fixed_assets, gen_config);
            if needed.is_empty() {
                continue;
            }
            for (&commodity, &desired_qty) in &needed {
                if desired_qty <= 0.0 {
                    continue;
                }
                let ref_price = get_reference_price(&commodity, market_history);
                let limit_price = match ref_price {
                    Some(p) => p * (1.0 + config.buy_premium_ratio),
                    None => continue,
                };
                let encumbrance = desired_qty * limit_price;

                if total_encumbered + encumbrance > max_encumber {
                    let remaining = max_encumber - total_encumbered;
                    if remaining <= 0.0 || limit_price <= 0.0 {
                        continue;
                    }
                    let affordable_qty = remaining / limit_price;
                    if affordable_qty <= 0.0 {
                        continue;
                    }
                    company.available_cash -= affordable_qty * limit_price;
                    company.debit_cash += affordable_qty * limit_price;
                    total_encumbered += affordable_qty * limit_price;

                    order_book
                        .bids
                        .entry(commodity)
                        .or_insert_with(Vec::new)
                        .push(Bid {
                            buyer_id: company.id.clone(),
                            commodity,
                            quantity: affordable_qty,
                            limit_price,
                            blueprint_id: None,
                            min_quality: None,
                        });
                } else {
                    company.available_cash -= encumbrance;
                    company.debit_cash += encumbrance;
                    total_encumbered += encumbrance;

                    order_book
                        .bids
                        .entry(commodity)
                        .or_insert_with(Vec::new)
                        .push(Bid {
                            buyer_id: company.id.clone(),
                            commodity,
                            quantity: desired_qty,
                            limit_price,
                            blueprint_id: None,
                            min_quality: None,
                        });
                }
            }
        }

        // Submit Sell Asks for outputs
        for building in &company_buildings {
            let method = &building.active_method;
            let production_scale = building.current_employment as f64 / 1000.0;

            // Calculate unit cost for this building's output
            let mut ref_prices: HashMap<Commodity, f64> = HashMap::default();
            for (&input_commodity, _) in &method.inputs {
                if let Some(price) = get_reference_price(&input_commodity, market_history) {
                    ref_prices.insert(input_commodity, price);
                }
            }

            let unit_cost = calculate_unit_cost(building, &ref_prices, 0.0);

            for (&commodity, &qty_per_1k) in &method.outputs {
                let sell_qty = qty_per_1k * production_scale;
                if sell_qty <= 0.0 {
                    continue;
                }

                // Phase 37/76: Per-commodity sell price with fallback chain:
                // 1. unit_cost * (1 + markup) — cost-based pricing
                // 2. get_reference_price(commodity) * (1 + markup) — market-based
                // 3. global_base_prices[commodity] * (1 + min_markup) — floor price
                // This ensures producers always submit asks when they have workers,
                // breaking the "no VWAP → no asks → no trades → no VWAP" deadlock.
                //
                // Phase 76 Rule 8 Enforcement: A rational actor NEVER sells below
                // actual unit_cost. The final ask price is clamped to
                // max(sell_price, unit_cost) when unit_cost > 0.0.
                let sell_price = if unit_cost > 0.0 {
                    unit_cost * (1.0 + markup)
                } else if let Some(ref_p) = get_reference_price(&commodity, market_history) {
                    ref_p * (1.0 + markup)
                } else if let Some(base_p) = market_history.global_base_prices.get(&commodity).copied() {
                    base_p * (1.0 + config.min_markup_ratio)
                } else {
                    continue;
                };

                // Phase 76: Rule 8 — Rational Actor Pricing Floor.
                // Never sell below actual production cost.
                let sell_price = if unit_cost > 0.0 {
                    sell_price.max(unit_cost)
                } else {
                    sell_price
                };

                if sell_price <= 0.0 {
                    continue;
                }

                order_book
                    .asks
                    .entry(commodity)
                    .or_insert_with(Vec::new)
                    .push(Ask {
                        seller_id: company.id.clone(),
                        commodity,
                        quantity: sell_qty,
                        limit_price: sell_price,
                        blueprint_id: None,
                        quality: None,
                        durability: None,
                    });
            }
        }

        // Also submit Sell Asks for excess inventory (fire sale if utilization is high)
        if utilization >= config.fire_sale_threshold {
            let company_inventory = compute_company_inventory(company, buildings);
            for (&commodity, &qty) in &company_inventory {
                if qty <= 0.0 {
                    continue;
                }
                let ref_price = match get_reference_price(&commodity, market_history) {
                    Some(p) => p,
                    None => continue,
                };
                let fire_sale_price = ref_price * (1.0 + config.min_markup_ratio);

                order_book
                    .asks
                    .entry(commodity)
                    .or_insert_with(Vec::new)
                    .push(Ask {
                        seller_id: company.id.clone(),
                        commodity,
                        quantity: qty * 0.5, // Sell half of excess inventory
                        limit_price: fire_sale_price,
                        blueprint_id: None,
                        quality: None,
                        durability: None,
                    });
            }
        }
    }

    messages
}

/// Settle executed trades with double-entry accounting and physical inventory routing.
///
/// # Arguments
/// * `trades` - Slice of executed trades from `match_orders`.
/// * `companies` - Mutable slice of all companies for cash settlement.
/// * `buildings` - Mutable slice of all buildings for inventory routing.
///
/// # Returns
/// A vector of diagnostic messages.
///
/// # Rules
/// * Cash: `buyer.debit_cash -= trade_value`, `seller.available_cash += trade_value`.
/// * Inventory: routes directly to `Building.inventory` — no Company-level field.
/// * Buyer building: belongs to `buyer_id` company + has commodity in BOM inputs.
/// * Seller building: belongs to `seller_id` company + has commodity in BOM outputs.
/// * If seller's building has insufficient inventory, trade is clamped to available.
/// * Double-entry: every debit has a matching credit.
pub fn settle_trades(
    trades: &[Trade],
    companies: &mut [Company],
    buildings: &mut [Building],
) -> Vec<String> {
    let mut messages = Vec::new();

    for trade in trades {
        let trade_value = trade.quantity * trade.execution_price;

        // --- Cash Settlement (Phase 24A.4: via TransferSettler) ---
        // Debit buyer: release encumbered cash and debit actual cash via transfer
        // Credit seller: add received cash via transfer
        // Skip for defense trades (buyer_id == "MIN-DEF") — cash is credited by
        // settle_defense_trades via TransferSettler's credit_company_by_id to
        // ensure proper bank balance sheet synchronization (Black Hole 1.19).
        if trade.buyer_id != "MIN-DEF" {
            // Find buyer and seller indices
            let buyer_idx = companies.iter().position(|c| c.id == trade.buyer_id);
            let seller_idx = companies.iter().position(|c| c.id == trade.seller_id);

            if let (Some(bi), Some(si)) = (buyer_idx, seller_idx) {
                // Use TransferSettler for proper double-entry settlement
                // This handles: buyer cash debit, seller cash credit, bank balance sheet sync
                let dummy_country = &mut crate::state::Country::default();
                let _ = crate::economy::transfer_settler::settle_transfer(
                    companies,
                    bi,
                    trade_value,
                    &crate::economy::transfer_settler::TransferRecipient::OtherCompany { recipient_idx: si },
                    dummy_country,
                );
                // Also release the buyer's encumbered debit_cash
                companies[bi].debit_cash -= trade_value;
                // Phase 45: Clear unfilled bid tracking for this commodity
                // since the bid was successfully filled.
                companies[bi].unfilled_bid_prices.remove(&trade.commodity);
            } else {
                // Fallback: manual settlement if indices not found
                if let Some(buyer) = companies.iter_mut().find(|c| c.id == trade.buyer_id) {
                    buyer.debit_cash -= trade_value;
                }
                if let Some(seller) = companies.iter_mut().find(|c| c.id == trade.seller_id) {
                    seller.available_cash += trade_value;
                    if let Some(ba) = &mut seller.brokerage_account {
                        ba.cash += trade_value;
                    }
                }
            }
        } else {
            // Defense trade: just release buyer's encumbered cash
            if let Some(buyer) = companies.iter_mut().find(|c| c.id == trade.buyer_id) {
                buyer.debit_cash -= trade_value;
            }
        }

        // --- Physical Inventory Routing ---
        // Find buyer's building that needs this commodity (has it in BOM inputs
        // or in an active construction project's required materials)
        let buyer_building_idx = buildings.iter().position(|b| {
            b.owner_id == trade.buyer_id
                && (
                    // Normal production: commodity is in BOM inputs
                    b.active_method.inputs.contains_key(&trade.commodity)
                    // Construction: commodity is in active project's required materials
                    || b.active_project.as_ref()
                        .map(|p| p.required_materials.contains_key(&trade.commodity))
                        .unwrap_or(false)
                )
        });

        if let Some(idx) = buyer_building_idx {
            let building = &mut buildings[idx];
            *building
                .inventory
                .entry(trade.commodity)
                .or_insert(0.0) += trade.quantity;
        } else {
            // Buyer has no building that consumes this commodity — deposit to first building
            if let Some(idx) = buildings.iter().position(|b| b.owner_id == trade.buyer_id) {
                let building = &mut buildings[idx];
                *building
                    .inventory
                    .entry(trade.commodity)
                    .or_insert(0.0) += trade.quantity;
            }
        }

        // Find seller's building that produces this commodity (has it in BOM outputs)
        // and has sufficient inventory
        let seller_building_idx = buildings.iter().position(|b| {
            b.owner_id == trade.seller_id
                && b.active_method.outputs.contains_key(&trade.commodity)
                && b.inventory.get(&trade.commodity).copied().unwrap_or(0.0) >= trade.quantity
        });

        if let Some(idx) = seller_building_idx {
            let building = &mut buildings[idx];
            let current = building.inventory.get(&trade.commodity).copied().unwrap_or(0.0);
            let new_qty = (current - trade.quantity).max(0.0);
            if new_qty > 0.0 {
                building.inventory.insert(trade.commodity, new_qty);
            } else {
                building.inventory.remove(&trade.commodity);
            }
        } else {
            // Fallback: find any building of the seller with this commodity
            let fallback_idx = buildings.iter().position(|b| {
                b.owner_id == trade.seller_id
                    && b.inventory.get(&trade.commodity).copied().unwrap_or(0.0) >= trade.quantity
            });

            if let Some(idx) = fallback_idx {
                let building = &mut buildings[idx];
                let current = building.inventory.get(&trade.commodity).copied().unwrap_or(0.0);
                let new_qty = (current - trade.quantity).max(0.0);
                if new_qty > 0.0 {
                    building.inventory.insert(trade.commodity, new_qty);
                } else {
                    building.inventory.remove(&trade.commodity);
                }
            } else {
                // Seller doesn't have the inventory — this is a phantom trade
                // The trade still settled in cash, but no goods were delivered
                messages.push(format!(
                    "WARNING: Seller {} has no building with {:?} inventory for trade of {}",
                    trade.seller_id, trade.commodity, trade.quantity
                ));
            }
        }
    }

    messages
}

/// Settle trades with physical tariff collection (Phase 11).
///
/// Wraps `settle_trades` for cash/inventory movement, then performs a second
/// pass to collect tariffs on cross-border trades. The tariff amount is
/// debited from the buyer's encumbered cash and credited to the buyer's
/// country treasury.
///
/// # Arguments
/// * `trades` - Executed trades to settle.
/// * `companies` - Mutable companies for cash settlement.
/// * `buildings` - Mutable buildings for inventory routing.
/// * `country` - The country whose companies are being settled (receives tariff revenue).
/// * `company_country` - Ephemeral lookup: company_id → country_name.
/// * `diplomacy` - Bilateral diplomatic relations matrix.
///
/// # Rules
/// * Calls `settle_trades` first for standard cash + inventory settlement.
/// * For each trade where buyer and seller are in different countries:
///   - Looks up the buyer's import tariff rate for the traded commodity.
///   - `tariff_amount = trade_value * tariff_rate`.
///   - Debits `tariff_amount` from buyer's `debit_cash` (additional encumbrance release).
///   - Credits `tariff_amount` to `country.budget.liquid_reserves`.
/// * Same-country trades incur no tariff.
/// * Companies not in the lookup table bypass tariff checks.
/// * Double-entry: buyer loses `trade_value + tariff`, seller gains `trade_value`,
///   treasury gains `tariff`. Sum = 0.
pub fn settle_trades_with_tariffs(
    trades: &[Trade],
    companies: &mut [Company],
    buildings: &mut [Building],
    country: &mut crate::state::Country,
    company_country: &std::collections::HashMap<String, String>,
    diplomacy: &std::collections::HashMap<String, std::collections::HashMap<String, crate::international::DiplomaticRelation>>,
    country_to_currency: &std::collections::HashMap<String, String>,
) -> Vec<String> {
    // Phase 1: Standard settlement (cash + inventory)
    let messages = settle_trades(trades, companies, buildings);

    // Phase 42: FX Reserves — forced currency conversion and import hard floor.
    // For cross-border trades, the buyer's country Central Bank must:
    // - Exports (seller is domestic): credit foreign currency to FX reserves,
    //   debit domestic currency to the exporter.
    // - Imports (buyer is domestic): check FX reserves for seller's currency;
    //   if insufficient, the trade fails (revert settlement).
    let domestic_ccy = country.macro_indicators.currency.clone();

    // Phase 2: Tariff collection on cross-border trades + FX conversion
    for trade in trades {
        let trade_value = trade.quantity * trade.execution_price;

        let buyer_country = match company_country.get(&trade.buyer_id) {
            Some(bc) => bc.clone(),
            None => continue, // Unknown company — bypass
        };
        let seller_country = match company_country.get(&trade.seller_id) {
            Some(sc) => sc.clone(),
            None => continue, // Unknown company — bypass
        };

        // Same-country trades incur no tariff and no FX conversion
        if buyer_country == seller_country {
            continue;
        }

        // Phase 42: FX conversion for cross-border trades.
        let is_import = buyer_country == country.name;
        let is_export = seller_country == country.name;

        if is_export {
            // Export: foreign buyer pays in foreign currency.
            // The Central Bank accumulates the foreign currency in fx_reserves.
            // Phase 43: Use the buyer's real currency code, not fake "IEU".
            let foreign_ccy = country_to_currency.get(&buyer_country)
                .cloned()
                .unwrap_or_else(|| "???".to_string());
            let exchange_rate = 1.0;
            let foreign_amount = trade_value * exchange_rate;
            *country.central_bank.fx_reserves.entry(foreign_ccy.clone()).or_insert(0.0) += foreign_amount;
        } else if is_import {
            // Import: check FX reserves for the seller's currency.
            // Phase 43: Use the seller's real currency code, not fake "IEU".
            let seller_ccy = country_to_currency.get(&seller_country)
                .cloned()
                .unwrap_or_else(|| "???".to_string());
            let exchange_rate = 1.0;
            let foreign_needed = trade_value * exchange_rate;
            let available_fx = country.central_bank.fx_reserves.get(&seller_ccy).copied().unwrap_or(0.0);
            if available_fx < foreign_needed {
                // Phase 42: Import fails — revert settlement.
                if let Some(buyer_idx) = companies.iter().position(|c| c.id == trade.buyer_id) {
                    companies[buyer_idx].available_cash += trade_value;
                    if let Some(ba) = &mut companies[buyer_idx].brokerage_account {
                        ba.cash += trade_value;
                    }
                }
                if let Some(seller_idx) = companies.iter().position(|c| c.id == trade.seller_id) {
                    companies[seller_idx].available_cash -= trade_value;
                    if let Some(ba) = &mut companies[seller_idx].brokerage_account {
                        ba.cash -= trade_value;
                    }
                }
                if let Some(idx) = buildings.iter().position(|b| b.owner_id == trade.buyer_id) {
                    if let Some(qty) = buildings[idx].inventory.get_mut(&trade.commodity) {
                        *qty = (*qty - trade.quantity).max(0.0);
                    }
                }
                continue; // Skip tariff collection for failed trade
            }
            // Sufficient FX: debit reserves.
            *country.central_bank.fx_reserves.entry(seller_ccy.clone()).or_insert(0.0) -= foreign_needed;
        }

        // Phase 29: Check FTA/customs union between buyer and seller countries.
        // If they have a free trade agreement or customs union, tariffs are
        // reduced or eliminated. Embargo penalties add to the tariff.
        let mut tariff_rate = country
            .trade_policy
            .import_tariffs
            .get(&trade.commodity)
            .copied()
            .unwrap_or(0.0);

        // Check diplomatic relations for FTA/embargo overrides
        if let Some(buyer_diplomacy) = diplomacy.get(&buyer_country) {
            if let Some(rel) = buyer_diplomacy.get(&seller_country) {
                if rel.customs_union {
                    // Customs union: eliminate all tariffs
                    tariff_rate = 0.0;
                } else if rel.free_trade {
                    // FTA: reduce tariff by 90% (not fully eliminated for non-customs-union)
                    tariff_rate *= 0.10;
                }
                // Embargo penalty adds to the effective tariff
                if rel.embargo_penalty > 0.0 {
                    tariff_rate += rel.embargo_penalty;
                }
            }
        }

        if tariff_rate <= 0.0 {
            continue;
        }

        let tariff_amount = trade_value * tariff_rate;

        // Phase 24A.4: Use TransferSettler for tariff collection (double-entry).
        // Debits buyer's cash and credits treasury via proper bank sync.
        if let Some(buyer_idx) = companies.iter().position(|c| c.id == trade.buyer_id) {
            let _ = crate::economy::transfer_settler::settle_transfer_to_treasury(
                companies,
                buyer_idx,
                tariff_amount,
                country,
            );
            // Also release the buyer's encumbered debit_cash for the tariff
            if let Some(buyer) = companies.get_mut(buyer_idx) {
                buyer.debit_cash = (buyer.debit_cash - tariff_amount).max(0.0);
            }
        } else {
            // Fallback: manual debit if buyer not found
            if let Some(buyer) = companies.iter_mut().find(|c| c.id == trade.buyer_id) {
                buyer.debit_cash = (buyer.debit_cash - tariff_amount).max(0.0);
            }
            // Credit tariff to the buyer's country treasury
            country.budget.liquid_reserves += tariff_amount;
        }
    }

    messages
}

/// Refund unfilled bids after order matching.
///
/// # Arguments
/// * `order_book` - Order book with remaining unfilled bids.
/// * `companies` - Mutable slice of companies for refund.
///
/// # Rules
/// * Refunds encumbered cash: `debit_cash -= refund`, `available_cash += refund`.
/// * Also restores to brokerage account so it's spendable next turn.
pub fn refund_unfilled_bids(
    order_book: &OrderBook,
    companies: &mut [Company],
) {
    for bids in order_book.bids.values() {
        for bid in bids {
            if let Some(company) = companies.iter_mut().find(|c| c.id == bid.buyer_id) {
                let refund = bid.quantity * bid.limit_price;
                company.debit_cash -= refund;
                company.available_cash += refund;
                if let Some(ba) = &mut company.brokerage_account {
                    ba.cash += refund;
                }
                // Phase 45: Track unfilled bid prices for dynamic price feedback.
                // Store the limit price so next turn's bid can be raised.
                company.unfilled_bid_prices.insert(bid.commodity, bid.limit_price);
            }
        }
    }
}

/// Settle defense procurement trades by syncing the seller's bank balance sheet.
///
/// This handles trades where `buyer_id == "MIN-DEF"`. The seller was already credited
/// by `settle_trades` (cash + brokerage_account), and the Treasury was already
/// debited (encumbered) at bid submission time. This function only syncs the
/// seller's cash and bank balance sheet (deposits + reserves) via
/// `TransferSettler::credit_company_by_id`, ensuring proper double-entry
/// accounting. `settle_trades` skips the cash credit for defense trades
/// (buyer_id == "MIN-DEF") so that this function can perform the full
/// credit atomically through the TransferSettler module (Black Hole 1.19).
///
/// # Arguments
/// * `trades` - All executed trades (only defense trades are processed).
/// * `companies` - Mutable slice of companies (for cash credit + bank sync).
/// * `_country` - Mutable country (unused — Treasury was already debited at encumbrance).
pub fn settle_defense_trades(
    trades: &[Trade],
    companies: &mut [Company],
    _country: &mut crate::state::Country,
) {
    for trade in trades {
        if trade.buyer_id != "MIN-DEF" {
            continue;
        }
        let trade_value = trade.quantity * trade.execution_price;
        if trade_value <= 0.0 {
            continue;
        }

        // Credit seller's cash AND sync bank balance sheet atomically
        // via TransferSettler (Black Hole 1.19).
        crate::economy::transfer_settler::credit_company_by_id(
            companies,
            &trade.seller_id,
            trade_value,
        );
    }
}

/// Refund unfilled defense bids back to the Treasury (per-country).
///
/// Defense bids (`buyer_id == "MIN-DEF"`) are not associated with any company,
/// so `refund_unfilled_bids` cannot refund them. This function computes the
/// refund as `total_encumbered - filled_encumbered`, where:
/// * `total_encumbered` = sum of all original defense bid values.
/// * `filled_encumbered` = sum of `trade.quantity * trade.bid_limit_price` for
///   defense trades where the seller is a company in this country.
///
/// The refund covers both unfilled bid portions and the price difference
/// between `limit_price` and `execution_price` on filled trades.
///
/// # Arguments
/// * `original_bids` - The defense bids originally submitted by this country's MoD.
/// * `trades` - All executed trades (only defense trades are considered).
/// * `companies` - Slice of this country's companies (to match sellers).
/// * `country` - Mutable country (for Treasury refund).
pub fn refund_unfilled_defense_bids_per_country(
    original_bids: &[crate::economy::order_book::Bid],
    trades: &[Trade],
    companies: &[Company],
    country: &mut crate::state::Country,
) {
    let total_encumbered: f64 = original_bids
        .iter()
        .map(|b| b.quantity * b.limit_price)
        .sum();

    let filled_paid: f64 = trades
        .iter()
        .filter(|t| {
            t.buyer_id == "MIN-DEF" && companies.iter().any(|c| c.id == t.seller_id)
        })
        .map(|t| t.quantity * t.execution_price)
        .sum();

    let refund = (total_encumbered - filled_paid).max(0.0);
    if refund > 0.0 {
        country.budget.liquid_reserves += refund;
    }
}

/// Execute production for all buildings after trade settlement.
///
/// # Arguments
/// * `buildings` - Mutable slice of all buildings (inventory consumed and produced).
/// * `commercial_buildings` - Mutable slice of warehouse buildings for overflow.
/// * `companies` - Mutable slice of companies (for warehouse fee deduction).
/// * `config` - B2B order configuration (for inventory capacity and storage fees).
///
/// # Returns
/// A vector of `ProductionResult` for each building.
///
/// # Rules
/// * Check delivered inputs in `building.inventory`.
/// * Calculate `fulfillment_ratio = min(input_available / input_required)` per BOM.
/// * Consume inputs: `building.inventory[commodity] -= required × fulfillment_ratio`.
/// * Produce outputs: `building.inventory[commodity] += output_per_1k × scale × fulfillment_ratio × efficiency`.
/// * Wages are NOT paid here — already settled in Phase W1.
/// * Overflow above `inventory_capacity` routes to warehouse (storage fees).
/// * If no warehouse capacity, excess perishes immediately (NO next-turn buffer).
pub fn execute_production_cycle(
    buildings: &mut [Building],
    commercial_buildings: &mut [crate::society::housing::CommercialBuilding],
    companies: &mut [Company],
    config: &B2bOrderConfig,
    sector_filter: Option<Sector>,
    efficiency_penalties: Option<&std::collections::HashMap<String, f64>>,
    gen_config: &crate::economy::generative_goods_config::GenerativeGoodsConfig,
    frontier_year: u32,
) -> Vec<ProductionResult> {
    use crate::economy::fixed_assets::machinery_factor;

    let mut results = Vec::new();

    for building in buildings.iter_mut() {
        // Skip brand-new construction sites (no workers, active project)
        if building.active_project.is_some() && building.worker_capacity == 0 {
            continue;
        }

        // Wave filtering: if sector_filter is Some, only process buildings in that sector
        if let Some(filter_sector) = sector_filter {
            if building.sector != filter_sector {
                continue;
            }
        }

        let method = &building.active_method;
        let production_scale = building.current_employment as f64 / 1000.0;
        let efficiency = method.efficiency;

        // Phase 19B: Fixed-asset cohorts provide a capacity multiplier.
        // Empty cohorts → factor = 1.0 (pre-Phase-19 behavior, no GDP cliff).
        // Installed machinery → factor = 1.0 + Σ(count × quality × condition × obs × unit_capacity).
        let machinery_factor = machinery_factor(&building.fixed_assets, frontier_year, gen_config);

        // Calculate required inputs and available inputs.
        // Phase 19B: Skip fixed-asset commodities — they're not consumed per-turn.
        let mut fulfillment_ratio = 1.0;
        for (&commodity, &qty_per_1k) in &method.inputs {
            if commodity.is_fixed_asset() {
                continue; // Fixed assets provide capacity, not consumable input.
            }
            let required = qty_per_1k * production_scale;
            if required > 0.0 {
                let available = building.inventory.get(&commodity).copied().unwrap_or(0.0);
                let ratio = available / required;
                if ratio < fulfillment_ratio {
                    fulfillment_ratio = ratio;
                }
            }
        }

        // Clamp fulfillment ratio to [0, 1]
        fulfillment_ratio = fulfillment_ratio.clamp(0.0, 1.0);

        // Apply blackout efficiency penalty (Wave 3 only)
        if let Some(penalties) = efficiency_penalties {
            if let Some(&penalty) = penalties.get(&building.id) {
                fulfillment_ratio *= (1.0 - penalty).max(0.0);
            }
        }

        // Consume inputs (skip fixed-asset commodities — they're not consumed).
        let mut inputs_consumed: HashMap<Commodity, f64> = HashMap::default();
        for (&commodity, &qty_per_1k) in &method.inputs {
            if commodity.is_fixed_asset() {
                continue;
            }
            let required = qty_per_1k * production_scale * fulfillment_ratio;
            if required > 0.0 {
                let available = building.inventory.get(&commodity).copied().unwrap_or(0.0);
                let consumed = required.min(available);
                let remaining = (available - consumed).max(0.0);
                if remaining > 0.0 {
                    building.inventory.insert(commodity, remaining);
                } else {
                    building.inventory.remove(&commodity);
                }
                inputs_consumed.insert(commodity, consumed);
            }
        }

        // Produce outputs (multiplied by the Phase 19B machinery capacity factor).
        let mut outputs_produced: HashMap<Commodity, f64> = HashMap::default();
        for (&commodity, &qty_per_1k) in &method.outputs {
            let produced = qty_per_1k * production_scale * fulfillment_ratio * efficiency * machinery_factor;
            if produced > 0.0 {
                *building.inventory.entry(commodity).or_insert(0.0) += produced;
                outputs_produced.insert(commodity, produced);
            }
        }

        // Fix 1.21: Track the financial value of destroyed overflow inventory.
        let unit_cost_for_writeoff = calculate_unit_cost(building, &HashMap::default(), 0.0);
        let mut inventory_write_down: f64 = 0.0;
        // Phase 29: Track total overflow costs (storage fees + write-downs)
        // for ROI-driven warehouse construction decisions.
        let mut overflow_costs_this_turn: f64 = 0.0;

        // Handle inventory overflow
        let total_inventory: f64 = building.inventory.values().sum();
        if total_inventory > building.inventory_capacity {
            let overflow = total_inventory - building.inventory_capacity;

            // Try to route overflow to warehouse
            let warehouse_capacity = find_warehouse_capacity(
                commercial_buildings,
                &building.region_id,
            );

            if warehouse_capacity >= overflow {
                // Route to warehouse — deduct storage fees from company, credit warehouse owner
                let storage_fee = overflow * config.warehouse_storage_fee_per_ton;
                overflow_costs_this_turn += storage_fee;
                let warehouse_owner = find_warehouse_owner(commercial_buildings, &building.region_id);
                let _debited = debit_company_by_id(companies, &building.owner_id, storage_fee);
                if let Some(ref owner_id) = warehouse_owner {
                    if !owner_id.is_empty() {
                        credit_company_by_id(companies, owner_id, _debited);
                    }
                }
                // Move overflow commodities to warehouse (proportionally)
                let ratio = overflow / total_inventory;
                for (&commodity, &qty) in &building.inventory.clone() {
                    let moved = qty * ratio;
                    if moved > 0.0 {
                        let remaining = (qty - moved).max(0.0);
                        if remaining > 0.0 {
                            building.inventory.insert(commodity, remaining);
                        } else {
                            building.inventory.remove(&commodity);
                        }
                        // Add to warehouse
                        deposit_to_warehouse(
                            commercial_buildings,
                            &building.region_id,
                            commodity,
                            moved,
                        );
                    }
                }
            } else if warehouse_capacity > 0.0 {
                // Partial warehouse storage, rest perishes
                let storable = warehouse_capacity;
                let ratio = storable / overflow;
                let storage_fee = storable * config.warehouse_storage_fee_per_ton;
                overflow_costs_this_turn += storage_fee;
                let warehouse_owner = find_warehouse_owner(commercial_buildings, &building.region_id);
                let _debited = debit_company_by_id(companies, &building.owner_id, storage_fee);
                if let Some(ref owner_id) = warehouse_owner {
                    if !owner_id.is_empty() {
                        credit_company_by_id(companies, owner_id, _debited);
                    }
                }
                let inv_ratio = storable / total_inventory;
                for (&commodity, &qty) in &building.inventory.clone() {
                    let moved = qty * inv_ratio;
                    if moved > 0.0 {
                        let remaining = (qty - moved).max(0.0);
                        if remaining > 0.0 {
                            building.inventory.insert(commodity, remaining);
                        } else {
                            building.inventory.remove(&commodity);
                        }
                        deposit_to_warehouse(
                            commercial_buildings,
                            &building.region_id,
                            commodity,
                            moved,
                        );
                    }
                }
                // Remaining overflow perishes — destroy excess
                let new_total: f64 = building.inventory.values().sum();
                if new_total > building.inventory_capacity {
                    let excess = new_total - building.inventory_capacity;
                    let destroy_ratio = excess / new_total;
                    // Fix 1.21: Record the financial write-down for destroyed inventory
                    inventory_write_down += excess * unit_cost_for_writeoff;
                    overflow_costs_this_turn += excess * unit_cost_for_writeoff;
                    for (&commodity, &qty) in &building.inventory.clone() {
                        let destroyed = qty * destroy_ratio;
                        let remaining = (qty - destroyed).max(0.0);
                        if remaining > 0.0 {
                            building.inventory.insert(commodity, remaining);
                        } else {
                            building.inventory.remove(&commodity);
                        }
                    }
                }
            } else {
                // No warehouse capacity — all overflow perishes immediately
                let destroy_ratio = overflow / total_inventory;
                // Fix 1.21: Record the financial write-down for destroyed inventory
                inventory_write_down += overflow * unit_cost_for_writeoff;
                overflow_costs_this_turn += overflow * unit_cost_for_writeoff;
                for (&commodity, &qty) in &building.inventory.clone() {
                    let destroyed = qty * destroy_ratio;
                    let remaining = (qty - destroyed).max(0.0);
                    if remaining > 0.0 {
                        building.inventory.insert(commodity, remaining);
                    } else {
                        building.inventory.remove(&commodity);
                    }
                }
            }
        }

        // Phase 29: Store overflow costs on building for regional aggregation.
        if overflow_costs_this_turn > 0.0 {
            building.extra.insert(
                "overflow_costs_this_turn".to_string(),
                serde_json::Value::from(overflow_costs_this_turn),
            );
        }

        // Update last_production
        for (&commodity, &qty) in &outputs_produced {
            building.last_production.insert(commodity, qty);
        }

        // Fix 1.20: Calculate actual financial metrics using unit cost.
        // The B2B/B2C cash flows are settled separately via settle_trades and
        // settle_b2c_clearing, but the ProductionResult must reflect the value
        // of outputs produced and inputs consumed for financial statements.
        let unit_cost = calculate_unit_cost(building, &HashMap::default(), 0.0);
        let input_costs: f64 = inputs_consumed
            .iter()
            .map(|(&commodity, &qty)| {
                let price = if unit_cost > 0.0 { unit_cost } else { 0.0 };
                qty * price * 0.5
            })
            .sum();
        let output_revenue: f64 = outputs_produced
            .iter()
            .map(|(_, &qty)| qty * unit_cost)
            .sum();

        // Fix 1.21: Record inventory write-down loss from overflow destruction.
        // The write_down value was accumulated during overflow handling above.
        let gross_profit = output_revenue - input_costs - inventory_write_down;

        // Update building.last_profit for corporate/tax processing
        building.last_profit = gross_profit;

        results.push(ProductionResult {
            inputs_consumed,
            outputs_produced,
            wages_paid: 0.0, // Wages paid in Phase W1, not here
            input_costs,
            output_revenue,
            gross_profit,
        });
    }

    results
}

/// Find available warehouse capacity in a region.
fn find_warehouse_capacity(
    commercial_buildings: &[crate::society::housing::CommercialBuilding],
    region_id: &str,
) -> f64 {
    commercial_buildings
        .iter()
        .filter(|b| {
            b.building_type == crate::society::housing::CommercialBuildingType::Warehouse
                && b.micro_region_id == region_id
        })
        .map(|b| {
            let total_stored: f64 = b
                .current_inventory
                .values()
                .flat_map(|batches| batches.iter().map(|batch| batch.quantity))
                .sum();
            (b.storage_capacity - total_stored).max(0.0)
        })
        .sum()
}

/// Find the owner company ID of the first warehouse with available capacity in a region.
fn find_warehouse_owner(
    commercial_buildings: &[crate::society::housing::CommercialBuilding],
    region_id: &str,
) -> Option<String> {
    commercial_buildings
        .iter()
        .filter(|b| {
            b.building_type == crate::society::housing::CommercialBuildingType::Warehouse
                && b.micro_region_id == region_id
        })
        .find(|b| {
            let total_stored: f64 = b
                .current_inventory
                .values()
                .flat_map(|batches| batches.iter().map(|batch| batch.quantity))
                .sum();
            (b.storage_capacity - total_stored) > 0.0
        })
        .map(|b| b.owner_id.clone())
}

/// Deposit a commodity quantity into the first available warehouse in a region.
fn deposit_to_warehouse(
    commercial_buildings: &mut [crate::society::housing::CommercialBuilding],
    region_id: &str,
    commodity: Commodity,
    quantity: f64,
) {
    let commodity_key: String = commodity.into();

    for building in commercial_buildings.iter_mut() {
        if building.building_type != crate::society::housing::CommercialBuildingType::Warehouse
            || building.micro_region_id != region_id
        {
            continue;
        }

        let total_stored: f64 = building
            .current_inventory
            .values()
            .flat_map(|batches| batches.iter().map(|batch| batch.quantity))
            .sum();

        let available = (building.storage_capacity - total_stored).max(0.0);
        if available <= 0.0 {
            continue;
        }

        let to_store = quantity.min(available);
        if to_store <= 0.0 {
            continue;
        }

        let batch = crate::society::housing::InventoryBatch {
            quantity: to_store,
            storage_turn: 0, // Will be set by caller in production
            owner_id: String::new(),
            accumulated_fees: 0.0,
            warehouse_id: building.id.clone(),
            fire_sale_discount: 0.0,
            acquisition_cost_per_unit: 0.0,
        };

        building
            .current_inventory
            .entry(commodity_key.clone())
            .or_insert_with(Vec::new)
            .push(batch);

        if to_store >= quantity {
            return; // All stored
        }
    }
}

// ── Phase 19B: Maintenance Service B2B Market ─────────────────────────────

/// Submit Buy Bids for `Commodity::MaintenanceServices` on behalf of factories
/// with degraded fixed-asset cohorts.
///
/// # Rules
/// * For each building with `fixed_assets`, compute `maintenance_services_needed`.
/// * If needed > 0, submit a Buy Bid for `MaintenanceServices` at a reference
///   price (from `market_history` or a fallback).
/// * Cash is encumbered exactly like normal B2B bids.
/// * MaintenanceServices Sell Asks are generated naturally by MaintenanceWorkshops
///   buildings via the normal output-ask loop in `submit_company_b2b_orders`
///   (MaintenanceServices is in their `method.outputs`).
pub fn submit_maintenance_service_bids(
    companies: &mut [Company],
    buildings: &[Building],
    order_book: &mut OrderBook,
    market_history: &MarketHistory,
    b2b_config: &B2bOrderConfig,
    gen_config: &crate::economy::generative_goods_config::GenerativeGoodsConfig,
) -> Vec<String> {
    use crate::economy::fixed_assets::maintenance_services_needed;
    use crate::registries::enums::Commodity;

    let mut messages = Vec::new();
    for company in companies.iter_mut() {
        let liquid = company.computed_liquid_capital();
        company.available_cash = liquid;
        let max_encumber = liquid * b2b_config.max_cash_encumbrance_ratio;
        let mut total_encumbered = 0.0;

        for building in buildings.iter().filter(|b| b.owner_id == company.id) {
            if building.fixed_assets.is_empty() {
                continue;
            }
            let needed = maintenance_services_needed(&building.fixed_assets, gen_config);
            if needed <= 0.0 {
                continue;
            }
            let ref_price = get_reference_price(&Commodity::MaintenanceServices, market_history)
                .unwrap_or(1.0);
            let limit_price = ref_price * (1.0 + b2b_config.buy_premium_ratio);
            let encumbrance = needed * limit_price;
            if total_encumbered + encumbrance > max_encumber {
                let remaining = max_encumber - total_encumbered;
                if remaining <= 0.0 || limit_price <= 0.0 {
                    continue;
                }
                let affordable_qty = remaining / limit_price;
                if affordable_qty <= 0.0 {
                    continue;
                }
                company.available_cash -= affordable_qty * limit_price;
                company.debit_cash += affordable_qty * limit_price;
                total_encumbered += affordable_qty * limit_price;
                order_book
                    .bids
                    .entry(Commodity::MaintenanceServices)
                    .or_insert_with(Vec::new)
                    .push(Bid {
                        buyer_id: company.id.clone(),
                        commodity: Commodity::MaintenanceServices,
                        quantity: affordable_qty,
                        limit_price,
                        blueprint_id: None,
                        min_quality: None,
                    });
            } else {
                company.available_cash -= encumbrance;
                company.debit_cash += encumbrance;
                total_encumbered += encumbrance;
                order_book
                    .bids
                    .entry(Commodity::MaintenanceServices)
                    .or_insert_with(Vec::new)
                    .push(Bid {
                        buyer_id: company.id.clone(),
                        commodity: Commodity::MaintenanceServices,
                        quantity: needed,
                        limit_price,
                        blueprint_id: None,
                        min_quality: None,
                    });
            }
        }
    }
    messages
}

/// Phase 19C: Submit Buy Bids for fixed-asset commodities (machinery/vehicles)
/// with **cash-bottlenecked** willingness-to-pay.
///
/// # Rules (affordability segmentation)
/// * Companies derive a `desired_wtp` from the reference price scaled by a
///   quality/durability premium (premium assets cost more but last longer).
/// * The actual `limit_price` is clamped by:
///   `affordable_wtp = min(desired_wtp, liquid_capital × max_cash_encumbrance_ratio / quantity)`.
/// * Cash-poor companies are forced toward cheaper, lower-quality substitutes:
///   they submit bids at a lower limit price, which only matches low-priced asks
///   (typically from lower-quality blueprints).
/// * If the company cannot afford even the baseline reference price, no bid is
///   submitted (the company goes without new machinery this turn).
/// * Bids carry `blueprint_id: None` and `min_quality: None` — the matching
///   engine fills the cheapest compatible ask first (price-time priority), so
///   cash-poor buyers naturally receive lower-quality assets.
pub fn submit_fixed_asset_purchase_bids(
    companies: &mut [Company],
    buildings: &[Building],
    order_book: &mut OrderBook,
    market_history: &MarketHistory,
    b2b_config: &B2bOrderConfig,
    gen_config: &crate::economy::generative_goods_config::GenerativeGoodsConfig,
) -> Vec<String> {
    let mut messages = Vec::new();
    for company in companies.iter_mut() {
        let liquid = company.computed_liquid_capital();
        company.available_cash = liquid;
        let max_encumber = liquid * b2b_config.max_cash_encumbrance_ratio;
        let mut total_encumbered = 0.0;

        for building in buildings.iter().filter(|b| b.owner_id == company.id) {
            let method = &building.active_method;
            let production_scale = building.current_employment as f64 / 1000.0;

            for (&commodity, &qty_per_1k) in &method.inputs {
                // Only fixed-asset commodities (skipped by the normal bid loop).
                if !commodity.is_fixed_asset() {
                    continue;
                }
                let desired_qty = qty_per_1k * production_scale;
                if desired_qty <= 0.0 {
                    continue;
                }

                let ref_price = match get_reference_price(&commodity, market_history) {
                    Some(p) => p,
                    None => continue,
                };

                // Desired willingness-to-pay: reference price scaled by a
                // quality/durability premium. Companies *want* premium assets
                // but can only afford what their cash allows.
                let quality_premium = gen_config.asset_quality_wtp_multiplier;
                let desired_wtp = ref_price * quality_premium;

                // Cash-bottlenecked limit: the most the company can pay per unit
                // given its remaining encumbrance headroom.
                let remaining_encumbrance = (max_encumber - total_encumbered).max(0.0);
                if remaining_encumbrance <= 0.0 {
                    break; // No budget left for asset purchases.
                }
                let affordable_wtp = remaining_encumbrance / desired_qty;
                let limit_price = desired_wtp.min(affordable_wtp);

                // If the affordable WTP is below the baseline reference price,
                // the company can only buy cheaper (lower-quality) substitutes.
                // If it's below a starvation threshold, skip (go without).
                if limit_price < ref_price * gen_config.asset_purchase_starvation_ratio {
                    continue;
                }

                let encumbrance = desired_qty * limit_price;
                company.available_cash -= encumbrance;
                company.debit_cash += encumbrance;
                total_encumbered += encumbrance;

                order_book
                    .bids
                    .entry(commodity)
                    .or_insert_with(Vec::new)
                    .push(Bid {
                        buyer_id: company.id.clone(),
                        commodity,
                        quantity: desired_qty,
                        limit_price,
                        blueprint_id: None,
                        min_quality: None,
                    });
            }

            // Phase 45: Replacement demand from degraded fixed-asset cohorts.
            // For each cohort, compute the condition deficit:
            //   replacement_demand = count * (1.0 - condition)
            // This represents the quantity of new machinery needed to restore
            // the building's production capacity to full.
            let mut replacement_needed: std::collections::HashMap<Commodity, f64> = std::collections::HashMap::default();
            for cohort in &building.fixed_assets {
                if cohort.is_scrapped() {
                    continue;
                }
                let deficit = cohort.count * (1.0 - cohort.condition);
                if deficit > 0.0 {
                    *replacement_needed.entry(cohort.commodity).or_insert(0.0) += deficit;
                }
            }

            for (&commodity, &qty) in &replacement_needed {
                if qty <= 0.0 {
                    continue;
                }
                let ref_price = match get_reference_price(&commodity, market_history) {
                    Some(p) => p,
                    None => continue,
                };
                let remaining_encumbrance = (max_encumber - total_encumbered).max(0.0);
                if remaining_encumbrance <= 0.0 {
                    break;
                }
                let quality_premium = gen_config.asset_quality_wtp_multiplier;
                let desired_wtp = ref_price * quality_premium;
                let affordable_wtp = remaining_encumbrance / qty;
                let limit_price = desired_wtp.min(affordable_wtp);
                if limit_price < ref_price * gen_config.asset_purchase_starvation_ratio {
                    continue;
                }
                let encumbrance = qty * limit_price;
                company.available_cash -= encumbrance;
                company.debit_cash += encumbrance;
                total_encumbered += encumbrance;
                order_book
                    .bids
                    .entry(commodity)
                    .or_insert_with(Vec::new)
                    .push(Bid {
                        buyer_id: company.id.clone(),
                        commodity,
                        quantity: qty,
                        limit_price,
                        blueprint_id: None,
                        min_quality: None,
                    });
            }
        }
    }
    messages
}

/// Settle MaintenanceServices trades: cash leg via `TransferSettler` (strict
/// double-entry), condition restoration applied to buyer's cohorts.
///
/// # Rules
/// * Filters `trades` for `Commodity::MaintenanceServices` only.
/// * Cash leg: uses `credit_company_by_id` (TransferSettler helper) to credit
///   the seller, ensuring bank balance-sheet sync. The buyer's encumbered cash
///   is released (debit_cash -= trade_value).
/// * Physical leg: NONE — MaintenanceServices is a service, consumed on delivery.
///   The buyer's cohort condition is restored proportionally to the quantity
///   bought.
/// * Returns the list of settled trades (for audit/logging).
pub fn settle_maintenance_service_trades(
    trades: &[Trade],
    companies: &mut [Company],
    buildings: &mut [Building],
    gen_config: &crate::economy::generative_goods_config::GenerativeGoodsConfig,
) -> Vec<String> {
    use crate::economy::fixed_assets::restore_cohort_condition;
    use crate::registries::enums::Commodity;

    let mut messages = Vec::new();
    for trade in trades.iter().filter(|t| t.commodity == Commodity::MaintenanceServices) {
        let trade_value = trade.quantity * trade.execution_price;

        // Release buyer's encumbered cash.
        if let Some(buyer) = companies.iter_mut().find(|c| c.id == trade.buyer_id) {
            buyer.debit_cash = (buyer.debit_cash - trade_value).max(0.0);
        }

        // Credit seller via TransferSettler helper (strict double-entry).
        // This syncs the seller's bank balance sheet — unlike the legacy
        // `settle_trades` which directly mutates `available_cash`/`brokerage_account.cash`.
        let credited = crate::economy::credit_company_by_id(companies, &trade.seller_id, trade_value);
        if !credited {
            messages.push(format!(
                "Maintenance service trade: seller {} not found for trade value {}",
                trade.seller_id, trade_value
            ));
        }

        // Restore cohort condition on the buyer's buildings.
        // Find all buildings owned by the buyer with fixed assets and distribute
        // the service quantity proportionally (restore_cohort_condition handles
        // the proportional distribution across cohorts within each building).
        let buyer_buildings: Vec<usize> = buildings
            .iter()
            .enumerate()
            .filter(|(_, b)| b.owner_id == trade.buyer_id && !b.fixed_assets.is_empty())
            .map(|(i, _)| i)
            .collect();

        if buyer_buildings.is_empty() {
            continue;
        }
        // Distribute the service quantity equally across the buyer's buildings
        // that have fixed assets (a simple, deterministic split).
        let per_building = trade.quantity / buyer_buildings.len() as f64;
        for &idx in &buyer_buildings {
            let building = &mut buildings[idx];
            restore_cohort_condition(&mut building.fixed_assets, per_building, gen_config);
        }
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::legal_form::{JointStockData, LegalForm};
    use crate::registries::enums::Sector;
    use crate::state::banking::BankBalanceSheet;
    use crate::state::{Country, Treasury};

    fn make_test_company(id: &str, cash: f64) -> Company {
        Company::new(
            id.to_string(),
            id.to_string(),
            Sector::LightIndustry,
            LegalForm::JointStockCompany(JointStockData::default()),
            100_000.0,
            cash,
            10,
        )
    }

    fn make_test_bank(id: &str, reserves: f64, deposits: f64) -> Company {
        let mut bank = Company::new(
            id.to_string(),
            id.to_string(),
            Sector::Banking,
            LegalForm::JointStockCompany(JointStockData::default()),
            1_000_000.0,
            0.0,
            5,
        );
        bank.balance_sheet = Some(BankBalanceSheet {
            reserves_at_central_bank: reserves,
            deposits,
            ..Default::default()
        });
        bank.bank_type = Some(crate::state::banking::BankType::Commercial);
        bank
    }

    fn make_test_country() -> Country {
        let mut country = Country::default();
        country.budget = Treasury {
            gdp: 1_000_000.0,
            population: 1000,
            nominal_budget: 500_000.0,
            liquid_reserves: 100_000.0,
            ..Default::default()
        };
        country
    }

    fn make_defense_trade(seller_id: &str, quantity: f64, price: f64) -> Trade {
        Trade {
            buyer_id: "MIN-DEF".to_string(),
            seller_id: seller_id.to_string(),
            commodity: Commodity::Ammunition,
            quantity,
            execution_price: price,
            bid_limit_price: price,
            blueprint_id: None,
            quality: None,
        }
    }

    fn make_defense_bid(quantity: f64, price: f64) -> Bid {
        Bid {
            buyer_id: "MIN-DEF".to_string(),
            commodity: Commodity::Ammunition,
            quantity,
            limit_price: price,
            blueprint_id: None,
            min_quality: None,
        }
    }

    // --- settle_defense_trades tests ---

    #[test]
    fn test_settle_defense_trades_credits_seller_and_syncs_bank() {
        let mut companies = vec![
            make_test_company("seller_0", 1_000.0),
            make_test_bank("bank_0", 500_000.0, 400_000.0),
        ];
        companies[0].primary_bank_id = Some("bank_0".to_string());

        let initial_cash = companies[0].brokerage_account.as_ref().unwrap().cash;
        let initial_deposits = companies[1].balance_sheet.as_ref().unwrap().deposits;
        let initial_reserves = companies[1].balance_sheet.as_ref().unwrap().reserves_at_central_bank;

        let trades = vec![make_defense_trade("seller_0", 100.0, 50.0)];
        let mut country = make_test_country();

        settle_defense_trades(&trades, &mut companies, &mut country);

        // Seller cash increased by trade value (100 * 50 = 5000)
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_cash + 5_000.0
        );
        // Bank deposits increased
        assert_eq!(
            companies[1].balance_sheet.as_ref().unwrap().deposits,
            initial_deposits + 5_000.0
        );
        // Bank reserves increased
        assert_eq!(
            companies[1].balance_sheet.as_ref().unwrap().reserves_at_central_bank,
            initial_reserves + 5_000.0
        );
    }

    #[test]
    fn test_settle_defense_trades_skips_non_defense() {
        let mut companies = vec![
            make_test_company("seller_0", 1_000.0),
            make_test_bank("bank_0", 500_000.0, 400_000.0),
        ];
        companies[0].primary_bank_id = Some("bank_0".to_string());

        let initial_cash = companies[0].brokerage_account.as_ref().unwrap().cash;
        let initial_deposits = companies[1].balance_sheet.as_ref().unwrap().deposits;

        let trades = vec![Trade {
            buyer_id: "buyer_0".to_string(),
            seller_id: "seller_0".to_string(),
            commodity: Commodity::Ammunition,
            quantity: 100.0,
            execution_price: 50.0,
            bid_limit_price: 50.0,
            blueprint_id: None,
            quality: None,
        }];
        let mut country = make_test_country();

        settle_defense_trades(&trades, &mut companies, &mut country);

        // Seller cash unchanged (non-defense trade skipped)
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_cash
        );
        // Bank deposits unchanged
        assert_eq!(
            companies[1].balance_sheet.as_ref().unwrap().deposits,
            initial_deposits
        );
    }

    #[test]
    fn test_settle_defense_trades_no_seller_bank() {
        let mut companies = vec![make_test_company("seller_0", 1_000.0)];

        let initial_cash = companies[0].brokerage_account.as_ref().unwrap().cash;

        let trades = vec![make_defense_trade("seller_0", 100.0, 50.0)];
        let mut country = make_test_country();

        settle_defense_trades(&trades, &mut companies, &mut country);

        // Seller cash still increased even without a bank
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_cash + 5_000.0
        );
    }

    #[test]
    fn test_settle_defense_trades_zero_value_skipped() {
        let mut companies = vec![make_test_company("seller_0", 1_000.0)];

        let initial_cash = companies[0].brokerage_account.as_ref().unwrap().cash;

        let trades = vec![make_defense_trade("seller_0", 0.0, 50.0)];
        let mut country = make_test_country();

        settle_defense_trades(&trades, &mut companies, &mut country);

        // No credit for zero-value trade
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_cash
        );
    }

    #[test]
    fn test_settle_defense_trades_seller_not_found() {
        let mut companies = vec![
            make_test_company("seller_0", 1_000.0),
            make_test_bank("bank_0", 500_000.0, 400_000.0),
        ];
        companies[0].primary_bank_id = Some("bank_0".to_string());

        let initial_deposits = companies[1].balance_sheet.as_ref().unwrap().deposits;

        let trades = vec![make_defense_trade("nonexistent_seller", 100.0, 50.0)];
        let mut country = make_test_country();

        settle_defense_trades(&trades, &mut companies, &mut country);

        // Bank unchanged — seller not found
        assert_eq!(
            companies[1].balance_sheet.as_ref().unwrap().deposits,
            initial_deposits
        );
    }

    // --- refund_unfilled_defense_bids_per_country tests ---

    #[test]
    fn test_refund_unfilled_defense_bids_full_refund() {
        let mut country = make_test_country();
        let initial_reserves = country.budget.liquid_reserves;

        let bids = vec![make_defense_bid(100.0, 50.0)];
        let trades: Vec<Trade> = vec![];
        let companies: Vec<Company> = vec![];

        refund_unfilled_defense_bids_per_country(&bids, &trades, &companies, &mut country);

        // Full refund: 100 * 50 = 5000
        assert_eq!(
            country.budget.liquid_reserves,
            initial_reserves + 5_000.0
        );
    }

    #[test]
    fn test_refund_unfilled_defense_bids_partial_fill() {
        let mut country = make_test_country();
        let initial_reserves = country.budget.liquid_reserves;

        let bids = vec![make_defense_bid(100.0, 50.0)];
        let trades = vec![make_defense_trade("seller_0", 60.0, 40.0)];
        let companies = vec![make_test_company("seller_0", 1_000.0)];

        refund_unfilled_defense_bids_per_country(&bids, &trades, &companies, &mut country);

        // Encumbered: 100 * 50 = 5000
        // Filled paid: 60 * 40 = 2400
        // Refund: 5000 - 2400 = 2600
        assert_eq!(
            country.budget.liquid_reserves,
            initial_reserves + 2_600.0
        );
    }

    #[test]
    fn test_refund_unfilled_defense_bids_fully_filled() {
        let mut country = make_test_country();
        let initial_reserves = country.budget.liquid_reserves;

        let bids = vec![make_defense_bid(100.0, 50.0)];
        let trades = vec![make_defense_trade("seller_0", 100.0, 50.0)];
        let companies = vec![make_test_company("seller_0", 1_000.0)];

        refund_unfilled_defense_bids_per_country(&bids, &trades, &companies, &mut country);

        // Fully filled at limit price — no refund
        assert_eq!(
            country.budget.liquid_reserves,
            initial_reserves
        );
    }

    #[test]
    fn test_refund_unfilled_defense_bids_no_bids() {
        let mut country = make_test_country();
        let initial_reserves = country.budget.liquid_reserves;

        let bids: Vec<Bid> = vec![];
        let trades = vec![make_defense_trade("seller_0", 100.0, 50.0)];
        let companies = vec![make_test_company("seller_0", 1_000.0)];

        refund_unfilled_defense_bids_per_country(&bids, &trades, &companies, &mut country);

        // No bids — no refund
        assert_eq!(
            country.budget.liquid_reserves,
            initial_reserves
        );
    }

    #[test]
    fn test_refund_unfilled_defense_bids_seller_not_in_country() {
        let mut country = make_test_country();
        let initial_reserves = country.budget.liquid_reserves;

        let bids = vec![make_defense_bid(100.0, 50.0)];
        let trades = vec![make_defense_trade("foreign_seller", 100.0, 50.0)];
        let companies: Vec<Company> = vec![]; // seller not in this country

        refund_unfilled_defense_bids_per_country(&bids, &trades, &companies, &mut country);

        // Seller not found in this country's companies — full refund
        assert_eq!(
            country.budget.liquid_reserves,
            initial_reserves + 5_000.0
        );
    }

    // --- settle_trades defense skip test ---

    #[test]
    fn test_settle_trades_skips_defense_cash_credit() {
        let mut companies = vec![
            make_test_company("seller_0", 1_000.0),
            make_test_bank("bank_0", 500_000.0, 400_000.0),
        ];
        companies[0].primary_bank_id = Some("bank_0".to_string());

        let initial_cash = companies[0].brokerage_account.as_ref().unwrap().cash;
        let initial_available = companies[0].available_cash;

        let trades = vec![make_defense_trade("seller_0", 100.0, 50.0)];
        let mut buildings: Vec<Building> = vec![];

        settle_trades(&trades, &mut companies, &mut buildings);

        // Seller cash NOT credited by settle_trades for defense trades
        assert_eq!(
            companies[0].brokerage_account.as_ref().unwrap().cash,
            initial_cash
        );
        assert_eq!(companies[0].available_cash, initial_available);
    }
}

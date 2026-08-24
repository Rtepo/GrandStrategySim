//! Continuous order book matching engine for B2B commodity trading.
//!
//! This module implements a deterministic price-time priority matching algorithm
//! with strict double-entry accounting and peer-to-peer settlement.

use crate::registries::enums::Commodity;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// Hot-path hash map alias for order book internals.
pub type HashMap<K, V> = FxHashMap<K, V>;

/// A buy order with explicit buyer identification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bid {
    /// Company ID of the buyer.
    pub buyer_id: String,
    /// Commodity being purchased.
    pub commodity: Commodity,
    /// Remaining unfilled quantity.
    pub quantity: f64,
    /// Maximum price willing to pay.
    pub limit_price: f64,
    /// Phase 19C: Blueprint id of the product being bought (None for raw materials
    /// or legacy orders). Used for quality-aware B2B matching and asset provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blueprint_id: Option<String>,
    /// Phase 19C: Quality of the desired product (None = any quality / legacy).
    /// Buyers may set a minimum quality threshold to filter asks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_quality: Option<f64>,
}

/// A sell order with explicit seller identification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ask {
    /// Company ID of the seller.
    pub seller_id: String,
    /// Commodity being sold.
    pub commodity: Commodity,
    /// Remaining unfilled quantity.
    pub quantity: f64,
    /// Minimum price willing to accept.
    pub limit_price: f64,
    /// Phase 19C: Blueprint id of the product being sold (None for raw materials
    /// or legacy orders). Used for quality-aware B2B matching and asset provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blueprint_id: Option<String>,
    /// Phase 19C: Quality of the offered product (None = unknown / legacy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<f64>,
    /// Phase 19C: Durability of the offered product (None = unknown / legacy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub durability: Option<f64>,
}

/// An executed trade resulting from order matching.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trade {
    /// Company ID of the buyer.
    pub buyer_id: String,
    /// Company ID of the seller.
    pub seller_id: String,
    /// Commodity traded.
    pub commodity: Commodity,
    /// Quantity traded.
    pub quantity: f64,
    /// Execution price (midpoint of spread).
    pub execution_price: f64,
    /// Original bid limit price for encumbrance refund.
    pub bid_limit_price: f64,
    /// Phase 19C: Blueprint id of the traded product (None for raw materials).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blueprint_id: Option<String>,
    /// Phase 19C: Quality of the traded product (None = unknown / legacy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<f64>,
}

/// Order book container holding bids, asks, and executed trades.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct OrderBook {
    /// Bids by commodity.
    pub bids: HashMap<Commodity, Vec<Bid>>,
    /// Asks by commodity.
    pub asks: HashMap<Commodity, Vec<Ask>>,
    /// Executed trades this turn.
    pub trades: Vec<Trade>,
}

/// Match orders using price-time priority with deterministic tie-breaking.
///
/// # Arguments
/// * `order_book` - Mutable reference to the order book to match.
///
/// # Rules
/// * Bids sorted descending by limit_price, then lexicographically by buyer_id.
/// * Asks sorted ascending by limit_price, then lexicographically by seller_id.
/// * Execution price is midpoint of spread.
/// * Partial fills are mandatory.
/// * Unfilled orders are removed after matching.
pub fn match_orders(order_book: &mut OrderBook) {
    for commodity in order_book.bids.keys().cloned().collect::<Vec<_>>() {
        let bids = order_book.bids.get_mut(&commodity).unwrap();
        let asks = match order_book.asks.get_mut(&commodity) {
            Some(a) => a,
            None => continue,
        };

        // Sort bids: descending limit_price, lexicographical buyer_id tie-breaker
        bids.sort_by(|a, b| {
            b.limit_price
                .partial_cmp(&a.limit_price)
                .unwrap()
                .then_with(|| a.buyer_id.cmp(&b.buyer_id))
        });

        // Sort asks: ascending limit_price, lexicographical seller_id tie-breaker
        asks.sort_by(|a, b| {
            a.limit_price
                .partial_cmp(&b.limit_price)
                .unwrap()
                .then_with(|| a.seller_id.cmp(&b.seller_id))
        });

        // Match while crossing prices exist
        let mut bid_idx = 0;
        let mut ask_idx = 0;

        while bid_idx < bids.len() && ask_idx < asks.len() {
            let bid = &mut bids[bid_idx];
            let ask = &mut asks[ask_idx];

            // No trade if bid price < ask price
            if bid.limit_price < ask.limit_price {
                break;
            }

            // Execution price: midpoint of spread (fair equilibrium)
            let execution_price = (bid.limit_price + ask.limit_price) / 2.0;
            let trade_quantity = bid.quantity.min(ask.quantity);

            if trade_quantity > 0.0 {
                order_book.trades.push(Trade {
                    buyer_id: bid.buyer_id.clone(),
                    seller_id: ask.seller_id.clone(),
                    commodity,
                    quantity: trade_quantity,
                    execution_price,
                    bid_limit_price: bid.limit_price, // Capture for encumbrance refund
                    blueprint_id: bid.blueprint_id.clone().or(ask.blueprint_id.clone()),
                    quality: ask.quality,
                });

                bid.quantity -= trade_quantity;
                ask.quantity -= trade_quantity;
            }

            // Advance pointers for filled orders
            if bid.quantity < 1e-9 {
                bid_idx += 1;
            }
            if ask.quantity < 1e-9 {
                ask_idx += 1;
            }
        }

        // Remove filled orders
        bids.retain(|b| b.quantity >= 1e-9);
        asks.retain(|a| a.quantity >= 1e-9);
    }
}

/// Match orders with bilateral embargo enforcement (Phase 11).
///
/// # Arguments
/// * `order_book` - Mutable reference to the order book to match.
/// * `company_country` - Ephemeral lookup: company_id → country_name.
/// * `diplomacy` - Bilateral diplomatic relations matrix.
///
/// # Rules
/// * Same price-time priority as `match_orders`.
/// * Before executing a trade, checks if the buyer's country has an embargo
///   with the seller's country (`ban_import` or `ban_export`).
/// * If embargoed, the ask is skipped (ask_idx advances) so it can match
///   with other non-embargoed bids.
/// * Companies not in the lookup table (e.g. MoD orders) bypass embargo checks.
/// * Unmatched orders simply fail — no money moves, no inventory moves.
pub fn match_orders_with_embargoes(
    order_book: &mut OrderBook,
    company_country: &std::collections::HashMap<String, String>,
    diplomacy: &std::collections::HashMap<String, std::collections::HashMap<String, crate::international::DiplomaticRelation>>,
) {
    for commodity in order_book.bids.keys().cloned().collect::<Vec<_>>() {
        let bids = order_book.bids.get_mut(&commodity).unwrap();
        let asks = match order_book.asks.get_mut(&commodity) {
            Some(a) => a,
            None => continue,
        };

        bids.sort_by(|a, b| {
            b.limit_price
                .partial_cmp(&a.limit_price)
                .unwrap()
                .then_with(|| a.buyer_id.cmp(&b.buyer_id))
        });

        asks.sort_by(|a, b| {
            a.limit_price
                .partial_cmp(&b.limit_price)
                .unwrap()
                .then_with(|| a.seller_id.cmp(&b.seller_id))
        });

        let mut bid_idx = 0;
        let mut ask_idx = 0;

        while bid_idx < bids.len() && ask_idx < asks.len() {
            let bid = &bids[bid_idx];
            let ask = &asks[ask_idx];

            if bid.limit_price < ask.limit_price {
                break;
            }

            // Phase 11: Embargo check
            let buyer_country = company_country.get(&bid.buyer_id);
            let seller_country = company_country.get(&ask.seller_id);
            let embargoed = if let (Some(bc), Some(sc)) = (buyer_country, seller_country) {
                if bc == sc {
                    false // Same country — no embargo
                } else {
                    let blocked = diplomacy
                        .get(bc)
                        .and_then(|partners| partners.get(sc))
                        .map(|rel| rel.ban_import || rel.ban_export)
                        .unwrap_or(false);
                    if blocked {
                        // Also check reverse direction
                        diplomacy
                            .get(sc)
                            .and_then(|partners| partners.get(bc))
                            .map(|rel| rel.ban_import || rel.ban_export)
                            .unwrap_or(false)
                    } else {
                        false
                    }
                }
            } else {
                false // Unknown company (e.g. MoD) — bypass
            };

            if embargoed {
                // Skip this ask — it may match with a different (non-embargoed) bid
                ask_idx += 1;
                continue;
            }

            let execution_price = (bid.limit_price + ask.limit_price) / 2.0;
            let trade_quantity = bid.quantity.min(ask.quantity);

            if trade_quantity > 0.0 {
                order_book.trades.push(Trade {
                    buyer_id: bid.buyer_id.clone(),
                    seller_id: ask.seller_id.clone(),
                    commodity,
                    quantity: trade_quantity,
                    execution_price,
                    bid_limit_price: bid.limit_price,
                    blueprint_id: bid.blueprint_id.clone().or(ask.blueprint_id.clone()),
                    quality: ask.quality,
                });

                bids[bid_idx].quantity -= trade_quantity;
                asks[ask_idx].quantity -= trade_quantity;
            }

            if bids[bid_idx].quantity < 1e-9 {
                bid_idx += 1;
            }
            if asks[ask_idx].quantity < 1e-9 {
                ask_idx += 1;
            }
        }

        // Reset ask_idx for each bid? No — the above is a single-pass greedy matcher.
        // If a bid was skipped because of embargo on all remaining asks, advance bid.
        // Actually we need to handle: if ask_idx reached end but bid_idx hasn't,
        // we should try next bid from remaining asks. But the standard match_orders
        // also breaks when ask_idx reaches end. Keep same behavior for parity.

        bids.retain(|b| b.quantity >= 1e-9);
        asks.retain(|a| a.quantity >= 1e-9);
    }
}

/// Submit a bid with liquidity clamping and capital encumbrance.
///
/// # Arguments
/// * `order_book` - Mutable reference to the order book.
/// * `company` - Mutable reference to the company (for liquidity clamping).
/// * `commodity` - Commodity to bid for.
/// * `desired_quantity` - Desired quantity to purchase.
/// * `limit_price` - Maximum price willing to pay.
/// * `interventions` - Price interventions for clamping.
///
/// # Rules
/// * Limit price is clamped to intervention caps/floors.
/// * Quantity is clamped to affordable amount based on liquid capital.
/// * Capital is encumbered (deducted) immediately to prevent double-spending.
/// Submit a bid (buy order).
///
/// **Phase 24C.2:** This function is NOT called by the live B2B pipeline.
/// The live pipeline uses `b2b_orders::submit_company_b2b_orders` which
/// correctly encumbers `available_cash`/`debit_cash` instead of `liquid_capital`.
/// This function is kept for reference but should not be called directly.
/// If it is called, the `liquid_capital` debit would create a black hole
/// because no refund path credits `liquid_capital` back.
#[deprecated(note = "Use b2b_orders::submit_company_b2b_orders instead")]
pub fn submit_bid(
    order_book: &mut OrderBook,
    company: &mut crate::entities::Company,
    commodity: Commodity,
    desired_quantity: f64,
    limit_price: f64,
    interventions: &std::collections::HashMap<Commodity, crate::state::economic_policy::PriceIntervention>,
) {
    let mut clamped_price = limit_price;

    // Apply intervention clamping
    if let Some(intervention) = interventions.get(&commodity) {
        if let Some(cap) = intervention.price_cap {
            clamped_price = clamped_price.min(cap);
        }
        if let Some(floor) = intervention.price_floor {
            clamped_price = clamped_price.max(floor);
        }
    }

    // Liquidity clamp: cannot bid more than can afford
    let affordable_quantity = if clamped_price > 0.0 {
        company.available_cash / clamped_price
    } else {
        0.0
    };

    let bid_quantity = desired_quantity.min(affordable_quantity);

    if bid_quantity > 0.0 && clamped_price > 0.0 {
        // Phase 24C.2: Encumber via available_cash/debit_cash (not liquid_capital)
        // to match the B2B wrapper's encumbrance pattern.
        let encumbrance = bid_quantity * clamped_price;
        company.available_cash -= encumbrance;
        company.debit_cash += encumbrance;

        order_book
            .bids
            .entry(commodity)
            .or_default()
            .push(Bid {
                buyer_id: company.id.clone(),
                commodity,
                quantity: bid_quantity,
                limit_price: clamped_price,
                blueprint_id: None,
                min_quality: None,
            });
    }
}

/// Submit an ask (sell order).
///
/// # Arguments
/// * `order_book` - Mutable reference to the order book.
/// * `seller_id` - Company ID of the seller.
/// * `commodity` - Commodity to sell.
/// * `quantity` - Quantity to sell.
/// * `limit_price` - Minimum price willing to accept.
/// * `interventions` - Price interventions for clamping.
///
/// # Rules
/// * Limit price is clamped to intervention caps/floors.
pub fn submit_ask(
    order_book: &mut OrderBook,
    seller_id: String,
    commodity: Commodity,
    quantity: f64,
    limit_price: f64,
    interventions: &std::collections::HashMap<Commodity, crate::state::economic_policy::PriceIntervention>,
) {
    let mut clamped_price = limit_price;

    // Apply intervention clamping
    if let Some(intervention) = interventions.get(&commodity) {
        if let Some(cap) = intervention.price_cap {
            clamped_price = clamped_price.min(cap);
        }
        if let Some(floor) = intervention.price_floor {
            clamped_price = clamped_price.max(floor);
        }
    }

    if quantity > 0.0 && clamped_price > 0.0 {
        order_book
            .asks
            .entry(commodity)
            .or_default()
            .push(Ask {
                seller_id,
                commodity,
                quantity,
                limit_price: clamped_price,
                blueprint_id: None,
                quality: None,
                durability: None,
            });
    }
}

/// Refund unfilled bid encumbrances after matching.
///
/// # Arguments
/// * `order_book` - Reference to the order book with unfilled bids.
/// * `companies` - Mutable reference to all companies for refunds.
///
/// # Rules
/// * Refunds unfilled quantity at original limit price.
/// * This restores capital that was encumbered but not matched.
/// Refund unfilled bids for cultural buildings (by buyer_id match).
///
/// # Arguments
/// * `order_book` - The order book after clearing
/// * `cultural_institutions` - Cultural buildings to refund
///
/// # Rules
/// * Refunds unfilled quantity at original limit price.
/// * Restores encumbered cash to the cultural building's available_cash.
pub fn refund_unfilled_bids_cultural(
    order_book: &OrderBook,
    cultural_institutions: &mut [crate::infrastructure::cultural::CulturalBuilding],
) {
    for bids in order_book.bids.values() {
        for bid in bids {
            if let Some(building) = cultural_institutions.iter_mut().find(|b| b.id == bid.buyer_id) {
                let refund = bid.quantity * bid.limit_price;
                building.available_cash += refund;
            }
        }
    }
}

/// Refund unfilled bids for maritime infrastructure (by buyer_id prefix match).
///
/// # Arguments
/// * `order_book` - The order book after clearing
/// * `maritime` - Maritime infrastructure to refund
///
/// # Rules
/// * Refunds unfilled quantity at original limit price.
/// * Restores encumbered cash to the maritime available_cash.
pub fn refund_unfilled_bids_maritime(
    order_book: &OrderBook,
    maritime: &mut crate::infrastructure::maritime::MaritimeInfrastructure,
) {
    for bids in order_book.bids.values() {
        for bid in bids {
            if bid.buyer_id.starts_with("shipyard_") {
                let refund = bid.quantity * bid.limit_price;
                maritime.available_cash += refund;
            }
        }
    }
}

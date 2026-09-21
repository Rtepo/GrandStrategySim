//! B2B Matched-Volume Diagnostic — Epic Test (Investigation 1)
//!
//! Instruments `submit_company_b2b_orders` to answer WHY B2B matched trade
//! volume is anemic (~650-5000 units vs ~66K+ per-country input needs, ~17%
//! fulfillment). Probes four candidate clamps:
//!
//! (a) Encumbrance ceiling: is `affordable_qty = remaining / total_per_unit`
//!     clamping bids to near-zero because `available_cash` is depleted?
//! (b) Math error in bid sizing (per-unit vs total encumbrance, batch logic)?
//! (c) `match_orders_with_embargoes` fill logic — min(bid,ask) or more
//!     restrictive (price-time priority, embargo filtering stranding bids)?
//! (d) Per-commodity structural under-trading (Steel, raw mats)?
//!
//! # Approach
//! Generates a world, runs 1 turn to establish market history (VWAP), then
//! directly calls `submit_company_b2b_orders` on the post-turn-1 state for
//! one country, snapshots the order book (bid qty / ask qty / limit prices
//! per commodity), runs `match_orders_with_embargoes`, and measures matched
//! volume per commodity. Also records per-company `liquid`, `max_encumber`,
//! and how many bids hit the encumbrance ceiling vs full-quantity bids.
//!
//! # Output
//! - `state/tests/diagnostic_output/b2b_volume_diagnostic.json`
//!
//! # Run
//! ```bash
//! CI=true cargo nextest run --release --features epic-tests,diagnostic \
//!   -E "test(test_b2b_volume_clamps)"
//! ```

#![cfg(feature = "diagnostic")]

use sim_engine::economy::b2b_orders::submit_company_b2b_orders;
use sim_engine::economy::market::order_book::{
    match_orders_with_embargoes, OrderBook,
};
use sim_engine::engine::turn::run_turn_inner;
use sim_engine::engine::turn_context::InMemoryTurnContext;
use sim_engine::engine::{generate_world, GenerateOptions, GeneratedWorld, StartYear};
use sim_engine::engine::diagnostic::{
    CapturingProbe, MassSinkWhitelist, HarnessTargets,
};
use sim_engine::registries::enums::Commodity;
use sim_engine::registries::Registries;
use std::collections::BTreeMap;
use std::path::PathBuf;
use tempfile::TempDir;

const OUTPUT_DIR: &str = "tests/diagnostic_output";

/// Per-commodity order-book snapshot (pre-match).
#[derive(Debug, Clone, serde::Serialize)]
struct CommodityBookSnapshot {
    commodity: String,
    bid_count: usize,
    total_bid_qty: f64,
    total_ask_qty: f64,
    /// Sum of (bid_qty * bid_limit_price) — total cash demanded by bids.
    total_bid_value: f64,
    /// Sum of (ask_qty * ask_limit_price).
    total_ask_value: f64,
    min_bid_price: f64,
    max_bid_price: f64,
    min_ask_price: f64,
    max_ask_price: f64,
    /// Best bid (highest) vs best ask (lowest) — does the spread cross?
    spread_crosses: bool,
    matched_qty: f64,
    matched_value: f64,
}

/// Per-company bid-sizing summary.
#[derive(Debug, Clone, serde::Serialize)]
struct CompanyBidSummary {
    company_id: String,
    sector: String,
    liquid: f64,
    max_encumber: f64,
    available_cash_pre: f64,
    available_cash_post: f64,
    debit_cash_post: f64,
    bid_count: usize,
    /// Sum of desired_qty across all this company's bids (what they WANTED).
    total_desired_qty: f64,
    /// Sum of actual submitted bid quantities (what they GOT to submit).
    total_submitted_qty: f64,
    /// True if at least one bid was clamped by the encumbrance ceiling.
    hit_encumbrance_ceiling: bool,
    /// Fulfilled FTE (employment) — 0 means furloughed/no production.
    fulfilled_fte: u32,
    /// Furloughed worker count.
    furloughed_workers_count: f64,
    /// Number of buildings owned.
    building_count: usize,
    /// Total inventory across all owned buildings (sum of all commodity qtys).
    total_inventory: f64,
    /// Output commodities this company's buildings produce (for supply-side analysis).
    output_commodities: Vec<String>,
    /// Ask count this company submitted (sell orders).
    ask_count: usize,
    /// Total ask quantity this company submitted.
    total_ask_qty: f64,
}

/// Aggregate diagnostic dump.
#[derive(Debug, Clone, serde::Serialize)]
struct B2bVolumeDiagnostic {
    dump_version: String,
    country: String,
    turn_sampled: u32,
    b2b_config: B2bOrderConfigSummary,
    companies: Vec<CompanyBidSummary>,
    order_book: Vec<CommodityBookSnapshot>,
    /// Total across all commodities.
    total_bid_qty: f64,
    total_ask_qty: f64,
    total_matched_qty: f64,
    total_matched_value: f64,
    /// Fulfillment ratio = matched / bid (demand-side).
    bid_fulfillment_ratio: f64,
    /// Fulfillment ratio = matched / ask (supply-side).
    ask_fulfillment_ratio: f64,
    /// Which clamp dominates (encumbrance / spread-no-cross / embargo / supply-shortage).
    dominant_clamp: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct B2bOrderConfigSummary {
    max_cash_encumbrance_ratio: f64,
    buy_premium_ratio: f64,
    freight_cost_reserve_ratio: f64,
    min_markup_ratio: f64,
    max_markup_ratio: f64,
}

/// Main diagnostic: generate world, run 1 turn, instrument B2B order submission.
#[test]
fn test_b2b_volume_clamps() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let data_dir = tmp.path();

    let registries = Registries::native_only();
    let options = GenerateOptions {
        country_count: 4,
        start_year: StartYear::Y1925,
        seed: Some(42),
    };

    let GeneratedWorld { state: mut initial_state, .. } =
        generate_world(data_dir, options, &registries).expect("world generation failed");

    let mut ctx = InMemoryTurnContext::load_from_disk(data_dir, &mut initial_state)
        .expect("failed to load turn context");

    let mut state = initial_state;

    // Run 1 turn to establish market history (VWAP / reference prices) so the
    // bootstrap path (turn 0) doesn't skew the bid-sizing analysis.
    let whitelist = MassSinkWhitelist::canonical();
    let targets = HarnessTargets {
        company_ids: vec![],
        bank_id: String::new(),
        region_id: String::new(),
        country_name: String::new(),
    };
    let mut probe = CapturingProbe::new(targets, whitelist);
    run_turn_inner(&mut state, &registries, &mut ctx, &mut probe)
        .expect("turn 0 failed");
    let turn_sampled = state.calendar.global_turn;

    // Pick the alphabetically-first country that has entities.
    let country = ctx.entities.keys().min().cloned().expect("no countries");
    let ents = ctx.entities.get_mut(&country).expect("country entities");
    let b2b_config = state
        .countries
        .get(&country)
        .map(|c| c.b2b_order_config.clone())
        .unwrap_or_default();
    let gen_cfg = state
        .countries
        .get(&country)
        .map(|c| c.generative_goods_config.clone())
        .unwrap_or_default();

    // Snapshot pre-submission available_cash per company for delta analysis.
    let pre_cash: std::collections::HashMap<String, f64> = ents
        .companies
        .iter()
        .map(|c| (c.id.clone(), c.available_cash))
        .collect();

    // Build owner_to_building_indices (same logic as turn.rs:rebuild_...).
    let mut owner_to_building_indices: rustc_hash::FxHashMap<String, Vec<usize>> =
        rustc_hash::FxHashMap::default();
    for (i, b) in ents.buildings.iter().enumerate() {
        owner_to_building_indices
            .entry(b.owner_id.clone())
            .or_default()
            .push(i);
    }

    // We need a fresh order book. The live turn resets task.order_book each
    // turn; here we build a clean one for instrumentation.
    let mut order_book = OrderBook::default();
    let buildings_snapshot: Vec<_> = ents.buildings.clone();

    // submit_company_b2b_orders needs &mut [Company], &[Building], &mut OrderBook,
    // &MarketHistory, &B2bOrderConfig, &GenerativeGoodsConfig, &owner_to_buildings.
    let market_history = &state.market_history;
    let messages = submit_company_b2b_orders(
        &mut ents.companies,
        &buildings_snapshot,
        &mut order_book,
        market_history,
        &b2b_config,
        &gen_cfg,
        &owner_to_building_indices,
    );
    let _ = messages;

    // --- Snapshot pre-match order book per commodity ---
    let mut pre_match: BTreeMap<String, CommodityBookSnapshot> = BTreeMap::new();
    for (commodity, bids) in &order_book.bids {
        let name = format!("{:?}", commodity);
        let total_bid_qty: f64 = bids.iter().map(|b| b.quantity).sum();
        let total_bid_value: f64 = bids.iter().map(|b| b.quantity * b.limit_price).sum();
        let min_bid = bids.iter().map(|b| b.limit_price).fold(f64::INFINITY, f64::min);
        let max_bid = bids.iter().map(|b| b.limit_price).fold(0.0_f64, f64::max);
        let entry = pre_match.entry(name).or_insert(CommodityBookSnapshot {
            commodity: format!("{:?}", commodity),
            bid_count: 0,
            total_bid_qty: 0.0,
            total_ask_qty: 0.0,
            total_bid_value: 0.0,
            total_ask_value: 0.0,
            min_bid_price: 0.0,
            max_bid_price: 0.0,
            min_ask_price: f64::INFINITY,
            max_ask_price: 0.0,
            spread_crosses: false,
            matched_qty: 0.0,
            matched_value: 0.0,
        });
        entry.bid_count = bids.len();
        entry.total_bid_qty = total_bid_qty;
        entry.total_bid_value = total_bid_value;
        entry.min_bid_price = min_bid;
        entry.max_bid_price = max_bid;
    }
    for (commodity, asks) in &order_book.asks {
        let name = format!("{:?}", commodity);
        let total_ask_qty: f64 = asks.iter().map(|a| a.quantity).sum();
        let total_ask_value: f64 = asks.iter().map(|a| a.quantity * a.limit_price).sum();
        let min_ask = asks.iter().map(|a| a.limit_price).fold(f64::INFINITY, f64::min);
        let max_ask = asks.iter().map(|a| a.limit_price).fold(0.0_f64, f64::max);
        let entry = pre_match.entry(name).or_insert(CommodityBookSnapshot {
            commodity: format!("{:?}", commodity),
            bid_count: 0,
            total_bid_qty: 0.0,
            total_ask_qty: 0.0,
            total_bid_value: 0.0,
            total_ask_value: 0.0,
            min_bid_price: 0.0,
            max_bid_price: 0.0,
            min_ask_price: f64::INFINITY,
            max_ask_price: 0.0,
            spread_crosses: false,
            matched_qty: 0.0,
            matched_value: 0.0,
        });
        entry.total_ask_qty = total_ask_qty;
        entry.total_ask_value = total_ask_value;
        entry.min_ask_price = min_ask;
        entry.max_ask_price = max_ask;
    }

    // --- Run matching (embargo-aware) ---
    // Build company_country + diplomacy lookups (empty embargoes for a single
    // country's internal trades; cross-country trades won't exist here since we
    // only submitted one country's orders).
    let company_country: std::collections::HashMap<String, String> = ents
        .companies
        .iter()
        .map(|c| (c.id.clone(), country.clone()))
        .collect();
    let diplomacy: std::collections::HashMap<
        String,
        std::collections::HashMap<String, sim_engine::international::DiplomaticRelation>,
    > = std::collections::HashMap::new();
    match_orders_with_embargoes(&mut order_book, &company_country, &diplomacy);

    // --- Record matched volume per commodity ---
    let mut matched_by_commodity: std::collections::HashMap<Commodity, (f64, f64)> =
        std::collections::HashMap::new();
    for trade in &order_book.trades {
        let (q, v) = matched_by_commodity
            .entry(trade.commodity)
            .or_insert((0.0, 0.0));
        *q += trade.quantity;
        *v += trade.quantity * trade.execution_price;
    }
    for (commodity, (qty, val)) in &matched_by_commodity {
        let name = format!("{:?}", commodity);
        if let Some(entry) = pre_match.get_mut(&name) {
            entry.matched_qty = *qty;
            entry.matched_value = *val;
        }
    }

    // Finalize spread_crosses + sanitize infinities.
    let mut book_snapshots: Vec<CommodityBookSnapshot> = pre_match.into_values().collect();
    for s in &mut book_snapshots {
        s.spread_crosses = s.max_bid_price >= s.min_ask_price && s.min_ask_price.is_finite();
        if !s.min_ask_price.is_finite() {
            s.min_ask_price = 0.0;
        }
        if !s.min_bid_price.is_finite() {
            s.min_bid_price = 0.0;
        }
    }
    book_snapshots.sort_by(|a, b| b.total_bid_qty.partial_cmp(&a.total_bid_qty).unwrap());

    // --- Per-company bid-sizing summary ---
    // We approximate "desired vs submitted" by comparing each company's bid
    // quantities in the order book to the BOM-implied demand. Since we can't
    // recompute desired_qty without re-running the inner loop, we record what
    // was actually submitted and the cash deltas.
    let mut company_summaries: Vec<CompanyBidSummary> = Vec::new();
    for c in &ents.companies {
        let pre = pre_cash.get(&c.id).copied().unwrap_or(0.0);
        let liquid = c
            .brokerage_account
            .as_ref()
            .map(|ba| ba.cash.max(0.0))
            .unwrap_or(c.available_cash.max(0.0));
        let max_encumber = liquid * b2b_config.max_cash_encumbrance_ratio;
        // Count this company's bids across all commodities.
        let mut bid_count = 0usize;
        let mut total_submitted_qty = 0.0;
        for bids in order_book.bids.values() {
            for b in bids {
                if b.buyer_id == c.id {
                    bid_count += 1;
                    total_submitted_qty += b.quantity;
                }
            }
        }
        // Count this company's asks across all commodities (supply side).
        let mut ask_count = 0usize;
        let mut total_ask_qty = 0.0;
        for asks in order_book.asks.values() {
            for a in asks {
                if a.seller_id == c.id {
                    ask_count += 1;
                    total_ask_qty += a.quantity;
                }
            }
        }
        // hit_encumbrance_ceiling heuristic: debit_cash increased by a non-trivial
        // amount AND available_cash dropped significantly relative to pre.
        let cash_drop = (pre - c.available_cash).max(0.0);
        let hit_ceiling = cash_drop > 0.0 && c.debit_cash > 0.0 && total_submitted_qty > 0.0;
        // Buildings owned + outputs + inventory.
        let owned_indices = owner_to_building_indices.get(&c.id);
        let building_count = owned_indices.map(|v| v.len()).unwrap_or(0);
        let mut total_inventory = 0.0_f64;
        let mut output_commodities: Vec<String> = Vec::new();
        let mut seen_out: std::collections::HashSet<String> = std::collections::HashSet::new();
        if let Some(indices) = owned_indices {
            for &bi in indices {
                let b = &ents.buildings[bi];
                for (&commodity, _) in &b.active_method.outputs {
                    let name = format!("{:?}", commodity);
                    if seen_out.insert(name.clone()) {
                        output_commodities.push(name);
                    }
                }
                for (_commodity, qty) in b.inventory.iter() {
                    total_inventory += *qty;
                }
            }
        }
        company_summaries.push(CompanyBidSummary {
            company_id: c.id.clone(),
            sector: format!("{:?}", c.sector),
            liquid,
            max_encumber,
            available_cash_pre: pre,
            available_cash_post: c.available_cash,
            debit_cash_post: c.debit_cash,
            bid_count,
            total_desired_qty: 0.0, // filled below if reconstructible; 0 = N/A
            total_submitted_qty,
            hit_encumbrance_ceiling: hit_ceiling,
            fulfilled_fte: c.fulfilled_fte,
            furloughed_workers_count: c.furloughed_workers_count,
            building_count,
            total_inventory,
            output_commodities,
            ask_count,
            total_ask_qty,
        });
    }
    company_summaries.sort_by(|a, b| {
        b.debit_cash_post
            .partial_cmp(&a.debit_cash_post)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // --- Aggregates ---
    let total_bid_qty: f64 = book_snapshots.iter().map(|s| s.total_bid_qty).sum();
    let total_ask_qty: f64 = book_snapshots.iter().map(|s| s.total_ask_qty).sum();
    let total_matched_qty: f64 = book_snapshots.iter().map(|s| s.matched_qty).sum();
    let total_matched_value: f64 = book_snapshots.iter().map(|s| s.matched_value).sum();
    let bid_fulfillment_ratio = if total_bid_qty > 0.0 {
        total_matched_qty / total_bid_qty
    } else {
        0.0
    };
    let ask_fulfillment_ratio = if total_ask_qty > 0.0 {
        total_matched_qty / total_ask_qty
    } else {
        0.0
    };

    // --- Determine dominant clamp ---
    let commodities_with_bids_no_asks = book_snapshots
        .iter()
        .filter(|s| s.total_bid_qty > 0.0 && s.total_ask_qty <= 0.0)
        .count();
    let commodities_with_bids_asks_no_cross = book_snapshots
        .iter()
        .filter(|s| {
            s.total_bid_qty > 0.0 && s.total_ask_qty > 0.0 && !s.spread_crosses
        })
        .count();
    let commodities_with_cross_partial_fill = book_snapshots
        .iter()
        .filter(|s| s.spread_crosses && s.matched_qty < s.total_bid_qty.min(s.total_ask_qty) - 1e-6)
        .count();
    let companies_with_low_liquid = company_summaries
        .iter()
        .filter(|c| c.liquid < 1000.0)
        .count();
    let companies_with_zero_bids = company_summaries
        .iter()
        .filter(|c| c.bid_count == 0)
        .count();

    let dominant_clamp = if total_bid_qty <= 0.0 {
        "NO_BIDS_SUBMITTED".to_string()
    } else if total_ask_qty <= 0.0 {
        "NO_ASKS_SUBMITTED".to_string()
    } else if commodities_with_bids_no_asks
        > commodities_with_bids_asks_no_cross + commodities_with_cross_partial_fill
    {
        format!(
            "SUPPLY_SHORTAGE ({} commodities have bids but zero asks)",
            commodities_with_bids_no_asks
        )
    } else if commodities_with_bids_asks_no_cross > 0
        && bid_fulfillment_ratio < 0.5
    {
        format!(
            "SPREAD_NO_CROSS ({} commodities: bids+asks exist but spread doesn't cross)",
            commodities_with_bids_asks_no_cross
        )
    } else if companies_with_low_liquid > company_summaries.len() / 2
        && bid_fulfillment_ratio < 0.5
    {
        format!(
            "ENCUMBRANCE_CEILING ({} of {} companies have liquid < 1000)",
            companies_with_low_liquid,
            company_summaries.len()
        )
    } else if bid_fulfillment_ratio < 0.5 && commodities_with_cross_partial_fill > 0 {
        format!(
            "PARTIAL_FILL_RESTRICTED ({} crossing commodities under-filled)",
            commodities_with_cross_partial_fill
        )
    } else {
        "NONE / MIXED".to_string()
    };

    let diagnostic = B2bVolumeDiagnostic {
        dump_version: "b2b-vol-v1".to_string(),
        country: country.clone(),
        turn_sampled,
        b2b_config: B2bOrderConfigSummary {
            max_cash_encumbrance_ratio: b2b_config.max_cash_encumbrance_ratio,
            buy_premium_ratio: b2b_config.buy_premium_ratio,
            freight_cost_reserve_ratio: b2b_config.freight_cost_reserve_ratio,
            min_markup_ratio: b2b_config.min_markup_ratio,
            max_markup_ratio: b2b_config.max_markup_ratio,
        },
        companies: company_summaries,
        order_book: book_snapshots,
        total_bid_qty,
        total_ask_qty,
        total_matched_qty,
        total_matched_value,
        bid_fulfillment_ratio,
        ask_fulfillment_ratio,
        dominant_clamp,
    };

    // --- Write dump ---
    let output_dir = PathBuf::from(OUTPUT_DIR);
    std::fs::create_dir_all(&output_dir).expect("failed to create output dir");
    let dump_path = output_dir.join("b2b_volume_diagnostic.json");
    let dump_json = serde_json::to_string_pretty(&diagnostic)
        .expect("failed to serialize diagnostic dump");
    std::fs::write(&dump_path, dump_json).expect("failed to write diagnostic dump");

    // --- Print high-visibility summary ---
    eprintln!();
    eprintln!("═══════════════════════════════════════════════════════════════");
    eprintln!("  B2B VOLUME DIAGNOSTIC — country={} turn={}", country, turn_sampled);
    eprintln!("═══════════════════════════════════════════════════════════════");
    eprintln!("  Total bid qty:        {:.0}", total_bid_qty);
    eprintln!("  Total ask qty:        {:.0}", total_ask_qty);
    eprintln!("  Total matched qty:    {:.0}", total_matched_qty);
    eprintln!("  Total matched value:  {:.0}", total_matched_value);
    eprintln!("  Bid fulfillment:      {:.1}%", bid_fulfillment_ratio * 100.0);
    eprintln!("  Ask fulfillment:      {:.1}%", ask_fulfillment_ratio * 100.0);
    eprintln!("  Dominant clamp:       {}", diagnostic.dominant_clamp);
    eprintln!("  Companies w/ zero bids: {}", companies_with_zero_bids);
    eprintln!("  Companies w/ low liquid: {}", companies_with_low_liquid);
    eprintln!("  Dump: {:?}", dump_path);
    eprintln!("═══════════════════════════════════════════════════════════════");
    eprintln!();

    // Sanity assertions (informational — do not hard-fail on anemic volume,
    // just confirm the harness ran and produced data).
    assert!(total_bid_qty >= 0.0, "bid qty should be non-negative");
    assert!(total_ask_qty >= 0.0, "ask qty should be non-negative");
    assert!(
        !diagnostic.order_book.is_empty() || total_bid_qty + total_ask_qty == 0.0,
        "order book snapshot should be populated if any orders exist"
    );
    assert!(dump_path.exists(), "dump file should exist");
}

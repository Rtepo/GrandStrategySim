//! The global turn orchestrator.
//!
//! This module defines `run_turn_in_memory`, which sequences the per-country
//! and global phases of one game turn. It operates purely in-memory via
//! `InMemoryTurnContext` — no disk I/O occurs during turn processing.

use crate::corporate::{process_companies, process_unions, CompanyLifecycle};
use crate::economy::debt_market::{
    clear_arrears, clear_savings_bonds_b2c, clear_secondary_debt_market, issue_treasury_securities,
    process_debt_service,
};
use crate::economy::market::{GlobalMarket, MarketOrders, MarketSignal};
use crate::economy::order_book::{
    match_orders, match_orders_with_embargoes, Ask, Bid, OrderBook, Trade,
};
use crate::economy::{
    accrue_retail_rents, allocate_owner_infrastructure_funding, apply_clearance_discount,
    apply_payment_in_kind, apply_rationing_to_demand, build_consumer_demand,
    calculate_diversity_bonus, check_terrorism_triggers, clear_b2c_markets,
    clear_education_slots_b2c, clear_health_capacity_b2c, clear_information_b2c,
    compute_propaganda_subsidy_rate, execute_production_cycle, generate_store_offers,
    market_history, order_book, populate_education_service_needs, populate_health_service_needs,
    populate_information_service_needs, process_all_royalty_payments,
    process_blueprint_royalty_payments, process_building_cycle_with_geology,
    process_cross_border_royalty_queue, process_demographics_and_labor, process_fishing_turn,
    process_justice_turn, process_prison_labor_turn, process_propaganda_turn,
    refund_unfilled_b2b_bids, refund_unfilled_defense_bids_per_country,
    reset_procurement_commitment, resolve_market_prices, settle_b2c_clearing,
    settle_defense_trades, settle_maintenance_service_trades, settle_trades,
    settle_trades_with_tariffs, submit_company_b2b_orders, submit_fixed_asset_purchase_bids,
    submit_maintenance_service_bids, trade_innovation_points_b2b,
    update_gdp_shares_from_employment, CountryTurnCtx,
};
use crate::entities::{Building, Company, LegalForm, Union};
use crate::government::{
    accumulate_storage_fees, apply_rationing_consequences, check_emergency_conditions,
    process_black_ops_funding, process_state_reserve_maintenance,
};
use crate::international::{balance_global_trade, process_diplomacy_turn};
use crate::military::war_economy::{
    execute_conscription, issue_war_bonds, process_expired_decrees, WarEconomyConfig,
};
use crate::military::{add_military_demand_to_market, process_military_turn};
use crate::politics::budget_lifecycle::{
    apply_budget_failure_consequence, draft_budget_bill, process_budget_lifecycle,
};
use crate::politics::fiscal_transfers::{
    check_commissary_administration, process_fiscal_transfers, process_local_elections,
    process_municipal_debt_service, process_regional_taxes, update_curial_faction_alignments,
};
use crate::politics::ministries::{
    allocate_cash_to_ministries, calculate_budget_needs, prepare_minister_strategies_with_parties,
    process_minister_post_clearing, sum_ministry_allocations,
};
use crate::politics::process_political_year;
use crate::registries::enums::Commodity;
use crate::registries::enums::Sector;
use crate::registries::Registries;
use crate::society::geography::{DemographicClass, RuralClass, UrbanClass};
use crate::society::housing::{CommercialBuilding, HousingBuilding};
use crate::state::Country;
use crate::state::GameState;
use crate::state::{process_banking_turn, process_tax_collection_turn, settle_trade_deficits};
use rand::SeedableRng;
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

/// Errors that can occur while running a global turn.
#[derive(Debug)]
pub enum TurnError {
    /// I/O error reading a save file.
    Io(std::io::Error),
    /// JSON parse error.
    Json(serde_json::Error),
    /// A required global file is missing.
    MissingFile(PathBuf),
    /// A generic runtime error message.
    Message(String),
    /// Debug information
    Debug(String),
}

impl fmt::Display for TurnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TurnError::Io(e) => write!(f, "I/O error: {e}"),
            TurnError::Json(e) => write!(f, "JSON error: {e}"),
            TurnError::MissingFile(p) => write!(f, "missing file: {}", p.display()),
            TurnError::Message(m) => write!(f, "{m}"),
            TurnError::Debug(m) => write!(f, "Debug: {m}"),
        }
    }
}

impl std::error::Error for TurnError {}

impl From<std::io::Error> for TurnError {
    fn from(e: std::io::Error) -> Self {
        TurnError::Io(e)
    }
}

impl From<serde_json::Error> for TurnError {
    fn from(e: serde_json::Error) -> Self {
        TurnError::Json(e)
    }
}

impl From<crate::io::entity_store::EntityStoreError> for TurnError {
    fn from(e: crate::io::entity_store::EntityStoreError) -> Self {
        TurnError::Message(e.to_string())
    }
}

/// Phase D.1.2: Pending OHS compensation event from the construction fraud
/// phase. Compensation is deferred until after the labor allocation matrix
/// is built so that funds can be routed to the exact injured worker class
/// via `settle_transfer` (Rule 1 + Rule 7).
#[derive(Debug, Clone)]
pub(crate) struct PendingOhsCompensation {
    /// Contractor company ID (the employer being debited).
    pub employer_id: String,
    /// Region ID where the accident occurred.
    pub region_id: String,
    /// Total compensation amount to debit from the employer.
    pub total_compensation: f64,
}

/// Per-country work bundle used by the parallel turn executor.
///
/// Holds the mutable [`Country`] reference (split-borrow from `GameState`), the
/// in-memory company/building lists and the per-country market orders. This
/// struct is `Send` and `Sync` because it owns all of its per-country data and
/// only borrows shared or disjoint mutable state from `GameState`.
#[derive(Debug)]
pub(crate) struct CountryTask<'a> {
    pub(crate) ctx: CountryTurnCtx<'a>,
    pub(crate) companies: Vec<Company>,
    pub(crate) unions: Vec<Union>,
    pub(crate) commercial_buildings: Vec<CommercialBuilding>,
    pub(crate) housing_buildings: Vec<HousingBuilding>,
    pub(crate) despawned_company_ids: Vec<String>,
    pub(crate) climate_config: crate::state::climate::ClimateConfig,
    pub(crate) orders: MarketOrders,
    pub(crate) market_signal: MarketSignal,
    /// Per-country B2B order book for infrastructure/maritime/cultural orders
    pub(crate) order_book: OrderBook,
    /// Phase 9: Tourism demand result (for sequential foreign-sector clamping + settlement)
    pub(crate) tourism_demand: crate::society::tourism::TourismDemandResult,
    /// E3: Heritage tourism boost per region (from heritage processing)
    pub(crate) heritage_tourism_boost: std::collections::BTreeMap<String, f64>,
    /// Phase 12: Labor allocation from W1, passed to D.5 payment in kind
    pub(crate) labor_allocation: Option<crate::economy::labor_market::LaborAllocationMatrix>,
    /// Phase D.5.1: Dominant demographic class per company, extracted from
    /// `labor_allocation` before it is consumed. Used by inspectorate bribery
    /// (Phase 22C) and OHS compensation to route funds to the exact class of
    /// the workers/officials at a specific company.
    /// Key: company_id → DemographicClass of the dominant FTE class.
    pub(crate) dominant_class_by_company:
        Option<std::collections::HashMap<String, DemographicClass>>,
    /// Phase D.1.2: Pending OHS compensation events from the construction
    /// fraud phase. Compensation is deferred until after the labor allocation
    /// matrix is built and the dominant class map is extracted, so that
    /// funds can be routed to the exact injured worker class.
    pub(crate) pending_ohs_compensations: Vec<PendingOhsCompensation>,
    /// Saved defense bids from pending_defense_orders, captured before draining
    /// into the global order book. Used for post-clearing refund calculation.
    pub(crate) saved_defense_bids: Vec<crate::economy::order_book::Bid>,
    /// Phase 17B: Education consumption per region (from B2C clearing), for assimilation.
    pub(crate) education_consumption: std::collections::BTreeMap<String, f64>,
    /// Phase 17B: Education needs per region (from populate_education_service_needs), for assimilation.
    pub(crate) education_needs: std::collections::BTreeMap<String, f64>,
    /// Phase 17C: Apostolic See remittance result (for sequential aggregation into global ledger).
    pub(crate) see_remittance: crate::economy::religious_economy::SeeRemittanceResult,
    /// Phase F4: Wealth extracted from deported illegal immigrants.
    /// Credited to foreign_sector_balance in the sequential post-pass.
    pub(crate) deported_wealth: f64,
    /// Phase 19A: Cross-border blueprint royalty outbox (emitted by this
    /// country's parallel royalty phase; consumed by the sequential post-parallel
    /// crediting pass that credits foreign licensors in their home country).
    pub(crate) cross_border_royalty_outbox:
        Vec<crate::economy::blueprints::CrossBorderRoyaltyQueueEntry>,
    /// Phase 95: Foreign patent fee outbox (emitted by this country's parallel
    /// R&D research phase; consumed by the sequential post-parallel pass that
    /// credits GlobalMarket.offshore_capital — money preserved, never destroyed).
    pub(crate) foreign_patent_fee_outbox: f64,
    /// Phase 23C: Per-region commute coverage ratio (0.0–1.0) from
    /// `clear_passenger_transport_b2c`. Used by the labor-market phase to
    /// compute how much FTE can commute into each region from neighbors.
    pub(crate) commute_coverage: std::collections::BTreeMap<String, f64>,
    /// Phase 24D: Per-country GDP expenditure accumulator (consumption, G, I, NX, shadow).
    /// Populated during the turn by B2C clearing, ministry procurement,
    /// fixed-asset purchases, construction, and shadow economy phases.
    pub(crate) gdp_acc: crate::economy::telemetry::GdpAccumulator,
    /// Phase 25: Retail prices collected from B2C clearing, for CPI calculation.
    pub(crate) retail_prices: Vec<(crate::registries::enums::Commodity, f64, f64)>,
    /// Phase 44: B2C consumer demand per commodity, collected for Market UI.
    pub(crate) b2c_demand: std::collections::HashMap<crate::registries::enums::Commodity, f64>,
    /// Phase 44: In-kind ledger from payment-in-kind processing (for imputed GDP).
    pub(crate) in_kind_ledger: crate::economy::finance::payment_in_kind::InKindLedger,
    /// Phase 44: Total imputed consumption value from subsistence economy.
    pub(crate) imputed_consumption: f64,
    /// Phase 28: Index of the State Employer pseudo-company in `companies`,
    /// if one was injected for labor clearing. Removed after wage accumulation.
    pub(crate) state_employer_idx: Option<usize>,
    /// Phase 33: Ministry public service wage pool routed to the State Employer.
    /// This amount was already debited from liquid_reserves by allocate_cash_to_ministries,
    /// so the State Employer post-clearing debit must be reduced by this amount
    /// to avoid double-debiting.
    pub(crate) ministry_public_service_pool: f64,
    /// Phase 93: Immutable Planet reference for vein-based geology lookups
    /// during the parallel production pass. Depletion is collected into
    /// `depletion_buffer` and applied sequentially after the parallel pass.
    pub(crate) planet: &'a crate::society::planet::Planet,
    /// Phase 93: Per-country depletion request buffer. Collected during the
    /// parallel production pass and applied to `state.planet` sequentially
    /// with pro-rata clamping after the parallel pass completes.
    pub(crate) depletion_buffer: Vec<crate::economy::production::geology::DepletionRequest>,
    /// Phase 94: Cached owner_id → building indices. Rebuilt with .clear()
    /// at phase boundaries that need it. Retains allocated capacity across
    /// turns to avoid allocator thrashing.
    pub(crate) owner_to_building_indices: rustc_hash::FxHashMap<String, Vec<usize>>,
    /// Phase 94: Cached company_id → company index. Rebuilt with .clear()
    /// at phase boundaries that need it.
    pub(crate) company_id_to_idx: rustc_hash::FxHashMap<String, usize>,
    /// Phase 94: Cached commercial building id → owner_id. Rebuilt with
    /// .clear() at phase boundaries that need it.
    pub(crate) store_id_to_owner: rustc_hash::FxHashMap<String, String>,
    /// Phase 15B (Agent 4): Total value of cross-border B2B trades settled
    /// this turn (sum of trade_value where buyer_country != seller_country).
    /// Used by the smuggling phase as the real trade volume instead of the
    /// previous magic `sum(production) * 1000.0` estimate (Rule 2).
    pub(crate) cross_border_trade_value: f64,
}

impl<'a> CountryTask<'a> {
    /// Rebuild the owner→buildings index map. Called at phase boundaries
    /// where building ownership lookups are needed. Uses .clear() to retain
    /// capacity from previous turns.
    pub(crate) fn rebuild_owner_to_building_indices(&mut self) {
        self.owner_to_building_indices.clear();
        for (i, b) in self.ctx.buildings.iter().enumerate() {
            self.owner_to_building_indices
                .entry(b.owner_id.clone())
                .or_default()
                .push(i);
        }
    }

    /// Rebuild the company_id→index map. Called at phase boundaries where
    /// company lookups are needed.
    pub(crate) fn rebuild_company_id_to_idx(&mut self) {
        self.company_id_to_idx.clear();
        for (i, c) in self.companies.iter().enumerate() {
            self.company_id_to_idx.insert(c.id.clone(), i);
        }
    }

    /// Rebuild the store_id→owner map from commercial buildings.
    pub(crate) fn rebuild_store_id_to_owner(&mut self) {
        self.store_id_to_owner.clear();
        for b in &self.commercial_buildings {
            if !b.owner_id.is_empty() {
                self.store_id_to_owner
                    .insert(b.id.clone(), b.owner_id.clone());
            }
        }
    }
}

/// Run a turn using in-memory context. NO disk I/O.
/// This is the sole production turn entry point.
///
/// # Arguments
/// * `state` - The global game state, including countries and currencies.
/// * `registries` - Immutable game data (production methods, tech tree, etc.).
/// * `ctx` - In-memory turn context (market, diplomacy, per-country entities).
///
/// # Returns
/// `Ok(())` when the turn completes, or a [`TurnError`] on failure.
///
/// # Rules
/// 1. Extracts market, diplomacy, and entities from `ctx` (no disk I/O).
/// 2. Runs the per-country phases in parallel across all countries with
///    `rayon`.
/// 3. Collects per-country `MarketOrders` and runs the global trade balancer
///    sequentially.
/// 4. Updates turn and year counters in `state.extra` and `state.calendar`.
/// 5. Writes results back into `ctx` (no disk persistence).
pub fn run_turn_in_memory(
    state: &mut GameState,
    registries: &Registries,
    ctx: &mut crate::engine::turn_context::InMemoryTurnContext,
) -> Result<(), TurnError> {
    run_turn_inner(
        state,
        registries,
        ctx,
        &mut crate::engine::diagnostic::NoopProbe,
    )
}

/// Generic turn runner with probe instrumentation.
///
/// When `P = NoopProbe`, all checkpoint calls are compiled away by LLVM
/// (zero-cost abstraction via `#[inline(always)]`).
/// When `P = CapturingProbe` (feature `diagnostic`), state snapshots are
/// captured at each phase seam for conservation analysis.
pub fn run_turn_inner<P: crate::engine::diagnostic::TurnProbe>(
    state: &mut GameState,
    registries: &Registries,
    ctx: &mut crate::engine::turn_context::InMemoryTurnContext,
    probe: &mut P,
) -> Result<(), TurnError> {
    // Extract context fields to avoid borrow conflicts with state.countries.iter_mut().
    let mut market = std::mem::take(&mut ctx.market);
    let mut diplomacy = std::mem::take(&mut ctx.diplomacy);
    let mut entities = std::mem::take(&mut ctx.entities);

    // prev_net_surplus was already captured by load_from_disk.
    let mut turn = state.calendar.global_turn;
    let mut year = state.calendar.current_year;

    let mut global_orders = MarketOrders::default();

    {
        // Convert CountryEntities into the tuple format expected by the turn loop.
        let mut entity_tuples: HashMap<
            String,
            (
                Vec<Company>,
                Vec<Building>,
                Vec<Union>,
                Vec<CommercialBuilding>,
                Vec<HousingBuilding>,
            ),
        > = HashMap::new();
        for (name, ents) in entities.drain() {
            entity_tuples.insert(
                name,
                (
                    ents.companies,
                    ents.buildings,
                    ents.unions,
                    ents.commercial_buildings,
                    ents.housing_buildings,
                ),
            );
        }

        // Phase 6.5: Use unified calendar from GameState
        let turn_calendar = &state.calendar;

        // Phase 86.5A: Pre-compute dynamic subsistence wage floor.
        // This replaces static `.max(1.0)` and `.max(1000.0)` floors with
        // a market-derived subsistence basket cost (Food + Clothing VWAP).
        // Computed before tasks creation to avoid borrow conflicts.
        let turn_config = state
            .countries
            .values()
            .next()
            .map(|c| c.turn_config.clone())
            .unwrap_or_default();
        let subsistence_wage_floor = crate::engine::turn_config::effective_wage(
            0.0, // Pass 0.0 to get the pure subsistence floor
            &state.market_history,
            &turn_config,
        );

        // Phase 93: Borrow Planet immutably before the mutable countries iteration
        // to satisfy the borrow checker (cannot have &state.planet and &mut state.countries
        // simultaneously without split borrowing).
        let planet_ref = &state.planet;

        let mut tasks: Vec<CountryTask> = state
            .countries
            .iter_mut()
            .map(|(name, country)| {
                let (companies, buildings, unions, commercial_buildings, housing_buildings) =
                    entity_tuples.remove(name).unwrap_or((
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                    ));
                let climate_config = state.climate_config.clone();
                CountryTask {
                    ctx: CountryTurnCtx {
                        country_name: name.clone(),
                        turn,
                        year,
                        registries,
                        country,
                        buildings,
                        market_prices: rustc_hash::FxHashMap::default(),
                    },
                    companies,
                    unions,
                    commercial_buildings,
                    housing_buildings,
                    despawned_company_ids: Vec::new(),
                    climate_config,
                    orders: MarketOrders::default(),
                    market_signal: MarketSignal::default(),
                    order_book: OrderBook::default(),
                    tourism_demand: crate::society::tourism::TourismDemandResult::default(),
                    heritage_tourism_boost: std::collections::BTreeMap::new(),
                    labor_allocation: None,
                    dominant_class_by_company: None,
                    pending_ohs_compensations: Vec::new(),
                    saved_defense_bids: Vec::new(),
                    education_consumption: std::collections::BTreeMap::new(),
                    education_needs: std::collections::BTreeMap::new(),
                    see_remittance: crate::economy::religious_economy::SeeRemittanceResult::default(
                    ),
                    deported_wealth: 0.0,
                    cross_border_royalty_outbox: Vec::new(),
                    foreign_patent_fee_outbox: 0.0,
                    commute_coverage: std::collections::BTreeMap::new(),
                    gdp_acc: crate::economy::telemetry::GdpAccumulator::default(),
                    retail_prices: Vec::new(),
                    b2c_demand: std::collections::HashMap::new(),
                    in_kind_ledger: crate::economy::finance::payment_in_kind::InKindLedger::default(
                    ),
                    imputed_consumption: 0.0,
                    state_employer_idx: None,
                    ministry_public_service_pool: 0.0,
                    planet: planet_ref,
                    depletion_buffer: Vec::new(),
                    owner_to_building_indices: rustc_hash::FxHashMap::default(),
                    company_id_to_idx: rustc_hash::FxHashMap::default(),
                    store_id_to_owner: rustc_hash::FxHashMap::default(),
                    cross_border_trade_value: 0.0,
                }
            })
            .collect();

        // Phase 11: Build ephemeral company_id -> country_name lookup for embargo/tariff enforcement.
        // Rebuilt every turn from the authoritative CountryTask grouping. No struct pollution.
        let company_country: HashMap<String, String> = tasks
            .iter()
            .flat_map(|task| {
                task.companies
                    .iter()
                    .map(|c| (c.id.clone(), task.ctx.country_name.clone()))
            })
            .collect();

        // Phase 43: Build country_name -> currency_code lookup for FX reserves.
        // Maps each country to its 3-letter currency code using the shared
        // currency zones in GameState.currencies. Used by settle_trades_with_tariffs
        // to accumulate/debit real foreign currency codes instead of fake "IEU".
        let country_to_currency: HashMap<String, String> = state
            .currencies
            .iter()
            .flat_map(|(ccy_code, currency)| {
                currency
                    .members
                    .iter()
                    .map(move |m| (m.clone(), ccy_code.clone()))
            })
            .collect();

        // Agent 4 — Phase 3: Build currency exchange rate map for FX conversion.
        // Maps currency code → exchange_rate vs global numeraire.
        // Used by settle_trades_with_tariffs to replace hardcoded exchange_rate = 1.0.
        let currency_rates: HashMap<String, f64> = state
            .currencies
            .iter()
            .map(|(ccy_code, currency)| (ccy_code.clone(), currency.exchange_rate))
            .collect();

        // Phase 94: Re-aggregate citizen_savings before the turn_start checkpoint
        // so that budget.citizen_savings is always fresh and consistent with
        // demo.savings. Without this, the M0 fiat walk sees a stale value at
        // turn_start, then a "jump" after process_demographics_and_labor runs,
        // creating a false FiatCreation violation.
        tasks.par_iter_mut().for_each(|task| {
            crate::economy::labor::aggregate_citizen_savings(task.ctx.country);
        });

        // ── DIAGNOSTIC CHECKPOINT 0: turn_start ──
        probe.checkpoint("turn_start", 0, turn, &market, &tasks);

        // ═══════════════════════════════════════════════════════════
        // PHASE 85: FACTIONAL DOMAIN MODIFIERS
        // Must run BEFORE process_demographics_and_labor so that:
        // - AristocraticEstate feudal dues reduce available_fte before labor market
        // - ClergyLand education/health capacity is available for social systems
        // - Domain modifiers are applied before any FTE allocation decisions
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            crate::society::factional_domains::apply_domain_modifiers(task.ctx.country);
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 14: PRISON LABOR PREPROCESSING
        // Must run BEFORE process_demographics_and_labor so that:
        // - PrivateLaborCamps: company.target_fte_demand is reduced before labor market
        // - IsolationCamp: targeted demographics have available_fte zeroed before labor pool
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            process_prison_labor_turn(task.ctx.country, &task.ctx.buildings, &mut task.companies);
        });

        tasks.par_iter_mut().for_each(|task| {
            process_demographics_and_labor(&mut task.ctx);
        });
        // Phase 13: Initialize per-class religion from country's dominant religion.
        // Runs after demographics processing creates class entries.
        tasks.par_iter_mut().for_each(|task| {
            let religion = task.ctx.country.macro_indicators.religion.clone();
            if !religion.is_empty() {
                for region in &mut task.ctx.country.regions {
                    for demo in region.class_demographics.rural_classes.values_mut() {
                        if demo.religion.is_empty() {
                            demo.religion = religion.clone();
                        }
                    }
                    for demo in region.class_demographics.urban_classes.values_mut() {
                        if demo.religion.is_empty() {
                            demo.religion = religion.clone();
                        }
                    }
                }
            }
        });
        // Phase 10: Initialize per-class culture from country's dominant culture.
        // This is the save-migration path for old saves that don't have the
        // `culture` field — empty values are backfilled with the dominant culture.
        tasks.par_iter_mut().for_each(|task| {
            let culture = task.ctx.country.macro_indicators.culture.clone();
            if !culture.is_empty() {
                for region in &mut task.ctx.country.regions {
                    for demo in region.class_demographics.rural_classes.values_mut() {
                        if demo.culture.is_empty() {
                            demo.culture = culture.clone();
                        }
                    }
                    for demo in region.class_demographics.urban_classes.values_mut() {
                        if demo.culture.is_empty() {
                            demo.culture = culture.clone();
                        }
                    }
                }
            }
        });
        // Phase 23C: Remit commuter wages back to home regions' class savings.
        // Commuters earned net wages (after PIT) in the host region; these are
        // distributed proportionally across all adjacent regions' classes as a
        // simplified remittance (since we don't track per-home-region FTE yet).
        // Phase 94: Moved here (before banking_turn_post checkpoint) so that
        // the citizen savings credit and the bank reserve debit (from the
        // labor phase wage batch sync) are in the same checkpoint interval.
        // Previously, this ran between production_cycle_post and
        // b2c_clearing_post, causing a false FiatCreation violation because
        // the credit appeared without the matching debit in the same phase.
        tasks.par_iter_mut().for_each(|task| {
            if let Some(ref labor_alloc) = task.labor_allocation {
                if labor_alloc.commuter_wages > 0.0 && labor_alloc.commuter_fte > 0.0 {
                    let wages = labor_alloc.commuter_wages;
                    // Find adjacent regions and distribute wages proportionally
                    // to their available FTE.
                    if let Some(host_region) = task.ctx.country.regions.first() {
                        let adjacent_ids: Vec<String> = host_region
                            .edges
                            .iter()
                            .filter(|e| {
                                e.edge_type == crate::society::geography::EdgeType::LandBorder
                            })
                            .map(|e| e.target_node.clone())
                            .collect();
                        let mut total_adjacent_fte = 0.0_f64;
                        for adj_id in &adjacent_ids {
                            if let Some(adj) =
                                task.ctx.country.regions.iter().find(|r| &r.id == adj_id)
                            {
                                total_adjacent_fte += adj
                                    .class_demographics
                                    .rural_classes
                                    .values()
                                    .chain(adj.class_demographics.urban_classes.values())
                                    .map(|d| d.available_fte)
                                    .sum::<f64>();
                            }
                        }
                        if total_adjacent_fte > 0.0 {
                            for adj_id in &adjacent_ids {
                                let adj_fte: f64 = task
                                    .ctx
                                    .country
                                    .regions
                                    .iter()
                                    .find(|r| &r.id == adj_id)
                                    .map(|r| {
                                        r.class_demographics
                                            .rural_classes
                                            .values()
                                            .chain(r.class_demographics.urban_classes.values())
                                            .map(|d| d.available_fte)
                                            .sum()
                                    })
                                    .unwrap_or(0.0);
                                let share = adj_fte / total_adjacent_fte;
                                let remittance = wages * share;
                                if remittance > 0.0 {
                                    if let Some(adj) = task
                                        .ctx
                                        .country
                                        .regions
                                        .iter_mut()
                                        .find(|r| &r.id == adj_id)
                                    {
                                        // Distribute proportionally across classes by available FTE.
                                        let classes: Vec<(bool, String, f64)> = adj
                                            .class_demographics
                                            .rural_classes
                                            .iter()
                                            .map(|(k, v)| (false, k.to_string(), v.available_fte))
                                            .chain(
                                                adj.class_demographics.urban_classes.iter().map(
                                                    |(k, v)| (true, k.to_string(), v.available_fte),
                                                ),
                                            )
                                            .collect();
                                        let class_total: f64 =
                                            classes.iter().map(|(_, _, f)| *f).sum();
                                        if class_total > 0.0 {
                                            for (is_urban, class_id, fte) in classes {
                                                let class_share = fte / class_total;
                                                let class_remittance = remittance * class_share;
                                                if is_urban {
                                                    if let Some(d) = UrbanClass::from_str(&class_id).and_then(|k| adj
                                                        .class_demographics
                                                        .urban_classes
                                                        .get_mut(&k))
                                                    {
                                                        d.savings += class_remittance;
                                                    }
                                                } else {
                                                    if let Some(d) = RuralClass::from_str(&class_id).and_then(|k| adj
                                                        .class_demographics
                                                        .rural_classes
                                                        .get_mut(&k))
                                                    {
                                                        d.savings += class_remittance;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });

        tasks.par_iter_mut().for_each(|task| {
            process_banking_turn(task.ctx.country, &mut task.companies, task.ctx.turn);
        });
        // ── DIAGNOSTIC CHECKPOINT: banking_turn_post ──
        probe.checkpoint("banking_turn_post", 7, turn, &market, &tasks);
        tasks.par_iter_mut().for_each(|task| {
            update_gdp_shares_from_employment(&mut task.ctx);
        });

        // ═══════════════════════════════════════════════════════════
        // RESURRECTION PHASE 10: EMERGENCY CONDITIONS CHECK
        // Must run before B2C clearing so rationing state is known.
        // Uses a snapshot of global market surplus for shortage detection.
        // ═══════════════════════════════════════════════════════════
        let surplus_snapshot = market.net_surplus.clone();
        tasks.par_iter_mut().for_each(|task| {
            check_emergency_conditions(task.ctx.country, &surplus_snapshot);
        });

        tasks.par_iter_mut().for_each(|task| {
            // Phase 4: Reset transient disruption modifiers (set by mass movements in previous turn)
            for company in &mut task.companies {
                company.temporary_disruption_modifier = 0.0;
            }
            process_unions(
                &mut task.companies,
                &mut task.unions,
                task.ctx.country,
                task.ctx.year,
            );
        });
        tasks.par_iter_mut().for_each(|task| {
            let base_wage = task
                .ctx
                .country
                .macro_indicators
                .average_wage
                .max(subsistence_wage_floor);

            // Phase 95: Build a blueprint lookup map (blueprint_id → ProductBlueprint)
            // from all companies' owned and licensed blueprints.
            let bp_lookup: std::collections::HashMap<
                String,
                &crate::economy::trade::blueprints::ProductBlueprint,
            > = {
                let mut m = std::collections::HashMap::new();
                for company in &task.companies {
                    for bp in &company.blueprints {
                        m.insert(bp.id.clone(), bp);
                    }
                }
                m
            };

            // Phase 94: Build a company_id → disruption map to avoid O(B×C)
            // linear scan inside the building loop.
            let mut company_disruption: rustc_hash::FxHashMap<String, f64> =
                rustc_hash::FxHashMap::default();
            for c in &task.companies {
                company_disruption.insert(c.id.clone(), c.temporary_disruption_modifier);
            }

            for building in &mut task.ctx.buildings {
                let disruption = company_disruption
                    .get(&building.owner_id)
                    .copied()
                    .unwrap_or(0.0);

                // Phase 95: Look up the active blueprint for this building.
                let active_bp = building
                    .active_method
                    .active_blueprint
                    .as_ref()
                    .and_then(|id| bp_lookup.get(id).copied());

                process_building_cycle_with_geology(
                    building,
                    task.ctx.country,
                    Some(task.planet),
                    Some(&mut task.depletion_buffer),
                    &mut task.orders,
                    &market.base_prices,
                    base_wage,
                    task.ctx.year,
                    task.ctx.registries,
                    disruption,
                    active_bp,
                );
            }
        });
        // ── DIAGNOSTIC CHECKPOINT 1: building_cycle_post ──
        probe.checkpoint("building_cycle_post", 1, turn, &market, &tasks);

        // Emergency Stabilization: Clone the immutable base prices (set once at
        // world generation in market_history.global_base_prices) so the clearing
        // engine can anchor PRICE_CAP/FLOOR to a stable reference instead of the
        // dynamically-updated VWAP-smoothed base prices that compound recursively.
        let immutable_base_prices = state.market_history.global_base_prices.clone();
        tasks.par_iter_mut().for_each(|task| {
            task.ctx.market_prices = resolve_market_prices(
                &task.orders,
                task.ctx.country,
                &market,
                &immutable_base_prices,
            );
            task.market_signal = build_market_signal(
                task.ctx.country,
                &task.orders,
                &market,
                &task.ctx.market_prices,
            );
        });

        // Phase 6.4: Continuous order book matching
        let mut global_order_book = OrderBook::default();

        // ═══════════════════════════════════════════════════════════
        // RESURRECTION PHASE 3: MILITARY — Drain pending defense orders
        // MoD B2B buy orders from last turn's Phase 8 are merged into the
        // global order book here, BEFORE matching. This ensures the MoD
        // only buys with cash it already received in last turn's Phase 8.
        // ═══════════════════════════════════════════════════════════
        for task in &mut tasks {
            // Save defense bids before draining for post-clearing refund calculation
            task.saved_defense_bids = task.ctx.country.pending_defense_orders.clone();
            for bid in task.ctx.country.pending_defense_orders.drain(..) {
                global_order_book
                    .bids
                    .entry(bid.commodity)
                    .or_insert_with(Vec::new)
                    .push(bid);
            }
        }

        // ═══════════════════════════════════════════════════════════
        // RESURRECTION PHASE 1: INFRASTRUCTURE PRE-CLEARING
        // ═══════════════════════════════════════════════════════════

        // 3.5a: Collect cultural donations (fundraising before B2B)
        tasks.par_iter_mut().for_each(|task| {
            let religion = task.ctx.country.macro_indicators.religion.clone();
            let average_wage = task.ctx.country.macro_indicators.average_wage;
            let config = task.ctx.country.cultural_relief_config.clone();
            crate::infrastructure::cultural::collect_cultural_donations(
                &mut task.ctx.country.regions,
                &mut task.companies,
                &mut task.ctx.country.cultural_institutions,
                &religion,
                average_wage,
                &config,
            );

            // Phase 28: Transfer collected donations from cultural buildings to
            // their owning Church/NGO companies so they can pay wages.
            // This is the organic funding mechanism — no magical seed capital.
            // Double-entry: DEBIT building.available_cash, CREDIT company.available_cash.
            for building in &mut task.ctx.country.cultural_institutions {
                if building.available_cash > 0.0 {
                    if let Some(ref owner_id) = building.owner_company_id {
                        let transfer = building.available_cash;
                        building.available_cash = 0.0;
                        if let Some(company) = task.companies.iter_mut().find(|c| &c.id == owner_id)
                        {
                            company.available_cash += transfer;
                            // Create or update brokerage account for labor market participation.
                            // The labor market requires brokerage_account.cash to compute
                            // max_affordable_fte. Without this, charities can never hire
                            // even when they have donation income.
                            if let Some(ref mut ba) = company.brokerage_account {
                                ba.cash += transfer;
                            } else {
                                company.brokerage_account =
                                    Some(crate::securities::BrokerageAccount {
                                        cash: transfer,
                                        ..Default::default()
                                    });
                            }
                        }
                    }
                }
            }
        });

        // Phase 17C: Apostolic See remittance (parallel — each country debits its own buildings/treasury)
        // The See ledger aggregation happens sequentially below.
        tasks.par_iter_mut().for_each(|task| {
            let religious_law = task
                .ctx
                .country
                .politics
                .religious_law_struct
                .clone()
                .unwrap_or_else(|| {
                    let reg = crate::society::culture_registry::registry();
                    let religion_key =
                        reg.religion_key_from_display(&task.ctx.country.macro_indicators.religion);
                    crate::politics::laws::ReligiousLaw::from_raw(
                        &task.ctx.country.politics.religious_law,
                        &religion_key,
                    )
                });
            let see_config = crate::economy::religious_economy::ApostolicSeeConfig::default();
            let mut temp_ledger = crate::economy::market::ApostolicSeeLedger::default();
            let result = crate::economy::religious_economy::process_see_remittance(
                task.ctx.country,
                &religious_law,
                &mut temp_ledger,
                &see_config,
            );
            task.see_remittance = result;
        });

        // Phase 17C: Sequentially aggregate See remittances into the global ledger.
        let total_see_remittance: f64 = tasks
            .iter()
            .map(|t| {
                t.see_remittance.secular_remittance + t.see_remittance.state_religion_remittance
            })
            .sum();
        market.apostolic_see_ledger.total_remittances += total_see_remittance;
        market.apostolic_see_ledger.global_charity_pool += total_see_remittance;

        // 3.5b: Distribute cash relief (direct transfers to serfs/laborers)
        tasks.par_iter_mut().for_each(|task| {
            let config = task.ctx.country.cultural_relief_config.clone();
            crate::infrastructure::cultural::distribute_cash_relief(
                &mut task.ctx.country.regions,
                &mut task.ctx.country.cultural_institutions,
                &config,
            );
        });

        // 3.6: Submit relief B2B buy orders (before market clearing)
        tasks.par_iter_mut().for_each(|task| {
            let config = task.ctx.country.cultural_relief_config.clone();
            crate::infrastructure::cultural::submit_relief_b2b_orders(
                &mut task.ctx.country.cultural_institutions,
                &mut task.order_book,
                &market,
                &config,
            );
        });

        // 3.7a: Submit shipyard construction B2B buy orders
        tasks.par_iter_mut().for_each(|task| {
            let config = task.ctx.country.maritime_config.clone();
            crate::infrastructure::maritime::submit_shipyard_construction_orders(
                &mut task.ctx.country.maritime_infrastructure,
                &mut task.order_book,
                &market,
                &config,
            );
        });

        // Phase 37: Mobilization advance — release first tranche (trigger_progress=0)
        // BEFORE B2B order submission so contractors have cash to bid for materials.
        // This breaks the "no cash → no bids → no materials → no progress → no tranche" deadlock.
        tasks.par_iter_mut().for_each(|task| {
            let _released = crate::construction::orders::release_construction_tranches(
                &mut task.ctx.buildings,
                &mut task.companies,
                task.ctx.country,
            );
        });

        // 3.8: Submit construction B2B buy orders for active building projects
        tasks.par_iter_mut().for_each(|task| {
            let b2b_config = task.ctx.country.b2b_order_config.clone();
            let _msgs = crate::construction::orders::submit_construction_b2b_orders(
                &mut task.companies,
                &task.ctx.buildings,
                &mut task.order_book,
                &state.market_history,
                &b2b_config,
            );
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 22A: CONSTRUCTION TENDER MARKET
        // Award expired tenders and create new construction projects
        // with contractor linkage. Runs after construction B2B orders
        // so new projects start ordering materials next turn.
        // ═══════════════════════════════════════════════════════════

        // Phase 24C.6: Property developer AI publishes new tenders based on
        // market opportunities (housing shortage, commercial vacancy, ROI).
        tasks.par_iter_mut().for_each(|task| {
            let mut tenders = std::mem::take(&mut task.ctx.country.phase22_tenders);
            // Use the first region's micro-region ID as the target
            let micro_region_id = task
                .ctx
                .country
                .regions
                .first()
                .map(|r| r.id.clone())
                .unwrap_or_else(|| "CENTRAL".to_string());
            let population = task
                .ctx
                .country
                .regions
                .first()
                .map(|r| r.population)
                .unwrap_or(100_000);
            // Build a HousingInventory from the task's housing buildings
            let housing_inventory = crate::society::housing::HousingInventory {
                buildings: task.housing_buildings.clone(),
            };
            // Convert market prices to String-keyed map for the developer API
            let market_prices: std::collections::BTreeMap<String, f64> = state
                .market_history
                .last_trade_price
                .iter()
                .map(|(k, v)| (format!("{:?}", k), *v))
                .collect();
            crate::corporate::development::publish_developer_tenders(
                &mut task.companies,
                &housing_inventory,
                &mut tenders,
                &market_prices,
                population,
                &micro_region_id,
                task.ctx.turn,
                task.ctx.year,
            );
            // Phase 40: Publish State-funded tenders from ministries with cash.
            // This fixes the root cause of the tender deadlock: the State never
            // published tenders, so only private developer tenders existed.
            crate::corporate::development::publish_state_tenders(
                task.ctx.country,
                &mut tenders,
                &micro_region_id,
                task.ctx.turn,
                task.ctx.year,
            );
            task.ctx.country.phase22_tenders = tenders;
        });

        // Phase 29: Construction companies submit bids on open tenders.
        // This makes the tender market functional — without bids, all
        // tenders would be cancelled on expiry.
        tasks.par_iter_mut().for_each(|task| {
            let mut tenders = std::mem::take(&mut task.ctx.country.phase22_tenders);
            let mut rng = rand::thread_rng();
            for tender in tenders.iter_mut() {
                if tender.status != crate::construction::tenders::TenderStatus::Open {
                    continue;
                }
                for company in &task.companies {
                    if company.sector != Sector::Construction {
                        continue;
                    }
                    if let Some((bid_cost, bid_margin, consortium)) =
                        crate::construction::tender_market::construction_bid_decision(
                            company, tender, &mut rng,
                        )
                    {
                        let _ = crate::construction::tender_market::submit_bid(
                            tender,
                            company,
                            bid_cost,
                            bid_margin,
                            consortium,
                            task.ctx.turn,
                        );
                    }
                }
            }
            task.ctx.country.phase22_tenders = tenders;
        });

        tasks.par_iter_mut().for_each(|task| {
            let mut tenders = std::mem::take(&mut task.ctx.country.phase22_tenders);
            let awarded = crate::construction::tender_market::process_tender_awards(
                &mut tenders,
                task.ctx.turn,
            );
            // Attach awarded projects to buildings in the tender's micro-region
            for (_tender_id, project, expansion_target) in awarded {
                // Phase 29: If this is an expansion tender, attach to the
                // specific building being expanded.
                if let Some(target_bldg_id) = expansion_target {
                    if let Some(building) = task
                        .ctx
                        .buildings
                        .iter_mut()
                        .find(|b| b.id == target_bldg_id)
                    {
                        if building.active_project.is_none() {
                            building.active_project = Some(project);
                        }
                    }
                    continue;
                }
                // New-building tender: find a matching building
                let region_id = project.micro_region_id.clone();
                let investor_id = project.investor_id.clone();
                let target_bt = project.target_building_type.clone();
                if let Some(building) = task.ctx.buildings.iter_mut().find(|b| {
                    b.region_id == region_id
                        && (b.owner_id == investor_id || investor_id.is_empty())
                        && b.active_project.is_none()
                        && (b.name == target_bt || target_bt.is_empty())
                }) {
                    building.active_project = Some(project);
                }
            }
            task.ctx.country.phase22_tenders = tenders;
        });

        // 3.9: Submit agricultural harvest asks (must be before global merge at 6.3)
        tasks.par_iter_mut().for_each(|task| {
            let market_prices: std::collections::BTreeMap<Commodity, f64> = state
                .market_history
                .vwap_per_commodity
                .iter()
                .map(|(c, p)| (*c, *p))
                .collect();
            for company in &task.companies {
                if company.sector == Sector::Agriculture {
                    let sell_orders = crate::agriculture::submit_harvest_asks(
                        company,
                        &task.commercial_buildings,
                        &market_prices,
                    );
                    for (commodity, quantity, ask_price) in sell_orders {
                        task.order_book
                            .asks
                            .entry(commodity)
                            .or_insert_with(Vec::new)
                            .push(Ask {
                                seller_id: company.id.clone(),
                                commodity,
                                quantity,
                                limit_price: ask_price,
                                blueprint_id: None,
                                quality: None,
                                durability: None,
                            });
                    }
                }
            }
        });

        // 3.7b: Add fleet commodity demand to market orders
        tasks.par_iter_mut().for_each(|task| {
            // Fleets not yet stored on Country; pass empty slice
            let fleets: Vec<crate::military::fleet::Fleet> = Vec::new();
            crate::military::add_fleet_demand_to_market(&fleets, &mut task.orders.orders);
        });

        // Collect all bids and asks from countries — Phase 6.3: Production Planning
        // Phase 23A: Manage deferred trades (increment counters, expire old ones).
        for task in &mut tasks {
            let freight_config = task.ctx.country.freight_logistics_config.clone();
            crate::economy::logistics::increment_deferral_counters(
                &mut task.ctx.country.deferred_trades,
            );
            let _expired = crate::economy::logistics::expire_deferred_trades(
                &mut task.ctx.country.deferred_trades,
                freight_config.max_deferred_turns,
            );
            // Expired trades have already had their encumbrance refunded at
            // deferral time; they are simply dropped here.
        }

        for task in &mut tasks {
            let b2b_config = task.ctx.country.b2b_order_config.clone();
            let gen_cfg = task.ctx.country.generative_goods_config.clone();

            // Phase 94: Rebuild owner→buildings index map with .clear() to
            // retain capacity and avoid allocator thrashing.
            task.rebuild_owner_to_building_indices();

            let messages = submit_company_b2b_orders(
                &mut task.companies,
                &task.ctx.buildings,
                &mut task.order_book,
                &state.market_history,
                &b2b_config,
                &gen_cfg,
                &task.owner_to_building_indices,
            );
            // Phase 19B: Submit maintenance-service bids for factories with
            // degraded fixed-asset cohorts. MaintenanceServices Sell Asks are
            // generated naturally by MaintenanceWorkshops buildings via the
            // normal output-ask loop above.
            let _maint_msgs = submit_maintenance_service_bids(
                &mut task.companies,
                &task.ctx.buildings,
                &mut task.order_book,
                &state.market_history,
                &b2b_config,
                &gen_cfg,
                &task.owner_to_building_indices,
            );

            // Phase 19C: Submit fixed-asset purchase bids (cash-bottlenecked).
            // Companies buy machinery/vehicles with willingness-to-pay clamped
            // by available cash — cash-poor firms buy cheaper, lower-quality
            // substitutes (or go without if they can't afford even the floor).
            let _asset_msgs = submit_fixed_asset_purchase_bids(
                &mut task.companies,
                &task.ctx.buildings,
                &mut task.order_book,
                &state.market_history,
                &b2b_config,
                &gen_cfg,
                &task.owner_to_building_indices,
            );
            // Merge per-country order_book into global_order_book
            for (commodity, bids) in &task.order_book.bids {
                global_order_book
                    .bids
                    .entry(*commodity)
                    .or_default()
                    .extend(bids.iter().cloned());
            }
            for (commodity, asks) in &task.order_book.asks {
                global_order_book
                    .asks
                    .entry(*commodity)
                    .or_default()
                    .extend(asks.iter().cloned());
            }
            task.order_book = OrderBook::default();
            let _ = messages;
        }

        // Match orders (Phase 11: embargo-aware matching)
        match_orders_with_embargoes(&mut global_order_book, &company_country, &diplomacy);
        let all_trades = global_order_book.trades.clone();

        // ── DIAGNOSTIC CHECKPOINT 2: b2b_orders_post ──
        probe.checkpoint("b2b_orders_post", 2, turn, &market, &tasks);

        // Phase 24A.1: Redistribute unfilled bids from global_order_book back to
        // per-country task.order_book so that refund functions can process them.
        // Previously, task.order_book was reset to empty at line 630 (after merging
        // into global_order_book), so the refund calls at lines 756-777 operated on
        // an empty order book and never released encumbered debit_cash.
        // Each refund function is selective by buyer_id, so redistributing all
        // unfilled bids to all tasks is safe — only the task that owns the buyer
        // entity will process the refund.
        let unfilled_bids: Vec<(Commodity, Vec<Bid>)> = global_order_book
            .bids
            .iter()
            .map(|(c, bids)| (*c, bids.clone()))
            .collect();
        for task in &mut tasks {
            for (commodity, bids) in &unfilled_bids {
                task.order_book
                    .bids
                    .entry(*commodity)
                    .or_default()
                    .extend(bids.iter().cloned());
            }
        }

        // Settle trades (peer-to-peer with double-entry accounting)
        // Phase 6.4a: Cash settlement + physical inventory routing to Building.inventory
        // Phase 23A: Freight procurement gate — cross-region trades must secure
        // FreightCapacity before physical settlement. Trades that cannot secure
        // freight are deferred (frozen) and retried next turn.
        for task in &mut tasks {
            let country_trades: Vec<Trade> = all_trades
                .iter()
                .filter(|t| {
                    task.companies
                        .iter()
                        .any(|c| c.id == t.buyer_id || c.id == t.seller_id)
                })
                .cloned()
                .collect();

            // Phase 23A-3: FREIGHT PROCUREMENT GATE
            // Agent 4 — Phase 6: Scale freight config by average_wage for
            // inflation-proofing (Rule 2).
            let freight_config = task
                .ctx
                .country
                .freight_logistics_config
                .scaled_for_economy(task.ctx.country.macro_indicators.average_wage);
            let mut network_overlay = task.ctx.country.transport_networks.clone();
            let regions = task.ctx.country.regions.clone();
            // Phase 30: Use market prices as fuel prices for multi-modal routing.
            let fuel_prices = task.ctx.market_prices.clone();
            let (secured_trades, new_deferred) =
                crate::economy::logistics::procure_freight_and_split_trades(
                    &country_trades,
                    &mut task.companies,
                    &mut task.ctx.buildings,
                    &regions,
                    &mut network_overlay,
                    &freight_config,
                    task.ctx.country,
                    &fuel_prices,
                    &diplomacy,
                    &company_country,
                );
            // Phase 30: Update the country's network overlay with congestion changes.
            task.ctx.country.transport_networks = network_overlay;
            // Merge deferred trades into the country's deferred list.
            task.ctx.country.deferred_trades.extend(new_deferred);

            let gen_config_clone = task.ctx.country.generative_goods_config.clone();
            let _msgs = settle_trades_with_tariffs(
                &secured_trades,
                &mut task.companies,
                &mut task.ctx.buildings,
                task.ctx.country,
                &company_country,
                &diplomacy,
                &country_to_currency,
                &currency_rates,
                task.ctx.turn,
                &gen_config_clone,
            );

            // Agent 4 — Phase 4: Accumulate cross-border trade value for the
            // smuggling phase. Replaces the previous magic `sum(production) * 1000.0`
            // estimate with the actual settled cross-border trade value (Rule 2).
            let cross_border_value: f64 = secured_trades
                .iter()
                .filter(|t| {
                    let bc = company_country.get(&t.buyer_id);
                    let sc = company_country.get(&t.seller_id);
                    bc.is_some() && sc.is_some() && bc != sc
                })
                .map(|t| t.quantity * t.execution_price)
                .sum();
            task.cross_border_trade_value = cross_border_value;

            // Phase 13: Storage transaction settlement will be wired when
            // warehouse extraction system produces FinancialTransaction records.

            // Black Hole 1.19: Credit defense sellers via TransferSettler.
            // settle_trades (called by settle_trades_with_tariffs) SKIPS cash
            // credit for defense trades (buyer_id == "MIN-DEF"). Instead,
            // settle_defense_trades credits the seller's brokerage_account.cash
            // AND syncs the bank balance sheet atomically via credit_company_by_id.
            let defense_trades: Vec<Trade> = secured_trades
                .iter()
                .filter(|t| t.buyer_id == "MIN-DEF")
                .cloned()
                .collect();
            if !defense_trades.is_empty() {
                settle_defense_trades(&defense_trades, &mut task.companies, task.ctx.country);
            }

            // Phase 19B: Settle maintenance-service trades.
            // Cash leg uses TransferSettler (strict double-entry); the service
            // is consumed on delivery (no physical inventory routing); cohort
            // condition is restored on the buyer's buildings.
            let gen_cfg = task.ctx.country.generative_goods_config.clone();
            let _maint_msgs = settle_maintenance_service_trades(
                &secured_trades,
                &mut task.companies,
                &mut task.ctx.buildings,
                &gen_cfg,
            );
            // Phase 24D: Accumulate fixed-asset purchases as GDP investment (I).
            // Only trades in fixed-asset commodities count as investment;
            // intermediate-goods trades are excluded to avoid double-counting.
            let investment: f64 = secured_trades
                .iter()
                .filter(|t| t.commodity.is_fixed_asset())
                .map(|t| t.quantity * t.execution_price)
                .sum();
            task.gdp_acc.investment += investment;
        }

        // ── DIAGNOSTIC CHECKPOINT 3: b2b_settlement_post ──
        probe.checkpoint("b2b_settlement_post", 3, turn, &market, &tasks);

        // ═══════════════════════════════════════════════════════════
        // PHASE 31: CRISIS MANAGEMENT AI — EXECUTIVE DECREES
        // Runs BEFORE ministry procurement so that tax adjustments, bond
        // issuance, and emergency subsidies are available for the turn.
        // Bypasses bill_lifecycle entirely (executive decrees only).
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            // Track consecutive zero-investment and zero-NX turns via extra map.
            let investment_zero = task.gdp_acc.investment <= 0.0;
            let nx_zero = task.gdp_acc.net_exports.abs() < 1e-9;

            let inv_turns = track_consecutive_zero(
                &mut task.ctx.country.macro_indicators.extra,
                "crisis_investment_zero_turns",
                investment_zero,
            );
            let trade_turns = track_consecutive_zero(
                &mut task.ctx.country.macro_indicators.extra,
                "crisis_trade_zero_turns",
                nx_zero,
            );

            // AI & Stability Audit (Pillar 4C): Counter-cyclical response runs
            // EVERY turn BEFORE crisis detection. This provides a smooth
            // Keynesian fiscal/monetary response to unemployment spikes,
            // preventing panic spirals before they become full crises.
            let counter_cyclical_msgs =
                crate::politics::crisis_management::counter_cyclical_response(
                    task.ctx.country,
                    turn,
                );
            if !counter_cyclical_msgs.is_empty() {
                let entry = serde_json::json!(counter_cyclical_msgs);
                task.ctx
                    .country
                    .macro_indicators
                    .extra
                    .insert("counter_cyclical_messages".to_string(), entry);
            }

            let crisis_msgs = crate::politics::crisis_management::execute_crisis_response(
                task.ctx.country,
                &mut task.companies,
                &task.ctx.market_prices,
                turn,
                inv_turns,
                trade_turns,
            );
            // Log crisis messages to the country's extra for telemetry.
            if !crisis_msgs.is_empty() {
                let entry = serde_json::json!(crisis_msgs);
                if let Some(arr) = task
                    .ctx
                    .country
                    .macro_indicators
                    .extra
                    .get_mut("crisis_decrees")
                    .and_then(|v| v.as_array_mut())
                {
                    arr.push(entry);
                } else {
                    task.ctx.country.macro_indicators.extra.insert(
                        "crisis_decrees".to_string(),
                        serde_json::Value::Array(vec![entry]),
                    );
                }
            }
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 32: PARLIAMENT BUILDING PAYROLL & PROCUREMENT
        // Pays MP and staff wages from Treasury, credits specific
        // ClassDemographics in the capital region (Bourgeoisie/Worker).
        // If Treasury cannot afford payroll: condition degrades, political
        // capital crashes, coalition tension rises. No money printed.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let parliament_msgs = process_parliament_building_payroll(task.ctx.country, turn);
            for msg in parliament_msgs {
                let entry = serde_json::json!(msg);
                if let Some(arr) = task
                    .ctx
                    .country
                    .macro_indicators
                    .extra
                    .get_mut("parliament_messages")
                    .and_then(|v| v.as_array_mut())
                {
                    arr.push(entry);
                } else {
                    task.ctx.country.macro_indicators.extra.insert(
                        "parliament_messages".to_string(),
                        serde_json::Value::Array(vec![entry]),
                    );
                }
            }
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 7: MINISTRY PROCUREMENT (Domestic Order Book)
        // CRITICAL: Uses settle_trades (NOT settle_trades_with_tariffs) —
        // ministry orders are domestic; tariffs must NOT apply.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            // Allocate cash from treasury to ministries
            allocate_cash_to_ministries(task.ctx.country);

            // Phase 24C.1: Extract ministry config to avoid borrow conflicts
            // when passing &mut country to spending functions.
            let mut ministry_config = task.ctx.country.politics.ministry_config.take();
            if let Some(ref mut config) = ministry_config {
                let parties = task.ctx.country.politics.active_parties.clone();
                let mut local_order_book = OrderBook::default();
                for ministry in &mut config.ministries {
                    let g_spent = prepare_minister_strategies_with_parties(
                        ministry,
                        &parties,
                        &mut task.companies,
                        &mut local_order_book,
                        task.ctx.country,
                    );
                    // Phase 42: Accumulate non-procurement ministry spending into GDP G.
                    task.gdp_acc.government_spending += g_spent;
                }
                // Phase 26: Populate the local order book with sell orders (asks)
                // from companies that have inventory of the commodities ministries
                // want to buy. Without this, ministry buy orders never match and
                // government spending (G) stays at zero.
                let ref_prices = &task.ctx.country.budget.extra.clone();
                for company in &task.companies {
                    // Compute company inventory from buildings owned by this company
                    let company_inventory: HashMap<Commodity, f64> = task
                        .ctx
                        .buildings
                        .iter()
                        .filter(|b| b.owner_id == company.id)
                        .flat_map(|b| b.inventory.iter())
                        .fold(HashMap::new(), |mut acc, (commodity, &qty)| {
                            *acc.entry(*commodity).or_insert(0.0) += qty;
                            acc
                        });
                    for (&commodity, &qty) in &company_inventory {
                        if qty <= 0.0 {
                            continue;
                        }
                        // Only add asks for commodities that ministries are bidding on
                        if !local_order_book.bids.contains_key(&commodity) {
                            continue;
                        }
                        let ref_price = ref_prices
                            .get(&format!("{:?}", commodity))
                            .and_then(|v| v.as_f64())
                            .unwrap_or(100.0);
                        let sell_price = ref_price * 1.1; // 10% markup over reference
                        local_order_book
                            .asks
                            .entry(commodity)
                            .or_insert_with(Vec::new)
                            .push(crate::economy::order_book::Ask {
                                seller_id: company.id.clone(),
                                commodity,
                                quantity: qty.min(1000.0), // cap to prevent dumping
                                limit_price: sell_price,
                                blueprint_id: None,
                                quality: None,
                                durability: None,
                            });
                    }
                }
                // Match ministry orders internally (domestic-only, no embargo check)
                match_orders(&mut local_order_book);
                // Settle ministry trades — DOMESTIC: use settle_trades (no tariffs)
                let ministry_trades = local_order_book.trades.clone();
                let gen_config_clone2 = task.ctx.country.generative_goods_config.clone();
                let _msgs = settle_trades(
                    &ministry_trades,
                    &mut task.companies,
                    &mut task.ctx.buildings,
                    task.ctx.turn,
                    &gen_config_clone2,
                );
                // Phase 24D: Accumulate ministry procurement as GDP government spending (G).
                let ministry_spend: f64 = ministry_trades
                    .iter()
                    .map(|t| t.quantity * t.execution_price)
                    .sum();
                task.gdp_acc.government_spending += ministry_spend;
                // Process post-clearing refunds for unfilled ministry bids
                for ministry in &mut config.ministries {
                    process_minister_post_clearing(
                        ministry,
                        &local_order_book,
                        &mut task.companies,
                        task.ctx.country,
                    );
                }
            }
            // Restore ministry config
            task.ctx.country.politics.ministry_config = ministry_config;
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 7B: JST PROCUREMENT (Local Government B2B Orders)
        // Phase D.8: Regional governments submit formal Buy Orders for
        // ConstructionMachinery and AdministrativeServices. Strict market
        // clearing — unfilled bids are refunded, infrastructure degrades.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let jst_config = crate::politics::jst_spending::JstSpendingConfig::default();
            let mut jst_order_book = OrderBook::default();

            // Step 1: Submit JST procurement bids.
            let _encumbrances = crate::politics::jst_spending::submit_jst_procurement_bids(
                task.ctx.country,
                &task.companies,
                &mut jst_order_book,
                &jst_config,
            );

            // Step 2: Populate sell orders (asks) from companies that have
            // inventory of the commodities JSTs want to buy.
            let ref_prices = &task.ctx.country.budget.extra.clone();
            for company in &task.companies {
                let company_inventory: HashMap<Commodity, f64> = task
                    .ctx
                    .buildings
                    .iter()
                    .filter(|b| b.owner_id == company.id)
                    .flat_map(|b| b.inventory.iter())
                    .fold(HashMap::new(), |mut acc, (commodity, &qty)| {
                        *acc.entry(*commodity).or_insert(0.0) += qty;
                        acc
                    });
                for (&commodity, &qty) in &company_inventory {
                    if qty <= 0.0 {
                        continue;
                    }
                    if !jst_order_book.bids.contains_key(&commodity) {
                        continue;
                    }
                    let ref_price = ref_prices
                        .get(&format!("{:?}", commodity))
                        .and_then(|v| v.as_f64())
                        .unwrap_or(100.0);
                    let sell_price = ref_price * 1.1;
                    jst_order_book
                        .asks
                        .entry(commodity)
                        .or_default()
                        .push(crate::economy::order_book::Ask {
                            seller_id: company.id.clone(),
                            commodity,
                            quantity: qty.min(1000.0),
                            limit_price: sell_price,
                            blueprint_id: None,
                            quality: None,
                            durability: None,
                        });
                }
            }

            // Step 3: Match JST orders.
            match_orders(&mut jst_order_book);

            // Step 4: Settle JST trades (DOMESTIC — no tariffs).
            let jst_trades = jst_order_book.trades.clone();
            let gen_config_jst = task.ctx.country.generative_goods_config.clone();
            // settle_trades handles seller crediting. JST buyer_id is not a
            // company, so settle_trades will skip the buyer cash credit
            // (similar to MIN-DEF). The encumbrance was already debited.
            let _msgs = settle_trades(
                &jst_trades,
                &mut task.companies,
                &mut task.ctx.buildings,
                task.ctx.turn,
                &gen_config_jst,
            );

            // Step 5: Record procured quantities and credit sellers via TransferSettler.
            let procured = crate::politics::jst_spending::settle_jst_trades(
                &jst_trades,
                &mut task.companies,
            );

            // Step 6: Refund unfilled JST bids.
            crate::politics::jst_spending::refund_unfilled_jst_bids(
                &jst_order_book,
                task.ctx.country,
            );

            // Step 7: Update infrastructure based on procurement.
            crate::politics::jst_spending::update_jst_infrastructure(
                task.ctx.country,
                &procured,
                &jst_config,
            );

            // Step 8: Collect local fees from citizens.
            crate::politics::jst_spending::collect_local_fees(
                task.ctx.country,
                &mut task.companies,
                &jst_config,
            );
        });

        // ═══════════════════════════════════════════════════════════
        // RESURRECTION PHASE 1: INFRASTRUCTURE POST-CLEARING
        // ═══════════════════════════════════════════════════════════

        // Post-clearing: Refund unfilled cultural bids
        tasks.par_iter_mut().for_each(|task| {
            order_book::refund_unfilled_bids_cultural(
                &task.order_book,
                &mut task.ctx.country.cultural_institutions,
            );
        });

        // Post-clearing: Refund unfilled shipyard construction bids
        tasks.par_iter_mut().for_each(|task| {
            order_book::refund_unfilled_bids_maritime(
                &task.order_book,
                &mut task.ctx.country.maritime_infrastructure,
            );
        });

        // Post-clearing: Refund unfilled company bids
        // Phase 24A.1: Use the CORRECT refund function from b2b_orders, which
        // properly releases debit_cash and restores available_cash + brokerage.
        // The old order_book::refund_unfilled_bids credited liquid_capital
        // (wrong field) and has been deleted.
        tasks.par_iter_mut().for_each(|task| {
            // Phase 94: Rebuild company_id→index map with .clear() to retain capacity.
            task.rebuild_company_id_to_idx();
            refund_unfilled_b2b_bids(
                &task.order_book,
                &mut task.companies,
                &task.company_id_to_idx,
            );
        });

        // Black Hole 1.19: Refund unfilled defense bids back to Treasury.
        // Defense bids (buyer_id == "MIN-DEF") are not companies, so
        // refund_unfilled_bids cannot refund them. This function computes
        // the refund as total_encumbered - filled_encumbered and restores
        // it to liquid_reserves.
        tasks.par_iter_mut().for_each(|task| {
            if !task.saved_defense_bids.is_empty() {
                refund_unfilled_defense_bids_per_country(
                    &task.saved_defense_bids,
                    &all_trades,
                    &task.companies,
                    task.ctx.country,
                );
            }
        });

        // Post-clearing: Advance shipyard construction projects
        tasks.par_iter_mut().for_each(|task| {
            crate::infrastructure::maritime::advance_shipyard_projects(
                &mut task.ctx.country.maritime_infrastructure,
                &task.order_book,
            );
        });

        // Post-clearing: Deliver relief goods to population
        tasks.par_iter_mut().for_each(|task| {
            crate::infrastructure::cultural::deliver_relief_goods(
                &task.order_book,
                &task.ctx.country.cultural_institutions,
                &mut task.ctx.country.regions,
            );
        });

        // Post-clearing: Process heritage effects (prestige, tourism)
        // E3: Store heritage tourism boost on the task for later tourism demand computation.
        tasks.par_iter_mut().for_each(|task| {
            let mut region_prestige = std::collections::BTreeMap::new();
            crate::infrastructure::heritage::process_heritage_effects(
                &mut task.ctx.buildings,
                task.ctx.year,
                &mut region_prestige,
                &mut task.heritage_tourism_boost,
            );
        });

        // Post-clearing: Process port utilization
        tasks.par_iter_mut().for_each(|task| {
            let config = task.ctx.country.maritime_config.clone();
            crate::infrastructure::maritime::process_ports_turn(
                &mut task.ctx.country.maritime_infrastructure,
                &config,
            );
        });

        // Post-clearing: Process shipyard/port maintenance
        tasks.par_iter_mut().for_each(|task| {
            let config = task.ctx.country.maritime_config.clone();
            crate::infrastructure::maritime::process_shipyard_maintenance(
                &mut task.ctx.country.maritime_infrastructure,
                &config,
                &mut task.companies,
            );
        });

        // Post-clearing: Process fleet upkeep
        tasks.par_iter_mut().for_each(|task| {
            // Fleets not yet stored on Country; pass empty slice
            let fleets: Vec<crate::military::fleet::Fleet> = Vec::new();
            let mut fleet_demand: HashMap<Commodity, f64> = HashMap::new();
            let (maintenance, wages) =
                crate::military::process_fleet_upkeep(&fleets, &mut fleet_demand);
            // Deduct from treasury
            let total_cost = maintenance + wages;
            let actual_deducted = task.ctx.country.budget.liquid_reserves.min(total_cost);
            task.ctx.country.budget.liquid_reserves -= actual_deducted;

            // Credit wages to citizen savings across all regions
            let wage_portion = wages.min(actual_deducted);
            if wage_portion > 0.0 {
                let num_regions = task.ctx.country.regions.len();
                if num_regions > 0 {
                    let per_region = wage_portion / num_regions as f64;
                    for region in &mut task.ctx.country.regions {
                        crate::economy::transfer_settler::credit_citizen_savings_region(
                            region, per_region,
                        );
                    }
                }
            }

            // Credit maintenance to a Construction sector company
            let maint_portion = actual_deducted - wage_portion;
            if maint_portion > 0.0 {
                let contractor_id = task
                    .companies
                    .iter()
                    .find(|c| c.sector == crate::registries::enums::Sector::Construction)
                    .map(|c| c.id.clone());
                if let Some(ref cid) = contractor_id {
                    crate::economy::transfer_settler::credit_company_by_id(
                        &mut task.companies,
                        cid,
                        maint_portion,
                    );
                }
            }
        });

        // ═══════════════════════════════════════════════════════════
        // RESURRECTION PHASE 3: MILITARY TURN
        // MIL-1: Upkeep (burn stockpiles, pay wages)
        // MIL-2: Supply delivery (B2B trades → depot → unit stockpiles)
        // MIL-3: Deterministic combat resolution
        // MIL-4: Casualty demographics + unit disbandment (conservation of mass)
        // MIL-5: Peasant devastation
        // MIL-6: War exhaustion decay
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let config = task.ctx.country.military_config.clone();
            let morale_config = task.ctx.country.morale_config.clone();
            let trades = all_trades.clone();
            let (_fronts, mil_messages) = process_military_turn(
                &mut task.ctx.country.order_of_battle,
                &mut task.ctx.country.military_fronts,
                &mut task.ctx.country.regions,
                &mut task.ctx.country.budget.liquid_reserves,
                &mut task.ctx.country.military_stockpile,
                &trades,
                &config,
                task.ctx.turn,
                &task.ctx.country.name,
                &morale_config,
                &mut task.ctx.country.pow_camp,
            );
            // Log military messages (in full implementation, would push to event log)
            let _ = mil_messages;
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 69: WAR ECONOMY
        // 69-A: Conscription (drain demographics → military units)
        // 69-B: War bond issuance (if at war and deficit exceeds threshold)
        // 69-C: Expired decree cleanup (after production, see below)
        // ═══════════════════════════════════════════════════════════
        let war_economy_config = WarEconomyConfig::default();
        tasks.par_iter_mut().for_each(|task| {
            // 69-A: Conscription
            let _conscription_result = execute_conscription(
                &mut task.ctx.country.regions,
                &mut task.ctx.country.order_of_battle,
                &mut task.ctx.country.war_economy,
                &war_economy_config,
                &task.ctx.country.name,
                task.ctx.turn,
            );

            // 69-B: War bond issuance if at war and deficit exceeds threshold
            let at_war = !task.ctx.country.at_war_with.is_empty();
            if at_war {
                let liquid = task.ctx.country.budget.liquid_reserves;
                let gdp: f64 = task.ctx.country.regions.iter().map(|r| r.gdp).sum();
                let deficit_threshold = gdp * war_economy_config.war_bond_deficit_threshold;
                if liquid < deficit_threshold {
                    let amount_needed = deficit_threshold - liquid;
                    let avg_wage = task
                        .ctx
                        .country
                        .regions
                        .iter()
                        .map(|r| r.gdp / (r.population as f64).max(1.0))
                        .sum::<f64>()
                        / (task.ctx.country.regions.len() as f64).max(1.0);
                    let _raised = issue_war_bonds(
                        task.ctx.country,
                        amount_needed,
                        &war_economy_config,
                        task.ctx.turn,
                        avg_wage,
                    );
                }
            }
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 6.4b-PRE: CONSTRUCTION PROGRESS
        // Consume delivered materials from building inventory into
        // active construction projects. Must run BEFORE production
        // execution so construction materials are not accidentally
        // consumed by production logic.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let unit_costs = std::collections::BTreeMap::new();
            let (_msgs, construction_investment) =
                crate::construction::orders::advance_construction_projects(
                    &mut task.ctx.buildings,
                    &mut task.companies,
                    &unit_costs,
                    task.ctx.country,
                );
            // Phase 34: Accumulate materials consumed (cost_spent delta) as
            // GDP investment (I). This is NOT tranche payments — tranches are
            // cash transfers, not capital formation. I only counts physical
            // materials consumed and work performed.
            task.gdp_acc.investment += construction_investment;
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 22A: TRANCHE PAYMENTS
        // Release milestone payments to contractors based on progress.
        // Runs after construction progress so tranches trigger on
        // updated progress values. Uses double-entry settlement.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let _released = crate::construction::orders::release_construction_tranches(
                &mut task.ctx.buildings,
                &mut task.companies,
                task.ctx.country,
            );
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 22B: CONSTRUCTION FRAUD & OHS
        // Material substitution fraud and OHS corner-cutting by
        // contractors. Runs after progress so fraud affects ongoing
        // projects. Defects accumulate on the project struct.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let justice_coverage = task
                .ctx
                .country
                .politics
                .justice_state
                .as_ref()
                .map(|js| js.justice_coverage)
                .unwrap_or(0.0);
            // D.4.7: Dynamic inspection probability — capacity per construction
            // site, not a hardcoded divisor. (Rule 2: no magic numbers)
            let total_construction_sites = task
                .ctx
                .buildings
                .iter()
                .filter(|b| b.active_project.is_some())
                .count() as f64;
            let inspection_prob = task
                .ctx
                .country
                .politics
                .inspectorate_state
                .as_ref()
                .map(|ist| {
                    if total_construction_sites > 0.0 {
                        ist.labor_inspection_capacity / total_construction_sites
                    } else {
                        0.0
                    }
                })
                .unwrap_or(0.0)
                .min(1.0);
            let mut rng = rand::rngs::StdRng::seed_from_u64(
                task.ctx.turn as u64 ^ task.ctx.country.name.len() as u64,
            );
            for building in &mut task.ctx.buildings {
                if let Some(project) = building.active_project.as_mut() {
                    if project.main_contractor_id.is_empty() {
                        continue;
                    }
                    let contractor_rep = task
                        .companies
                        .iter()
                        .find(|c| c.id == project.main_contractor_id)
                        .map(|c| c.reputation_score)
                        .unwrap_or(50.0);
                    // Material fraud
                    let prices_std: std::collections::HashMap<crate::registries::enums::Commodity, f64> =
                        task.ctx.market_prices.iter().map(|(k, v)| (*k, *v)).collect();
                    let _ = crate::construction::fraud::try_material_fraud(
                        project,
                        contractor_rep,
                        justice_coverage,
                        inspection_prob,
                        &mut rng,
                        &prices_std,
                    );
                    // OHS cut
                    let _ = crate::construction::fraud::try_ohs_cut(
                        project,
                        contractor_rep,
                        justice_coverage,
                        inspection_prob,
                        &mut rng,
                    );
                    // Workplace accident check
                    if let Some(casualties) =
                        crate::construction::fraud::check_workplace_accident(project, &mut rng)
                    {
                        project.ohs_accidents += 1;
                        // Phase 24D: Apply casualties to class demographics (not just region population).
                        // Split: 30% dead, 70% disabled (typical OHS accident ratio).
                        let casualties_i = casualties as i64;
                        let dead = (casualties_i as f64 * 0.3).round() as i64;
                        let disabled = casualties_i - dead;
                        let region_id = building.region_id.clone();

                        // Phase 25: OHS compensation — dynamic multiplier, not hardcoded.
                        // compensation = COMPENSATION_WAGE_MULTIPLIER × average_wage × total_casualties
                        // D.1.2: Compensation is DEFERRED until after the labor allocation
                        // matrix is built, so it can be routed to the exact injured worker
                        // class via settle_transfer (Rule 1 + Rule 7). No silent clamping —
                        // the full amount is attempted; insolvency triggers the Syndic.
                        let avg_wage = task.ctx.country.macro_indicators.average_wage;
                        let compensation_per_casualty =
                            avg_wage * crate::construction::fraud::COMPENSATION_WAGE_MULTIPLIER;
                        let total_compensation = compensation_per_casualty * casualties_i as f64;

                        let employer_id = project.main_contractor_id.clone();
                        if total_compensation > 0.0 {
                            task.pending_ohs_compensations.push(PendingOhsCompensation {
                                employer_id: employer_id.clone(),
                                region_id: region_id.clone(),
                                total_compensation,
                            });
                        }

                        // Apply casualties to population immediately (physical effect).
                        if let Some(region) = task
                            .ctx
                            .country
                            .regions
                            .iter_mut()
                            .find(|r| r.id == region_id)
                        {
                            region.population = region.population.saturating_sub(dead);
                            // Apply to both rural and urban classes (construction workers
                            // can be from either demographic).
                            crate::economy::telemetry::apply_casualties_to_labor(
                                region, dead, disabled, true,
                            );
                            crate::economy::telemetry::apply_casualties_to_labor(
                                region, dead, disabled, false,
                            );
                        }
                    }
                }
            }
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 23B: TRANSPORT NETWORK DEGRADATION & MAINTENANCE
        // Network links degrade each turn (physical). Maintenance is
        // funded by the Treasury (double-entry: cash debited, links restored).
        // Phase 25: Strict realism — if no Construction-sector company exists,
        // maintenance CANNOT happen. Links degrade and are NOT repaired. The
        // treasury is NOT debited. The economy suffers the physical consequences.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            // Degrade all network links by 1% per turn.
            crate::economy::transport_networks::degrade_networks(
                &mut task.ctx.country.transport_networks,
                0.01,
            );
            // Phase 25: Only repair if a Construction-sector company exists.
            // If no construction company is available, the work cannot physically
            // happen — links remain degraded.
            let has_construction_company = task
                .companies
                .iter()
                .any(|c| c.sector == Sector::Construction);
            if has_construction_company {
                let spent = crate::economy::transport_networks::process_network_maintenance(
                    &mut task.ctx.country.transport_networks,
                    &mut task.ctx.country.budget.liquid_reserves,
                    1000.0, // repair cost per condition point
                );
                // Phase 25: Credit the first Construction-sector company for
                // the repair work (double-entry: treasury debited, company credited).
                if spent > 0.0 {
                    if let Some(construction_co) = task
                        .companies
                        .iter_mut()
                        .find(|c| c.sector == Sector::Construction)
                    {
                        if let Some(ref mut ba) = construction_co.brokerage_account {
                            ba.cash += spent;
                        } else {
                            construction_co.available_cash += spent;
                        }
                    }
                }
            }
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 15A: WEATHER, CONDITION DEGRADATION, OSP VOLUNTEER ALLOCATION
        // Weather events are generated per-region based on climate + season.
        // Building condition degrades each turn (physical, no money).
        // OSP volunteer FTE is injected into NGO firehouse buildings.
        // All must run BEFORE production so capacity is available for disasters.
        // ═══════════════════════════════════════════════════════════
        let current_season = turn_calendar.get_season();
        let current_turn = turn_calendar.global_turn;
        tasks.par_iter_mut().for_each(|task| {
            crate::economy::weather::process_weather_turn(
                task.ctx.country,
                current_season,
                current_turn,
            );
        });

        tasks.par_iter_mut().for_each(|task| {
            let config = task.ctx.country.maintenance_config.clone();
            crate::economy::maintenance::process_condition_degradation(
                &mut task.ctx.buildings,
                &config,
            );
        });

        // Phase 19B: Degrade fixed-asset cohorts (machinery wear & tear).
        // Runs after building-condition degradation so the building stress
        // factor reflects the post-degradation condition. Scrapped cohorts
        // (condition ≤ 0) are removed to keep the cohort vector compact.
        tasks.par_iter_mut().for_each(|task| {
            let gen_cfg = task.ctx.country.generative_goods_config.clone();
            for building in &mut task.ctx.buildings {
                if building.fixed_assets.is_empty() {
                    continue;
                }
                let _scrapped = crate::economy::fixed_assets::degrade_cohorts(
                    &mut building.fixed_assets,
                    building.condition,
                    &gen_cfg,
                );
                crate::economy::fixed_assets::remove_scrapped(&mut building.fixed_assets);
            }
        });

        tasks.par_iter_mut().for_each(|task| {
            crate::economy::osp::process_osp_volunteer_allocation(
                &task.companies,
                &mut task.ctx.buildings,
                task.ctx.country,
            );
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 8: WAVE-BASED PRODUCTION EXECUTION
        // Wave 1: Energy sector produces Commodity::Energy/Heat from fuel
        // Phase 8.1: Grid distribution (Commodity::Energy → ElectricitySupply)
        // Phase 8.2: Utility consumption (deficits, penalties, billing)
        // Wave 3: General production (with blackout efficiency penalties)
        // Phase 8.3: Waste collection & processing
        // ═══════════════════════════════════════════════════════════

        // Wave 1: Energy production only
        tasks.par_iter_mut().for_each(|task| {
            let b2b_config = task.ctx.country.b2b_order_config.clone();
            let gen_cfg = task.ctx.country.generative_goods_config.clone();
            let _results = execute_production_cycle(
                &mut task.ctx.buildings,
                &mut task.commercial_buildings,
                &mut task.companies,
                &b2b_config,
                Some(Sector::Energy),
                None,
                &gen_cfg,
                task.ctx.year,
            );
        });

        // ── DIAGNOSTIC CHECKPOINT: production_cycle_post ──
        probe.checkpoint("production_cycle_post", 5, turn, &market, &tasks);

        // Phase 8.1: Grid Distribution — Phase 81: New energy grid distribution.
        // Replaces the old distribute_utilities for electricity. The new system
        // performs DC flow balancing, overproduction handling, and load shedding.
        // Water/heat distribution still uses the old distribute_utilities.
        // Phase 81 Wave 2: Clone market_history for PPA VWAP computation.
        let ppa_market_history = state.market_history.clone();
        let grid_penalties_per_task: Vec<std::collections::HashMap<String, f64>> = tasks
            .par_iter_mut()
            .enumerate()
            .map(|(_i, task)| {
                let utility_config = task.ctx.country.utility_config.clone();
                // Phase 81 Wave 2: Build fuel prices from market prices for
                // merit-order spot market clearing.
                let fuel_prices: std::collections::HashMap<Commodity, f64> = task
                    .ctx
                    .market_prices
                    .iter()
                    .map(|(&k, &v)| (k, v))
                    .collect();
                // Phase 81 Wave 2: PPA negotiation — bilateral long-term contracts
                // between generators and industrial consumers. Runs before grid
                // distribution so PPA-contracted MW is reserved.
                let global_base_price = task
                    .ctx
                    .market_prices
                    .get(&Commodity::Energy)
                    .copied()
                    .unwrap_or(100.0);
                let avg_wage = task
                    .ctx
                    .country
                    .macro_indicators
                    .average_wage
                    .max(subsistence_wage_floor);
                crate::energy::ppa::negotiate_ppas(
                    task.ctx.country,
                    &task.ctx.buildings,
                    &fuel_prices,
                    avg_wage,
                    &ppa_market_history,
                    global_base_price,
                    current_turn,
                );
                // Phase 81: New energy grid distribution (electricity only).
                let grid_result = crate::energy::grid::distribute_grid_power(
                    task.ctx.country,
                    &mut task.ctx.buildings,
                    &task.housing_buildings,
                    &task.commercial_buildings,
                    current_season,
                    &fuel_prices,
                );
                // Phase 81: Degrade grid condition based on load factor.
                crate::energy::grid::degrade_grid_condition(
                    &mut task.ctx.country.power_grid_state,
                    &grid_result.interconnector_flows,
                );
                // Phase 80: Still distribute water/heat via the old system.
                let _dist_result = crate::utilities::grid::distribute_utilities(
                    &mut task.ctx.country.regions,
                    &mut task.ctx.buildings,
                    &mut task.housing_buildings,
                    &mut task.commercial_buildings,
                    &utility_config,
                    current_season,
                );
                grid_result.building_efficiency_penalties
            })
            .collect();

        // Phase 8.2: Utility Consumption — calculate deficits, penalties, billing
        // Collect efficiency penalties per task for Wave 3.
        // Phase 81: Merge grid penalties (load shedding + industrial buff) with
        // utility consumption penalties (water/sewage). Grid penalties take
        // precedence for electricity-related buildings.
        let task_penalties: Vec<std::collections::HashMap<String, f64>>;
        {
            let mut penalties_per_task: Vec<std::collections::HashMap<String, f64>> = Vec::new();
            tasks.iter_mut().enumerate().for_each(|(task_idx, task)| {
                let utility_config = task.ctx.country.utility_config.clone();
                let pricing_config = task.ctx.country.utility_pricing_config.clone();
                let result = crate::utilities::consumption::process_utility_consumption(
                    &mut task.ctx.country.regions,
                    &task.housing_buildings,
                    &task.commercial_buildings,
                    &mut task.companies,
                    &utility_config,
                    &pricing_config,
                    current_season,
                );
                // Phase 81: Merge grid penalties with utility consumption penalties.
                let mut merged = result.building_efficiency_penalties;
                if let Some(grid_pens) = grid_penalties_per_task.get(task_idx) {
                    for (building_id, penalty) in grid_pens {
                        // Grid penalties (load shedding = positive, industrial buff = negative)
                        // take precedence over utility consumption penalties.
                        let existing = merged.get(building_id).copied().unwrap_or(0.0);
                        // For load shedding (positive), take the max (worst case).
                        // For industrial buff (negative), it overrides any positive penalty
                        // since the buff means the building has surplus energy.
                        if *penalty < 0.0 {
                            merged.insert(building_id.clone(), *penalty);
                        } else {
                            merged.insert(building_id.clone(), existing.max(*penalty));
                        }
                    }
                }
                penalties_per_task.push(merged);
            });
            task_penalties = penalties_per_task;
        }

        // Phase 82: Thermal grid pipe degradation + smog computation.
        // Pipes degrade each turn (faster in winter). Smog is computed from
        // emissions and accumulates with natural decay. Smog is distributed
        // to cadastre parcels as particulate pollution.
        tasks.par_iter_mut().for_each(|task| {
            let winter_severity = match current_season {
                crate::state::Season::Winter => 2.0,
                crate::state::Season::Autumn => 1.0,
                crate::state::Season::Spring => 0.5,
                crate::state::Season::Summer => 0.0,
            };
            for region in &mut task.ctx.country.regions {
                // Degrade thermal grid pipes
                region.thermal_grid.degrade(winter_severity);

                // Compute smog for this region.
                // Emissions sources:
                // - standalone_emissions: from housing/commercial heating (coal stoves, etc.)
                // - centralized_emissions: from heating plants and power plants
                // - industrial_emissions: from heavy industry production
                // For now, use the per-turn emission breakdown stored in local_pollution.
                // The actual emission computation happens during production and consumption
                // processing; here we just accumulate and decay.
                let area_km2 = region.land_use_inventory.total_area / 100.0;
                let standalone = region.local_pollution.standalone_emissions;
                let centralized = region.local_pollution.centralized_emissions;
                let industrial = region.local_pollution.industrial_emissions;
                crate::environment::smog::compute_smog_for_region(
                    &mut region.local_pollution,
                    standalone,
                    centralized,
                    industrial,
                    area_km2,
                );

                // Distribute smog to cadastre parcels
                crate::environment::smog::distribute_smog_to_parcels(
                    &mut task.ctx.country.cadastre,
                    &region.id,
                    region.local_pollution.smog_level,
                );

                // ═══════════════════════════════════════════════════════════
                // PHASE 83: HYDRO GRID — Water reserve regeneration (Step 16)
                // PARADIGM SHIFT: Groundwater and surface water regenerate
                // naturally, with quality drift toward natural baselines
                // (groundwater → 0.9, surface → 0.6).
                // ═══════════════════════════════════════════════════════════
                let aquifer_capacity = region.water_reserves.groundwater_volume
                    + region.water_reserves.groundwater_regen_rate * 100.0;
                region.water_reserves.regenerate(aquifer_capacity);

                // ═══════════════════════════════════════════════════════════
                // PHASE 83: HYDRO GRID — Pipe degradation (Step 15)
                // Water and sewer pipes degrade over time, increasing
                // leakage and transmission losses.
                // ═══════════════════════════════════════════════════════════
                region.water_network.degrade(winter_severity);
                region.sewer_network.degrade();

                // ═══════════════════════════════════════════════════════════
                // PHASE 83: BIOHAZARD COMPUTATION (Step 11)
                // PARADIGM SHIFT: Biological pollution from standalone
                // sanitation, sewer overflow, industrial wastewater, and
                // low-quality water consumption. Distinct from smog.
                // ═══════════════════════════════════════════════════════════
                let area_km2 = region.land_use_inventory.total_area / 100.0;
                let standalone_bio = region.local_pollution.standalone_biohazard;
                let sewage_overflow_bio = region.local_pollution.sewage_overflow_biohazard;
                let industrial_bio = region.local_pollution.industrial_biohazard;
                crate::environment::smog::compute_biohazard_for_region(
                    &mut region.local_pollution,
                    standalone_bio,
                    sewage_overflow_bio,
                    industrial_bio,
                    &[], // Building receipts populated by consumption track
                    area_km2,
                );

                // Distribute biohazard to cadastre parcels
                crate::environment::smog::distribute_biohazard_to_parcels(
                    &mut task.ctx.country.cadastre,
                    &region.id,
                    region.local_pollution.biohazard_level,
                );
            }
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 82/83: MUNICIPAL AI — Heating & Infrastructure Investment
        // Runs AFTER utility pricing, thermal grid degradation, smog/biohazard
        // computation, and water reserve regeneration (Rule 16: Temporal
        // Causality). The AI observes current demand/supply and crises, then
        // produces investment plans for the NEXT turn's construction system.
        //
        // This is a SEQUENTIAL per-country pass (not parallel) because it
        // writes to country.municipal_infrastructure_plan and may debit
        // budget.liquid_reserves for funded crisis CAPEX.
        // ═══════════════════════════════════════════════════════════
        for task in &mut tasks {
            let avg_wage = task
                .ctx
                .country
                .macro_indicators
                .average_wage
                .max(subsistence_wage_floor);
            // Mortality cost per death: dynamic multiple of average wage (Rule 2).
            let mortality_cost_per_death = avg_wage * 50.0;
            // Municipal budget allocation: a fraction of liquid reserves.
            // Use 10% of liquid reserves as the infrastructure budget pool.
            let available_budget = task.ctx.country.budget.liquid_reserves * 0.10;
            let financing_available = task.ctx.country.budget.liquid_reserves > 0.0;

            // Build plant cost data for heating AI (all types unlocked by default;
            // the construction system / tech tree will gate actual builds).
            let heating_plant_costs: Vec<crate::energy::municipal_heating_ai::PlantTypeCostData> = {
                use crate::energy::heating_types::HeatingPlantType;
                vec![
                    crate::energy::municipal_heating_ai::PlantTypeCostData {
                        plant_type: HeatingPlantType::WoodBoiler,
                        fuel_opex_per_gj: 3.0,
                        maintenance_opex_per_gj: 0.5,
                        capex_per_gj: 10.0,
                        tech_unlocked: true,
                        geologically_eligible: true,
                    },
                    crate::energy::municipal_heating_ai::PlantTypeCostData {
                        plant_type: HeatingPlantType::CoalHeatPlant,
                        fuel_opex_per_gj: 2.0,
                        maintenance_opex_per_gj: 0.5,
                        capex_per_gj: 15.0,
                        tech_unlocked: true,
                        geologically_eligible: true,
                    },
                    crate::energy::municipal_heating_ai::PlantTypeCostData {
                        plant_type: HeatingPlantType::LigniteHeatPlant,
                        fuel_opex_per_gj: 2.5,
                        maintenance_opex_per_gj: 0.5,
                        capex_per_gj: 12.0,
                        tech_unlocked: true,
                        geologically_eligible: true,
                    },
                    crate::energy::municipal_heating_ai::PlantTypeCostData {
                        plant_type: HeatingPlantType::NaturalGasHeatPlant,
                        fuel_opex_per_gj: 1.5,
                        maintenance_opex_per_gj: 0.3,
                        capex_per_gj: 20.0,
                        tech_unlocked: true,
                        geologically_eligible: true,
                    },
                    crate::energy::municipal_heating_ai::PlantTypeCostData {
                        plant_type: HeatingPlantType::GeothermalHeatPlant,
                        fuel_opex_per_gj: 0.1,
                        maintenance_opex_per_gj: 0.3,
                        capex_per_gj: 50.0,
                        tech_unlocked: true,
                        geologically_eligible: false, // No geothermal trait by default
                    },
                ]
            };

            // Build plant cost data for water AI.
            let water_plant_costs: Vec<
                crate::energy::municipal_infrastructure_ai::WaterPlantCostData,
            > = {
                use crate::utilities::hydro_types::WaterPlantType;
                vec![
                    crate::energy::municipal_infrastructure_ai::WaterPlantCostData {
                        plant_type: WaterPlantType::SlowSandFilter,
                        opex_per_liter: 0.0001,
                        capex_per_liter: 0.5,
                        output_water_quality: 0.95,
                        tech_unlocked: true,
                        geologically_eligible: true,
                    },
                    crate::energy::municipal_infrastructure_ai::WaterPlantCostData {
                        plant_type: WaterPlantType::ModernTreatmentPlant,
                        opex_per_liter: 0.0005,
                        capex_per_liter: 2.0,
                        output_water_quality: 0.99,
                        tech_unlocked: true,
                        geologically_eligible: true,
                    },
                    crate::energy::municipal_infrastructure_ai::WaterPlantCostData {
                        plant_type: WaterPlantType::DesalinationPlant,
                        opex_per_liter: 0.004,
                        capex_per_liter: 5.0,
                        output_water_quality: 0.99,
                        tech_unlocked: true,
                        geologically_eligible: false, // No coastal access by default
                    },
                ]
            };

            // Build plant cost data for wastewater (sanitation) AI.
            let wastewater_plant_costs: Vec<
                crate::energy::municipal_infrastructure_ai::WastewaterPlantCostData,
            > = {
                use crate::utilities::hydro_types::WastewaterPlantType;
                vec![
                    crate::energy::municipal_infrastructure_ai::WastewaterPlantCostData {
                        plant_type: WastewaterPlantType::PrimarySettling,
                        opex_per_liter: 0.0002,
                        capex_per_liter: 0.8,
                        discharge_quality: 0.30,
                        tech_unlocked: true,
                    },
                    crate::energy::municipal_infrastructure_ai::WastewaterPlantCostData {
                        plant_type: WastewaterPlantType::AdvancedWastewaterPlant,
                        opex_per_liter: 0.001,
                        capex_per_liter: 3.0,
                        discharge_quality: 0.85,
                        tech_unlocked: true,
                    },
                ]
            };

            // Aggregate per-region plans into country-level plans.
            // For countries with multiple regions, we sum CAPEX and take the
            // highest-priority (crisis) plan as the representative.
            let mut combined_thermal =
                crate::energy::municipal_heating_ai::HeatingInvestmentPlan::default();
            let mut combined_water =
                crate::energy::municipal_infrastructure_ai::WaterInvestmentPlan::default();
            let mut combined_sanitation =
                crate::energy::municipal_infrastructure_ai::SanitationInvestmentPlan::default();
            let mut combined_waste =
                crate::energy::municipal_infrastructure_ai::WasteInvestmentPlan::default();
            let mut combined_electrical =
                crate::energy::municipal_infrastructure_ai::ElectricalInvestmentPlan::default();

            for region in &task.ctx.country.regions {
                // --- Heating AI ---
                // Derive district heating demand from urban population.
                // Approximate buildings wanting DH from urban class population.
                let urban_pop: i64 = region
                    .class_demographics
                    .urban_classes
                    .values()
                    .map(|d| d.population)
                    .sum();
                let district_heating_demand = (urban_pop / 5).max(0) as usize; // ~5 people per building

                // Effective heat supply from thermal grid throughput.
                // Use throughput as raw supply; with 1 active plant as default.
                let raw_heat_supply = region.thermal_grid.pipe_network_km * 50.0; // ~50 GJ per km of pipe
                let effective_heat_supply = region
                    .thermal_grid
                    .effective_heat_supply(raw_heat_supply, 1);
                let heat_demand_gj = urban_pop as f64 * 0.5; // ~0.5 GJ per person per turn

                // Estimated deaths from smog: scale smog level by population.
                let estimated_deaths_from_smog =
                    region.local_pollution.smog_level * urban_pop as f64 * 0.001;

                let thermal_plan = crate::energy::municipal_heating_ai::run_municipal_heating_ai(
                    &region.thermal_grid,
                    district_heating_demand,
                    effective_heat_supply,
                    heat_demand_gj,
                    &heating_plant_costs,
                    avg_wage,
                    mortality_cost_per_death,
                    estimated_deaths_from_smog,
                    financing_available,
                );

                // Accumulate thermal plan
                combined_thermal.estimated_capex += thermal_plan.estimated_capex;
                combined_thermal.expected_mortality_reduction_value +=
                    thermal_plan.expected_mortality_reduction_value;
                combined_thermal.pipe_expansion_km += thermal_plan.pipe_expansion_km;
                if thermal_plan.new_plant_type.is_some() {
                    combined_thermal.new_plant_type = thermal_plan.new_plant_type;
                }
                if thermal_plan.passes_cost_benefit_gate {
                    combined_thermal.passes_cost_benefit_gate = true;
                }
                if !thermal_plan.rationale.is_empty() {
                    if !combined_thermal.rationale.is_empty() {
                        combined_thermal.rationale.push_str("; ");
                    }
                    combined_thermal
                        .rationale
                        .push_str(&format!("[{}] {}", region.id, thermal_plan.rationale));
                }

                // --- Water AI ---
                let buildings_wanting_mains = district_heating_demand; // Same building count approximation
                let treatment_throughput = region.water_network.throughput_liters;
                let water_demand = urban_pop as f64 * 200.0; // ~200 liters per person per turn
                let estimated_deaths_from_water_quality = if region.water_network.current_quality
                    < 0.9
                    && region.water_network.current_quality > 0.0
                {
                    urban_pop as f64 * (1.0 - region.water_network.current_quality) * 0.01
                } else {
                    0.0
                };
                let dehydration_mortality = region.winter_mortality_multiplier; // Reuse as proxy

                let water_plan =
                    crate::energy::municipal_infrastructure_ai::run_water_investment_ai(
                        &region.water_network,
                        &region.water_reserves,
                        buildings_wanting_mains,
                        treatment_throughput,
                        water_demand,
                        &water_plant_costs,
                        avg_wage,
                        mortality_cost_per_death,
                        estimated_deaths_from_water_quality,
                        dehydration_mortality,
                        financing_available,
                    );

                combined_water.estimated_capex += water_plan.estimated_capex;
                combined_water.expected_mortality_reduction_value +=
                    water_plan.expected_mortality_reduction_value;
                combined_water.expand_pipes_km += water_plan.expand_pipes_km;
                if water_plan.build_treatment_plant.is_some() {
                    combined_water.build_treatment_plant = water_plan.build_treatment_plant;
                }
                if water_plan.is_crisis {
                    combined_water.is_crisis = true;
                }
                if water_plan.passes_cost_benefit_gate {
                    combined_water.passes_cost_benefit_gate = true;
                }
                if !water_plan.rationale.is_empty() {
                    if !combined_water.rationale.is_empty() {
                        combined_water.rationale.push_str("; ");
                    }
                    combined_water
                        .rationale
                        .push_str(&format!("[{}] {}", region.id, water_plan.rationale));
                }

                // --- Sanitation AI ---
                let buildings_wanting_sewers = district_heating_demand;
                let sewer_throughput = region.sewer_network.throughput_liters;
                let wastewater_treatment_capacity = region.water_network.throughput_liters * 0.5; // Approximate
                let biohazard_level = region.local_pollution.biohazard_level;
                let estimated_deaths_from_biohazard = biohazard_level * urban_pop as f64 * 0.001;
                let industrial_corrosion_cost = 0.0; // Simplified; no industrial corrosion data at this phase

                let sanitation_plan =
                    crate::energy::municipal_infrastructure_ai::run_sanitation_investment_ai(
                        &region.sewer_network,
                        &region.water_reserves,
                        buildings_wanting_sewers,
                        sewer_throughput,
                        wastewater_treatment_capacity,
                        biohazard_level,
                        &wastewater_plant_costs,
                        avg_wage,
                        mortality_cost_per_death,
                        estimated_deaths_from_biohazard,
                        industrial_corrosion_cost,
                        financing_available,
                    );

                combined_sanitation.estimated_capex += sanitation_plan.estimated_capex;
                combined_sanitation.expected_mortality_reduction_value +=
                    sanitation_plan.expected_mortality_reduction_value;
                combined_sanitation.expand_sewers_km += sanitation_plan.expand_sewers_km;
                if sanitation_plan.build_wastewater_plant.is_some() {
                    combined_sanitation.build_wastewater_plant =
                        sanitation_plan.build_wastewater_plant;
                }
                if sanitation_plan.is_crisis {
                    combined_sanitation.is_crisis = true;
                }
                if sanitation_plan.passes_cost_benefit_gate {
                    combined_sanitation.passes_cost_benefit_gate = true;
                }
                if !sanitation_plan.rationale.is_empty() {
                    if !combined_sanitation.rationale.is_empty() {
                        combined_sanitation.rationale.push_str("; ");
                    }
                    combined_sanitation
                        .rationale
                        .push_str(&format!("[{}] {}", region.id, sanitation_plan.rationale));
                }

                // --- Waste AI (Phase 84) ---
                // Aggregate waste grid state into a waste investment plan.
                let uncollected_waste_mass = region.waste_grid.total_uncollected();
                let landfill_utilization = region.waste_grid.landfill_utilization;
                let waste_crisis =
                    landfill_utilization >= 1.0 || uncollected_waste_mass > avg_wage * 100.0;
                let waste_plan = crate::energy::municipal_infrastructure_ai::WasteInvestmentPlan {
                    expand_collection_routes_km: if uncollected_waste_mass > 0.0 {
                        5.0
                    } else {
                        0.0
                    },
                    build_waste_plant: if landfill_utilization >= 0.9 {
                        Some(crate::utilities::waste_grid::WastePlantType::ControlledLandfill)
                    } else {
                        None
                    },
                    estimated_capex: if uncollected_waste_mass > 0.0 {
                        5.0 * avg_wage * 500.0
                    } else {
                        0.0
                    },
                    expected_mortality_reduction_value: uncollected_waste_mass
                        * mortality_cost_per_death
                        * 0.01,
                    landfill_utilization,
                    uncollected_waste_mass,
                    passes_cost_benefit_gate: financing_available && uncollected_waste_mass > 0.0,
                    is_crisis: waste_crisis,
                    rationale: if uncollected_waste_mass > 0.0 {
                        format!(
                            "[{}] Uncollected waste: {:.1} tons, landfill util: {:.2}",
                            region.id, uncollected_waste_mass, landfill_utilization
                        )
                    } else {
                        String::new()
                    },
                };

                combined_waste.estimated_capex += waste_plan.estimated_capex;
                combined_waste.expected_mortality_reduction_value +=
                    waste_plan.expected_mortality_reduction_value;
                combined_waste.expand_collection_routes_km +=
                    waste_plan.expand_collection_routes_km;
                if waste_plan.build_waste_plant.is_some() {
                    combined_waste.build_waste_plant = waste_plan.build_waste_plant;
                }
                if waste_plan.is_crisis {
                    combined_waste.is_crisis = true;
                }
                if waste_plan.passes_cost_benefit_gate {
                    combined_waste.passes_cost_benefit_gate = true;
                }
                if !waste_plan.rationale.is_empty() {
                    if !combined_waste.rationale.is_empty() {
                        combined_waste.rationale.push_str("; ");
                    }
                    combined_waste.rationale.push_str(&waste_plan.rationale);
                }
            }

            // --- Electrical AI (simplified — no separate electrical AI function) ---
            // The electrical investment plan is derived from power grid state.
            // For now, use a basic deficit check.
            combined_electrical.is_crisis = false;
            combined_electrical.passes_cost_benefit_gate = false;

            // --- Unified Municipal AI: combine all domain plans ---
            let unified_plan = crate::energy::municipal_infrastructure_ai::run_unified_municipal_ai(
                combined_thermal,
                combined_electrical,
                combined_water,
                combined_sanitation,
                combined_waste,
                available_budget,
            );

            // Store the plan on the country for the construction system.
            task.ctx.country.municipal_infrastructure_plan = unified_plan;
        }

        // Phase 44: Residential Rent Collection — double-entry transfer.
        // Debit occupying class savings, credit owner entity (State treasury or class savings).
        // No money creation or destruction.
        tasks.par_iter_mut().for_each(|task| {
            for hb in &task.housing_buildings {
                let occupied = hb.primary_slots.occupied_slots as f64;
                let rent_per_slot = hb.primary_slots.rent_per_slot;
                if occupied <= 0.0 || rent_per_slot <= 0.0 {
                    continue;
                }
                let total_rent = occupied * rent_per_slot;
                let owner = &hb.owner;

                // Determine which region this housing belongs to
                let region = task.ctx.country.regions.iter_mut().find(|r| {
                    r.id == hb.micro_region_id || r.micro_regions.contains_key(&hb.micro_region_id)
                });
                if region.is_none() {
                    continue;
                }
                let region = region.unwrap();

                // Debit rent from the occupying class's savings.
                // Distribute the debit proportionally across all classes in the region
                // based on their population share (simplified model).
                let total_class_pop: i64 = region
                    .class_demographics
                    .rural_classes
                    .values()
                    .map(|d| d.population)
                    .sum::<i64>()
                    + region
                        .class_demographics
                        .urban_classes
                        .values()
                        .map(|d| d.population)
                        .sum::<i64>();
                if total_class_pop <= 0 {
                    continue;
                }

                let debit_per_capita = total_rent / total_class_pop as f64;
                // Phase 94: Track actual debited amount — debits are clamped
                // to .max(0.0) but the credit must match the ACTUAL debit,
                // not the theoretical total_rent. Otherwise money is created
                // when citizens can't afford the full rent.
                let mut actual_rent_debited = 0.0;
                for demographics in region.class_demographics.rural_classes.values_mut() {
                    let owed = debit_per_capita * demographics.population as f64;
                    let before = demographics.savings;
                    demographics.savings = (demographics.savings - owed).max(0.0);
                    actual_rent_debited += before - demographics.savings;
                }
                for demographics in region.class_demographics.urban_classes.values_mut() {
                    let owed = debit_per_capita * demographics.population as f64;
                    let before = demographics.savings;
                    demographics.savings = (demographics.savings - owed).max(0.0);
                    actual_rent_debited += before - demographics.savings;
                }

                // Credit ACTUAL debited rent to the owner entity (Rule 1).
                if actual_rent_debited <= 0.0 {
                    continue;
                }
                if owner.starts_with("STATE:") {
                    task.ctx.country.budget.liquid_reserves += actual_rent_debited;
                } else if owner.starts_with("CLASS:Aristocracy:") {
                    if let Some(d) = region
                        .class_demographics
                        .rural_classes
                        .get_mut(&RuralClass::Aristocracy)
                    {
                        d.savings += actual_rent_debited;
                    }
                } else if owner.starts_with("CLASS:Bourgeoisie:") {
                    if let Some(d) = region
                        .class_demographics
                        .urban_classes
                        .get_mut(&UrbanClass::Bourgeoisie)
                    {
                        d.savings += actual_rent_debited;
                    }
                }
                // Unknown owner prefixes: rent is lost (should not happen with genesis housing).
            }
        });

        // Wave 3: General production (all non-energy sectors, with blackout penalties)
        tasks.par_iter_mut().enumerate().for_each(|(i, task)| {
            let b2b_config = task.ctx.country.b2b_order_config.clone();
            let gen_cfg = task.ctx.country.generative_goods_config.clone();
            let blackout_penalties = &task_penalties[i];
            // Phase 40: Merge blackout penalties with company productivity penalties
            // from wage arrears. Buildings owned by companies with arrears produce less.
            let mut merged_penalties: HashMap<String, f64> = blackout_penalties.clone();
            for company in &task.companies {
                // Phase 41: Striking companies have 100% production penalty (0 output).
                if company.is_striking {
                    for building in &task.ctx.buildings {
                        if building.owner_id == company.id {
                            merged_penalties.insert(
                                building.id.clone(),
                                1.0, // 100% production penalty
                            );
                        }
                    }
                } else if company.productivity_penalty > 0.0 {
                    for building in &task.ctx.buildings {
                        if building.owner_id == company.id {
                            merged_penalties.insert(
                                building.id.clone(),
                                merged_penalties.get(&building.id).copied().unwrap_or(0.0)
                                    + company.productivity_penalty,
                            );
                        }
                    }
                }
            }
            let _results = execute_production_cycle(
                &mut task.ctx.buildings,
                &mut task.commercial_buildings,
                &mut task.companies,
                &b2b_config,
                None,
                Some(&merged_penalties),
                &gen_cfg,
                task.ctx.year,
            );

            // Phase 29: Aggregate per-region overflow fees for ROI-driven
            // warehouse construction decisions by logistics companies.
            let mut regional_overflow: std::collections::BTreeMap<String, f64> =
                std::collections::BTreeMap::new();
            for building in &task.ctx.buildings {
                if let Some(costs) = building
                    .extra
                    .get("overflow_costs_this_turn")
                    .and_then(|v| v.as_f64())
                {
                    if costs > 0.0 {
                        *regional_overflow
                            .entry(building.region_id.clone())
                            .or_insert(0.0) += costs;
                    }
                }
            }
            task.ctx.country.regional_overflow_fees = regional_overflow;
        });

        // Phase 8.3: Waste Collection & Processing (legacy Phase 8 system)
        tasks.par_iter_mut().for_each(|task| {
            let _waste_result = crate::utilities::waste_collection::process_waste_turn(
                &mut task.ctx.country.regions,
                &mut task.ctx.buildings,
                &mut task.companies,
                &task.housing_buildings,
                &task.commercial_buildings,
                current_season,
            );
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 84: WASTE EPIC — Solid Waste Management & Circular Economy
        // 10-step waste processing (W.1–W.10), runs after consumption,
        // before mortality. Mass-conserved waste generation from actual
        // consumption receipts. Trash streams B2B-excluded. WtE outputs
        // ash. Landfills have hard capacity stop. Dual fee billing.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            crate::utilities::waste_grid::process_waste_epic_turn(
                &mut task.ctx.country.regions,
                &mut task.ctx.buildings,
                &task.housing_buildings,
                &task.commercial_buildings,
                current_season,
            );
        });

        // Phase 25: Restock retail stores from producer buildings.
        // After production, transfer a portion of output goods from producer
        // buildings to retail stores in the same region. This is a simplified
        // wholesale mechanism — in a full implementation, wholesalers would
        // buy via B2B and distribute to stores. Here we simulate the physical
        // flow of goods from factories to retail shelves.
        // Phase 76: Clone market_history for dynamic acquisition_cost pricing.
        let restock_market_history = state.market_history.clone();
        tasks.par_iter_mut().for_each(|task| {
            use crate::registries::enums::Commodity;
            use crate::society::housing::{CommercialBuildingType, InventoryBatch};

            // Phase 76: Consumer goods that retail stores should stock.
            // Expanded from the previous 6-item list to cover all B2C-relevant
            // commodities so that retail stores actually receive inventory
            // for goods like Fruit, LuxuryClothing, etc.
            let consumer_goods: Vec<Commodity> = vec![
                Commodity::Food,
                Commodity::Cereal,
                Commodity::Vegetable,
                Commodity::Meat,
                Commodity::Fruit,
                Commodity::Clothing,
                Commodity::Furniture,
                Commodity::Televisions,
                Commodity::Radio,
                Commodity::Agd,
                Commodity::Cars,
                Commodity::Luxury,
                Commodity::LuxuryClothing,
                Commodity::LuxuryFurniture,
                Commodity::Fish,
                Commodity::Livestock,
            ];

            // Build a map: region_id → list of (building_idx, commodity, qty)
            // for producer buildings with surplus output goods
            let mut surplus_by_region: HashMap<String, Vec<(usize, Commodity, f64)>> =
                HashMap::new();
            for (i, b) in task.ctx.buildings.iter().enumerate() {
                for &commodity in &consumer_goods {
                    let qty = b.inventory.get(&commodity).copied().unwrap_or(0.0);
                    if qty > 10.0 {
                        surplus_by_region
                            .entry(b.region_id.clone())
                            .or_default()
                            .push((i, commodity, qty));
                    }
                }
            }

            // For each retail store, restock from nearby producers
            for store in &mut task.commercial_buildings {
                if store.retail_profile.is_none() {
                    continue;
                }
                if store.building_type != CommercialBuildingType::RetailStore
                    && store.building_type != CommercialBuildingType::DepartmentStore
                    && store.building_type != CommercialBuildingType::ShoppingCenter
                {
                    continue;
                }

                let region_id = &store.micro_region_id;
                let surpluses = match surplus_by_region.get(region_id) {
                    Some(s) => s,
                    None => continue,
                };

                for &(building_idx, commodity, qty) in surpluses {
                    // Transfer 30% of surplus to the retail store
                    let transfer_qty = (qty * 0.3).min(qty);
                    if transfer_qty <= 0.0 {
                        continue;
                    }

                    // Deduct from producer building
                    let remaining = (qty - transfer_qty).max(0.0);
                    if remaining > 0.0 {
                        task.ctx.buildings[building_idx]
                            .inventory
                            .insert(commodity, remaining);
                    } else {
                        task.ctx.buildings[building_idx]
                            .inventory
                            .remove(&commodity);
                    }

                    // Add to retail store inventory
                    let key: String = commodity.into();
                    // Phase 76: Use dynamic reference price instead of hardcoded 100.0.
                    // Falls back to 100.0 only if no market history exists (should not
                    // happen after Phase 76 generator fix that seeds global_base_prices).
                    let ref_price =
                        market_history::get_reference_price(&commodity, &restock_market_history)
                            .unwrap_or(100.0);
                    let batch = InventoryBatch {
                        quantity: transfer_qty,
                        storage_turn: turn,
                        owner_id: store.owner_id.clone(),
                        accumulated_fees: 0.0,
                        warehouse_id: store.id.clone(),
                        fire_sale_discount: 0.0,
                        acquisition_cost_per_unit: ref_price,
                    };
                    store.current_inventory.entry(key).or_default().push(batch);
                }
            }
        });

        // Update market history with VWAP
        market_history::update_vwap(&mut state.market_history, &all_trades);
        // Phase 79: Update rolling VWAP history for SRA shock-responsive triggers.
        market_history::update_vwap_history(
            &mut state.market_history,
            market_history::VWAP_HISTORY_WINDOW,
        );

        // Phase 81 Wave 2: Expire PPAs that have reached their end turn.
        // Runs after market history update so the next turn's PPA negotiation
        // has access to the latest VWAP data.
        tasks.par_iter_mut().for_each(|task| {
            crate::energy::ppa::expire_ppas(task.ctx.country, current_turn);
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 69-C: EXPIRED DECREE CLEANUP
        // After production, remove expired production decrees and
        // restore original production methods on affected buildings.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            process_expired_decrees(
                &mut task.ctx.buildings,
                &mut task.ctx.country.war_economy,
                task.ctx.turn,
            );
        });

        // Phase 6.5: Agricultural sub-sequence (Phase 6.3.5)
        tasks.par_iter_mut().for_each(|task| {
            for company in &mut task.companies {
                crate::agriculture::transition_agricultural_states(
                    company,
                    turn_calendar,
                    task.ctx.registries,
                    &mut task.ctx.buildings,
                );
            }
        });
        tasks.par_iter_mut().for_each(|task| {
            for company in &mut task.companies {
                crate::agriculture::calculate_agricultural_fte_demand(
                    company,
                    turn_calendar,
                    task.ctx.registries,
                );
            }
        });

        // Phase 47: Apply seasonal furlough AFTER agricultural FTE demand
        // and BEFORE set_wage_offers. This reduces target_fte_demand and
        // fulfilled_fte to standby levels for off-season companies, and
        // holds furloughed workers in furloughed_workers_count so they
        // don't flood the labor market.
        //
        // Emergency Stabilization: Furlough re-instatement and attrition run
        // BEFORE seasonal furlough and BEFORE production so re-instated workers
        // can participate in this turn's production cycle.
        tasks.par_iter_mut().for_each(|task| {
            // 1. Re-instate furloughed workers when conditions improve
            crate::corporate::process_furlough_reinstatement(
                &mut task.companies,
                &task.ctx.buildings,
            );
            // 2. Attrition: workers quit after prolonged unpaid furlough
            crate::corporate::process_furlough_attrition(&mut task.companies);
            // 3. Seasonal furlough (existing)
            crate::corporate::apply_seasonal_furlough_all(&mut task.companies, current_season);
        });

        // Phase 25: Set wage offers AFTER agricultural FTE demand is computed
        // and AFTER B2B trades have settled. This ensures:
        // 1. target_fte_demand is correct (agriculture may have updated it)
        // 2. brokerage_account.cash reflects actual post-B2B remaining cash
        // 3. The labor clearing's affordability check matches the wage offer
        tasks.par_iter_mut().for_each(|task| {
            let market_avg_wage = task.ctx.country.macro_indicators.average_wage;
            crate::corporate::set_wage_offers(&mut task.companies, market_avg_wage);
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 23C: COMMUTING & PASSENGER TRANSPORT B2C
        // Build commute map, clear PassengerTransport B2C for commuters.
        // Public (JST) operators are subsidized; private charge market price.
        // Coverage ratio determines how many workers can commute to
        // adjacent regions for jobs.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let commuting_config = task.ctx.country.commuting_config.clone();
            let network_overlay = task.ctx.country.transport_networks.clone();
            let regions_snapshot = task.ctx.country.regions.clone();
            let service_config = task.ctx.country.service_pricing_config.clone();

            // Build commute map for all adjacent region pairs.
            let _commute_map = crate::economy::commuting::build_commute_map(
                &regions_snapshot,
                &network_overlay,
                &commuting_config,
            );

            // Populate commute service needs (PassengerTransport demand per region).
            let commute_needs = crate::economy::b2c_services::populate_commute_service_needs(
                task.ctx.country,
                commuting_config.capacity_per_km * commuting_config.commute_frequency,
            );

            // Clear PassengerTransport B2C market.
            let coverage = crate::economy::b2c_services::clear_passenger_transport_b2c(
                &mut task.ctx.buildings,
                &mut task.companies,
                task.ctx.country,
                &commute_needs,
                &service_config,
            );
            task.commute_coverage = coverage;
        });

        // Phase 89: Reset accumulated_pit before labor clearing (W1).
        // PIT is accumulated during labor clearing and read by tax collection (Phase 7).
        tasks.par_iter_mut().for_each(|task| {
            task.ctx.country.accumulated_pit = 0.0;
        });

        // W1: Wage payment (labor market resolution) with PIT withholding (Fix 1.22)
        // Phase 18B: Compute garnishment rates from community service cohorts
        // Phase 23C: Inject commuter FTE based on PassengerTransport coverage.
        // Phase 25: Set wage offers BEFORE labor clearing (fixes 100% unemployment).
        // Phase 28: Inject State Employer pseudo-company so state buildings
        // (police, military, courts) participate in labor clearing. State wages
        // are funded from the treasury and accumulate to GDP government spending (G).
        tasks.par_iter_mut().for_each(|task| {
            // Phase 28: Create State Employer pseudo-company.
            // Aggregates all state buildings' worker_capacity as labor demand.
            // Funded from country.budget.liquid_reserves (treasury payroll).
            task.state_employer_idx = {
                let state_buildings_capacity: u32 = task
                    .ctx
                    .buildings
                    .iter()
                    .filter(|b| b.owner_id == "State")
                    .map(|b| b.worker_capacity)
                    .sum();
                if state_buildings_capacity == 0 {
                    None
                } else {
                    let base_wage = task
                        .ctx
                        .country
                        .macro_indicators
                        .average_wage
                        .max(subsistence_wage_floor);
                    let civil_service_wage =
                        base_wage * task.ctx.country.turn_config.civil_service_wage_ratio;
                    let total_payroll = state_buildings_capacity as f64 * civil_service_wage;
                    // Phase 33: Add ministry public service wage pool to the
                    // State Employer's funded_payroll. This pool was contributed
                    // by Healthcare/Education ministries and was already debited
                    // from liquid_reserves by allocate_cash_to_ministries.
                    let ministry_pool = task.ctx.country.ministry_public_service_pool;
                    task.ministry_public_service_pool = ministry_pool;
                    task.ctx.country.ministry_public_service_pool = 0.0;
                    let combined_payroll = total_payroll + ministry_pool;
                    // Fund from treasury, but only what's available
                    let available_treasury = task.ctx.country.budget.liquid_reserves;
                    let funded_payroll = combined_payroll.min(available_treasury + ministry_pool);
                    if funded_payroll <= 0.0 {
                        None
                    } else {
                        let funded_fte = funded_payroll / civil_service_wage;
                        // Distribute state FTE across regions proportionally to population
                        let first_region = task
                            .ctx
                            .country
                            .regions
                            .first()
                            .map(|r| r.id.clone())
                            .unwrap_or_default();
                        let mut state_company = crate::entities::Company::new(
                            format!("STATE-EMPLOYER-{}", task.ctx.country.name),
                            format!("State Employer ({})", task.ctx.country.name),
                            crate::registries::enums::Sector::PublicServices,
                            crate::entities::LegalForm::StateMonopoly(
                                crate::entities::legal_form::StateMonopolyData::default(),
                            ),
                            0.0,
                            funded_payroll,
                            state_buildings_capacity,
                        );
                        state_company.region_id = first_region;
                        state_company.target_fte_demand = funded_fte.round() as u32;
                        state_company.physical_fte_demand = funded_fte.round() as u32;
                        state_company.offered_wage_per_fte = civil_service_wage;
                        state_company.state_share = 1.0;
                        task.companies.push(state_company);
                        Some(task.companies.len() - 1)
                    }
                }
            };

            // Phase 18B: Compute garnishment rates from justice state BEFORE mutable borrow
            let garnishment_rates = task
                .ctx
                .country
                .politics
                .justice_state
                .as_ref()
                .map(|js| {
                    crate::economy::sentencing::compute_garnishment_rates(js, task.ctx.country)
                })
                .unwrap_or_default();

            let pit_rate = task.ctx.country.tax_rates.income_tax.rate;

            // Phase 25: Clear labor market for ALL regions, not just the first.
            // The old code only processed regions.iter_mut().next(), leaving
            // all other regions' labor markets uncleared.
            let mut aggregated_allocation =
                crate::economy::labor_market::LaborAllocationMatrix::default();

            let num_regions = task.ctx.country.regions.len();
            for region_idx in 0..num_regions {
                // Phase 23C: Compute commuter inflow FTE for this region.
                let commuter_inflow_fte = {
                    let region = &task.ctx.country.regions[region_idx];
                    let coverage = task
                        .commute_coverage
                        .get(&region.id)
                        .copied()
                        .unwrap_or(0.0);
                    if coverage <= 0.0 {
                        0.0
                    } else {
                        let adjacent_land = region
                            .edges
                            .iter()
                            .filter(|e| {
                                e.edge_type == crate::society::geography::EdgeType::LandBorder
                            })
                            .count();
                        let local_pool: f64 = region
                            .class_demographics
                            .rural_classes
                            .values()
                            .chain(region.class_demographics.urban_classes.values())
                            .map(|d| d.available_fte)
                            .sum();
                        (coverage * 0.05 * adjacent_land as f64 * local_pool).min(local_pool * 0.5)
                    }
                };

                let region = &mut task.ctx.country.regions[region_idx];

                // Phase 24D: Fix commuter double-count. Before clearing the
                // labor market, deduct the commuter outflow FTE from the home
                // region's available_fte. This prevents the same workers from
                // being counted both as local labor AND as commuter inflow
                // in adjacent regions. The deducted FTE will be re-credited
                // as commuter wages by the labor market resolver.
                if commuter_inflow_fte > 0.0 {
                    crate::economy::telemetry::mark_commuting_out(region, commuter_inflow_fte);
                }

                let labor_allocation = crate::economy::labor_market::resolve_regional_labor_market(
                    region,
                    &mut task.companies,
                    None,
                    turn_calendar,
                    &crate::economy::labor_market::LaborConfig::default(),
                    pit_rate,
                    &garnishment_rates,
                    commuter_inflow_fte,
                    &task.ctx.country.politics.civil_rights_law,
                    &crate::politics::citizenship::DiscriminationConfig::default(),
                );
                aggregated_allocation.merge(labor_allocation);
            }
            task.labor_allocation = Some(aggregated_allocation);
        });

        // Phase 25: Feed back actual fulfilled FTE and wages from the bottom-up
        // labor clearing into the macro indicators. The top-down model
        // (process_demographics_and_labor) runs before the clearing and computes
        // employed_total and average_wage from circular formulas. Here we
        // overwrite them with REAL values from actual hiring, so that next
        // turn's top-down model starts from the correct baseline.
        tasks.par_iter_mut().for_each(|task| {
            let total_fulfilled: f64 = task.companies.iter().map(|c| c.fulfilled_fte as f64).sum();
            let total_wages: f64 = task
                .companies
                .iter()
                .map(|c| c.offered_wage_per_fte * c.fulfilled_fte as f64)
                .sum();
            let actual_avg_wage = if total_fulfilled > 0.0 {
                total_wages / total_fulfilled
            } else {
                // Phase 42: Preserve previous average_wage instead of cascading to 0.
                // This prevents PIT from collapsing to 0 when labor clearing fails.
                task.ctx.country.macro_indicators.average_wage
            };
            let labor_market = &mut task.ctx.country.macro_indicators.labor_market;
            let labor_force = (task.ctx.country.budget.population as f64
                * labor_market.labor_force_participation
                / 100.0)
                .max(1.0);
            labor_market.employed_total = total_fulfilled;
            let unemployed = (labor_force - total_fulfilled).max(0.0);
            labor_market.unemployed = unemployed;
            labor_market.unemployment_rate = (unemployed / labor_force * 100.0).max(0.0);
            // Emergency Stabilization: Aggregate total furloughed workers
            // across all companies for the macro dashboard.
            labor_market.furloughed_total = task
                .companies
                .iter()
                .map(|c| c.furloughed_workers_count)
                .sum();
            // Phase 25: Overwrite the top-down average_wage with the actual
            // market-cleared wage. This prevents the divergent feedback loop
            // where the top-down model compounds wages each turn.
            task.ctx.country.macro_indicators.average_wage = actual_avg_wage;

            // Phase 28: Accumulate State Employer wages as GDP government spending (G).
            // The state employer's wage payments represent government consumption
            // (public-sector payroll). Debit treasury for actual wages paid.
            // Phase 33: Reduce the debit by the ministry public service pool,
            // which was already debited from liquid_reserves by
            // allocate_cash_to_ministries. This avoids double-debiting.
            if let Some(idx) = task.state_employer_idx.take() {
                if idx < task.companies.len() {
                    let state_wages = task.companies[idx].fulfilled_fte as f64
                        * task.companies[idx].offered_wage_per_fte;
                    if state_wages > 0.0 {
                        // The ministry pool portion was already debited.
                        let ministry_pool = task.ministry_public_service_pool;
                        let treasury_debit = (state_wages - ministry_pool).max(0.0);
                        let debit = treasury_debit.min(task.ctx.country.budget.liquid_reserves);
                        task.ctx.country.budget.liquid_reserves -= debit;
                        // All state wages (including ministry-funded) flow into G.
                        task.gdp_acc.government_spending += state_wages;
                    }
                    // Remove the pseudo-company so it doesn't interfere with
                    // production, B2B, or save logic.
                    task.companies.remove(idx);
                }
            }
        });

        // Phase 37/38: Save prev_fulfilled_fte and prev_offered_wage_per_fte
        // for next turn's hiring frictions and sticky wage rigidity.
        // This must run AFTER all labor clearing is complete and AFTER the
        // state employer pseudo-company is removed.
        tasks.par_iter_mut().for_each(|task| {
            for c in &mut task.companies {
                c.prev_fulfilled_fte = c.fulfilled_fte;
                c.prev_offered_wage_per_fte = c.offered_wage_per_fte;
                // Phase 41: Reset strike flag at end of turn.
                // Strikes are per-turn events. If the union's strike_fund was
                // exhausted, the strike was already ended in pay_strike_benefits.
                // Otherwise, the strike ends naturally after one turn.
                c.is_striking = false;
            }
        });

        // Phase 25: Sync building.current_employment from company.fulfilled_fte.
        // This is the critical missing link between labor clearing and production.
        // Labor clearing sets company.fulfilled_fte, but production reads
        // building.current_employment. Without this sync, production uses stale
        // employment values and GDP stays at 0.
        tasks.par_iter_mut().for_each(|task| {
            // Build a map from company_id → fulfilled_fte
            let mut fulfilled_by_company: HashMap<String, f64> = HashMap::new();
            for c in &task.companies {
                fulfilled_by_company.insert(c.id.clone(), c.fulfilled_fte as f64);
            }
            // For each building, find its owner's fulfilled_fte and distribute
            // proportionally across the owner's buildings (by worker_capacity).
            let mut buildings_by_owner: HashMap<String, Vec<usize>> = HashMap::new();
            for (i, b) in task.ctx.buildings.iter().enumerate() {
                buildings_by_owner
                    .entry(b.owner_id.clone())
                    .or_default()
                    .push(i);
            }
            for (owner_id, building_indices) in &buildings_by_owner {
                let total_fulfilled = fulfilled_by_company.get(owner_id).copied().unwrap_or(0.0);
                if total_fulfilled <= 0.0 {
                    for &i in building_indices {
                        task.ctx.buildings[i].current_employment = 0;
                    }
                    continue;
                }
                let total_capacity: u32 = building_indices
                    .iter()
                    .map(|&i| task.ctx.buildings[i].worker_capacity)
                    .sum();
                if total_capacity == 0 {
                    continue;
                }
                for &i in building_indices {
                    let b = &task.ctx.buildings[i];
                    let share = b.worker_capacity as f64 / total_capacity as f64;
                    let employed = (total_fulfilled * share) as u32;
                    task.ctx.buildings[i].current_employment = employed.min(b.worker_capacity);
                }
            }
        });

        // Fix 1.22: Credit withheld PIT + garnishments to each country's Treasury
        tasks.par_iter_mut().for_each(|task| {
            if let Some(ref labor_alloc) = task.labor_allocation {
                if labor_alloc.pit_withheld > 0.0 {
                    task.ctx.country.budget.liquid_reserves += labor_alloc.pit_withheld;
                    // Phase 89: Accumulate PIT for reporting symmetry with VAT.
                    task.ctx.country.accumulated_pit += labor_alloc.pit_withheld;
                }
                // Phase 18B: Route community service garnishments to Treasury
                if labor_alloc.garnishments_withheld > 0.0 {
                    task.ctx.country.budget.liquid_reserves += labor_alloc.garnishments_withheld;
                }
            }
        });

        // Phase 18A: Route TemporaryWorker remittances to foreign_sector_balance.
        // Remittances were already deducted from net_wage in labor_market.rs
        // (savings credited with net_wage - remittance). Here we credit the
        // withheld total to the foreign sector balance — the external world's
        // purchasing pool. This seals the FiatDestruction leak: previously the
        // amount was only recorded in shadow_economy_state and vanished from M0.
        tasks.par_iter_mut().for_each(|task| {
            if let Some(ref labor_alloc) = task.labor_allocation {
                if labor_alloc.remittances_withheld > 0.0 {
                    let remittance = labor_alloc.remittances_withheld;
                    if let Some(ref mut state) = task.ctx.country.politics.shadow_economy_state {
                        state.total_remittances_outbound = remittance;
                    } else {
                        task.ctx.country.politics.shadow_economy_state =
                            Some(crate::economy::legal_status::ShadowEconomyState {
                                total_remittances_outbound: remittance,
                                ..Default::default()
                            });
                    }
                }
            }
        });
        // Sequential: credit total remittances to foreign_sector_balance (Rule 1).
        let total_remittances: f64 = tasks
            .iter()
            .map(|t| {
                t.labor_allocation
                    .as_ref()
                    .map(|la| la.remittances_withheld)
                    .unwrap_or(0.0)
            })
            .sum();
        if total_remittances > 0.0 {
            market.foreign_sector_balance += total_remittances;
        }

        // Phase 18A: Shadow Economy Processing
        // Processes shadow employment: companies in labor-intensive sectors
        // with ShadowEmployment records pay shadow wages (no PIT).
        // Runs after labor market resolution and before tax collection.
        tasks.par_iter_mut().for_each(|task| {
            // Phase 28: Trigger shadow employment for companies that can't
            // fill their labor demand through legal channels.
            let mut rng = rand::thread_rng();
            crate::economy::legal_status::trigger_shadow_employment(
                task.ctx.country,
                &mut task.companies,
                &mut rng,
            );

            let shadow_result = crate::economy::legal_status::process_shadow_economy_turn(
                task.ctx.country,
                &mut task.companies,
            );
            if shadow_result.total_pit_evaded > 0.0 {
                if let Some(ref mut state) = task.ctx.country.politics.shadow_economy_state {
                    state.total_hidden_fte = shadow_result.total_hidden_fte;
                    state.total_pit_evaded = shadow_result.total_pit_evaded;
                }
            }
            // Phase 24D: Accumulate shadow wages as shadow GDP.
            task.gdp_acc.shadow_gdp += shadow_result.total_shadow_wages;
        });

        // Phase D.5.1: Extract dominant demographic class per company from
        // the labor allocation matrix before it is consumed by D.5 payment
        // in kind. This map is used later by inspectorate bribery (Phase 22C)
        // and OHS compensation to route funds to the exact class of workers.
        tasks.par_iter_mut().for_each(|task| {
            if let Some(ref la) = task.labor_allocation {
                use std::collections::HashMap;
                let mut fte_by_company: HashMap<String, HashMap<DemographicClass, f64>> =
                    HashMap::new();
                for ((company_id, demographic_class), &fte) in &la.fte {
                    let class_map = fte_by_company
                        .entry(company_id.clone())
                        .or_default();
                    let entry = class_map.entry(*demographic_class).or_insert(0.0);
                    *entry += fte;
                }
                let mut dominant: HashMap<String, DemographicClass> = HashMap::new();
                for (company_id, classes) in fte_by_company {
                    if let Some(demographic_class) = classes
                        .into_iter()
                        .max_by(|(_, a), (_, b)| {
                            a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
                        })
                        .map(|(k, _)| k)
                    {
                        dominant.insert(company_id, demographic_class);
                    }
                }
                task.dominant_class_by_company = Some(dominant);
            }
        });

        // Phase D.1.2: Process deferred OHS compensations now that the
        // dominant class map is available. Route compensation through
        // settle_transfer (Rule 1: bank balance synchronized) to the exact
        // injured worker class (Rule 7: no communization). If the employer
        // cannot pay, mark for Syndic bankruptcy processing.
        tasks.par_iter_mut().for_each(|task| {
            use crate::economy::transfer_settler::{settle_transfer, TransferRecipient, TransferError};

            let pending: Vec<PendingOhsCompensation> =
                std::mem::take(&mut task.pending_ohs_compensations);
            for comp in pending {
                let employer_idx = task.companies.iter().position(|c| c.id == comp.employer_id);
                let employer_idx = match employer_idx {
                    Some(idx) => idx,
                    None => continue,
                };
                let region_idx = task
                    .ctx
                    .country
                    .regions
                    .iter()
                    .position(|r| r.id == comp.region_id);
                let region_idx = match region_idx {
                    Some(idx) => idx,
                    None => continue,
                };

                // Determine the injured worker class from the dominant class map.
                // If the contractor has no entry (zero employees), credit to the
                // region's default labor class as a fallback.
                let (class_key, is_rural) = task
                    .dominant_class_by_company
                    .as_ref()
                    .and_then(|map| map.get(&comp.employer_id))
                    .map(|dc| (dc.to_string(), dc.is_rural()))
                    .unwrap_or_else(|| {
                        // Fallback: use "Workers" urban class if no labor data
                        ("Worker".to_string(), false)
                    });

                let recipient = TransferRecipient::CitizenSavings {
                    region_idx,
                    is_rural,
                    class_key: class_key.clone(),
                };

                // Attempt full compensation — no silent clamping.
                let result = settle_transfer(
                    &mut task.companies,
                    employer_idx,
                    comp.total_compensation,
                    &recipient,
                    task.ctx.country,
                );

                if matches!(result, Err(TransferError::InsufficientCash)) {
                    // Employer cannot pay — mark for Syndic bankruptcy.
                    // The compensation claim becomes a wage-priority claim in
                    // the Syndic's waterfall distribution (wages are first priority).
                    if let Some(ref mut employer) = task.companies.get_mut(employer_idx) {
                        // Force negative equity to trigger Syndic liquidation
                        // in the next lifecycle phase.
                        employer.company_capital -= comp.total_compensation;
                    }
                    // Still credit the victim class — the Syndic will cover from
                    // asset liquidation proceeds. For now, credit from treasury
                    // as a state guarantee (the state covers unpaid wages when
                    // the employer is insolvent, then claims via the Syndic).
                    if comp.total_compensation > 0.0 {
                        if let Some(region) = task.ctx.country.regions.get_mut(region_idx) {
                            if is_rural {
                                if let Some(rk) = RuralClass::from_str(&class_key) {
                                    if let Some(demo) = region.class_demographics.rural_classes.get_mut(&rk) {
                                        demo.savings += comp.total_compensation;
                                    }
                                }
                            } else {
                                if let Some(uk) = UrbanClass::from_str(&class_key) {
                                    if let Some(demo) = region.class_demographics.urban_classes.get_mut(&uk) {
                                        demo.savings += comp.total_compensation;
                                    }
                                }
                            }
                        }
                        // Debit treasury for the state guarantee
                        task.ctx.country.budget.liquid_reserves -=
                            task.ctx.country.budget.liquid_reserves.min(comp.total_compensation);
                    }
                }
            }
        });

        // D.5: Payment in kind (deduct harvest for subsistence)
        // Phase 25: Process ALL regions, not just the first.
        tasks.par_iter_mut().for_each(|task| {
            let num_regions = task.ctx.country.regions.len();
            for region_idx in 0..num_regions {
                let region = &mut task.ctx.country.regions[region_idx];
                let mut harvest_bundle: std::collections::BTreeMap<
                    String,
                    std::collections::BTreeMap<Commodity, f64>,
                > = std::collections::BTreeMap::new();

                for company in &task.companies {
                    if company.sector == Sector::Agriculture {
                        let company_harvest: std::collections::BTreeMap<Commodity, f64> = task
                            .ctx
                            .buildings
                            .iter()
                            .filter(|b| b.owner_id == company.id)
                            .flat_map(|b| b.inventory.iter())
                            .map(|(c, &q)| (*c, q))
                            .collect();
                        if !company_harvest.is_empty() {
                            harvest_bundle.insert(company.id.clone(), company_harvest);
                        }
                    }
                }

                let labor_allocation = task.labor_allocation.take().unwrap_or_default();
                let (in_kind_ledger, _nutritional_deficit) =
                    apply_payment_in_kind(region, &labor_allocation, &mut harvest_bundle, turn);
                // Phase 44: Capture the in-kind ledger for imputed GDP calculation.
                // Merge deductions into the task-level ledger.
                for (company_id, deductions) in &in_kind_ledger.deductions {
                    let entry = task
                        .in_kind_ledger
                        .deductions
                        .entry(company_id.clone())
                        .or_default();
                    for (&commodity, &qty) in deductions {
                        *entry.entry(commodity).or_insert(0.0) += qty;
                    }
                }
                for (company_id, &offset) in &in_kind_ledger.cash_offsets {
                    *task
                        .in_kind_ledger
                        .cash_offsets
                        .entry(company_id.clone())
                        .or_insert(0.0) += offset;
                }
            }
        });

        // Phase 44: Calculate imputed GDP from in-kind deductions.
        // Value each deducted commodity at VWAP or base_price and add to GDP.
        tasks.par_iter_mut().for_each(|task| {
            if task.in_kind_ledger.deductions.is_empty() {
                return;
            }
            let base_prices = &market.base_prices;
            let mut total_imputed = 0.0;
            for (company_id, deductions) in &task.in_kind_ledger.deductions {
                // Find the region for this company
                let company_region = task
                    .companies
                    .iter()
                    .find(|c| &c.id == company_id)
                    .map(|c| c.region_id.clone());
                if company_region.is_none() {
                    continue;
                }
                let region_id = company_region.unwrap();

                let mut company_imputed = 0.0;
                for (&commodity, &qty) in deductions {
                    if qty <= 0.0 {
                        continue;
                    }
                    // Use VWAP from market history, fallback to base_price, fallback to 100.0
                    let price = state
                        .market_history
                        .last_trade_price
                        .get(&commodity)
                        .copied()
                        .unwrap_or_else(|| base_prices.get(&commodity).copied().unwrap_or(100.0));
                    company_imputed += qty * price;
                }
                if company_imputed > 0.0 {
                    total_imputed += company_imputed;
                    task.gdp_acc
                        .add_imputed_consumption(&region_id, company_imputed);
                }
            }
            task.imputed_consumption = total_imputed;
        });

        // D.6: Deposit remaining harvest to warehouses
        // World Generation & Climate Audit (v0.5.3): FIX — Previously this
        // loop iterated over ALL regions for EACH company, calling
        // calculate_harvest_yield_and_rot N times per company (where N =
        // number of regions in the country). This caused N× mass duplication
        // of harvest yield. Now each company is harvested exactly once,
        // using its own region's climate profile.
        tasks.par_iter_mut().for_each(|task| {
            for company in &mut task.companies {
                // Find the company's own region by region_id.
                let region_idx = task
                    .ctx
                    .country
                    .regions
                    .iter()
                    .position(|r| r.id == company.region_id);
                if let Some(region_idx) = region_idx {
                    let region = &task.ctx.country.regions[region_idx];
                    crate::agriculture::calculate_harvest_yield_and_rot(
                        company,
                        turn_calendar,
                        task.ctx.registries,
                        &task.climate_config,
                        region,
                        &mut task.ctx.country.budget,
                        &mut task.commercial_buildings,
                        turn,
                    );
                }
            }
        });

        // Phase 10: Accumulate storage fees (debt only, no money moves)
        tasks.par_iter_mut().for_each(|task| {
            accumulate_storage_fees(&mut task.commercial_buildings);
        });

        tasks.par_iter_mut().for_each(|task| {
            let mut all_destroyed_batches = Vec::new();
            for building in &mut task.commercial_buildings {
                let (_decayed, destroyed_batches) = building.apply_perishability(turn);
                all_destroyed_batches.extend(destroyed_batches);
            }
            for batch in all_destroyed_batches {
                crate::government::settle_rot_fees(
                    &batch,
                    &mut task.companies,
                    &mut task.ctx.country.budget,
                    &task.commercial_buildings,
                );
            }
        });

        // Phase 29: Periodic storage fee settlement — warehouse owners
        // collect accumulated fees from batch owners. If owners cannot pay,
        // batches are seized and liquidated. This makes warehousing a real
        // revenue stream for logistics companies.
        tasks.par_iter_mut().for_each(|task| {
            let _collected = crate::government::settle_periodic_storage_fees(
                &mut task.commercial_buildings,
                &mut task.companies,
                &mut task.ctx.country.budget,
            );
        });
        tasks.par_iter_mut().for_each(|task| {
            for company in &mut task.companies {
                let (despawn_signal, reclamation_data) =
                    crate::agriculture::process_agricultural_despawn(
                        company,
                        &mut task.ctx.country.budget,
                        &mut task.commercial_buildings,
                        company.is_in_receivership,
                    );
                if let Some(id) = despawn_signal {
                    task.despawned_company_ids.push(id);
                }
                if reclamation_data.total_hectares_reclaimed > 0 {
                    // Phase 25: Reclaim land in the company's actual region,
                    // not just the first region.
                    let company_region_id = company.region_id.clone();
                    if let Some(region) = task
                        .ctx
                        .country
                        .regions
                        .iter_mut()
                        .find(|r| r.id == company_region_id)
                    {
                        crate::agriculture::reclaim_agricultural_land(region, reclamation_data);
                    }
                }
            }
        });
        tasks.par_iter_mut().for_each(|task| {
            task.companies
                .retain(|c| !task.despawned_company_ids.contains(&c.id));
        });

        // Phase 6.5: B2C Market Phases R1-R7
        // Phase 44: Removed the wasted R1 consumer demand build — it was computed
        // and immediately discarded (`let _consumer_demand = ...`). The demand is
        // rebuilt during R6 clearing where it is actually used.

        tasks.par_iter_mut().for_each(|task| {
            // R4: Reset procurement commitments for wholesalers
            for building in &mut task.commercial_buildings {
                reset_procurement_commitment(building);
            }
        });

        tasks.par_iter_mut().for_each(|task| {
            // R5: Apply clearance discounts for stale inventory
            for building in &mut task.commercial_buildings {
                let commodity_keys: Vec<String> =
                    building.current_inventory.keys().cloned().collect();
                for commodity_key in commodity_keys {
                    if let Ok(commodity) = Commodity::try_from(commodity_key.as_str()) {
                        let market_price =
                            market_history::get_reference_price(&commodity, &state.market_history)
                                .unwrap_or(1.0);
                        apply_clearance_discount(building, commodity, turn, market_price);
                    }
                }
            }
        });

        tasks.par_iter_mut().for_each(|task| {
            // R6: Clear B2C markets
            // Phase 25: Process ALL regions, not just the first. The old code
            // only cleared B2C for regions.iter_mut().next(), leaving all other
            // regions' consumer demand and retail settlement unprocessed.
            // Phase 41: Reset accumulated_vat before B2C clearing.
            task.ctx.country.accumulated_vat = 0.0;
            let num_regions = task.ctx.country.regions.len();
            // Phase 94: Rebuild index maps BEFORE the region loop to avoid
            // borrow conflicts with &mut task.ctx.country.regions[region_idx].
            // These maps don't depend on region — they map commercial_buildings
            // and companies, which are stable within the B2C phase.
            task.rebuild_store_id_to_owner();
            task.rebuild_company_id_to_idx();
            for region_idx in 0..num_regions {
                let region = &mut task.ctx.country.regions[region_idx];
                let avg_wage = task.ctx.country.macro_indicators.average_wage;
                let mut consumer_demand = build_consumer_demand(
                    region,
                    turn,
                    &task.ctx.market_prices,
                    avg_wage,
                    &task.housing_buildings,
                );
                // Phase 44: Net out in-kind deductions from B2C consumer demand.
                // Serfs (and partially FreePeasants/LandlessLaborers) have their
                // subsistence needs met in-kind. Their B2C demand for those
                // commodities must be reduced to avoid double-counting.
                for ((lid, ldc), deductions) in &task.in_kind_ledger.deductions_by_class {
                    if lid != &region.id {
                        continue;
                    }
                    // E.6.3: consumer_demand.demand uses (String, DemographicClass) keys
                    let key = (lid.clone(), *ldc);
                    if let Some(class_demand) = consumer_demand.demand.get_mut(&key) {
                        for (&commodity, &deducted_qty) in deductions {
                            if let Some(existing) = class_demand.get_mut(&commodity) {
                                *existing = (*existing - deducted_qty).max(0.0);
                            }
                        }
                    }
                    // Also reduce total_demand
                    for (&commodity, &deducted_qty) in deductions {
                        if let Some(existing) = consumer_demand.total_demand.get_mut(&commodity) {
                            *existing = (*existing - deducted_qty).max(0.0);
                        }
                    }
                }
                // Phase 10: Apply rationing to consumer demand before clearing
                if task.ctx.country.rationing_system.active {
                    let rationing = task.ctx.country.rationing_system.clone();
                    apply_rationing_to_demand(&mut consumer_demand, &rationing);
                }
                let mut store_offers = generate_store_offers(&task.commercial_buildings, turn);
                let clearing_result = clear_b2c_markets(
                    &mut store_offers,
                    &consumer_demand,
                    &mut task.commercial_buildings,
                    turn,
                    Some(&task.ctx.country.generative_goods_config),
                );
                // Phase 16A: Route B2C revenue through TransferSettler for proper bank sync
                // Phase 41: Pass VAT rates for transactional VAT collection.
                let vat_rates = task.ctx.country.tax_rates.vat.clone();
                let (_b2c_settled, vat_collected) = settle_b2c_clearing(
                    &clearing_result.store_revenue,
                    &consumer_demand,
                    &task.commercial_buildings,
                    &mut task.companies,
                    region,
                    &vat_rates,
                    &task.store_id_to_owner,
                    &task.company_id_to_idx,
                );
                // Phase 41: Credit treasury with VAT (SINGLE credit — no double-counting in tax turn).
                if vat_collected > 0.0 {
                    task.ctx.country.budget.liquid_reserves += vat_collected;
                    task.ctx.country.accumulated_vat += vat_collected;
                }
                // Phase 24D: Accumulate B2C revenue as GDP final consumption (C).
                // Phase 35: Tag consumption by region for per-region GDP accounting.
                let b2c_revenue: f64 = clearing_result.store_revenue.values().sum();
                task.gdp_acc.add_consumption(&region.id, b2c_revenue);
                // Phase 25: Collect retail prices for CPI calculation
                task.retail_prices
                    .extend(clearing_result.retail_prices.iter().cloned());
                // Phase 44: Collect B2C consumer demand per commodity for Market UI.
                for (&commodity, &qty) in &consumer_demand.total_demand {
                    *task.b2c_demand.entry(commodity).or_insert(0.0) += qty;
                }
            }
        });

        // ── DIAGNOSTIC CHECKPOINT: b2c_clearing_post ──
        probe.checkpoint("b2c_clearing_post", 6, turn, &market, &tasks);

        // Phase 47: Degrade household durable cohorts by one turn.
        // Runs after B2C clearing, before telemetry. Durable goods
        // (Furniture, Cars, Televisions, Clothing, etc.) slowly wear out
        // and are scrapped when condition reaches 0.
        tasks.par_iter_mut().for_each(|task| {
            for region in &mut task.ctx.country.regions {
                for demographics in region.class_demographics.rural_classes.values_mut() {
                    crate::economy::trade::retail::degrade_household_durables(demographics);
                }
                for demographics in region.class_demographics.urban_classes.values_mut() {
                    crate::economy::trade::retail::degrade_household_durables(demographics);
                }
            }
        });

        // Phase 25: Update retail VWAP from B2C clearing for CPI calculation.
        let all_retail_prices: Vec<(crate::registries::enums::Commodity, f64, f64)> = tasks
            .iter()
            .flat_map(|t| t.retail_prices.iter().cloned())
            .collect();
        market_history::update_retail_vwap(&mut state.market_history, &all_retail_prices);

        // Bugfix Sprint: Clear supply/demand/b2c volumes ONCE at this point
        // (after B2C clearing has produced task.b2c_demand, but before we
        // aggregate it). This is the single coherent reset point that
        // eliminates stale loaded values from market.json while preserving
        // the current turn's B2C demand. The old Phase-80 FIX avoided
        // clearing at end-of-turn (which would erase B2C); clearing here
        // before B2C aggregation is the root-cause fix.
        market.supply_volume.clear();
        market.demand_volume.clear();
        market.b2c_demand_volume.clear();
        market.net_trade.clear();

        // Phase 44: Aggregate B2C consumer demand into market.demand_volume
        // and market.b2c_demand_volume so the Market UI shows total demand
        // (B2B + B2C) per commodity, while keeping a separate B2C-only
        // aggregator for the identity `Supply − Demand + Net Trade = Net Surplus`.
        for task in &tasks {
            for (&commodity, &qty) in &task.b2c_demand {
                *market.demand_volume.entry(commodity).or_insert(0.0) += qty;
                *market.b2c_demand_volume.entry(commodity).or_insert(0.0) += qty;
            }
        }

        tasks.par_iter_mut().for_each(|task| {
            // R7: Accrue retail rents and update leases
            let shopping_center_ids: Vec<String> = task
                .commercial_buildings
                .iter()
                .filter(|b| {
                    b.building_type
                        == crate::society::housing::CommercialBuildingType::ShoppingCenter
                })
                .map(|b| b.id.clone())
                .collect();

            for building_id in &shopping_center_ids {
                // Phase 24C.9: Compute diversity bonus and anchor tenant before
                // mutable borrow to avoid borrow checker conflicts.
                let diversity_bonus = {
                    let building = task
                        .commercial_buildings
                        .iter()
                        .find(|b| b.id == *building_id);
                    if let Some(b) = building {
                        calculate_diversity_bonus(b, &task.commercial_buildings)
                    } else {
                        0.0
                    }
                };
                let anchor_tenant = {
                    let building = task
                        .commercial_buildings
                        .iter()
                        .find(|b| b.id == *building_id);
                    if let Some(b) = building {
                        let mut best: Option<String> = None;
                        let mut best_sales: f64 = 0.0;
                        if let Some(profile) = &b.shopping_center_profile {
                            for tenant_id in &profile.tenant_building_ids {
                                if let Some(tenant) = task
                                    .commercial_buildings
                                    .iter()
                                    .find(|t| t.id == *tenant_id)
                                {
                                    let sales: f64 = tenant
                                        .retail_profile
                                        .as_ref()
                                        .map(|p| p.units_sold_last_turn.values().sum())
                                        .unwrap_or(0.0);
                                    if sales > best_sales {
                                        best_sales = sales;
                                        best = Some(tenant_id.clone());
                                    }
                                }
                            }
                        }
                        best
                    } else {
                        None
                    }
                };
                // Phase 24C-Final: Route rent payments through TransferSettler.
                // Pre-collect tenant owner IDs and rent amounts while we have
                // immutable access to commercial_buildings, then process the
                // transfers with mutable access to companies and country.
                let sc_idx = task
                    .commercial_buildings
                    .iter()
                    .position(|b| b.id == *building_id);
                if let Some(idx) = sc_idx {
                    // Step 1: Collect rent payment info (immutable borrow)
                    let tenant_payments: Vec<(String, f64, String)> = {
                        let sc = &task.commercial_buildings[idx];
                        sc.retail_leases
                            .iter()
                            .filter(|lease| {
                                let age = turn.saturating_sub(lease.start_turn);
                                age < lease.duration_turns
                            })
                            .map(|lease| {
                                let rent_due = lease.leased_sqm * lease.rent_per_sqm;
                                // Look up tenant building's owner
                                let owner_id = task
                                    .commercial_buildings
                                    .iter()
                                    .find(|b| b.id == lease.tenant_id)
                                    .map(|b| b.owner_id.clone())
                                    .unwrap_or_default();
                                (lease.tenant_id.clone(), rent_due, owner_id)
                            })
                            .collect()
                    };
                    // Step 2: Update lease list (keep only active leases)
                    let active_leases: Vec<crate::society::housing::RetailLease> = {
                        let sc = &task.commercial_buildings[idx];
                        sc.retail_leases
                            .iter()
                            .filter(|lease| {
                                let age = turn.saturating_sub(lease.start_turn);
                                age < lease.duration_turns
                            })
                            .cloned()
                            .collect()
                    };
                    task.commercial_buildings[idx].retail_leases = active_leases;
                    // Step 3: Process rent transfers via TransferSettler
                    let _rent_collected = accrue_retail_rents(
                        &mut task.commercial_buildings[idx],
                        &tenant_payments,
                        &mut task.companies,
                        task.ctx.country,
                        turn,
                    );
                    // Apply the diversity bonus and anchor tenant
                    if let Some(ref mut profile) =
                        task.commercial_buildings[idx].shopping_center_profile
                    {
                        profile.diversity_bonus = diversity_bonus;
                        profile.anchor_tenant = anchor_tenant;
                    }
                }
            }

            // Sign leases using index-based two-pass pattern to avoid borrow conflict
            let lease_pairs: Vec<(usize, Vec<usize>)> = task
                .commercial_buildings
                .iter()
                .enumerate()
                .filter(|(_, b)| {
                    b.building_type
                        == crate::society::housing::CommercialBuildingType::ShoppingCenter
                })
                .map(|(sc_idx, _)| {
                    let candidates: Vec<usize> = task
                        .commercial_buildings
                        .iter()
                        .enumerate()
                        .filter(|(i, store)| {
                            *i != sc_idx
                                && store
                                    .retail_profile
                                    .as_ref()
                                    .map(|p| p.landlord_building_id.is_none())
                                    .unwrap_or(false)
                        })
                        .map(|(i, _)| i)
                        .collect();
                    (sc_idx, candidates)
                })
                .collect();

            for (sc_idx, candidate_indices) in lease_pairs {
                for store_idx in candidate_indices {
                    let lease = {
                        let (lo, hi) = if sc_idx < store_idx {
                            let (lo, hi) = task.commercial_buildings.split_at_mut(store_idx);
                            (&mut lo[sc_idx], &mut hi[0])
                        } else {
                            let (lo, hi) = task.commercial_buildings.split_at_mut(sc_idx);
                            (&mut hi[0], &mut lo[store_idx])
                        };
                        let sc = lo;
                        let store = hi;
                        if let (Some(sc_profile), Some(store_profile)) =
                            (&mut sc.shopping_center_profile, &mut store.retail_profile)
                        {
                            if store_profile.landlord_building_id.is_none() {
                                let lease = crate::society::housing::RetailLease {
                                    tenant_id: store.id.clone(),
                                    leased_sqm: store.retail_capacity,
                                    rent_per_sqm: sc.rent_per_sqm,
                                    start_turn: turn,
                                    duration_turns: 12,
                                };
                                store_profile.landlord_building_id = Some(sc.id.clone());
                                store_profile.leased_sqm = store.retail_capacity;
                                sc_profile.tenant_building_ids.push(store.id.clone());
                                Some(lease)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };
                    if let Some(l) = lease {
                        task.commercial_buildings[sc_idx].retail_leases.push(l);
                    }
                }
            }
        });

        // ═══════════════════════════════════════════════════════════
        // RESURRECTION PHASE 10: RATIONING CONSEQUENCES
        // After B2C clearing — mortality and unrest penalties from rationing.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            apply_rationing_consequences(task.ctx.country);
        });

        // ═══════════════════════════════════════════════════════════
        // ═══════════════════════════════════════════════════════════
        // PHASE 9: TOURISM (Two-Pass: Compute → Clamp → Settle)
        // Runs after B2C clearing (citizens may be depleted), before process_companies.
        // Pass 1 (parallel): Compute demand, debit domestic savings. No company credits.
        // Sequential: Clamp foreign inflow to foreign_sector_balance.
        // Pass 2 (parallel): Credit companies with clamped amounts.
        // ═══════════════════════════════════════════════════════════

        // Compute total world population for PPP forex calculation (B1).
        let total_world_population: i64 = tasks
            .iter()
            .flat_map(|t| {
                t.ctx.country.regions.iter().flat_map(|r| {
                    r.class_demographics
                        .rural_classes
                        .values()
                        .chain(r.class_demographics.urban_classes.values())
                })
            })
            .map(|c| c.population)
            .sum();

        // PASS 1 (parallel): Compute demand, debit domestic savings.
        // No company credits, no market mutation.
        // Clone the climate config to avoid borrowing tasks immutably while
        // also borrowing it mutably in the parallel pass.
        let climate_config_clone = tasks[0].climate_config.clone();
        let foreign_sector_balance = market.foreign_sector_balance;
        tasks.par_iter_mut().for_each(|task| {
            let result = crate::society::tourism::compute_tourism_demand(
                task.ctx.country,
                &task.commercial_buildings,
                &task.companies,
                current_season,
                &climate_config_clone,
                &task.heritage_tourism_boost,
                foreign_sector_balance,
                total_world_population,
            );
            task.tourism_demand = result;
        });

        // SEQUENTIAL: Clamp foreign inflow to foreign_sector_balance.
        // Foreign tourist spending comes from the external world's spending pool.
        // Can never go below zero — foreign spending is clamped to this balance.
        let total_foreign_requested: f64 = tasks
            .iter()
            .map(|t| t.tourism_demand.total_foreign_requested)
            .sum();
        let foreign_scaling_ratio = if total_foreign_requested > 0.0 {
            (market.foreign_sector_balance / total_foreign_requested).min(1.0)
        } else {
            0.0
        };
        let total_foreign_actual = total_foreign_requested * foreign_scaling_ratio;
        market.foreign_sector_balance -= total_foreign_actual;

        // PASS 2 (parallel): Credit companies with clamped amounts.
        tasks.par_iter_mut().for_each(|task| {
            crate::society::tourism::settle_tourism_revenue(
                &mut task.companies,
                &task.tourism_demand,
                foreign_scaling_ratio,
            );
        });

        tasks.par_iter_mut().for_each(|task| {
            // Phase 24C.7: Update information quality tier for each company
            // based on capital and average wage (fog-of-war information asymmetry).
            let avg_wage = task
                .ctx
                .country
                .macro_indicators
                .average_wage
                .max(subsistence_wage_floor);
            for company in &mut task.companies {
                let total_capital = company.fixed_capital + company.liquid_capital;
                let quality = crate::corporate::bounded_rationality::determine_information_quality(
                    total_capital,
                    avg_wage,
                );
                company.information_quality = Some(quality);
            }
            process_companies(
                &mut task.companies,
                &mut task.ctx.buildings,
                task.ctx.country,
                task.ctx.year,
                &task.market_signal,
                task.ctx.turn,
                task.labor_allocation.as_ref(),
            );
        });
        tasks.par_iter_mut().for_each(|task| {
            // R2.1: Process cooperative federations.
            crate::corporate::federation::process_federations(
                &mut task.companies,
                task.ctx.country,
                task.ctx.turn,
            );
        });
        tasks.par_iter_mut().for_each(|task| {
            crate::corporate::mergers::process_mergers_and_acquisitions(
                &mut task.companies,
                &mut task.ctx.buildings,
                task.ctx.country,
                task.ctx.year,
                &task.market_signal,
                task.ctx.turn,
            );
        });
        tasks.par_iter_mut().for_each(|task| {
            // Phase 96: Process Strategic Resolution for energy companies in receivership.
            // This must run BEFORE lifecycle so recovered companies aren't liquidated.
            for company in &mut task.companies {
                if company.is_in_receivership {
                    let mut resolution =
                        crate::utilities::resolution::StrategicResolution::new(company.id.clone());
                    resolution.process_turn(
                        company,
                        &mut task.ctx.country.budget.liquid_reserves,
                    );
                }
            }
        });
        tasks.par_iter_mut().for_each(|task| {
            CompanyLifecycle::process_lifecycle(
                &mut task.companies,
                &mut task.ctx.buildings,
                task.ctx.country,
                task.ctx.year,
                &task.market_signal,
            );

            // Phase D: remove tombstoned companies (liquidated/merged) once per turn.
            task.companies.retain(|c| !c.is_liquidated);
        });
        // ═══════════════════════════════════════════════════════════
        // RESURRECTION PHASE 2: SECURITIES MARKET SEQUENCE (SEC-1 to SEC-8)
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let current_turn = task.ctx.turn;
            let config = task.ctx.country.securities_config.clone();

            // SEC-1: Collect fund capital from demographics (NAV-based unit issuance)
            // Functions filter by fund_type internally, so pass full slice as funds
            let len = task.companies.len();
            let (funds_half, companies_half) = task.companies.split_at_mut(len);
            crate::securities::funds::collect_fund_capital(
                funds_half,
                &mut task.ctx.country.regions,
                companies_half,
                &config,
                current_turn,
                task.ctx.country.macro_indicators.average_wage,
            );

            // SEC-2: Submit fund orders (deterministic valuation score)
            let len = task.companies.len();
            let (funds_half, companies_half) = task.companies.split_at_mut(len);
            crate::securities::funds::submit_fund_orders(
                funds_half,
                &mut task.ctx.country.stock_exchange,
                companies_half,
                &task.ctx.country.mbs_pool,
                &task.ctx.country.covered_bonds_issued,
                &config,
                current_turn,
                task.ctx.country.politics.vip_registry.as_ref(),
            );

            // SEC-3: Securitize eligible loans into MBS (banks submit Ask orders)
            for bank in task
                .companies
                .iter_mut()
                .filter(|c| c.balance_sheet.is_some())
            {
                crate::securities::mbs::securitize_loans(
                    bank,
                    &mut task.ctx.country.mbs_pool,
                    &mut task.ctx.country.stock_exchange,
                    &config,
                    current_turn,
                );
            }

            // SEC-4: Create covered bonds from eligible mortgage pools
            for bank in task
                .companies
                .iter_mut()
                .filter(|c| c.balance_sheet.is_some())
            {
                let _ = crate::securities::covered_bonds::create_covered_bond(
                    bank,
                    &mut task.ctx.country.covered_bonds_issued,
                    &mut task.ctx.country.stock_exchange,
                    &config,
                    1_000_000.0,       // Default principal
                    0.05,              // Default coupon rate
                    current_turn + 48, // 4-year maturity
                    current_turn,
                );
            }

            // SEC-5: Match all securities orders on the exchange
            let _trades = task.ctx.country.stock_exchange.match_securities_orders(
                &mut task.companies,
                &mut task.ctx.country.mbs_pool,
                &mut task.ctx.country.covered_bonds_issued,
                &mut task.ctx.country.budget,
                current_turn,
                Some(&mut task.ctx.country.capital_gains_tax),
            );

            // R3.1: Update share prices for all listed companies after order matching.
            // Uses VWAP of this turn's trades, or mean-reversion drift if no trades.
            task.ctx.country.stock_exchange.update_share_prices(
                &mut task.companies,
                current_turn,
                &task.ctx.country.securities_config,
            );

            // R3.2: Recompute market index after share prices are updated.
            // Use mem::take to avoid borrow conflict (market_index is inside stock_exchange).
            {
                let mut idx = std::mem::take(
                    &mut task.ctx.country.stock_exchange.market_index,
                );
                idx.compute(&task.ctx.country.stock_exchange, &task.companies);
                task.ctx.country.stock_exchange.market_index = idx;
            }

            // SEC-6: Process MBS coupon payments (debit bank, credit owners)
            crate::securities::mbs::process_mbs_turn(
                &mut task.ctx.country.mbs_pool,
                &mut task.companies,
                current_turn,
            );

            // SEC-6b: Process covered bond coupon payments
            crate::securities::covered_bonds::process_covered_bonds_turn(
                &mut task.ctx.country.covered_bonds_issued,
                &mut task.companies,
                current_turn,
            );

            // SEC-7: Process derivatives (CDS premiums + futures mark-to-market)
            crate::securities::derivatives::process_cds_premiums(
                &mut task.ctx.country.active_derivatives,
                &mut task.companies,
                current_turn,
            );
            crate::securities::derivatives::process_futures_mark_to_market(
                &mut task.ctx.country.active_futures,
                &mut task.companies,
                current_turn,
            );

            // SEC-7b: CCP margin management
            crate::securities::ccp::process_ccp_margins(
                &mut task.ctx.country.central_counterparty,
                &mut task.companies,
                &task.ctx.country.active_futures,
                &config,
                current_turn,
            );

            // SEC-8: Charge fund management and performance fees
            let len = task.companies.len();
            let (funds_half, companies_half) = task.companies.split_at_mut(len);
            crate::securities::funds::charge_fund_fees(funds_half, companies_half, &config);

            // SEC-8a: Process fund coupon payments from MBS/covered bonds
            // Phase 86.5B: Wire previously-disconnected coupon payment logic.
            // Funds holding MBS/covered bonds receive coupon interest from the
            // Treasury (the issuer). Double-entry: Treasury debited, fund
            // brokerage cash credited.
            for fund in task.companies.iter_mut() {
                if fund.fund_ledger.is_some() {
                    crate::securities::funds::process_fund_coupon_payments(
                        fund,
                        task.ctx.country,
                        current_turn,
                    );
                }
            }

            // SEC-8b: KNF compliance audits
            // Phase 36: Use the country's ACTUAL central bank instead of a
            // fresh default. The old code created CentralBank::default() with
            // zero reference rate and empty reserves, meaning KNF audits ran
            // against meaningless data.
            crate::securities::knf::process_knf_compliance(
                &mut task.ctx.country.knf,
                &mut task.companies,
                &mut task.ctx.country.budget,
                &mut task.ctx.country.central_bank,
                &config,
                current_turn,
            );

            // SEC-8c: Process trade finance (bills of lading delivery)
            crate::securities::trade_finance::process_bills_of_lading(
                &mut task.ctx.country.bills_of_lading,
                &mut task.companies,
                &mut task.ctx.country.working_capital_loans,
                current_turn,
            );

            // SEC-8d: CCP default waterfall — check for defaulted CCP members
            // Phase 86.5B: Wire previously-disconnected default waterfall.
            // If any CCP member has a margin_deficit exceeding their posted
            // margin, they are in default and the CCP waterfall kicks in
            // (margin → default fund → mutualization).
            let defaulted_members: Vec<String> = task
                .ctx
                .country
                .central_counterparty
                .members
                .iter()
                .filter(|(_, m)| {
                    m.margin_deficit > m.posted_margin
                        && m.status != crate::securities::ccp::MemberStatus::Defaulted
                })
                .map(|(id, _)| id.clone())
                .collect();
            for member_id in &defaulted_members {
                crate::securities::ccp::process_ccp_default_waterfall(
                    &mut task.ctx.country.central_counterparty,
                    &mut task.companies,
                    member_id,
                    current_turn,
                );
            }
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 7: TAX COLLECTION
        // The Treasury collects all progressive taxes (PIT, CIT, VAT,
        // wealth tax, capital gains), regional taxes, and fiscal transfers
        // BEFORE any spending occurs. This ensures the Treasury has
        // sufficient liquid reserves to disburse funds.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let current_turn = task.ctx.turn;

            // Phase 40: Calculate budget needs based on GDP and ideology.
            // This runs every turn to ensure ministries always have non-zero
            // allocated_cash (the root cause of the zero-budget bug).
            calculate_budget_needs(task.ctx.country);

            // 1. LEGISLATIVE BUDGET CYCLE (once per year / on government change)
            let is_budget_year = current_turn % 4 == 0;
            if is_budget_year {
                let bill = draft_budget_bill(task.ctx.country, current_turn);
                let parliament = task.ctx.country.politics.parliament.clone();
                let active_parties = task.ctx.country.politics.active_parties.clone();
                let upper_house = task.ctx.country.politics.upper_house.clone();
                let coalition = task.ctx.country.politics.coalition.clone();
                let (final_bill, enacted, _msg) = process_budget_lifecycle(
                    bill,
                    &parliament,
                    &active_parties,
                    &upper_house,
                    &task.ctx.country.politics.constitution,
                    &coalition,
                    current_turn,
                );
                if enacted {
                    // Phase 40: Write back final bill allocations to ministry_config.
                    // Previously, the enacted bill was dropped — allocations never
                    // reached the ministries, leaving them at 0.0.
                    if let Some(ref mut config) = task.ctx.country.politics.ministry_config {
                        for (i, ministry) in config.ministries.iter_mut().enumerate() {
                            if let Some(final_alloc) = final_bill.final_ministries.get(i) {
                                ministry.allocated_cash = final_alloc.allocated_cash;
                            }
                        }
                    }
                } else {
                    apply_budget_failure_consequence(task.ctx.country, final_bill);
                }
            }

            // 2. ALL TAX COLLECTION (NATIONAL + LOCAL + FISCAL TRANSFERS)
            let tax_result = process_tax_collection_turn(
                task.ctx.country,
                &task.companies,
                &task.ctx.buildings,
                current_turn,
            );

            // Phase 42: Physically debit companies for CIT and wealth tax.
            // The tax module is read-only; it returns liabilities for the caller to apply.
            let mut total_actual_collected = tax_result.actual_pit_collected
                + tax_result.vat_collected
                + tax_result.wealth_tax_collected
                + tax_result.exit_tax_collected
                + tax_result.customs_revenue
                + tax_result.state_property_revenue;
            let mut total_cit_debited = 0.0;
            // Phase 94: Accumulate CIT+wealth tax debits per bank for batch sync.
            // Without this, debiting company cash without debiting bank reserves
            // causes M0 FiatCreation (treasury increases, bank reserves unchanged).
            let mut tax_bank_debits: std::collections::HashMap<String, f64> =
                std::collections::HashMap::new();
            for liability in &tax_result.liabilities {
                if let Some(company) = task
                    .companies
                    .iter_mut()
                    .find(|c| c.id == liability.company_id)
                {
                    let to_debit = liability.cit_actual + liability.wealth_tax_actual;
                    if to_debit > 0.0 {
                        // Debit from available_cash first, then brokerage cash.
                        let from_cash = to_debit.min(company.available_cash);
                        company.available_cash -= from_cash;
                        let remaining = to_debit - from_cash;
                        if remaining > 0.0 {
                            if let Some(ref mut ba) = company.brokerage_account {
                                let from_broker = remaining.min(ba.cash);
                                ba.cash -= from_broker;
                            }
                        }
                        total_cit_debited += liability.cit_actual;
                        // Phase 94: Accumulate for batch bank sync.
                        if let Some(ref bank_id) = company.primary_bank_id {
                            *tax_bank_debits.entry(bank_id.clone()).or_insert(0.0) += to_debit;
                        }
                    }
                }
            }
            total_actual_collected += total_cit_debited;

            // Phase 94: Batch bank sync for CIT+wealth tax — debit bank
            // deposits and reserves by the exact amounts debited from
            // companies. No clamping (negative reserves = CB Lombard borrowing).
            for (bank_id, total_debit) in &tax_bank_debits {
                if let Some(bank) = task.companies.iter_mut().find(|c| c.id == *bank_id) {
                    if let Some(ref mut bs) = bank.balance_sheet {
                        bs.deposits -= total_debit;
                        bs.reserves_at_central_bank -= total_debit;
                    }
                }
            }

            // Phase 42: Route only the ACTUAL collected amounts to the treasury.
            // VAT and customs were already physically credited during trade clearing.
            let pit_routing = crate::state::TaxRouting {
                microregion_share: 0.0,
                region_share: 0.0,
                central_share: 1.0,
                national_exception: true,
                extra: Default::default(),
            };
            // Only route PIT + CIT + wealth tax (VAT/customs already credited).
            let route_amount = tax_result.actual_pit_collected
                + total_cit_debited
                + tax_result.wealth_tax_collected;
            if route_amount > 0.0 {
                crate::state::route_tax_collection_to_country(
                    route_amount,
                    &pit_routing,
                    task.ctx.country,
                    "",
                    format!("STATE_{}", task.ctx.country.name),
                    crate::state::TaxType::PIT,
                );
            }

            // Phase 38: Store tax result on country for Finance tab display.
            // Update cit_collected to reflect actual debited amount.
            // Phase 43: Add withheld PIT (collected at source in labor market)
            // to the stored tax_result so the Finance tab displays the real total.
            let withheld_pit = task
                .labor_allocation
                .as_ref()
                .map(|la| la.pit_withheld)
                .unwrap_or(0.0);
            let mut tax_result_stored = tax_result.clone();
            tax_result_stored.cit_collected = total_cit_debited;
            tax_result_stored.pit_collected = withheld_pit;
            tax_result_stored.actual_pit_collected = withheld_pit;
            tax_result_stored.total_revenue = total_actual_collected + withheld_pit;
            task.ctx.country.last_tax_result = Some(tax_result_stored);
            let total_property_tax = process_regional_taxes(task.ctx.country, &mut task.companies);
            // Phase 89: Store aggregated property tax in last_tax_result for Finance UI.
            if let Some(ref mut tax_result) = task.ctx.country.last_tax_result {
                tax_result.property_tax_collected = total_property_tax;
            }
            let transfer_config = crate::politics::system::FiscalTransferConfig::from_state_structure(
                task.ctx.country.politics.state_structure,
                &task.ctx.country.politics.state_structure_config,
            );
            process_fiscal_transfers(task.ctx.country, &transfer_config);
            check_commissary_administration(task.ctx.country);

            // Phase D.7: Process local elections and faction alignments.
            // Runs after fiscal transfers so that election outcomes reflect
            // the current fiscal situation. Elections fire on the year boundary
            // (years_to_next_election == 0), faction alignments update yearly.
            process_local_elections(task.ctx.country, task.ctx.year);
            update_curial_faction_alignments(task.ctx.country);

            // Phase D.9: Process equalization payments (Janosikowe).
            // Runs after fiscal transfers (D.1) and before JST spending (D.8),
            // so that equalization-augmented reserves are available for procurement.
            let equalization_config =
                crate::politics::equalization::EqualizationConfig::default();
            crate::politics::equalization::process_equalization(
                task.ctx.country,
                &equalization_config,
            );

            // Phase D.10: Process active unfunded mandates.
            // Runs after fiscal transfers and equalization, before JST spending.
            crate::politics::local_legislation::process_active_mandates(task.ctx.country);

            // Phase 15B: Customs evasion recovery — recover evaded taxes
            // scaled by CustomsCapacity from customs_office buildings.
            if tax_result.taxes_evaded > 0.0 {
                let _recovered = crate::economy::smuggling::process_customs_evasion_recovery(
                    task.ctx.country,
                    &task.ctx.buildings,
                    tax_result.taxes_evaded,
                );
            }

            // Phase 29 / D.1.1: Corruption tax leakage — corruption embezzles
            // a fraction of CURRENT-TURN tax revenue to corrupt officials'
            // class savings. No fiat is destroyed (Rule 1). Historical
            // reserves are not subject to leakage (Rule 16: temporal causality).
            let corruption_index = task
                .ctx
                .country
                .politics
                .inspectorate_state
                .as_ref()
                .map(|ist| ist.corruption_index)
                .unwrap_or(0.0);
            // Current-turn tax revenue only (PIT + CIT + VAT + wealth + exit + customs + property)
            let tax_revenue_this_turn =
                total_actual_collected + withheld_pit + total_property_tax;
            // Identify corrupt officials: employees of inspectorate buildings.
            // Use the dominant_class_by_company map to find their demographic class.
            let mut corrupt_officials: Vec<(usize, String, bool)> = Vec::new();
            if let Some(ref dominant_map) = task.dominant_class_by_company {
                use crate::registries::enums::Commodity;
                let inspection_commodities = [
                    Commodity::SanitaryInspectionCapacity,
                    Commodity::BuildingInspectionCapacity,
                    Commodity::EnvironmentalInspectionCapacity,
                    Commodity::LaborInspectionCapacity,
                ];
                for b in &task.ctx.buildings {
                    let produces_inspection = b
                        .last_production
                        .iter()
                        .any(|(c, &qty)| inspection_commodities.contains(c) && qty > 0.0);
                    if !produces_inspection || b.owner_id.is_empty() {
                        continue;
                    }
                    if let Some(demographic_class) = dominant_map.get(&b.owner_id) {
                        let region_idx = task
                            .ctx
                            .country
                            .regions
                            .iter()
                            .position(|r| r.id == b.region_id);
                        if let Some(ridx) = region_idx {
                            let is_rural = demographic_class.is_rural();
                            let entry = (ridx, demographic_class.to_string(), is_rural);
                            // Deduplicate
                            if !corrupt_officials.contains(&entry) {
                                corrupt_officials.push(entry);
                            }
                        }
                    }
                }
            }
            let _leakage = crate::economy::justice::bribery::apply_corruption_tax_leakage(
                task.ctx.country,
                corruption_index,
                tax_revenue_this_turn,
                &corrupt_officials,
            );
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 8: STATE SPENDING & ALLOCATION
        // Now that tax revenue has been collected, the Treasury services
        // debt, issues new securities if needed, allocates cash to
        // ministries, and ministries submit B2B orders and execute
        // spending strategies.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let current_turn = task.ctx.turn;

            // 3. ALL DEBT SERVICE (NATIONAL + LOCAL, BEFORE any discretionary spending)
            process_debt_service(task.ctx.country, &mut task.companies, current_turn);
            process_municipal_debt_service(task.ctx.country, &mut task.companies);

            // Phase 10: State reserve warehouse maintenance (physical upkeep)
            process_state_reserve_maintenance(task.ctx.country, &mut task.companies);

            // Phase 10: Black Ops funding (strict double-entry: debit reserves, credit intelligence)
            process_black_ops_funding(task.ctx.country, task.ctx.registries);

            // Phase 47: Emergency Retail Subsidy — if a region's last retail
            // company is failing, the Treasury injects a subsidy to cover
            // minimum upkeep and wages. Strict double-entry, hard-capped by
            // available Treasury liquid reserves.
            let _retail_subsidy = crate::economy::trade::retail::process_emergency_retail_subsidy(
                task.ctx.country,
                &mut task.companies,
            );

            // 4. ARREARS CHECK (if in default, prioritize arrears repayment)
            if task.ctx.country.debt_market.total_arrears > 0.0
                && task.ctx.country.budget.liquid_reserves > 0.0
            {
                clear_arrears(task.ctx.country);
            }

            // 5. WHOLESALE DEBT ISSUANCE (BEFORE ministry allocation)
            let promised_total =
                sum_ministry_allocations(&task.ctx.country.politics.ministry_config);
            let deficit = promised_total - task.ctx.country.budget.liquid_reserves;
            if deficit > 0.0 && !task.ctx.country.debt_market.is_locked_out_of_primary {
                issue_treasury_securities(task.ctx.country, deficit, current_turn);
                // Phase 38: DSPW auction settlement — primary dealer banks
                // pull-purchase from auction inventory created above.
                // This runs immediately after issuance so the treasury
                // is funded before ministry cash allocation.
                crate::state::banking::dspw_auction_settlement(
                    task.ctx.country,
                    &mut task.companies,
                    current_turn,
                );
            }

            // Phase 29: Anti-corruption budget reallocation feedback loop.
            // If corruption is high, shift budget priorities toward
            // Justice/InternalSecurity before cash allocation.
            let _severity =
                crate::politics::anti_corruption::run_anti_corruption_feedback(task.ctx.country);

            // 6. MINISTRY CASH ALLOCATION (HARD-CAPPED by physical reserves)
            // Phase 40: Recalculate budget needs before the second allocation
            // pass to ensure ministries have non-zero targets.
            calculate_budget_needs(task.ctx.country);
            // D.6.2: Wire inspectorate_priority to inspectorate funding.
            // When a crisis profile has inspectorate_priority > 0, multiply
            // the Justice/InternalSecurity ministry's allocated cash to boost
            // inspectorate effectiveness during crises.
            let insp_priority = task
                .ctx
                .country
                .macro_indicators
                .extra
                .get("inspectorate_priority")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            allocate_cash_to_ministries(task.ctx.country);
            if insp_priority > 0.0 {
                if let Some(ref mut config) = task.ctx.country.politics.ministry_config {
                    for ministry in &mut config.ministries {
                        let is_justice = ministry.id.contains("Justice")
                            || ministry.id.contains("InternalSecurity")
                            || ministry.id.contains("Security");
                        if is_justice {
                            ministry.allocated_cash *= 1.0 + insp_priority;
                        }
                    }
                }
            }

            // 7. MINISTRY PHASE A: Strategies + B2B Order Submission + Direct Transfers
            // Clone active_parties to avoid simultaneous mutable+immutable borrow of country
            let active_parties = task.ctx.country.politics.active_parties.clone();
            let mut ministry_config = task.ctx.country.politics.ministry_config.take();
            if let Some(ref mut config) = ministry_config {
                let mut local_order_book = OrderBook::default();
                for ministry in &mut config.ministries {
                    let g_spent = prepare_minister_strategies_with_parties(
                        ministry,
                        &active_parties,
                        &mut task.companies,
                        &mut local_order_book,
                        task.ctx.country,
                    );
                    // Phase 42: Accumulate non-procurement ministry spending into GDP G.
                    task.gdp_acc.government_spending += g_spent;
                }
            }
            task.ctx.country.politics.ministry_config = ministry_config;
        });

        // Phase 29: State construction of inspectorate buildings.
        // When corruption is high, the Justice/InternalSecurity ministry
        // publishes construction tenders for new inspectorate buildings.
        tasks.par_iter_mut().for_each(|task| {
            let _count = crate::politics::anti_corruption::maybe_publish_inspectorate_tender(
                task.ctx.country,
                task.ctx.turn,
            );
        });

        // 8. MINISTRY PHASE B: Post-Clearing Reconciliation
        // (B2B market clearing already occurred above as placeholder;
        //  in full implementation, ministry buy orders would be matched there)
        tasks.par_iter_mut().for_each(|task| {
            let mut ministry_config = task.ctx.country.politics.ministry_config.take();
            if let Some(ref mut config) = ministry_config {
                let order_book = OrderBook::default(); // placeholder — would be the global order book
                for ministry in &mut config.ministries {
                    process_minister_post_clearing(
                        ministry,
                        &order_book,
                        &mut task.companies,
                        task.ctx.country,
                    );
                }
            }
            task.ctx.country.politics.ministry_config = ministry_config;
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 13: SOCIAL PROGRAMS + CHARITY (THIRD PILLAR)
        // Order: 1) Charity fundraising → 2) Social welfare distribution → 3) Charity distribution
        // Fundraising must precede welfare so charities have cash to distribute.
        // Welfare must precede charity distribution so charity supplements gaps.
        // All transfers are strict double-entry.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            // 13a: Charity fundraising — collect donations from wealthy/co-religionists
            crate::society::charities::process_charity_fundraising(
                &mut task.companies,
                task.ctx.country,
                task.ctx.turn,
            );
        });

        tasks.par_iter_mut().for_each(|task| {
            // 13b: Social welfare distribution — execute active SocialPrograms
            crate::politics::social_programs::execute_social_welfare(
                task.ctx.country,
                &mut task.companies,
                task.ctx.turn,
            );
        });

        tasks.par_iter_mut().for_each(|task| {
            // 13c: Charity distribution — distribute relief to poorest classes
            crate::society::charities::process_charity_distribution(
                &mut task.companies,
                task.ctx.country,
                task.ctx.turn,
            );
        });

        // 9. RETAIL SAVINGS BONDS B2C WINDOW
        // Cash raised here funds NEXT turn's budget (causality rule)
        tasks.par_iter_mut().for_each(|task| {
            clear_savings_bonds_b2c(task.ctx.country, task.ctx.turn);
        });

        // 10. SECONDARY DEBT MARKET CLEARING (wholesale only)
        tasks.par_iter_mut().for_each(|task| {
            clear_secondary_debt_market(&mut task.ctx.country.debt_market, task.ctx.turn);
        });

        // Phase 35: Gate process_political_year to run only once per year
        // (on the last turn of each year, turn 23/47/71...). Previously this
        // ran every turn, causing the election timer to tick 24× too fast
        // (elections every 4 turns instead of every 4 years) and political
        // capital to be regenerated every turn, masking the payroll failure
        // cascade that drives it to 0.0.
        let is_year_boundary = turn > 0 && (turn + 1).is_multiple_of(24);

        // Phase 39: Apply ideology tax policy every turn (not just on election).
        // This ensures wealth/capital-gains tax brackets always reflect the
        // ruling ideology. Player agency is expressed through elections.
        tasks.par_iter_mut().for_each(|task| {
            crate::politics::apply_ruling_ideology_policies(task.ctx.country);
        });

        // Phase 39: Check snap election every turn (not just at year boundary).
        // This breaks provisional government deadlocks immediately instead of
        // waiting up to 23 turns for the year-boundary political processing.
        tasks.par_iter_mut().for_each(|task| {
            let msgs = crate::politics::check_snap_election(task.ctx.country, task.ctx.turn);
            for msg in msgs {
                task.ctx.country.budget.extra.insert(
                    format!("snap_election_msg_{}", task.ctx.turn),
                    serde_json::Value::String(msg),
                );
            }
        });

        // Phase 39: Run election if due every turn (not just at year boundary).
        // This ensures snap elections take effect immediately.
        tasks.par_iter_mut().for_each(|task| {
            let unrest = task.ctx.country.macro_indicators.social_unrest;
            let msgs =
                crate::politics::run_election_if_due(task.ctx.country, unrest, task.ctx.turn);
            for msg in msgs {
                task.ctx.country.budget.extra.insert(
                    format!("election_msg_{}", task.ctx.turn),
                    serde_json::Value::String(msg),
                );
            }
        });

        if is_year_boundary {
            tasks.par_iter_mut().for_each(|task| {
                process_political_year(
                    task.ctx.country,
                    &mut task.companies,
                    &mut task.unions,
                    task.ctx.year,
                );
            });
        }

        // Phase 48: Process political turn EVERY TURN (not just year boundary).
        // This wires the previously-dead-code `process_political_turn` into the
        // engine loop, enabling per-turn legislation advancement, leader trait
        // effects, VIP incapacity checks, and committee reviews.
        // Full law enactment (enact_bill → enact_law) is enabled from the first
        // integration — no logging-only phase. Latent bugs are fixed directly.
        //
        // CRITICAL: This also drains `vip_registry.pending_unnatural_deaths` at
        // the start of each turn to trigger immediate succession for
        // assassinated/couped/executed leaders (Zombie Leader prevention).
        tasks.par_iter_mut().for_each(|task| {
            // Collect councilors from regional governance structures.
            let councilors: Vec<crate::politics::local_council::Councilor> = task
                .ctx
                .country
                .regions
                .iter()
                .flat_map(|r| {
                    r.governance
                        .as_ref()
                        .map(|g| g.council.councilors.clone())
                        .unwrap_or_default()
                })
                .collect();

            // Use default ChaosConfig and TraitRegistry if not loaded from JSON.
            let chaos_config = crate::politics::chaos_config::ChaosConfig::default();
            let trait_registry = crate::politics::traits::TraitRegistry::default();

            let msgs = crate::politics::process_political_turn(
                task.ctx.country,
                &mut task.companies,
                &mut task.unions,
                &councilors,
                &chaos_config,
                Some(&trait_registry),
                task.ctx.turn,
            );

            // Store messages for diagnostics.
            for msg in msgs {
                task.ctx.country.budget.extra.insert(
                    format!("political_turn_msg_{}_{}", task.ctx.turn, msg.len()),
                    serde_json::Value::String(msg),
                );
            }

            // Phase 48: Process sunset provision expirations every turn.
            let sunset_msgs = crate::politics::legislation::process_sunset_expirations(
                task.ctx.country,
                task.ctx.turn,
            );
            for msg in sunset_msgs {
                task.ctx.country.budget.extra.insert(
                    format!("sunset_msg_{}_{}", task.ctx.turn, msg.len()),
                    serde_json::Value::String(msg),
                );
            }
        });

        // ═══════════════════════════════════════════════════════════
        // RESURRECTION PHASE 3: MoD B2B ORDER SUBMISSION
        // After Phase 8 fiscal sequence, the Ministry of Defense has received
        // its allocated cash. It now submits B2B buy orders for military
        // commodities. These orders are stored in pending_defense_orders
        // and will be merged into the global OrderBook at the START of the
        // NEXT turn's Phase 6.4, ensuring the MoD never spends cash it
        // hasn't received yet (cross-turn causality).
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let config = task.ctx.country.military_config.clone();
            let mod_cash = task.ctx.country.budget.liquid_reserves * 0.3; // Reserve 30% for MoD procurement
            let market_prices = &market.base_prices;
            let bids = crate::military::submit_defense_b2b_orders(
                &task.ctx.country.order_of_battle.flatten(),
                &config,
                mod_cash,
                market_prices,
            );
            // Encumber the cash (deduct from liquid_reserves)
            let total_encumbered: f64 = bids.iter().map(|b| b.quantity * b.limit_price).sum();
            task.ctx.country.budget.liquid_reserves =
                (task.ctx.country.budget.liquid_reserves - total_encumbered).max(0.0);
            task.ctx.country.pending_defense_orders = bids;
        });

        // Phase 10: Strategic Reserve Agency buy/sell orders (price stabilization)
        // Phase 79: Pass market_history snapshot for moving-average VWAP triggers.
        let market_history_snapshot = state.market_history.clone();
        tasks.par_iter_mut().for_each(|task| {
            let market_snapshot = market.clone();
            for company in &mut task.companies {
                if matches!(company.legal_form, LegalForm::StrategicReserveAgency(_)) {
                    crate::corporate::manager::manage_strategic_reserves(
                        company,
                        task.ctx.country,
                        &market_snapshot,
                        &market_history_snapshot,
                        &mut task.orders,
                    );
                }
            }
        });

        // Add military commodity demand to market before clearing
        tasks.par_iter_mut().for_each(|task| {
            add_military_demand_to_market(
                &task.ctx.country.order_of_battle.flatten(),
                &mut task.orders.orders,
            );
        });

        // ═══════════════════════════════════════════════════════════
        // RESURRECTION PHASE 4: REAL ECONOMY — Phase 9/9.1/9.2
        // ═══════════════════════════════════════════════════════════

        // Phase 9: R&D, Fishing, Infrastructure
        tasks.par_iter_mut().for_each(|task| {
            // 9.1: Corporate R&D and patent expiration
            let corp_config = task.ctx.country.corporate_tech_config.clone();
            let average_wage = task.ctx.country.macro_indicators.average_wage.max(1.0);
            crate::economy::corporate_rd::check_patent_expiration(
                &mut task.companies,
                task.ctx.turn,
            );
            crate::economy::corporate_rd::allocate_corporate_rd_budget(
                &mut task.companies,
                &corp_config,
                average_wage,
            );
            // Phase 24C.4/95: Hook R&D method research and licensing evaluation.
            // R&D spending now purchases Innovation Points from universities via
            // B2B settle_transfer (no cash destruction). Foreign patent fees
            // are collected in the outbox for sequential GlobalMarket crediting.
            let tech_tree = task.ctx.registries.tech_tree.clone();

            // Build building inventories snapshot (same pattern as innovation trading).
            let mut building_inventories: std::collections::BTreeMap<
                String,
                std::collections::BTreeMap<Commodity, f64>,
            > = std::collections::BTreeMap::new();
            for building in &task.ctx.buildings {
                building_inventories.insert(building.id.clone(), building.inventory.clone());
            }

            let foreign_fees = crate::economy::corporate_rd::execute_corporate_method_research(
                &mut task.companies,
                &mut task.ctx.buildings,
                &mut building_inventories,
                &tech_tree,
                task.ctx.country,
                average_wage,
                task.ctx.turn,
            );
            task.foreign_patent_fee_outbox += foreign_fees;

            // Sync building inventories back.
            for building in &mut task.ctx.buildings {
                if let Some(inv) = building_inventories.get(&building.id) {
                    building.inventory = inv.clone();
                }
            }

            // Evaluate licensing opportunities (needs a snapshot of all companies)
            let companies_snapshot = task.companies.clone();
            // Phase E.6: Pass discovered techs for State patent licensing.
            let discovered_techs = task.ctx.country.budget.science.discovered.clone();
            crate::economy::corporate_rd::evaluate_licensing_opportunities(
                &mut task.companies,
                &companies_snapshot,
                &tech_tree,
                &corp_config,
                average_wage,
                &state.market_history,
                task.ctx.turn,
                &discovered_techs,
            );

            // Phase 95: Assign active_blueprint to buildings.
            // For each company, for each building owned by that company:
            // - If the building produces a blueprint-eligible commodity, find
            //   the company's best blueprint for that commodity and assign it.
            // - If the company has no blueprint but has a LicensedBlueprint,
            //   assign the licensed blueprint ID.
            // - Otherwise, clear active_blueprint (legacy behavior).
            {
                // Build a lookup: company_id → Vec<(commodity, blueprint_id, design_score)>
                let mut bp_lookup: std::collections::HashMap<
                    String,
                    Vec<(crate::registries::enums::Commodity, String, f64)>,
                > = std::collections::HashMap::new();
                for company in &task.companies {
                    let entries: Vec<(crate::registries::enums::Commodity, String, f64)> = company
                        .blueprints
                        .iter()
                        .map(|bp| (bp.output_commodity, bp.id.clone(), bp.quality))
                        .collect();
                    bp_lookup.insert(company.id.clone(), entries);

                    // Licensed blueprints are handled in the second pass below
                    // (they require cross-company blueprint lookup).
                }

                // Second pass: add licensed blueprints to the lookup.
                // For each company with licensed_blueprints, find the output_commodity
                // by searching all companies' blueprints for the matching blueprint_id.
                let all_bps: std::collections::HashMap<
                    String,
                    crate::registries::enums::Commodity,
                > = {
                    let mut m = std::collections::HashMap::new();
                    for company in &task.companies {
                        for bp in &company.blueprints {
                            m.insert(bp.id.clone(), bp.output_commodity);
                        }
                    }
                    m
                };

                for company in &task.companies {
                    if company.licensed_blueprints.is_empty() {
                        continue;
                    }
                    let entry = bp_lookup.entry(company.id.clone()).or_default();
                    for licensed in &company.licensed_blueprints {
                        if let Some(&commodity) = all_bps.get(&licensed.blueprint_id) {
                            // Use a low score so owned blueprints are preferred.
                            entry.push((commodity, licensed.blueprint_id.clone(), -1.0));
                        }
                    }
                }

                // Now assign active_blueprint to each building.
                for building in &mut task.ctx.buildings {
                    // Find the first blueprint-eligible output commodity.
                    let eligible_commodity = building
                        .active_method
                        .outputs
                        .keys()
                        .find(|c| c.is_blueprint_eligible())
                        .copied();

                    if let Some(commodity) = eligible_commodity {
                        // Find the best blueprint for this commodity owned by the building's owner.
                        let best_bp = bp_lookup.get(&building.owner_id).and_then(|entries| {
                            entries
                                .iter()
                                .filter(|(c, _, _)| *c == commodity)
                                .max_by(|a, b| {
                                    a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal)
                                })
                                .map(|(_, id, _)| id.clone())
                        });

                        building.active_method.active_blueprint = best_bp;
                    } else {
                        building.active_method.active_blueprint = None;
                    }
                }
            }

            // 9.2: Fishing turn — deterministic, no rand
            let fishing_config = task.ctx.country.fishing_config.clone();
            let _harvest = process_fishing_turn(
                &mut task.ctx.country.fish_stocks,
                &mut task.ctx.country.fish_farms,
                &task.ctx.country.fishing_policies,
                &mut task.order_book,
                &fishing_config,
                &state.market_history,
                task.ctx.turn,
            );

            // 9.3: Infrastructure funding and production
            let infra_config = task.ctx.country.infrastructure_config.clone();
            let mut local_govs: std::collections::BTreeMap<String, f64> =
                std::collections::BTreeMap::new();
            let mut company_cash: std::collections::BTreeMap<String, f64> =
                std::collections::BTreeMap::new();
            for company in &task.companies {
                company_cash.insert(company.id.clone(), company.available_cash);
            }
            let average_wage = task.ctx.country.macro_indicators.average_wage.max(1.0);
            allocate_owner_infrastructure_funding(
                &mut task.ctx.buildings,
                &mut task.ctx.country.budget,
                &mut local_govs,
                &mut company_cash,
                &infra_config,
                average_wage,
            );
            // Phase A.4: Write back the mutated cash maps to their source
            // entities (Rule 1: double-entry bookkeeping — every debit must
            // have a counterparty credit, and the debit must actually persist).
            // Without this write-back, `company_cash` deductions were silently
            // discarded, creating a money-creation leak (Rule 1 violation).
            for company in &mut task.companies {
                if let Some(&cash) = company_cash.get(&company.id) {
                    company.available_cash = cash;
                }
            }
            // Write back local government cash if the country has local govs.
            // NOTE: `Country` does not currently have a `local_governments`
            // field — the `local_govs` map is populated empty and only used
            // in-memory by `allocate_owner_infrastructure_funding`. When local
            // government treasury support is added, write back here. This is
            // a latent gap documented in the plan (Risks section).
        });

        // Phase 9.1: B2C Service Clearing (Education + Healthcare)
        // Moved here per blueprint revision — aligned with consumer budgeting phase
        tasks.par_iter_mut().for_each(|task| {
            let service_config = task.ctx.country.service_pricing_config.clone();
            let mut building_inventories: std::collections::BTreeMap<
                String,
                std::collections::BTreeMap<Commodity, f64>,
            > = std::collections::BTreeMap::new();

            // Populate building inventories from Building.inventory
            for building in &task.ctx.buildings {
                building_inventories.insert(building.id.clone(), building.inventory.clone());
            }

            // Phase 14: Populate service_needs from demographics (fixes dead B2C clearing)
            let education_needs = populate_education_service_needs(task.ctx.country);
            let health_needs = populate_health_service_needs(task.ctx.country);

            // Clear education slots (Phase 16A: routed through TransferSettler)
            // Phase 17B: Capture education consumption per region for assimilation coverage.
            let edu_consumption = clear_education_slots_b2c(
                &mut task.ctx.buildings,
                &mut task.companies,
                task.ctx.country,
                &education_needs,
                &mut building_inventories,
                &service_config,
            );
            task.education_consumption = edu_consumption;
            task.education_needs = education_needs;

            // Phase B.3: Education Progression — consume EducationSlots to shift
            // demographics.education shares (none → basic → secondary → higher).
            // Runs after B2C clearing (temporal causality: consumption data is
            // available) and before assimilation (Phase 17B benefits from
            // updated education coverage). The updated shares are visible to
            // process_demographics_and_labor NEXT turn (one-turn lag — Rule 16).
            let _progression_result = crate::economy::labor::process_education_progression_turn(
                task.ctx.country,
                &task.education_consumption,
                &task.education_needs,
            );

            // Clear health capacity (Phase 16A: routed through TransferSettler)
            clear_health_capacity_b2c(
                &mut task.ctx.buildings,
                &mut task.companies,
                task.ctx.country,
                &health_needs,
                &mut building_inventories,
                &service_config,
            );

            // Phase 18C: Information B2C clearing with propaganda subsidy
            let info_needs = populate_information_service_needs(task.ctx.country);
            let propaganda_subsidy_rate = compute_propaganda_subsidy_rate(task.ctx.country);
            let info_result = clear_information_b2c(
                &mut task.ctx.buildings,
                &mut task.companies,
                task.ctx.country,
                &info_needs,
                &mut building_inventories,
                &service_config,
                propaganda_subsidy_rate,
            );

            // Phase 18C: Process propaganda effects (scaled by consumption ratio)
            let _propaganda_result =
                process_propaganda_turn(task.ctx.country, info_result.consumption_ratio);

            // Sync building inventories back from the temporary map
            for building in &mut task.ctx.buildings {
                if let Some(inv) = building_inventories.get(&building.id) {
                    building.inventory = inv.clone();
                }
            }
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 14: JUSTICE SYSTEM COVERAGE
        // Runs after B2B/B2C clearing so building inventories reflect
        // current-turn JusticeCapacity and SecurityCapacity production.
        // Calculates dynamic crime demand, applies frozen cash penalties
        // to companies, and updates corruption OPEX overhead.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let mut building_inventories: std::collections::BTreeMap<
                String,
                std::collections::BTreeMap<Commodity, f64>,
            > = std::collections::BTreeMap::new();
            for building in &task.ctx.buildings {
                building_inventories.insert(building.id.clone(), building.inventory.clone());
            }
            process_justice_turn(
                task.ctx.country,
                &task.ctx.buildings,
                &mut task.companies,
                &building_inventories,
            );
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 18B: VIGILANTE JUSTICE (VIGILANTE JUSTICE) + OMBUDSMAN (Ombudsman)
        // Vigilante justice: triggers in regions with < 0.15 justice or
        // security coverage AND high unrest. Summary executions reduce
        // population, mutilations reduce available_fte. Creates
        // DisasterType::VigilanteMob events.
        // Ombudsman: detects legal dualism rights violations, generates
        // unrest and scandals. Runs after justice system, before pogroms.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let _vigilante_result = crate::economy::sentencing::check_vigilante_justice(
                task.ctx.country,
                &task.ctx.buildings,
                current_turn,
            );
        });

        tasks.par_iter_mut().for_each(|task| {
            let _ombudsman_result =
                crate::economy::sentencing::process_ombudsman_turn(task.ctx.country);
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 15A: MAINTENANCE SPENDING + DISASTER CHECKS
        // Maintenance: companies pay to restore building condition (double-entry).
        // Disasters: triggered by weather events + poor building condition.
        // Mitigated by FireProtectionCapacity and ShelterCapacity from production.
        // Must run AFTER production + justice so capacity is available.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let config = task.ctx.country.maintenance_config.clone();
            crate::economy::maintenance::process_maintenance_spending(
                &mut task.ctx.buildings,
                &mut task.companies,
                &config,
            );
        });

        tasks.par_iter_mut().for_each(|task| {
            let disaster_result = crate::economy::disasters::check_disaster_triggers(
                task.ctx.country,
                &task.ctx.buildings,
                current_turn,
                task.ctx.country.weather_state.seed,
            );
            // Phase 24D: Apply disaster casualties to class demographics.
            // Disasters already decremented region.population; here we also
            // decrement class-level available_fte and track deceased/disabled.
            // Split: 40% dead, 60% disabled (disaster injury ratio).
            for event in &disaster_result.disasters {
                if event.casualties <= 0 {
                    continue;
                }
                let dead = (event.casualties as f64 * 0.4).round() as i64;
                let disabled = event.casualties - dead;
                if let Some(region) = task
                    .ctx
                    .country
                    .regions
                    .iter_mut()
                    .find(|r| r.id == event.region_id)
                {
                    // Apply to both rural and urban classes.
                    crate::economy::telemetry::apply_casualties_to_labor(
                        region, dead, disabled, true,
                    );
                    crate::economy::telemetry::apply_casualties_to_labor(
                        region, dead, disabled, false,
                    );
                }
            }
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 22D: PRIVATE OVERSIGHT + CIVIL LAWSUITS + KIO APPEALS
        // Private inspectors detect hidden defects. Civil lawsuits
        // freeze defendant assets and award damages. KIO appeals
        // challenge tender awards. All use double-entry settlement.
        // Runs after disasters so collapse events can trigger lawsuits.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let justice_coverage = task
                .ctx
                .country
                .politics
                .justice_state
                .as_ref()
                .map(|js| js.justice_coverage)
                .unwrap_or(0.0);
            let mut rng = rand::rngs::StdRng::seed_from_u64((current_turn as u64).wrapping_mul(23));
            // Process pending civil lawsuits
            let mut lawsuits = std::mem::take(&mut task.ctx.country.phase22_lawsuits);
            let _resolved = crate::economy::civil_lawsuits::process_civil_lawsuits(
                &mut lawsuits,
                justice_coverage,
                &mut task.companies,
                task.ctx.country,
                current_turn,
                &mut rng,
            );
            task.ctx.country.phase22_lawsuits = lawsuits;
            // Process pending KIO appeals
            let mut appeals = std::mem::take(&mut task.ctx.country.phase22_kio_appeals);
            let _kio_resolved = crate::government::kio::process_kio_appeals(
                &mut appeals,
                justice_coverage,
                &mut task.companies,
                task.ctx.country,
                current_turn,
                &mut rng,
            );
            task.ctx.country.phase22_kio_appeals = appeals;
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 15B: SMUGGLING + CUSTOMS EVASION RECOVERY
        // Smuggling: grey economy bypasses tariffs; border enforcement intercepts.
        // Customs: recovers evaded taxes scaled by CustomsCapacity.
        // Both run in parallel (per-country, no cross-country deps).
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            // Agent 4 — Phase 4: Use actual cross-border trade value instead of
            // the previous magic `sum(production) * 1000.0` estimate (Rule 2).
            // Smuggling is a function of cross-border trade only, not total
            // domestic output (Rule 15).
            let trade_volume = task.cross_border_trade_value;

            let _smuggling_result = crate::economy::smuggling::process_smuggling_turn(
                task.ctx.country,
                &task.ctx.buildings,
                trade_volume,
            );
        });

        // Phase 9.2: Innovation Trading + Royalty Payments
        tasks.par_iter_mut().for_each(|task| {
            let innovation_config = task.ctx.country.innovation_config.clone();
            let mut building_inventories: std::collections::BTreeMap<
                String,
                std::collections::BTreeMap<Commodity, f64>,
            > = std::collections::BTreeMap::new();
            for building in &task.ctx.buildings {
                building_inventories.insert(building.id.clone(), building.inventory.clone());
            }

            // Trade innovation points from universities to State
            trade_innovation_points_b2b(
                &mut task.ctx.buildings,
                &mut task.companies,
                &mut task.ctx.country,
                &mut building_inventories,
                &innovation_config,
            );

            // Sync building inventories back
            for building in &mut task.ctx.buildings {
                if let Some(inv) = building_inventories.get(&building.id) {
                    building.inventory = inv.clone();
                }
            }

            // Phase E.1: Execute State Fundamental Research (was dead code).
            // Consumes domain-specific innovation points from the treasury pool
            // to discover Fundamental technologies.
            let tech_tree = crate::registries::tech_tree_data::default_tech_tree();
            crate::economy::state_sector::state_research::execute_state_research(
                &mut task.ctx.country.budget,
                &tech_tree,
                task.ctx.turn,
            );

            // Process all royalty payments (private + state patents)
            let corp_tech_config = task.ctx.country.corporate_tech_config.clone();
            let mut planned_production: std::collections::BTreeMap<String, f64> =
                std::collections::BTreeMap::new();
            // Phase E.4: Build company_id → primary output commodity map for VWAP lookup.
            let mut company_output_commodities: std::collections::HashMap<String, Commodity> =
                std::collections::HashMap::new();
            for building in &task.ctx.buildings {
                let scale = building.current_employment as f64 / 1000.0;
                let total_output: f64 =
                    building.active_method.outputs.values().sum::<f64>() * scale;
                if total_output > 0.0 {
                    *planned_production
                        .entry(building.owner_id.clone())
                        .or_insert(0.0) += total_output;
                }
                // Track the primary output commodity (first non-intangible output).
                if !building.active_method.outputs.is_empty() {
                    if let Some((&commodity, &_qty)) = building
                        .active_method
                        .outputs
                        .iter()
                        .find(|(_, &q)| q > 0.0)
                    {
                        if !commodity.is_intangible() {
                            company_output_commodities
                                .entry(building.owner_id.clone())
                                .or_insert(commodity);
                        }
                    }
                }
            }

            process_all_royalty_payments(
                &mut task.companies,
                &state.market_history,
                &planned_production,
                &company_output_commodities,
                &innovation_config,
                &corp_tech_config,
                &mut task.ctx.country.budget,
                task.ctx.country.macro_indicators.average_wage.max(1.0),
            );

            // Phase 19A: Process blueprint royalties via TransferSettler.
            // Cross-border entries are written to this task's outbox for
            // sequential post-parallel crediting.
            let country_name = task.ctx.country_name.clone();
            process_blueprint_royalty_payments(
                &mut task.companies,
                &task.ctx.buildings,
                &state.market_history,
                &mut task.ctx.country.budget,
                &country_name,
                &mut task.cross_border_royalty_outbox,
            );
        });

        // Phase 19A: Sequential post-parallel crediting of cross-border
        // blueprint royalties. Each country's parallel phase emitted FX outflows
        // (debited licensees); here we credit foreign licensors in their home
        // country's company slice. Runs sequentially for determinism.
        // Step 1: Collect all outbox entries into a single flat queue (immutable borrow).
        let mut all_cross_border_entries: Vec<
            crate::economy::blueprints::CrossBorderRoyaltyQueueEntry,
        > = Vec::new();
        for task in tasks.iter() {
            all_cross_border_entries.extend(task.cross_border_royalty_outbox.iter().cloned());
        }
        // Step 2: For each destination country, credit its companies (mutable borrow).
        if !all_cross_border_entries.is_empty() {
            for dest_task in &mut tasks {
                let dest_country = dest_task.ctx.country_name.clone();
                let entries_for_dest: Vec<
                    crate::economy::blueprints::CrossBorderRoyaltyQueueEntry,
                > = all_cross_border_entries
                    .iter()
                    .filter(|e| e.licensor_country == dest_country)
                    .cloned()
                    .collect();
                if !entries_for_dest.is_empty() {
                    let _msgs = process_cross_border_royalty_queue(
                        &entries_for_dest,
                        &mut dest_task.companies,
                    );
                }
            }
        }
        // Step 3: Clear all outboxes after processing.
        for task in &mut tasks {
            task.cross_border_royalty_outbox.clear();
        }

        // Phase 95: Sequential post-parallel crediting of foreign patent fees.
        // Each country's parallel R&D phase emitted FX outflows (debited
        // companies via settle_transfer to ForeignEntity). Here we credit the
        // sum to GlobalMarket.offshore_capital — money preserved, never destroyed.
        let total_foreign_fees: f64 = tasks.iter().map(|t| t.foreign_patent_fee_outbox).sum();
        if total_foreign_fees > 0.0 {
            market.foreign_patent_fee_ledger += total_foreign_fees;
            market.offshore_capital += total_foreign_fees;
        }
        for task in &mut tasks {
            task.foreign_patent_fee_outbox = 0.0;
        }

        // Phase 28/36: Store current-turn sector employment and wage data for
        // next-turn ToT computation. Uses a two-slot approach:
        //   _prev_employment = previous turn's end values (for snapshot comparison)
        //   _cur_employment  = current turn's end values
        // At end of turn: move _cur_* → _prev_*, then store current as _cur_*.
        // The snapshot compares current sector data to _prev_* (previous turn).
        // Phase 36 fix: Previously, _prev_* was overwritten with current-turn
        // data, making the snapshot compare current-to-current (ToT always 0%).
        for task in &mut tasks {
            use std::collections::HashMap;
            let mut by_sector: HashMap<crate::registries::enums::Sector, (f64, f64)> =
                HashMap::new();
            for c in &task.companies {
                let entry = by_sector.entry(c.sector).or_insert((0.0, 0.0));
                entry.0 += c.fulfilled_fte as f64;
                entry.1 += c.offered_wage_per_fte * c.fulfilled_fte as f64;
            }
            for (sector, (fte, wages)) in &by_sector {
                let avg_wage = if *fte > 0.0 { *wages / *fte } else { 0.0 };
                if let Some(share) = task.ctx.country.budget.sectors.get_mut(sector) {
                    // Move _cur_* to _prev_* (previous turn's values become the comparison baseline)
                    if let Some(cur_emp) = share.extra.remove("_cur_employment") {
                        share.extra.insert("_prev_employment".to_string(), cur_emp);
                    }
                    if let Some(cur_wage) = share.extra.remove("_cur_avg_wage") {
                        share.extra.insert("_prev_avg_wage".to_string(), cur_wage);
                    }
                    // Store current turn's values as _cur_*
                    share
                        .extra
                        .insert("_cur_employment".to_string(), serde_json::Value::from(*fte));
                    share.extra.insert(
                        "_cur_avg_wage".to_string(),
                        serde_json::Value::from(avg_wage),
                    );
                }
            }
        }

        // Phase 42: Safety clamp — ensure FX reserves never go negative.
        // Phase 43: Also purge legacy "IEU" keys from fx_reserves.
        for task in &mut tasks {
            task.ctx.country.central_bank.fx_reserves.remove("IEU");
            for fx in task.ctx.country.central_bank.fx_reserves.values_mut() {
                if *fx < 0.0 {
                    *fx = 0.0;
                }
            }
            // Phase 42: Political capital per-turn regeneration.
            // process_political_year resets it once per year, but payroll failures
            // drain it every turn. Add a small per-turn regen so it doesn't
            // collapse to 0 between yearly resets.
            let pc_regen = 2.0 / 24.0; // ~2.0/year, spread per turn
            let pc_cap = 100.0;
            let current_pc = task.ctx.country.politics.political_capital;
            task.ctx.country.politics.political_capital = (current_pc + pc_regen).min(pc_cap);
        }

        // ═══════════════════════════════════════════════════════════
        // PHASE 15B: CROSS-COUNTRY MIGRATION + DEPORTATIONS
        // Two-pass sequential process (needs all countries simultaneously):
        // 1. Collection: compute migration flows from pressure differentials.
        // 2. Settlement: apply flows (origin loses, destination gains).
        // 3. Deportations: per-country, remove illegal immigrants per policy.
        // Population is strictly conserved across countries.
        // ═══════════════════════════════════════════════════════════
        {
            // Phase R6: Migration config — replaces all magic numbers.
            let migration_config =
                crate::economy::labor::migration::MigrationConfig::default();

            // Pass 1: Collect migration flows (read-only access to countries)
            let mut country_refs: HashMap<
                String,
                (&crate::state::Country, &[crate::entities::Building], u32),
            > = HashMap::new();
            for task in &tasks {
                let disaster_count = task.ctx.country.weather_state.active_events.len() as u32;
                country_refs.insert(
                    task.ctx.country_name.clone(),
                    (task.ctx.country, &task.ctx.buildings, disaster_count),
                );
            }
            let flows = crate::economy::migration::collect_migration_flows(
                &country_refs,
                turn,
                Some(&state.treaty_registry),
                &migration_config,
            );

            // Pass 2: Apply migration flows (mutable access to countries)
            let mut country_mut_refs: HashMap<String, &mut crate::state::Country> = HashMap::new();
            let mut buildings_mut_refs: HashMap<String, &mut [crate::entities::Building]> =
                HashMap::new();
            for task in &mut tasks {
                country_mut_refs.insert(task.ctx.country_name.clone(), task.ctx.country);
                buildings_mut_refs.insert(task.ctx.country_name.clone(), &mut task.ctx.buildings);
            }
            crate::economy::migration::apply_migration_flows(
                &mut country_mut_refs,
                &mut buildings_mut_refs,
                &flows,
                &migration_config,
            );

            // Pass 3: Process deportations per country (parallel-safe, single country)
            let deportation_config = migration_config.clone();
            tasks.par_iter_mut().for_each(|task| {
                let border_cap =
                    crate::economy::migration::sum_border_enforcement_capacity(&task.ctx.buildings);
                let (_deported, wealth) =
                    crate::economy::migration::process_deportations(task.ctx.country, border_cap, &deportation_config);
                task.deported_wealth = wealth;
            });
            // Sequential: credit deported wealth to foreign_sector_balance (F4).
            // Deportees take their savings out of the country → external world gains.
            let total_deported_wealth: f64 =
                tasks.iter().map(|t| t.deported_wealth).sum();
            if total_deported_wealth > 0.0 {
                market.foreign_sector_balance += total_deported_wealth;
            }
        }

        // ═══════════════════════════════════════════════════════════
        // PHASE 15C: INSPECTORATES + STATE FORESTS
        // Inspectorates: detect violations, issue fines (double-entry),
        // increase justice_demand. Runs after production + justice so
        // inspection capacity is available from building outputs.
        // State Forests: timber growth, sustainable harvest, profit
        // remittance from StateMonopoly company to Treasury.
        // Both run per-country in parallel (no cross-country deps).
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let _inspectorate_result = crate::economy::inspectorates::process_inspectorates_turn(
                task.ctx.country,
                &mut task.companies,
                &task.ctx.buildings,
                current_turn,
                task.ctx.country.macro_indicators.average_wage,
                task.planet,
            );
        });

        tasks.par_iter_mut().for_each(|task| {
            let _forest_result = crate::economy::state_forests::process_state_forests_turn(
                task.ctx.country,
                &mut task.companies,
                &mut task.ctx.buildings,
            );
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 22C: CONSTRUCTION INSPECTIONS + BRIBERY
        // PIP (labor inspectorate) inspects active construction sites
        // for OHS violations and material fraud. Fleet range limits
        // which sites can be reached. Corrupt inspectors may accept
        // bribes via CitizenSavings (no building reserve mutation).
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            // Compute PIP fleet ranges from inspectorate buildings
            let pip_ranges = crate::economy::inspectorate_fleet::compute_inspectorate_fleet_ranges(
                &task.ctx.buildings,
                crate::registries::enums::Commodity::LaborInspectionCapacity,
            );
            let pip_capacity: f64 = task
                .ctx
                .buildings
                .iter()
                .map(|b| {
                    b.last_production
                        .get(&crate::registries::enums::Commodity::LaborInspectionCapacity)
                        .copied()
                        .unwrap_or(0.0)
                })
                .sum();
            let corruption_index = task
                .ctx
                .country
                .politics
                .inspectorate_state
                .as_ref()
                .map(|ist| ist.corruption_index)
                .unwrap_or(0.0);
            let mut rng = rand::rngs::StdRng::seed_from_u64((current_turn as u64).wrapping_mul(22));
            // Reset bribe counter
            if let Some(ref mut ist) = task.ctx.country.politics.inspectorate_state {
                ist.bribes_accepted_this_turn = 0;
                ist.labor_inspection_capacity = pip_capacity;
            }
            // Inspect construction sites within range
            for building in &task.ctx.buildings {
                let (has_project, defect, ohs_ratio, contractor_id, region_id) =
                    match &building.active_project {
                        Some(p) if !p.main_contractor_id.is_empty() => (
                            true,
                            p.structural_defect,
                            p.ohs_coverage_ratio,
                            p.main_contractor_id.clone(),
                            building.region_id.clone(),
                        ),
                        _ => continue,
                    };
                if !has_project {
                    continue;
                }
                // Check if within PIP range
                // D.6.1: Use Planet haversine distance for spatial partitioning.
                // No O(N²) building-pair loops — distance is computed per
                // inspectorate-region / target-region pair.
                let planet = task.planet;
                let in_range = crate::economy::inspectorate_fleet::is_within_inspection_range(
                    &pip_ranges,
                    &region_id,
                    |insp_region, target_region| {
                        planet.distance_between_regions(insp_region, target_region)
                    },
                );
                if !in_range {
                    continue;
                }
                // Find contractor index
                let contractor_idx = match task.companies.iter().position(|c| c.id == contractor_id)
                {
                    Some(idx) => idx,
                    None => continue,
                };
                // Detect violations
                let violation_detected = defect > 0.05 || ohs_ratio < 0.8;
                if !violation_detected {
                    continue;
                }
                // D.4.2: Scale fines by average_wage (Rule 2: no magic numbers)
                let avg_wage = task.ctx.country.macro_indicators.average_wage;
                let fine = (defect * avg_wage * 50.0 + (1.0 - ohs_ratio) * avg_wage * 20.0)
                    .max(avg_wage * 5.0);
                // Attempt bribe — D.5.1: Determine bribe recipient deterministically
                // from the dominant class employed at the specific inspectorate
                // building that is inspecting this site. No hardcoded "bourgeoisie".
                let inspector_region_idx = task
                    .ctx
                    .country
                    .regions
                    .iter()
                    .position(|r| r.id == region_id)
                    .unwrap_or(0);

                // Find the specific inspectorate building in range of this site.
                // D.5.1: If no inspectorate has eligible staff (empty building),
                // the bribe cannot occur.
                let bribe_recipient: Option<(String, bool)> = pip_ranges.iter()
                    .find(|(_, insp_region, range)| {
                        let dist = crate::economy::inspectorate_fleet::simple_region_distance(
                            insp_region,
                            &region_id,
                        );
                        dist <= *range
                    })
                    .and_then(|(insp_building_id, _, _)| {
                        // Find the inspectorate building's owning company
                        task.ctx.buildings.iter()
                            .find(|b| b.id == *insp_building_id)
                            .map(|b| b.owner_id.clone())
                    })
                    .and_then(|insp_company_id| {
                        // Look up the dominant class of the inspectorate's owning company
                        task.dominant_class_by_company
                            .as_ref()
                            .and_then(|map| map.get(&insp_company_id))
                            .map(|dc| (dc.to_string(), dc.is_rural()))
                    });

                // If no eligible inspector (empty inspectorate building), skip bribe
                let bribe_result = if let Some((class_key, is_rural)) = bribe_recipient {
                    crate::economy::bribery::try_bribe(
                        contractor_idx,
                        fine,
                        corruption_index,
                        inspector_region_idx,
                        &class_key,
                        is_rural,
                        current_turn,
                        &mut task.companies,
                        task.ctx.country,
                        &mut rng,
                    )
                } else {
                    // No eligible inspector — no bribe possible, proceed to fine
                    None
                };
                // If bribe rejected or no bribe attempted, levy the fine
                let bribe_accepted = bribe_result.as_ref().map(|b| b.accepted).unwrap_or(false);
                if !bribe_accepted {
                    let available = task.companies[contractor_idx]
                        .brokerage_account
                        .as_ref()
                        .map(|b| b.cash)
                        .unwrap_or(task.companies[contractor_idx].available_cash);
                    let actual_fine = fine.min(available);
                    if actual_fine > 0.01 {
                        let _ = crate::economy::transfer_settler::settle_transfer_to_treasury(
                            &mut task.companies,
                            contractor_idx,
                            actual_fine,
                            task.ctx.country,
                        );
                        // Reputation penalty for detected violation
                        task.companies[contractor_idx].reputation_score =
                            (task.companies[contractor_idx].reputation_score - 5.0).max(0.0);
                    }
                }
            }
            // Update corruption index
            if let Some(ref mut ist) = task.ctx.country.politics.inspectorate_state {
                let justice_cov = task
                    .ctx
                    .country
                    .politics
                    .justice_state
                    .as_ref()
                    .map(|js| js.justice_coverage)
                    .unwrap_or(0.0);
                crate::economy::bribery::update_corruption_index(
                    &mut ist.corruption_index,
                    ist.bribes_accepted_this_turn,
                    justice_cov,
                );
            }
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 17C: MONASTERY PRODUCTION + CHURCH FUND
        // Monastery production: cultural buildings with production_method
        // generate commodities; revenue credits owning company via TransferSettler.
        // Church Fund: state religion countries pay building maintenance from
        // Treasury to owning companies via credit_company_by_id.
        // Both run per-country in parallel (no cross-country deps).
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let _monastery_value = crate::economy::religious_economy::process_monastery_production(
                &mut task.ctx.country.cultural_institutions,
                &mut task.companies,
            );
        });

        tasks.par_iter_mut().for_each(|task| {
            let religious_law = task
                .ctx
                .country
                .politics
                .religious_law_struct
                .clone()
                .unwrap_or_else(|| {
                    let reg = crate::society::culture_registry::registry();
                    let religion_key =
                        reg.religion_key_from_display(&task.ctx.country.macro_indicators.religion);
                    crate::politics::laws::ReligiousLaw::from_raw(
                        &task.ctx.country.politics.religious_law,
                        &religion_key,
                    )
                });
            let church_fund_result = crate::economy::religious_economy::process_church_fund(
                task.ctx.country,
                &mut task.companies,
                &religious_law,
            );
            // Phase 28: Church fund is a State expenditure (treasury → church).
            // It counts as government spending (G) in GDP.
            task.gdp_acc.government_spending += church_fund_result.total_paid;
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 18A: AMNESTY & LEGALIZATION
        // When AmnestyLaw is active, a percentage of Illegal population
        // is legalized each turn (with affordability clamp on fees).
        // Runs after shadow economy processing and before Phase 17B
        // assimilation, so legalized workers can immediately assimilate.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let _amnesty_result = crate::economy::legal_status::process_amnesty_turn(
                task.ctx.country,
                &mut task.companies,
            );
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 78: RELIGIOUS AUTHORITY COMPUTATION
        // Computes per-religion authority scores (0.0–1.0) based on buildings,
        // charity, holy sites, and clergy-to-follower ratio. Must run BEFORE
        // religious conversion (which uses authority scores).
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let config = crate::society::religious_authority::ReligiousAuthorityConfig::default();
            let authority = crate::society::religious_authority::process_religious_authority_turn(
                task.ctx.country,
                &task.ctx.country.cultural_institutions,
                &config,
                &task.companies,
            );
            task.ctx.country.religious_authority_state.authority = authority;
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 17B: RELIGIOUS CONVERSION + INSTITUTIONAL ASSIMILATION
        // Runs after B2C education clearing (which provides consumption data)
        // and after religious authority computation (which provides authority
        // scores). Conversion runs first (religious composition settles before
        // ethnic assimilation), then assimilation uses dual-channel coverage
        // (education + Integration Centers) with syncretism bounding.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            // Step 1: Religious conversion (driven by ReligiousAuthority).
            let authority = task.ctx.country.religious_authority_state.authority.clone();
            let _conversion_result =
                crate::economy::assimilation::process_religious_conversion_turn(
                    task.ctx.country,
                    &authority,
                );

            // Step 2: Institutional assimilation (dual-channel: education + integration centers).
            let edu_consumption = task.education_consumption.clone();
            let edu_needs = task.education_needs.clone();
            let _assimilation_result = crate::economy::assimilation::process_assimilation_turn(
                task.ctx.country,
                &task.ctx.buildings,
                &edu_consumption,
                &edu_needs,
            );
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 17C: POGROMS (ETHNIC/RELIGIOUS VIOLENCE)
        // Triggered by: high social unrest, religious distance, low justice
        // coverage, wealth inequality. Blocked by OpenCitizenship law.
        // Effects: zero-sum wealth transfer, casualties, emigration.
        // Runs per-country in parallel after assimilation (demographics settled).
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let pogrom_config = crate::economy::ethnic_violence::PogromConfig::default();
            let _pogrom_results = crate::economy::ethnic_violence::check_pogrom_triggers(
                task.ctx.country,
                &task.ctx.buildings,
                &pogrom_config,
                current_turn,
            );
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 18C: TERRORISM (ASYMMETRIC WARFARE)
        // Triggered by extreme radicalization + high unrest + low intelligence.
        // Destroys state buildings, reduces B2B inventory, creates casualties.
        // Runs after pogroms (Phase 17C), before See reinvestment.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let _terrorism_result =
                check_terrorism_triggers(task.ctx.country, &mut task.ctx.buildings, current_turn);
        });

        tasks.sort_by(|a, b| a.ctx.country_name.cmp(&b.ctx.country_name));

        // ═══════════════════════════════════════════════════════════
        // PHASE 17C: APOSTOLIC SEE REINVESTMENT (GLOBAL)
        // Distributes the global charity pool to poor countries and invests
        // FDI in the See's host country. Runs sequentially after all countries
        // have been sorted and merged, since it needs access to all companies.
        // ═══════════════════════════════════════════════════════════
        {
            let see_config = crate::economy::religious_economy::ApostolicSeeConfig::default();
            let _gdp_per_capita: std::collections::BTreeMap<String, f64> = tasks
                .iter()
                .map(|t| {
                    (
                        t.ctx.country_name.clone(),
                        t.ctx.country.macro_indicators.average_wage,
                    )
                })
                .collect();
            let see_country = market.apostolic_see_ledger.see_country.clone();

            // Collect Religion-sector company IDs from all tasks for charity distribution.
            let religion_company_ids: Vec<String> = tasks
                .iter()
                .flat_map(|t| {
                    t.companies
                        .iter()
                        .filter(|c| c.sector == crate::registries::enums::Sector::Religion)
                        .take(3)
                        .map(|c| c.id.clone())
                })
                .collect();

            // Collect See country company IDs for FDI.
            let see_company_ids: Vec<String> = tasks
                .iter()
                .filter(|t| t.ctx.country_name == see_country)
                .flat_map(|t| t.companies.iter().map(|c| c.id.clone()))
                .collect();

            // Process See reinvestment by crediting companies across tasks.
            if market.apostolic_see_ledger.global_charity_pool > see_config.reinvestment_threshold {
                let available = market.apostolic_see_ledger.global_charity_pool
                    - see_config.reinvestment_threshold;
                let charity_amount = available * see_config.charity_distribution_rate;
                let fdi_amount = available * see_config.fdi_rate;

                // Charity: distribute to Religion-sector companies.
                if charity_amount > 0.0 && !religion_company_ids.is_empty() {
                    let per_company = charity_amount / religion_company_ids.len() as f64;
                    let mut distributed = 0.0_f64;
                    for task in tasks.iter_mut() {
                        for company_id in &religion_company_ids {
                            if distributed >= charity_amount {
                                break;
                            }
                            let amount = per_company.min(charity_amount - distributed);
                            if amount > 0.0
                                && crate::economy::transfer_settler::credit_company_by_id(
                                    &mut task.companies,
                                    company_id,
                                    amount,
                                )
                            {
                                distributed += amount;
                            }
                        }
                    }
                    market.apostolic_see_ledger.global_charity_pool -= distributed;
                }

                // FDI: invest in See country companies.
                if fdi_amount > 0.0 && !see_company_ids.is_empty() {
                    let per_company = fdi_amount / see_company_ids.len() as f64;
                    let mut invested = 0.0_f64;
                    for task in tasks.iter_mut() {
                        for company_id in &see_company_ids {
                            if invested >= fdi_amount {
                                break;
                            }
                            let amount = per_company.min(fdi_amount - invested);
                            if amount > 0.0
                                && crate::economy::transfer_settler::credit_company_by_id(
                                    &mut task.companies,
                                    company_id,
                                    amount,
                                )
                            {
                                invested += amount;
                            }
                        }
                    }
                    market.apostolic_see_ledger.global_charity_pool -= invested;
                }
            }
        }

        for task in &tasks {
            merge_orders(&mut global_orders, &task.orders);
        }

        // Update global base prices from the local cleared prices produced by
        // resolve_market_prices, and recompute global net surplus from the
        // aggregated order book.
        let mut price_samples: HashMap<Commodity, Vec<f64>> = HashMap::new();
        for task in &tasks {
            for (&good, &price) in &task.ctx.market_prices {
                price_samples.entry(good).or_default().push(price);
            }
        }
        let mut new_prices: Vec<(Commodity, f64)> = Vec::with_capacity(price_samples.len());
        for (good, samples) in &mut price_samples {
            // Sort samples so the floating-point average is independent of the
            // (randomized) task/HashMap iteration order.
            samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let avg = samples.iter().sum::<f64>() / samples.len() as f64;
            // Exponential smoothing to dampen the global price feedback loop.
            let old = market.base_prices.get(good).copied().unwrap_or(100.0);
            let smoothed = old * 0.7 + avg * 0.3;
            new_prices.push((*good, smoothed));
        }
        for (good, price) in new_prices {
            market.base_prices.insert(good, price);
        }
        market.net_surplus = global_orders
            .orders
            .iter()
            .map(|(good, order)| (*good, order.sell - order.buy))
            .collect();

        // ═══════════════════════════════════════════════════════════
        // PHASE 29: PMI DIFFUSION INDEX (end-of-turn computation)
        // Replaces the old employment/capacity ratio PMI with a proper
        // diffusion index using Orders, Production, Employment, Deliveries,
        // and Inventories. Uses previous-turn telemetry for delta calculations.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let sectors: Vec<Sector> = task.ctx.country.budget.sectors.keys().copied().collect();
            for sector in sectors {
                // Gather previous-turn telemetry from sector.extra
                let prev_telemetry: HashMap<String, f64> = {
                    let share = task.ctx.country.budget.sectors.get(&sector);
                    let mut prev = HashMap::new();
                    if let Some(s) = share {
                        for key in &[
                            "_prev_orders",
                            "_prev_production",
                            "_prev_deliveries",
                            "_prev_inventory",
                        ] {
                            if let Some(v) = s.extra.get(*key).and_then(|v| v.as_f64()) {
                                prev.insert(key.to_string(), v);
                            }
                        }
                    }
                    prev
                };

                let (pmi, components) = crate::economy::indicators::compute_pmi_diffusion_index(
                    sector,
                    &task.ctx.buildings,
                    &global_order_book,
                    &prev_telemetry,
                );

                // Store PMI and sub-components in sector.extra
                if let Some(share) = task.ctx.country.budget.sectors.get_mut(&sector) {
                    share
                        .extra
                        .insert("pmi".to_string(), serde_json::Value::from(pmi));
                    for (key, value) in &components {
                        share
                            .extra
                            .insert(key.clone(), serde_json::Value::from(*value));
                    }
                }
            }
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 24D: MACROECONOMIC TELEMETRY (end-of-turn aggregation)
        // Computes GDP (expenditure approach), dual inflation (CPI & PPI),
        // and money supply (M0/M3) from actual cash flows and VWAP data.
        // Runs in parallel (per-country, no cross-country deps).
        // Net exports are set to 0 here and updated after balance_global_trade.
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let prev_gdp = task.ctx.country.macro_indicators.gdp_breakdown.official_gdp;
            let gdp_breakdown = crate::economy::telemetry::compute_gdp(&task.gdp_acc, prev_gdp);

            let prev_inflation = task.ctx.country.macro_indicators.inflation_indices.clone();
            let inflation_indices = crate::economy::telemetry::compute_inflation(
                &state.market_history,
                &prev_inflation,
            );

            let prev_m3 = task.ctx.country.macro_indicators.money_supply.m3;
            let mut money_supply =
                crate::economy::telemetry::compute_money_supply(&task.companies, task.ctx.country);
            money_supply.previous_m3 = prev_m3;

            // Update MacroData: inflation is now driven by CPI delta.
            task.ctx.country.macro_indicators.inflation = inflation_indices.cpi_inflation;
            task.ctx.country.macro_indicators.gdp_breakdown = gdp_breakdown;
            task.ctx.country.macro_indicators.inflation_indices = inflation_indices;
            task.ctx.country.macro_indicators.money_supply = money_supply;

            // Also update Treasury.gdp for downstream consumers (CB rate setter, etc.)
            task.ctx.country.budget.gdp =
                task.ctx.country.macro_indicators.gdp_breakdown.official_gdp;
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 85A: GUILD SYSTEM CYCLE
        // Phase 86.5B: Wire previously-disconnected guild system.
        // - Check guild formation in GuildBurgher domains
        // - Execute production for existing guilds (consumes raw_inventory
        //   purchased in N-1, produces output)
        // - Distribute dividends pro-rata by production volume
        // - Check dissolution for guilds below min_members
        // Must run BEFORE urbanization cycle so that emancipation checks
        // see updated guild state (Rule 16).
        // ═══════════════════════════════════════════════════════════
        tasks.par_iter_mut().for_each(|task| {
            let guild_config = task.ctx.country.guild_config.clone();
            let average_wage = task
                .ctx
                .country
                .macro_indicators
                .average_wage
                .max(subsistence_wage_floor);

            // 85A.1: Check guild formation in GuildBurgher domains
            for region in &task.ctx.country.regions {
                for micro in region.micro_regions.values() {
                    if !matches!(
                        micro.faction_type,
                        crate::society::geography::FactionDomainType::GuildBurgher
                    ) {
                        continue;
                    }
                    // Aggregate cottage_fte by sector from class demographics
                    let mut cottage_fte_by_sector: std::collections::BTreeMap<String, f64> =
                        std::collections::BTreeMap::new();
                    for demographics in region.class_demographics.rural_classes.values() {
                        // Sum cottage FTE — the sector is derived from the domain
                        // For now, aggregate all cottage FTE into a single sector key
                        *cottage_fte_by_sector
                            .entry("crafts".to_string())
                            .or_insert(0.0) += demographics.cottage_fte_allocated;
                    }
                    if let Some(sector_str) =
                        crate::economy::guild_system::check_guild_formation_trigger(
                            micro,
                            &cottage_fte_by_sector,
                            &guild_config,
                        )
                    {
                        // Check if a guild for this domain+sector already exists
                        let guild_id = format!("GUILD-{}-{}", micro.id, sector_str);
                        let already_exists = task.companies.iter().any(|c| c.id == guild_id);
                        if !already_exists {
                            // Gather contributing classes with their savings
                            let contributing_classes: Vec<(String, f64, f64)> = region
                                .class_demographics
                                .rural_classes
                                .iter()
                                .filter(|(_, d)| d.cottage_fte_allocated > 0.0)
                                .map(|(class_id, d)| {
                                    (class_id.to_string(), d.cottage_fte_allocated, d.savings)
                                })
                                .collect();
                            if !contributing_classes.is_empty() {
                                let sector = match sector_str.as_str() {
                                    "LightIndustry" => {
                                        crate::registries::enums::Sector::LightIndustry
                                    }
                                    "LocalServices" => {
                                        crate::registries::enums::Sector::LocalServices
                                    }
                                    _ => crate::registries::enums::Sector::LightIndustry,
                                };
                                let mut guild = crate::economy::guild_system::create_guild(
                                    micro,
                                    sector,
                                    &contributing_classes,
                                    average_wage,
                                    &guild_config,
                                );
                                guild.region_id = region.id.clone();
                                task.companies.push(guild);
                            }
                        }
                    }
                }
            }

            // 85A.2: Execute production for existing guilds
            // Guilds consume from guild_raw_inventory (purchased in N-1)
            // and produce finished goods. Output is added to company inventory.
            for company in &mut task.companies {
                if !matches!(
                    company.legal_form,
                    crate::entities::legal_form::LegalForm::Guild(_)
                ) {
                    continue;
                }
                // Get member FTE from the guild's domain region
                let member_fte = company
                    .legal_form
                    .guild_data()
                    .map(|d| d.member_workshop_ids.len() as f64 * 10.0)
                    .unwrap_or(0.0);
                if member_fte <= 0.0 {
                    continue;
                }
                // Determine recipe based on guild sector
                let (recipe_input, recipe_output) = match company.sector {
                    crate::registries::enums::Sector::LightIndustry => (
                        crate::registries::enums::Commodity::IndustrialFiber,
                        crate::registries::enums::Commodity::Clothing,
                    ),
                    crate::registries::enums::Sector::LocalServices => (
                        crate::registries::enums::Commodity::Food,
                        crate::registries::enums::Commodity::Clothing,
                    ),
                    _ => continue,
                };
                let result = crate::economy::guild_system::execute_guild_production(
                    company,
                    member_fte,
                    recipe_input,
                    recipe_output,
                    1.0,  // 1 unit input per unit output
                    10.0, // 10 FTE per unit output
                    crate::registries::enums::Commodity::MixedWaste,
                    0.05, // 5% waste
                );
                // Add output to company available_cash (guilds sell via B2C)
                // The output is stored in the result; revenue is added to liquid_capital
                company.liquid_capital += result.revenue;
                // Store profit for dividend distribution
                company.liquid_capital += result.revenue;
            }

            // 85A.3: Distribute dividends for profitable guilds
            for company in &mut task.companies {
                if !matches!(
                    company.legal_form,
                    crate::entities::legal_form::LegalForm::Guild(_)
                ) {
                    continue;
                }
                // Estimate profit as a fraction of liquid_capital
                let profit = company.liquid_capital * 0.1;
                if profit > 0.0 {
                    let member_shares: Vec<(String, f64)> = company
                        .legal_form
                        .guild_data()
                        .map(|d| {
                            d.master_class_ids
                                .iter()
                                .map(|id| (id.clone(), 1.0))
                                .collect()
                        })
                        .unwrap_or_default();
                    let dividends = crate::economy::guild_system::distribute_guild_dividends(
                        company,
                        profit,
                        &member_shares,
                    );
                    // Credit dividends to class savings in the guild's region
                    let region_id = company.region_id.clone();
                    if let Some(region) = task
                        .ctx
                        .country
                        .regions
                        .iter_mut()
                        .find(|r| r.id == region_id)
                    {
                        for (class_id, amount) in &dividends {
                            if let Some(rk) = RuralClass::from_str(class_id) {
                                if let Some(demographics) =
                                    region.class_demographics.rural_classes.get_mut(&rk)
                                {
                                    demographics.savings += amount;
                                }
                            }
                        }
                    }
                }
            }

            // 85A.4: Check dissolution for guilds below min_members
            let config = task.ctx.country.guild_config.clone();
            let mut guilds_to_dissolve: Vec<usize> = Vec::new();
            for (idx, company) in task.companies.iter().enumerate() {
                if !matches!(
                    company.legal_form,
                    crate::entities::legal_form::LegalForm::Guild(_)
                ) {
                    continue;
                }
                let member_count = company
                    .legal_form
                    .guild_data()
                    .map(|d| d.member_workshop_ids.len() as u32)
                    .unwrap_or(0);
                if member_count < config.min_members {
                    // For now, dissolve immediately if below min_members
                    // (turns_below_min tracking would require persistent state)
                    guilds_to_dissolve.push(idx);
                }
            }
            // Dissolve guilds in reverse order
            for &idx in guilds_to_dissolve.iter().rev() {
                let mut company = task.companies[idx].clone();
                let remaining_members: Vec<(String, f64)> = company
                    .legal_form
                    .guild_data()
                    .map(|d| {
                        d.master_class_ids
                            .iter()
                            .map(|id| (id.clone(), 1.0))
                            .collect()
                    })
                    .unwrap_or_default();
                let distributions =
                    crate::economy::guild_system::dissolve_guild(&mut company, &remaining_members);
                // Credit welfare fund distributions to class savings
                let region_id = company.region_id.clone();
                if let Some(region) = task
                    .ctx
                    .country
                    .regions
                    .iter_mut()
                    .find(|r| r.id == region_id)
                {
                    for (class_id, amount) in &distributions {
                        if let Some(rk) = RuralClass::from_str(class_id) {
                            if let Some(demographics) =
                                region.class_demographics.rural_classes.get_mut(&rk)
                            {
                                demographics.savings += amount;
                            }
                        }
                    }
                }
                task.companies.remove(idx);
            }
        });

        // ═══════════════════════════════════════════════════════════
        // PHASE 85B: URBANIZATION CYCLE
        // Runs after B2C clearing and GDP computation, before entity collection.
        // - 85B.1: Emancipation check (GuildBurgher domains → City Regions)
        // - 85B.2: Annexation attempts (City Regions → adjacent parcels)
        // Must run here so that emancipation/annexation effects are visible
        // to the next turn's demographics/labor phase (Rule 16).
        // ═══════════════════════════════════════════════════════════
        let urbanization_config = crate::society::urbanization::EmancipationConfig::default();
        tasks.par_iter_mut().for_each(|task| {
            crate::society::urbanization::process_urbanization_cycle(
                task.ctx.country,
                &mut task.companies,
                &mut task.ctx.buildings,
                &mut task.housing_buildings,
                &mut task.commercial_buildings,
                turn,
                &urbanization_config,
            );
        });

        // Phase 93: Collect depletion buffers from all tasks before they are
        // consumed. These will be applied to state.planet after tasks are dropped
        // (releasing the immutable &state.planet borrow).
        let mut all_depletion_requests: Vec<crate::economy::production::geology::DepletionRequest> =
            Vec::new();
        for task in &mut tasks {
            all_depletion_requests.append(&mut task.depletion_buffer);
        }

        // ── DIAGNOSTIC CHECKPOINT 4: turn_end (pre-writeback) ──
        probe.checkpoint("turn_end", 4, turn, &market, &tasks);

        // Collect entities back from tasks into ctx.entities format.
        for task in tasks {
            let name = task.ctx.country_name.clone();
            entities.insert(
                name,
                crate::engine::turn_context::CountryEntities {
                    companies: task.companies,
                    buildings: task.ctx.buildings,
                    unions: task.unions,
                    commercial_buildings: task.commercial_buildings,
                    housing_buildings: task.housing_buildings,
                },
            );
        }

        // Phase 93: Apply batch depletion to state.planet with pro-rata clamping.
        // This runs after tasks are dropped, releasing the &state.planet borrow.
        // When multiple mines share a vein and total extraction > current_reserves,
        // remaining ore is distributed proportionally (Rule 5 & 20).
        if !all_depletion_requests.is_empty() {
            crate::economy::production::geology::apply_depletion_batch(
                &mut state.planet,
                &all_depletion_requests,
            );
        }

        // Phase 93: Resolve completed geological surveys.
        // Runs after tasks are dropped (releasing &state.planet) so we can
        // mutate the planet to mark discovered veins. Each country's survey
        // ledger is processed against the shared planet.
        //
        // Split borrow: planet and countries are disjoint fields on GameState.
        let planet = &mut state.planet;
        for country in state.countries.values_mut() {
            crate::economy::production::geology::resolve_geological_surveys(
                planet,
                &mut country.geological_survey_ledger,
                year,
            );
        }
    }

    // Phase 54: Record banking history for sparkline tooltips.
    // Runs after all parallel per-country processing is complete and countries
    // are fully updated. Aggregates bank balance sheets per country and stores
    // a rolling window of (reserves, deposits, loans) for UI sparklines.
    for country_name in state.countries.keys() {
        let mut total_reserves = 0.0_f64;
        let mut total_deposits = 0.0_f64;
        let mut total_loans = 0.0_f64;
        if let Some(ents) = entities.get(country_name) {
            for c in &ents.companies {
                if let Some(ref bs) = c.balance_sheet {
                    total_reserves += bs.reserves_at_central_bank;
                    total_deposits += bs.deposits;
                    total_loans += bs.loans_issued.iter().map(|l| l.principal).sum::<f64>();
                }
            }
        }
        let history = state
            .banking_history
            .entry(country_name.clone())
            .or_default();
        history.record(turn, total_reserves, total_deposits, total_loans);
    }

    // Phase 29: Dynamic tariff adjustment based on economic conditions.
    // The ruling party adjusts tariffs in response to trade deficits and
    // domestic industry health before global trade is balanced.
    for country in state.countries.values_mut() {
        crate::politics::trade_policy::adjust_tariffs_for_conditions(country);
    }

    let trade_result = balance_global_trade(state, &global_orders, &market, &diplomacy);

    // Bugfix Sprint: Aggregate per-commodity net trade (imports − exports) from
    // the per-country CommodityTradeEntry vectors into market.net_trade.
    // Sign convention: positive = net importer (goods arrived), negative = net exporter.
    // This is used by the UI identity: Supply − Demand + Net Trade = Net Surplus.
    for delta in &trade_result.deltas {
        for entry in &delta.commodity_entries {
            *market.net_trade.entry(entry.commodity).or_insert(0.0) +=
                entry.import_volume - entry.export_volume;
        }
    }

    // Phase 10: Settle trade deficits via Forex/Gold reserves
    let trade_balances: HashMap<String, f64> = trade_result
        .deltas
        .iter()
        .map(|d| (d.country_name.clone(), d.trade_balance))
        .collect();
    let _settlement_results = settle_trade_deficits(state, &trade_balances, turn);

    // Phase 24D: Update net exports in GDP breakdown from trade balances.
    // The parallel telemetry pass set net_exports = 0; here we patch it with
    // the actual trade balance from balance_global_trade.
    for (country_name, net_exports) in &trade_balances {
        if let Some(country) = state.countries.get_mut(country_name) {
            country.macro_indicators.gdp_breakdown.net_exports = *net_exports;
            country.macro_indicators.gdp_breakdown.official_gdp =
                country.macro_indicators.gdp_breakdown.consumption
                    + country.macro_indicators.gdp_breakdown.government_spending
                    + country.macro_indicators.gdp_breakdown.investment
                    + country.macro_indicators.gdp_breakdown.net_exports;
            country.budget.gdp = country.macro_indicators.gdp_breakdown.official_gdp;
        }
    }

    // Phase 35: Reconcile regional GDP from the national GDP breakdown.
    // National GDP is strictly derived as sum(region.gdp). All four GDP
    // components (C, G, I, NX) are distributed proportionally by population
    // share. This ensures sum(region.gdp) == budget.gdp after every turn.
    for (_country_name, net_exports) in &trade_balances {
        if let Some(country) = state.countries.get_mut(_country_name) {
            let total_pop: i64 = country.regions.iter().map(|r| r.population).sum();
            let total_pop_f = total_pop.max(1) as f64;
            let bd = &country.macro_indicators.gdp_breakdown;
            let national_c = bd.consumption;
            let national_g = bd.government_spending;
            let national_i = bd.investment;
            let national_nx = *net_exports;

            for region in &mut country.regions {
                let pop_share = region.population as f64 / total_pop_f;
                region.gdp = (national_c + national_g + national_i + national_nx) * pop_share;
            }

            // Strict derivation: national GDP = sum(regional GDP)
            let regional_gdp_sum: f64 = country.regions.iter().map(|r| r.gdp).sum();
            country.budget.gdp = regional_gdp_sum;
            country.macro_indicators.gdp_breakdown.official_gdp = regional_gdp_sum;
        }
    }

    // Phase 34: prev_net_surplus is now captured at the START of the turn
    // (see above), not at the end. The old end-of-turn capture caused
    // prev_net_surplus == current net_surplus, producing ToT% = 0.00% always.

    // Phase 24F: Record telemetry history samples for ToT/YoY delta computation.
    // Runs after all GDP/inflation/money_supply updates are finalized.
    for country in state.countries.values_mut() {
        let md = &country.macro_indicators;
        // Phase 90: Compute peasant population (FreePeasant + Serf) for telemetry.
        let mut peasant_pop: f64 = 0.0;
        for region in &country.regions {
            if let Some(fp) = region.class_demographics.rural_classes.get(&RuralClass::FreePeasant) {
                peasant_pop += fp.population as f64;
            }
            if let Some(serf) = region.class_demographics.rural_classes.get(&RuralClass::Serf) {
                peasant_pop += serf.population as f64;
            }
        }
        let furloughed = md.labor_market.furloughed_total;
        let pop = country.budget.population as f64;
        let gdp_pc = if pop > 0.0 {
            md.gdp_breakdown.official_gdp / pop
        } else {
            0.0
        };
        let sample = crate::state::macro_data::TelemetrySample {
            turn,
            year,
            official_gdp: md.gdp_breakdown.official_gdp,
            shadow_gdp: md.gdp_breakdown.shadow_gdp,
            cpi_index: md.inflation_indices.cpi_index,
            ppi_index: md.inflation_indices.ppi_index,
            cpi_inflation: md.inflation_indices.cpi_inflation,
            ppi_inflation: md.inflation_indices.ppi_inflation,
            m0: md.money_supply.m0,
            m3: md.money_supply.m3,
            unemployment_pct: md.labor_market.unemployment_rate,
            average_wage: md.average_wage,
            corruption_index: country
                .politics
                .inspectorate_state
                .as_ref()
                .map(|ist| ist.corruption_index)
                .unwrap_or(0.0),
            total_deceased: 0,       // filled below
            total_disabled: 0,       // filled below
            unable_to_work_fte: 0.0, // filled below
            population: country.budget.population,
            liquid_reserves: country.budget.liquid_reserves,
            peasant_population: peasant_pop,
            furloughed_total: furloughed,
            gdp_per_capita: gdp_pc,
        };
        country.macro_indicators.telemetry_history.push(sample);
    }

    // Phase 24F: Also aggregate OHS casualty counts into the latest telemetry sample.
    for country in state.countries.values_mut() {
        let mut total_deceased: i64 = 0;
        let mut total_disabled: i64 = 0;
        let mut unable_to_work_fte: f64 = 0.0;
        for region in &country.regions {
            for demo in region.class_demographics.rural_classes.values() {
                total_deceased += demo.deceased;
                total_disabled += demo.active_disabled;
                unable_to_work_fte += demo.unable_to_work;
            }
            for demo in region.class_demographics.urban_classes.values() {
                total_deceased += demo.deceased;
                total_disabled += demo.active_disabled;
                unable_to_work_fte += demo.unable_to_work;
            }
        }
        if let Some(last) = country
            .macro_indicators
            .telemetry_history
            .samples
            .last_mut()
        {
            last.total_deceased = total_deceased;
            last.total_disabled = total_disabled;
            last.unable_to_work_fte = unable_to_work_fte;
        }
    }

    // Phase 39: Drain deferred diplomatic action queue sequentially.
    // This runs after all parallel per-country processing is complete,
    // safely handling cross-country mutations (embassy construction,
    // funding transfers) without borrow-checker conflicts.
    crate::state::diplomatic_actions::drain_diplomatic_actions(
        state,
        &state.diplomatic_config.clone(),
    );

    // Phase 24C.3: Auto re-entry for sovereign default forex lockout.
    // Decrement the remaining turns and unlock countries whose default period has ended.
    let mut countries_to_unlock: Vec<String> = Vec::new();
    for (country_id, country) in state.countries.iter_mut() {
        if country.sovereign_default_turns_remaining > 0 {
            country.sovereign_default_turns_remaining -= 1;
            if country.sovereign_default_turns_remaining == 0 {
                countries_to_unlock.push(country_id.clone());
            }
        }
    }
    for country_id in &countries_to_unlock {
        state.forex_market.unlock_country(country_id);
    }

    // Phase 11/66: Update diplomatic relations dynamically based on this turn's events.
    // Must run after balance_global_trade (needs trade balance data) and after
    // military processing (needs front data). New relations apply to next turn's B2B.
    // Phase 66: Also processes ambassador presence, spy activity, and espionage risk.
    let mut intel_updates: Vec<(String, String, crate::international::fog_of_war::IntelLevel)> =
        Vec::new();
    let mut expel_actions: Vec<(String, String)> = Vec::new();
    process_diplomacy_turn(
        state,
        &mut diplomacy,
        &state.diplomatic_config,
        state.calendar.global_turn,
        &mut intel_updates,
        &mut expel_actions,
    );

    // Phase 66: Process intel updates from spy activity
    for (observer, target, new_level) in &intel_updates {
        let target_country = match state.countries.get(target) {
            Some(c) => c,
            None => continue,
        };
        let true_gdp = target_country.budget.gdp;
        let true_military = target_country.order_of_battle.unit_count() as u32;
        let true_treasury = target_country.budget.liquid_reserves;

        let observer_intel = state
            .foreign_intelligence
            .entry(observer.clone())
            .or_default();
        let intel = observer_intel
            .entry(target.clone())
            .or_insert_with(crate::international::fog_of_war::ForeignIntelligence::unknown);
        let mut rng = rand::thread_rng();
        intel.update_from_true_values(
            true_gdp,
            true_military,
            true_treasury,
            *new_level,
            state.calendar.global_turn,
            &mut rng,
        );
    }

    // Phase 66: Process expelled spies — queue ExpelDiplomat actions
    for (home, host) in &expel_actions {
        state.pending_diplomatic_actions.push(
            crate::state::diplomatic_actions::DiplomaticAction::ExpelDiplomat {
                home_country: home.clone(),
                host_country: host.clone(),
            },
        );
    }
    // Drain the newly queued expulsion actions
    crate::state::diplomatic_actions::drain_diplomatic_actions(
        state,
        &state.diplomatic_config.clone(),
    );

    // Phase 66: Process intel for all observer-target pairs
    let country_names: Vec<String> = state.countries.keys().cloned().collect();
    let fog_config = state.fog_of_war_config.clone();
    let current_turn = state.calendar.global_turn;
    let mut foreign_intelligence = std::mem::take(&mut state.foreign_intelligence);
    for observer in &country_names {
        for target in &country_names {
            if observer == target {
                continue;
            }
            crate::international::fog_of_war::process_intel_turn(
                state,
                observer,
                target,
                &mut foreign_intelligence,
                &fog_config,
                current_turn,
            );
        }
    }
    state.foreign_intelligence = foreign_intelligence;

    // Phase 67: Process treaty negotiations, expiry, and reputation recovery.
    let treaty_config = state.treaty_config.clone();
    let current_turn_for_treaties = state.calendar.global_turn;

    // Advance treaty negotiations (requires diplomacy matrix + ambassador counts)
    let diplomacy_ref = &diplomacy;
    let mut ambassador_counts: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, u32>,
    > = std::collections::BTreeMap::new();
    for (name, country) in &state.countries {
        if let Some(reg) = &country.politics.vip_registry {
            for vip in reg.vips.values() {
                if let Some(post) = &vip.diplomatic_post {
                    *ambassador_counts
                        .entry(name.clone())
                        .or_default()
                        .entry(post.host_country.clone())
                        .or_insert(0) += 1;
                }
            }
        }
    }
    state.treaty_registry.advance_negotiations(
        current_turn_for_treaties,
        &treaty_config,
        diplomacy_ref,
        &ambassador_counts,
    );

    // Expire treaties that have reached their duration
    state
        .treaty_registry
        .expire_finished_treaties(current_turn_for_treaties);

    // Reputation recovery for all countries (if no new violations this turn)
    let rep_config = state.reputation_config.clone();
    for country in state.countries.values_mut() {
        country.global_reputation.recover(&rep_config);
    }

    // Phase 67: AI doctrine evaluation and execution for all AI nations.
    let doctrine_config = state.doctrine_config.clone();
    let country_names: Vec<String> = state.countries.keys().cloned().collect();
    for ai_country_name in &country_names {
        let doctrine = crate::international::ai_doctrines::evaluate_doctrine(
            state,
            ai_country_name,
            &doctrine_config,
        );
        // Update the country's doctrine
        if let Some(country) = state.countries.get_mut(ai_country_name) {
            country.geopolitical_doctrine = doctrine.clone();
        }
        // Execute doctrine — generate diplomatic actions
        let mut rng = rand::thread_rng();
        let actions = crate::international::ai_doctrines::execute_doctrine(
            state,
            ai_country_name,
            &doctrine,
            &doctrine_config,
            current_turn_for_treaties,
            &mut rng,
        );
        state.pending_diplomatic_actions.extend(actions);
    }

    // Drain any new diplomatic actions from AI doctrines
    let diplo_config = state.diplomatic_config.clone();
    crate::state::diplomatic_actions::drain_diplomatic_actions(state, &diplo_config);

    // Phase 68: Process international organizations — integration progression, voting evolution.
    let org_config = state.org_config.clone();
    let populations: std::collections::BTreeMap<String, u64> = state
        .countries
        .iter()
        .map(|(k, v)| (k.clone(), v.budget.population))
        .collect();
    state.international_organizations.process_turn(
        current_turn_for_treaties,
        &org_config,
        &populations,
    );

    // Phase 68: Enforce directives — apply fines for non-compliance (double-entry).
    let fines = state
        .international_organizations
        .enforce_directives(current_turn_for_treaties);
    for (country_name, fine_amount, _reason) in fines {
        if let Some(country) = state.countries.get_mut(&country_name) {
            if country.budget.liquid_reserves >= fine_amount {
                country.budget.liquid_reserves -= fine_amount;
                // Credit to the organization that issued the directive
                // (Find the org that has this country as a member with the directive)
                for org in &mut state.international_organizations.organizations {
                    if org.is_member(&country_name) {
                        org.budget.liquid_reserves += fine_amount;
                        break;
                    }
                }
            }
        }
    }

    // Phase 68: Expire sanctions that have reached their duration.
    state
        .active_sanctions
        .expire_finished_sanctions(current_turn_for_treaties);

    // Phase 68: Apply reputation damage to sanctioned countries.
    let sanction_config = state.sanction_config.clone();
    let _rep_config = state.reputation_config.clone();
    let sanctioned_countries: Vec<String> = state
        .countries
        .keys()
        .filter(|name| {
            state
                .active_sanctions
                .is_sanctioned(name, current_turn_for_treaties)
        })
        .cloned()
        .collect();
    for country_name in sanctioned_countries {
        if let Some(country) = state.countries.get_mut(&country_name) {
            country.global_reputation.score = (country.global_reputation.score
                - sanction_config.reputation_damage_per_turn)
                .max(-100.0);
        }
    }

    // Bugfix Sprint: Merge B2B order volumes into the (already-cleared-and-
    // B2C-populated) supply/demand volumes. The turn-start clear at line ~2693
    // eliminated stale values; B2C was added immediately after; now B2B is
    // merged. Result: supply_volume = current_B2B, demand_volume = B2C + current_B2B.
    // The old Phase-80 FIX comment warned against clearing here — that was
    // because the clear used to happen at this location (end-of-turn). The
    // clear now happens at turn start (before B2C aggregation), so this merge
    // is safe and correct.
    for (&good, order) in &global_orders.orders {
        *market.supply_volume.entry(good).or_insert(0.0) += order.sell;
        *market.demand_volume.entry(good).or_insert(0.0) += order.buy;
    }
    // No disk persistence — entities stay in ctx.

    turn += 1;
    // Phase 27: 1 Year = 24 Turns (2 turns per month). Year only increments
    // after a full year of 24 turns has passed. Guard with turn > 0 to avoid
    // firing on turn 0 (game start).
    if turn > 0 && turn.is_multiple_of(24) {
        year += 1;

        // Phase 57: Capital Gains Tax year-end settlement.
        // Runs at fiscal year-end (every 24 turns). Settles all accrued gains/losses,
        // applies tax-loss harvesting with 5-year carry-forward, and credits treasury.
        for country in state.countries.values_mut() {
            // Collect entity IDs and their brokerage cash for the settlement.
            let entity_ids: Vec<String> =
                country.capital_gains_tax.accruals.keys().cloned().collect();

            // Build a map of entity_id → mutable brokerage cash reference.
            // We need to debit from brokerage accounts, so we collect the cash amounts
            // and update them after settlement.
            let mut entity_cash: std::collections::HashMap<String, f64> =
                std::collections::HashMap::new();

            // Get cash from companies' brokerage accounts.
            for entity_id in &entity_ids {
                // Try to find the entity in the turn context entities.
                if let Some(entities) = entities.get(&country.name) {
                    if let Some(company) = entities.companies.iter().find(|c| c.id == *entity_id) {
                        if let Some(ref acct) = company.brokerage_account {
                            entity_cash.insert(entity_id.clone(), acct.cash);
                        }
                    }
                }
            }

            // R4.2: Settle year-end with ACTUAL cash debits.
            // Collect (entity_id, tax_amount) pairs from settle_year_end, then
            // apply debits to brokerage accounts in a sequential post-pass.
            // This replaces the no-op closure that credited treasury without
            // debiting taxpayer cash (fiat creation bug).
            let mut tax_debits: Vec<(String, f64)> = Vec::new();
            let treasury_ref = &mut country.budget.liquid_reserves;
            let total_tax = country.capital_gains_tax.settle_year_end(
                treasury_ref,
                |entity_id, amount| {
                    // Record the debit; actual cash deduction happens after settlement
                    // to avoid borrow conflict with country.capital_gains_tax.
                    tax_debits.push((entity_id.to_string(), amount));
                    true // Accept the debit; collection happens post-pass
                },
            );

            // R4.2: Apply actual cash debits to entity brokerage accounts.
            // If entity lacks sufficient cash, debit what's available (deterministic
            // arrears behavior — no fiat creation from Treasury credit without debit).
            let mut actual_collected = 0.0_f64;
            for (entity_id, tax_amount) in &tax_debits {
                if let Some(entities_ctx) = entities.get_mut(&country.name) {
                    if let Some(company) = entities_ctx.companies.iter_mut().find(|c| c.id == *entity_id) {
                        if let Some(ref mut acct) = company.brokerage_account {
                            let debit = tax_amount.min(acct.cash.max(0.0));
                            acct.cash -= debit;
                            actual_collected += debit;
                        }
                    }
                }
            }
            // R4.2: If actual collected < total_tax (insufficient cash), the
            // treasury was over-credited. Reverse the difference to preserve
            // double-entry. The unpaid amount becomes implicit arrears (loss
            // carry-forward handles future offset).
            let over_credit = total_tax - actual_collected;
            if over_credit > 0.0 {
                country.budget.liquid_reserves -= over_credit;
            }

            // Record the tax collected for UI display.
            country.capital_gains_tax.tax_collected_this_year = total_tax;
            country.capital_gains_tax.annual_tax_history.push(total_tax);
            if country.capital_gains_tax.annual_tax_history.len() > 60 {
                country.capital_gains_tax.annual_tax_history.remove(0);
            }

            // R6.5: Activate dynamic fund creation — once per political year
            // per country, attempt to create a new hedge fund from a wealthy VIP.
            if let Some(ref registry) = country.politics.vip_registry {
                // Find eligible VIPs: influence > 50, age 35-65, ambitious
                let eligible_vips: Vec<_> = registry
                    .vips
                    .values()
                    .filter(|v| {
                        v.base_influence > 50
                            && v.age >= 35
                            && v.age <= 65
                            && crate::corporate::market_behavior::evaluate_market_behavior(
                                &v.traits,
                            )
                            .risk_tolerance
                            > 0.6
                    })
                    .cloned()
                    .collect();
                // Try the first eligible VIP (limit 1 new fund per year)
                if let Some(vip) = eligible_vips.first() {
                    if let Some((fund_id, fund_company)) =
                        crate::securities::funds::try_create_fund_from_vip(
                            vip,
                            None,
                            country,
                            turn,
                            false,
                        )
                    {
                        // Insert the fund into the entity context
                        if let Some(entities_ctx) = entities.get_mut(&country.name) {
                            entities_ctx.companies.push(fund_company);
                        }
                        let _ = fund_id;
                    }
                }
            }

            // R8.5: Reset tax-advantaged contribution tracking at fiscal year-end.
            for region in &mut country.regions {
                for class in region.class_demographics.rural_classes.values_mut() {
                    class.tax_advantaged_contributions_this_year = 0.0;
                }
                for class in region.class_demographics.urban_classes.values_mut() {
                    class.tax_advantaged_contributions_this_year = 0.0;
                }
            }
        }
    }
    // Runs sequentially (post-parallel) for determinism. Each country processes
    // its own cadastre, border conflicts, zoning plans, and arbitration cases.
    for (country_name, country) in &mut state.countries {
        use crate::corporate::market_behavior::evaluate_market_behavior;
        use crate::society::cadastre as cad;
        use crate::society::real_estate_market as rem;

        let current_turn = turn;

        // 59.1: Legal certainty degradation
        cad::process_certainty_degradation(&mut country.cadastre, &country.legal_certainty_config);

        // 59.1: Cadastral survey funding (per region, debiting RegionalBudget)
        // Collect region IDs first to avoid borrow issues
        let region_ids: Vec<String> = country
            .regions
            .iter()
            .filter(|r| r.node_type == crate::society::geography::NodeType::LandRegion)
            .map(|r| r.id.clone())
            .collect();

        for region_id in &region_ids {
            // Find the region's governance and development level
            let (dev_level, has_governance) = country
                .regions
                .iter()
                .find(|r| &r.id == region_id)
                .map(|r| (r.development_level, r.governance.is_some()))
                .unwrap_or((0.0, false));

            if !has_governance {
                continue;
            }

            // Get mutable access to the region's governance budget
            let region_idx = country.regions.iter().position(|r| &r.id == region_id);
            if let Some(idx) = region_idx {
                let governance = &mut country.regions[idx].governance;
                if let Some(gov) = governance {
                    let budget = &mut gov.budget;
                    cad::fund_cadastral_survey(
                        &mut country.cadastre,
                        region_id,
                        budget,
                        &country.cadastre_config,
                        &country.legal_certainty_config,
                        dev_level,
                    );
                }
            }
        }

        // 59.2: Border conflict generation
        let mut rng = rand::thread_rng();
        cad::generate_border_conflicts(
            &mut country.cadastre,
            &mut country.border_conflicts,
            &country.legal_certainty_config,
            &country.cadastre_config,
            current_turn,
            &mut rng,
        );

        // 59.6: Court capacity and border conflict resolution
        let justice_law = country.politics.justice_law.clone().unwrap_or_default();
        let court_wait_time = justice_law.court_wait_time_target;
        for region_id in &region_ids {
            let court_capacity =
                if let Some(idx) = country.regions.iter().position(|r| &r.id == region_id) {
                    let gov = &country.regions[idx].governance;
                    if let Some(g) = gov {
                        cad::compute_court_capacity(&g.budget, &justice_law, &court_wait_time)
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };

            cad::process_border_conflicts(
                &mut country.cadastre,
                &mut country.border_conflicts,
                court_capacity,
                current_turn,
            );
        }

        // 59.3: Autonomous zoning plan enactment by governors
        // Governors enact plans based on national quotas and their traits.
        let quota = country.national_zoning_quota.clone();
        for region_idx in 0..country.regions.len() {
            if country.regions[region_idx].node_type
                != crate::society::geography::NodeType::LandRegion
            {
                continue;
            }
            let region_id = country.regions[region_idx].id.clone();
            let dev_level = country.regions[region_idx].development_level;

            // Get governor traits and derive preferences
            let governor_traits = country.regions[region_idx]
                .governance
                .as_ref()
                .map(|g| g.head.traits.clone())
                .unwrap_or_default();

            let modifiers = evaluate_market_behavior(&governor_traits);
            let preferences = cad::derive_governor_preferences(&modifiers);

            // Check if there's already an active plan
            let has_active_plan = country.regions[region_idx]
                .governance
                .as_ref()
                .map(|g| g.zoning_plans.active_plan_for_region(&region_id).is_some())
                .unwrap_or(false);

            if !has_active_plan {
                // Governor enacts a new plan
                let next_id = country.regions[region_idx]
                    .governance
                    .as_ref()
                    .map(|g| g.zoning_plans.next_plan_id)
                    .unwrap_or(0);

                let plan = cad::governor_enact_zoning_plan(
                    &region_id,
                    &quota,
                    &preferences,
                    current_turn,
                    next_id,
                );

                if let Some(gov) = country.regions[region_idx].governance.as_mut() {
                    gov.zoning_plans.enact_plan(plan);
                    gov.zoning_plans.next_plan_id += 1;
                }
            }

            // Advance implementation progress (budget-draining)
            if let Some(gov) = country.regions[region_idx].governance.as_mut() {
                if let Some(plan) = gov.zoning_plans.active_plan_for_region_mut(&region_id) {
                    let budget = &mut gov.budget;
                    cad::advance_zoning_implementation(
                        &mut country.cadastre,
                        plan,
                        budget,
                        &country.cadastre_config,
                        current_turn,
                    );
                }
            }

            let _ = dev_level;
        }

        // 59.4: Apply externality penalties
        cad::apply_externality_penalties(&mut country.cadastre, &country.externality_config);

        // 59.5: Arbitration case processing
        cad::process_arbitration_cases(
            &mut country.arbitration_court,
            &country.arbitration_config,
            current_turn,
            &mut rng,
        );

        // 59.5: Pay accrued arbitration compensation from treasury
        cad::pay_arbitration_compensation(
            &mut country.arbitration_court,
            &mut country.budget.liquid_reserves,
        );

        // Phase 62.2: Process adverse possession (Zasiedzenie)
        let ap_config = cad::AdversePossessionConfig::default();
        rem::process_adverse_possession(
            &mut country.cadastre,
            &mut country.regions,
            current_turn,
            &ap_config,
            &mut rng,
        );

        // Phase 62.4: Process immissions (pollution spread via topological graph)
        let immission_config = cad::ImmissionConfig::default();
        rem::process_immissions(
            &mut country.cadastre,
            &mut country.arbitration_court,
            &immission_config,
            current_turn,
        );

        // Phase 62.5: Process VIP health from pollution
        // Extract region data first to avoid borrow conflicts.
        let vip_region_map: std::collections::HashMap<String, String> = {
            let mut map = std::collections::HashMap::new();
            if let Some(ref vip_registry) = country.politics.vip_registry {
                for (vip_id, vip) in &vip_registry.vips {
                    if let Some(region_id) = rem::infer_vip_region(vip, country) {
                        map.insert(vip_id.clone(), region_id);
                    }
                }
            }
            map
        };
        // Compute regional pollution levels
        let region_pollution: std::collections::HashMap<String, f64> = {
            let mut rp = std::collections::HashMap::new();
            let mut counts: std::collections::HashMap<String, u32> =
                std::collections::HashMap::new();
            for parcel in country.cadastre.parcels.values() {
                *rp.entry(parcel.region_id.clone()).or_insert(0.0) += parcel.pollution_level;
                *counts.entry(parcel.region_id.clone()).or_insert(0) += 1;
            }
            let keys: Vec<String> = rp.keys().cloned().collect();
            for key in keys {
                let total = rp.remove(&key).unwrap_or(0.0);
                let count = *counts.get(&key).unwrap_or(&1) as f64;
                rp.insert(key, total / count);
            }
            rp
        };
        if let Some(ref mut vip_registry) = country.politics.vip_registry {
            let vip_ids: Vec<String> = vip_registry.vips.keys().cloned().collect();
            for vip_id in vip_ids {
                let vip = match vip_registry.vips.get_mut(&vip_id) {
                    Some(v) => v,
                    None => continue,
                };
                if vip.is_dead {
                    continue;
                }
                let region_id = match vip_region_map.get(&vip_id) {
                    Some(r) => r.clone(),
                    None => continue,
                };
                let pollution = *region_pollution.get(&region_id).unwrap_or(&0.0);
                if pollution > immission_config.health_impact_threshold {
                    vip.health.physical_health -=
                        pollution * immission_config.physical_health_decay_rate;
                    vip.health.mental_health -=
                        pollution * immission_config.mental_health_decay_rate;
                    vip.health.physical_health = vip.health.physical_health.max(0.0);
                    vip.health.mental_health = vip.health.mental_health.max(0.0);
                    if vip.health.physical_health < immission_config.death_threshold {
                        vip.is_dead = true;
                        vip.death_turn = Some(current_turn);
                        vip.incapacity = crate::politics::vip_registry::IncapacityStatus::Dead;
                    }
                    if vip.health.mental_health < immission_config.breakdown_threshold
                        && matches!(
                            vip.incapacity,
                            crate::politics::vip_registry::IncapacityStatus::Healthy
                        )
                    {
                        vip.incapacity = crate::politics::vip_registry::IncapacityStatus::Sick;
                    }
                } else {
                    vip.health.physical_health = (vip.health.physical_health
                        + immission_config.health_recovery_rate)
                        .min(1.0);
                    vip.health.mental_health =
                        (vip.health.mental_health + immission_config.health_recovery_rate).min(1.0);
                }
            }
        }

        let _ = country_name;
    }

    // Sync state.calendar so the TUI and snapshots show the correct turn/year.
    // AI & Stability Audit (Pillar 1A): Use start_month-aware formula to fix
    // the time-travel bug where Turn 1 jumped back to January instead of
    // staying in September (start_month = 9). The old formula
    // `((turn - 1) % 24) / 2 + 1` ignored start_month entirely.
    state.calendar.global_turn = turn;
    state.calendar.current_year = year;
    if turn > 0 {
        let turn_in_year = (turn - 1) % 24;
        state.calendar.current_month =
            (turn_in_year / 2 + state.calendar.start_month.saturating_sub(1)) % 12 + 1;
        state.calendar.half_month = turn_in_year % 2 == 1;
    } else {
        // Turn 0: use start_month directly (game start).
        state.calendar.current_month = state.calendar.start_month.max(1).min(12);
        state.calendar.half_month = false;
    }
    update_storage(state, turn, year);

    // Write results back into ctx (no disk I/O).
    ctx.market = market;
    ctx.diplomacy = diplomacy;
    ctx.entities = entities;

    Ok(())
}

/// Phase 31: Track consecutive zero-value turns for crisis detection.
///
/// Stores a counter in the country's `macro_indicators.extra` map.
/// If `is_zero` is true, increments the counter; otherwise resets to 0.
/// Returns the current (post-update) counter value.
fn track_consecutive_zero(
    extra: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    is_zero: bool,
) -> u32 {
    let current = extra
        .get(key)
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(0);
    let new_val = if is_zero { current + 1 } else { 0 };
    extra.insert(key.to_string(), serde_json::json!(new_val));
    new_val
}

/// Builds a per-country `MarketSignal` from cleared prices, order imbalance,
/// sector PMI and capital-market data.
fn build_market_signal(
    country: &Country,
    orders: &MarketOrders,
    global_market: &GlobalMarket,
    prices: &FxHashMap<Commodity, f64>,
) -> MarketSignal {
    let interest_rate = country.central_bank.interest_rates.reference_rate;

    let mut sector_pmi = FxHashMap::default();
    for (sector, share) in &country.budget.sectors {
        if let Some(pmi) = share.extra.get("pmi").and_then(|v| v.as_f64()) {
            sector_pmi.insert(*sector, pmi);
        }
    }

    let mut demand_surplus = FxHashMap::default();
    for (good, order) in &orders.orders {
        demand_surplus.insert(*good, order.buy - order.sell);
    }

    MarketSignal {
        prices: prices.clone(),
        demand_surplus,
        sector_pmi,
        global_surplus: global_market.net_surplus.clone(),
        interest_rate,
        stock_confidence: country.budget.stock_market.confidence,
        stock_index: country.budget.stock_market.index,
    }
}

fn update_storage(state: &mut GameState, turn: u32, year: u32) {
    state
        .extra
        .insert("current_turn".to_string(), serde_json::Value::from(turn));
    state
        .extra
        .insert("year".to_string(), serde_json::Value::from(year));
}

fn merge_orders(target: &mut MarketOrders, source: &MarketOrders) {
    let mut goods: Vec<&Commodity> = source.orders.keys().collect();
    goods.sort();
    for good in goods {
        let order = &source.orders[good];
        let entry = target.orders.entry(*good).or_default();
        entry.buy += order.buy;
        entry.sell += order.sell;
    }
}

// ============================================================================
// PHASE 32: PARLIAMENT BUILDING PAYROLL & PROCUREMENT
// ============================================================================

/// Process Parliament building payroll and procurement for one turn.
///
/// # Rules (User-Mandated Corrections)
/// * MP salaries → credited to `"Bourgeoisie"` (urban) in capital region.
/// * Staff salaries → credited to `"Worker"` (urban) in capital region.
/// * If Treasury cannot afford payroll: NO money printed, payroll fails.
///   - `building.condition -= 0.05` (rapid degradation)
///   - `political_capital -= 20.0` (massive hit)
///   - Coalition partners' `factional_tension += 0.15` (splintering risk)
/// * When Parliament is suspended (State of Emergency):
///   - MP wages not paid (savings).
///   - Staff wages continue (skeleton crew) → credited to `"Worker"`.
///   - Goods consumption reduced by 80%.
fn process_parliament_building_payroll(
    country: &mut crate::state::Country,
    _current_turn: u32,
) -> Vec<String> {
    let mut messages = Vec::new();

    // Find the Parliament building (Sector::Government).
    // Buildings are stored in the data directory, not in Country directly.
    // We check if there's a parliament_struct to determine if parliament exists.
    let has_parliament = country.politics.parliament_struct.is_some();
    if !has_parliament {
        return messages;
    }

    // Check if parliament is suspended.
    let parliament_suspended = country
        .politics
        .state_of_emergency
        .as_ref()
        .map(|soe| soe.can_bypass_parliament())
        .unwrap_or(false);

    // Calculate total seats (MPs) and staff.
    let total_seats = country
        .politics
        .parliament_struct
        .as_ref()
        .map(|p| p.lower_seats())
        .unwrap_or(0);
    if total_seats == 0 {
        return messages;
    }

    let staff_count = total_seats * 2; // 2 staff per MP.
                                       // Phase 86.5A: Use config-driven wage ratios instead of magic numbers.
    let average_wage = country.budget.gdp / country.budget.population.max(1) as f64
        * country.turn_config.parliament_mp_salary_wage_ratio;
    let mp_salary = average_wage * country.turn_config.parliament_mp_salary_multiplier;
    let staff_salary = average_wage * country.turn_config.parliament_staff_salary_ratio;

    let mp_payroll = if parliament_suspended {
        0.0 // MPs not paid when suspended.
    } else {
        total_seats as f64 * mp_salary
    };
    let staff_payroll = staff_count as f64 * staff_salary; // Staff always paid.
    let total_payroll = mp_payroll + staff_payroll;

    // Find the capital region.
    let capital_idx = country.regions.iter().position(|r| r.is_capital);

    // Check if Treasury can afford the payroll.
    if country.budget.liquid_reserves < total_payroll {
        // PAYROLL FAILS — no money printed (Correction 4).
        let shortfall = total_payroll - country.budget.liquid_reserves;

        // 1. Political capital crashes.
        // Phase 35: Scale the penalty per-turn (20.0/24.0 ≈ 0.83) so the
        // yearly total is still ~20. Previously this deducted 20.0 EVERY
        // turn, which cascaded political_capital to 0.0 in 4 turns after
        // the yearly regeneration from process_political_year.
        country.politics.political_capital =
            (country.politics.political_capital - 20.0 / 24.0).max(0.0);

        // 2. Coalition factional tension rises.
        let coalition = country.politics.coalition.clone();
        for party_id in &coalition {
            if let Some(party) = country.politics.active_parties.get_mut(party_id) {
                party.organization.factional_tension =
                    (party.organization.factional_tension + 0.15).min(1.0);
            }
        }
        // Also affect ruling party.
        let ruling = country.politics.ruling_party.clone();
        if let Some(party) = country.politics.active_parties.get_mut(&ruling) {
            party.organization.factional_tension =
                (party.organization.factional_tension + 0.15).min(1.0);
        }

        messages.push(format!(
            "[PARLIAMENT BANKRUPT] Payroll failed — shortfall: {:.0}. Political capital collapsing, coalition tension rising.",
            shortfall
        ));

        // No wages credited to any class.
        return messages;
    }

    // Treasury can afford payroll — debit and credit specific classes.
    country.budget.liquid_reserves -= total_payroll;

    // Credit MP salaries to Bourgeoisie in capital region (Correction 3).
    if mp_payroll > 0.0 {
        if let Some(cap_idx) = capital_idx {
            let region = &mut country.regions[cap_idx];
            if let Some(bourgeoisie) = region
                .class_demographics
                .urban_classes
                .get_mut(&UrbanClass::Bourgeoisie)
            {
                bourgeoisie.savings += mp_payroll;
            } else if let Some(worker) = region.class_demographics.urban_classes.get_mut(&UrbanClass::Worker) {
                // Fallback: if no Bourgeoisie, credit to Worker.
                worker.savings += mp_payroll;
            }
        }
    }

    // Credit staff salaries to Worker in capital region (Correction 3).
    if staff_payroll > 0.0 {
        if let Some(cap_idx) = capital_idx {
            let region = &mut country.regions[cap_idx];
            if let Some(worker) = region.class_demographics.urban_classes.get_mut(&UrbanClass::Worker) {
                worker.savings += staff_payroll;
            }
        }
    }

    messages.push(format!(
        "[PARLIAMENT] Payroll: {:.0} (MPs: {:.0}, Staff: {:.0}) {}",
        total_payroll,
        mp_payroll,
        staff_payroll,
        if parliament_suspended {
            "(suspended — MPs unpaid)"
        } else {
            ""
        }
    ));

    messages
}

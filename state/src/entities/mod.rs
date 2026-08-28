//! Corporate entities — companies and individual buildings.
//!
//! This module defines the typed in-memory representation of the dynamic
//! production actors: [`Company`] (a collection of buildings) and
//! [`Building`] (a single production site). Both structs preserve the Polish
//! JSON keys used by the Python engine and carry an `#[serde(flatten)]`
//! `extra: Map<String, Value>` catch-all for lossless round-trips.

use crate::registries::enums::{Commodity, Sector};
use crate::registries::tech_tree::TechId;
use crate::state::banking::{BankBalanceSheet, BankType, Borrower};
use crate::state::treasury::ProductionMethodChoice;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};

pub mod legal_form;
pub mod union;

pub use legal_form::*;
pub use union::*;

// ============================================================================
// PHASE 6.3: AGRICULTURAL STRUCTURES
// ============================================================================

/// Agricultural production profile (only for Sector::Agriculture companies)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AgriculturalProfile {
    /// Arable land hectares for annual crops (requires sowing each season)
    #[serde(default)]
    pub arable_land_hectares: f64,

    /// Plantation hectares for perennial crops (orchards, coffee, etc.)
    #[serde(default)]
    pub plantation_hectares: f64,

    /// Active crop batches currently growing/harvesting
    #[serde(default)]
    pub batches: Vec<CropBatch>,

    /// Stabilization Sprint: Parcel IDs from the Cadastre that this farm
    /// owns or leases. Links the farm to specific spatial land parcels,
    /// constraining maximum output by physical land area and soil fertility.
    /// Stored as serializable u32 indices (ParcelId -> u32 via parcel_id_to_index).
    #[serde(default)]
    pub owned_parcel_ids: Vec<u32>,
}

/// Active crop batch tracked per-company
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CropBatch {
    /// Crop type identifier
    pub crop_id: String,

    /// Planned hectares (max physical size of the field, permanent)

    pub planned_hectares: f64,

    /// Active hectares (actually growing this cycle, resets each harvest)
    #[serde(default)]
    pub active_hectares: f64,

    /// Current state in the agricultural cycle
    pub state: CropState,

    /// Turn when this batch was planted
    pub planted_turn: u32,

    /// Accumulated yield (tons) so far
    pub accumulated_yield: f64,

    /// Rot accumulator (0.0 = no rot, 1.0 = 100% destroyed)
    pub rot_accumulator: f64,
}

/// Agricultural crop state in the production cycle
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CropState {
    /// Wintering/dormant (between harvest and next sowing)
    #[default]
    Idle,
    /// Sowing phase (arable crops only - costs money for seeds/fertilizer)
    Sowing,
    /// Growing phase (vegetative growth)
    Growing,
    /// Harvesting phase (labor-intensive, rot penalty applies)
    Harvesting,
}


// ============================================================================
// PHASE 7: PATENT AND LICENSING STRUCTURES
// ============================================================================

/// Patent granted to a company for a commercial technology.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Patent {
    /// Technology ID this patent covers.

    pub tech_id: TechId,
    /// Turn when patent was granted.

    pub granted_turn: u32,
    /// Turn when patent expires.

    pub expires_turn: u32,
    /// VWAP ratio for royalty calculation (e.g., 0.05 for 5% of output commodity VWAP).

    pub royalty_vwap_ratio: f64,
}

/// Licensed production method obtained from another company.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LicensedMethod {
    /// Technology ID of the licensed method.

    pub tech_id: TechId,
    /// Company ID of the licensor (patent holder).

    pub licensor_company_id: String,
    /// Turn when license was signed.

    pub licensed_turn: u32,
}

// ============================================================================
// STAGE C: TAX EXEMPTION TRAIT
// ============================================================================

/// Tax exemption trait for entities.
pub trait TaxExempt {
    /// Check if entity is tax-exempt based on sovereign ownership.
    ///
    /// # Arguments
    /// * `sovereign_id` - The sovereign entity ID (e.g., country.id or state_treasury_id)
    ///
    /// # Returns
    /// * `true` if sovereign entity owns 100% of the entity
    fn is_tax_exempt(&self, sovereign_id: &str) -> bool;

    /// Get exemption reason.
    ///
    /// # Returns
    /// * Reason for exemption if applicable
    fn exemption_reason(&self) -> Option<String>;
}

/// Implement for state-owned entities (using owners map, NOT deprecated state_share).
impl TaxExempt for Company {
    fn is_tax_exempt(&self, sovereign_id: &str) -> bool {
        // Check if sovereign entity owns 100% of the company
        self.owners.get(sovereign_id).is_some_and(|&share| share >= 1.0)
    }

    fn exemption_reason(&self) -> Option<String> {
        Some("Sovereign entity - self-taxation prohibited".to_string())
    }
}

/// Aggregate ownership distribution for a company (`"akcjonariat"`).
pub type ShareholderRegister = BTreeMap<String, u64>;

/// Aggregate production/employment statistics for a regional cluster or national champion.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct AggregatedStats {
    /// Total employment across all aggregated units (was: zatrudnienie).
    #[serde(default)]
    pub total_employment: u32,
    /// Total production by commodity (was: produkcja).
    #[serde(default)]
    pub total_production: BTreeMap<Commodity, f64>,
    /// Total dividends distributed (was: dywidendy).
    #[serde(default)]
    pub total_dividends: f64,
    /// Any additional aggregate fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// Phase 7: Pending expansion request from `CorporateAction::Expand`.
/// Set by `apply_action`, consumed by `process_companies` to create
/// a `ConstructionProject` on the appropriate building.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PendingExpansion {
    /// Investment amount (fixed capital to add on completion).
    pub investment: f64,
    /// New worker capacity to add on completion.
    pub new_workers: u32,
}

/// A company / corporate entity (`firma`).
///
/// Phase 47: Defines a company's seasonal operating envelope.
/// Companies with a seasonal profile furlough workers during off-season,
/// retaining only a standby crew for maintenance/security.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SeasonalProfile {
    /// Seasons in which the company operates at full capacity.
    /// Off-season seasons trigger furlough logic.
    #[serde(default)]
    pub active_seasons: std::collections::BTreeSet<crate::state::Season>,
    /// Fraction of peak FTE retained as standby crew during off-season (0.0-1.0).
    /// Default 0.20 (20% standby for maintenance/security).
    #[serde(default = "default_standby_fte_fraction")]
    pub standby_fte_fraction: f64,
    /// Current operational state, recomputed each turn from calendar + profile.
    #[serde(default)]
    pub current_state: SeasonalState,
}

fn default_standby_fte_fraction() -> f64 {
    0.20
}

/// Phase 47: Seasonal operational state of a company.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SeasonalState {
    #[default]
    /// Company is operating at full capacity (in-season).
    Active,
    /// Company is in off-season furlough with standby crew only.
    Furloughed,
}

/// AI & Stability Audit (Pillar 4B): Proto-learning ledger tracking the ROI
/// outcome of past actions. If an action type (e.g., Expansion) was followed
/// by declining ROI, a negative penalty weight is applied to future decisions
/// of the same type. This simulates trial-and-error learning without requiring
/// complex ML.
///
/// # Lifecycle
/// * **Birth**: Created with `Default` when a company is created.
/// * **Life**: Updated each turn — new actions are recorded, old outcomes are
///   evaluated, weights are recomputed.
/// * **Death**: Pruned entries >12 turns old are removed. The ledger is
///   destroyed when the company goes bankrupt.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ActionLedger {
    /// Map of action type → list of (turn_taken, net_profit_at_action) pairs.
    /// The ROI is evaluated 3 turns later by comparing current net_profit
    /// to the recorded value.
    #[serde(default)]
    pub action_records: BTreeMap<String, Vec<(u32, f64)>>,

    /// Current penalty weights per action type (0.0 = no penalty, 1.0 = full block).
    /// Computed from action_records: if average ROI < 0, penalty increases.
    #[serde(default)]
    pub action_weights: BTreeMap<String, f64>,
}

impl ActionLedger {
    /// Record that a major action was taken this turn.
    ///
    /// # Arguments
    /// * `action_type` - String identifier for the action (e.g., "Expand", "Furlough")
    /// * `turn` - Current global turn
    /// * `net_profit` - Current net profit (for later ROI comparison)
    pub fn record_action(&mut self, action_type: &str, turn: u32, net_profit: f64) {
        self.action_records
            .entry(action_type.to_string())
            .or_default()
            .push((turn, net_profit));
    }

    /// Evaluate past actions and update penalty weights.
    ///
    /// For each action recorded 3+ turns ago, compute the ROI delta
    /// (current_profit - recorded_profit) and update the weight.
    /// Old records (>12 turns) are pruned.
    ///
    /// # Arguments
    /// * `current_turn` - Current global turn
    /// * `current_net_profit` - Current net profit for ROI comparison
    pub fn evaluate_and_update(&mut self, current_turn: u32, current_net_profit: f64) {
        const EVALUATION_DELAY: u32 = 3;
        const PRUNE_AGE: u32 = 12;

        for (action_type, records) in &mut self.action_records {
            let mut roi_sum = 0.0;
            let mut roi_count = 0;

            records.retain(|(turn, profit_at_action)| {
                let age = current_turn.saturating_sub(*turn);
                // Prune old records
                if age > PRUNE_AGE {
                    return false;
                }
                // Evaluate records that are old enough (3+ turns)
                if age >= EVALUATION_DELAY {
                    let roi = current_net_profit - profit_at_action;
                    roi_sum += roi;
                    roi_count += 1;
                }
                true
            });

            // Update weight: negative ROI → higher penalty
            if roi_count > 0 {
                let avg_roi = roi_sum / roi_count as f64;
                // weight = clamp(-avg_roi * 0.5, 0.0, 1.0)
                // Negative ROI (profit declined) → positive weight (penalty)
                // Positive ROI (profit improved) → zero weight (no penalty)
                let weight = (-avg_roi * 0.5).clamp(0.0, 1.0);
                self.action_weights.insert(action_type.clone(), weight);
            }
        }

        // Remove empty action_types
        self.action_records.retain(|_, v| !v.is_empty());
    }

    /// Get the penalty weight for a given action type (0.0 = no penalty, 1.0 = full block).
    pub fn weight_for(&self, action_type: &str) -> f64 {
        self.action_weights.get(action_type).copied().unwrap_or(0.0)
    }
}

/// `Company` stores ownership through the typed [`LegalForm`] enum and links
/// to an independent [`Union`] via `union_id`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Company {
    /// Company identifier, e.g. `[KRS-2BD-2395]` (`"id"`).
    #[serde(default)]
    pub id: String,
    /// File stem of the sector file this company was loaded from.
    ///
    /// # Rules
    /// * Not persisted; used by `save_companies` to round-trip companies back
    ///   to the original ASCII file names.
    #[serde(skip)]
    pub file_stem: String,
    /// Company name (was: nazwa).
    #[serde(default)]
    pub name: String,
    /// GDP sector (was: sektor).
    #[serde(default)]
    pub sector: Sector,
    /// Region identifier for regional aggregates (was: region_id).
    #[serde(default)]
    pub region_id: String,
    /// Legal form of the company (was: legal_form).
    #[serde(default)]
    pub legal_form: LegalForm,
    /// State share in `[0.0, 1.0]` (was: udzial_panstwa).
    #[serde(default)]
    pub state_share: f64,
    /// Fixed capital (was: kapital_trwaly).
    #[serde(default)]
    pub fixed_capital: f64,
    /// Liquid capital (was: kapital_plynny).
    #[serde(default)]
    pub liquid_capital: f64,
    /// Phase 6.5: Available cash for B2B orders (uncommitted)
    #[serde(default)]
    pub available_cash: f64,
    /// Phase 6.5: Cash committed to bids but not yet settled
    #[serde(default)]
    pub debit_cash: f64,
    /// Phase 6.5: Cash received from asks but not yet settled
    #[serde(default)]
    pub credit_cash: f64,
    /// Phase 45: Unfilled bid prices from last turn, for dynamic price feedback.
    /// Maps commodity → last unfilled bid limit price.
    /// When a bid goes unfilled, the buyer raises its next bid price.
    #[serde(default)]
    pub unfilled_bid_prices: std::collections::HashMap<Commodity, f64>,
    /// Liabilities (was: zobowiazania).
    #[serde(default)]
    pub liabilities: f64,
    /// Company capital (was: kapital_firmy).
    #[serde(default)]
    pub company_capital: f64,
    /// Number of issued shares (was: liczba_akcji).
    #[serde(default)]
    pub shares_count: u64,
    /// Share price (was: cena_akcji).
    #[serde(default)]
    pub share_price: f64,
    /// Shareholders (was: akcjonariat).
    #[serde(default)]
    pub shareholders: ShareholderRegister,
    /// Price history (was: historia_cen).
    #[serde(default)]
    pub price_history: Vec<f64>,
    /// Financial history (was: historia_finansowa).
    #[serde(default)]
    pub financial_history: Vec<Value>,
    /// Safety level (was: poziom_bhp).
    #[serde(default)]
    pub safety_level: f64,
    /// Optional union / syndicate that represents this company's workers (was: union_id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub union_id: Option<String>,
    /// Building IDs owned by this company (was: budynki).
    #[serde(default)]
    pub building_ids: Vec<String>,
    /// Aggregate scale factor (was: scale_factor).
    #[serde(default)]
    pub scale_factor: u32,
    /// Aggregate worker capacity (was: pojemnosc_pracownikow).
    #[serde(default)]
    pub worker_capacity: u32,
    /// National champion flag (was: is_national_champion).
    #[serde(default)]
    pub is_national_champion: bool,
    /// Whether the company is listed on the stock exchange (was: is_listed).
    #[serde(default)]
    pub is_listed: bool,
    /// CRITICAL: Universal ownership tracking for dividend routing
    /// Maps owner_id (fund_id, founder_id, state_id) to equity percentage (0.0 - 1.0)
    /// This ensures dividends can always be routed to the correct entity.
    #[serde(default)]
    pub owners: BTreeMap<String, f64>,
    /// Free float percentage for listed companies (0.0 - 1.0)
    /// Represents shares circulating on public market (was: free_float).
    #[serde(default)]
    pub free_float: f64,
    /// Aggregate production/employment statistics.
    #[serde(default)]
    pub aggregated_stats: AggregatedStats,
    /// STAGE D PHASE 2: Bank type (only applicable if sector == Banking).
    /// None for non-banking companies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bank_type: Option<BankType>,
    /// STAGE D PHASE 2: Balance sheet (only applicable if sector == Banking).
    /// None for non-banking companies.
    /// For non-banking companies, use existing liquid_capital/liabilities fields.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub balance_sheet: Option<BankBalanceSheet>,
    /// STAGE D PHASE 2: Bank-specific margin over XIBOR for loan pricing.
    /// None for non-banking companies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loan_margin: Option<f64>,
    /// Phase D.4: Brokerage account for trading securities.
    /// Attached to Companies, Demographics, and Institutional Investors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brokerage_account: Option<crate::securities::BrokerageAccount>,
    /// Phase 16: ID of the bank company that holds this company's deposits.
    /// Used by TransferSettler to sync bank balance sheets on cash transfers.
    /// None for companies without a brokerage account or using cash-only.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub primary_bank_id: Option<String>,
    /// Phase 24A.3: ID of the commercial bank that issued this company's working-capital loan.
    /// Used to route corporate interest payments to the correct lending bank via
    /// double-entry accounting. None for companies with no bank loans.
    /// If liabilities > 0 but this is None, the liabilities are invalid (wiped in process_companies).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub outstanding_loan_bank_id: Option<String>,
    /// Phase D.4: Fund type for institutional investors.
    /// None for non-institutional companies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fund_type: Option<crate::securities::FundType>,
    /// Phase D.4: Fund ledger for institutional investor operations.
    /// None for non-institutional companies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fund_ledger: Option<crate::securities::FundLedger>,
    /// Phase 5: Temporary disruption modifier from mass movements (0-1, reset each turn)
    /// Prevents permanent economic death by using transient modifier instead of mutating base stats
    #[serde(default)]
    pub temporary_disruption_modifier: f64,
    /// Phase 6.2: Target FTE demand for this turn (before liquidity clamping)
    /// Phase 77: Changed from f64 to u32 — humans are discrete units.
    #[serde(default)]
    pub target_fte_demand: u32,
    /// Phase 6.2: Offered wage per FTE (currency units per FTE)
    #[serde(default)]
    pub offered_wage_per_fte: f64,
    /// Phase 38: Previous turn's offered wage. Used for Keynesian downward
    /// wage rigidity — wages cannot drop more than 3% per turn.
    #[serde(default)]
    pub prev_offered_wage_per_fte: f64,
    /// Phase 40: Accumulated unpaid wages owed to workers (wage arrears).
    /// When a company cannot afford full payroll, the FTE retention floor
    /// keeps workers employed but unpaid wages accrue here as a liability.
    /// Repaid automatically from future cash (30% of available cash per turn).
    #[serde(default)]
    pub wage_arrears: f64,
    /// Emergency Stabilization: Accumulated unpaid severance owed to laid-off
    /// workers. When a company cannot afford full severance, the unpaid portion
    /// accrues here as a liability. Repaid from future cash at 30%/turn (same
    /// pattern as wage_arrears). Ensures firing is expensive even when cash is
    /// low, forcing the corporate AI to prefer furlough over permanent layoffs.
    #[serde(default)]
    pub severance_arrears: f64,
    /// Emergency Stabilization: Cumulative turns workers have been furloughed.
    /// Incremented each turn for every worker in `furloughed_workers_count`.
    /// Reset to 0 when workers are re-instated. Drives the furlough attrition
    /// rate — workers quit after prolonged unpaid furlough, returning to the
    /// general labor pool. Prevents the "eternal furlough" trap (Rule 8).
    #[serde(default)]
    pub furlough_turns_accumulated: u32,
    /// Phase 40: Productivity penalty from wage arrears (0.0–0.50).
    /// Reduces production output proportionally. Capped at 50% to prevent
    /// total output collapse while still penalizing non-payment.
    #[serde(default)]
    pub productivity_penalty: f64,
    /// Phase 41: Target wage — the company's long-run wage goal.
    /// Adjusts slowly (max 2% per turn) toward market average or profitability-based target.
    /// The offered_wage is then clamped to [target_wage * 0.95, target_wage * 1.05].
    #[serde(default)]
    pub target_wage: f64,
    /// Phase 41: Whether this company's workforce is currently on strike.
    /// Striking companies have 0.0 productivity for that turn.
    /// Workers are not paid by the company during a strike; the union pays
    /// strike benefits from its strike_fund.
    #[serde(default)]
    pub is_striking: bool,
    /// Phase 6.2: FTE actually secured after market clearing
    /// Phase 77: Changed from f64 to u32 — humans are discrete units.
    #[serde(default)]
    pub fulfilled_fte: u32,
    /// Phase 37: FTE secured in the previous turn. Used for hiring frictions
    /// (max 15% growth per turn) and severance pay calculations.
    /// Phase 77: Changed from f64 to u32 — humans are discrete units.
    #[serde(default)]
    pub prev_fulfilled_fte: u32,
    /// Phase 6.3: Physical FTE demand (raw requirement before liquidity clamping)
    /// Used for rot calculation to prevent "Broke Farmer Exploit"
    /// Phase 77: Changed from f64 to u32 — humans are discrete units.
    #[serde(default)]
    pub physical_fte_demand: u32,
    /// Phase 6.3: Receivership status (Commissionership) for bankrupt agricultural companies
    #[serde(default)]
    pub is_in_receivership: bool,
    /// Phase 6.3: Agricultural profile (None for non-agricultural sectors)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agricultural_profile: Option<AgriculturalProfile>,
    /// Phase 7: Accumulated R&D investment budget.
    #[serde(default)]
    pub rd_budget: f64,
    /// Phase 7: Patents owned by this company.
    #[serde(default)]
    pub patents: Vec<Patent>,
    /// Phase 7: Licensed production methods from other companies.
    #[serde(default)]
    pub licensed_methods: Vec<LicensedMethod>,
    /// Phase 24C.7: Bounded rationality information quality tier.
    /// Determines how accurately the company estimates costs and market conditions.
    /// Computed each turn from company capital and average wage.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub information_quality: Option<crate::corporate::bounded_rationality::InformationQuality>,
    /// Phase 7: Pending expansion request from `CorporateAction::Expand`.
    /// Set by `apply_action`, consumed by `process_companies` to create
    /// a `ConstructionProject` on the appropriate building.
    #[serde(skip)]
    pub pending_expansion: Option<PendingExpansion>,
    /// Phase 18A: Shadow employment (off-the-books undocumented workers).
    /// None for companies that don't hire illegals or aren't in labor-intensive sectors.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub shadow_employment: Option<crate::economy::legal_status::ShadowEmployment>,
    /// Phase 19A: Product blueprints owned (designed) by this company.
    /// Each blueprint is a generative product design with quality, durability,
    /// and a bill of materials (with chosen substitutes).
    #[serde(default)]
    pub blueprints: Vec<crate::economy::blueprints::ProductBlueprint>,
    /// Phase 19A: Blueprints this company has licensed from other companies
    /// (domestic or foreign) or from the state. Royalties are paid each turn
    /// the company produces output under a licensed blueprint.
    #[serde(default)]
    pub licensed_blueprints: Vec<crate::economy::blueprints::LicensedBlueprint>,

    /// Phase 22D: Reputation score (0.0 = ruined, 100.0 = exemplary).
    /// Drives tender blacklist exclusion and lawsuit penalty scaling.
    /// Default 50.0 (neutral start). Clamped to [0, 100].
    #[serde(default = "default_reputation_score")]
    pub reputation_score: f64,

    /// Phase 35: Rolling history of recent donation inflows (last N turns).
    /// Used by NGO/Religion sectors to smooth wage budgets and prevent
    /// one-turn donation spikes from causing mass hiring, or next-turn
    /// donation droughts from causing mass firing. Empty for non-charity sectors.
    #[serde(default)]
    pub donation_history: Vec<f64>,

    /// Phase 35: DSPW (Primary Dealer) status for banks.
    /// When true, this bank is authorized to participate directly in primary
    /// sovereign bond auctions. Non-DSPW banks can only buy on secondary market.
    #[serde(default)]
    pub is_dspw: bool,

    /// Phase 35: Consumer loan portfolio (B2C loans issued to households).
    /// Each entry tracks an outstanding consumer loan with the class demographic
    /// key, principal, interest rate, and issuing bank.
    #[serde(default)]
    pub consumer_loans: Vec<crate::state::banking::ConsumerLoan>,

    /// Phase 39: Annual profit accumulator for SOE dividend calculation.
    /// Accumulates `last_profit` each turn; drained annually during
    /// process_political_year to pay dividends to the treasury.
    #[serde(default)]
    pub annual_profit_accumulator: f64,

    /// Phase 47: Seasonal operation profile for climate-dependent companies.
    /// None = year-round operation (default for all non-seasonal sectors).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seasonal_profile: Option<SeasonalProfile>,

    /// Phase 47: FTE currently on furlough (authorized seasonal leave).
    /// These workers are "held" by the company and do NOT participate in the
    /// active labor market clearing during off-season. They are automatically
    /// re-instated when the season reactivates. This prevents furloughed workers
    /// from flooding the general labor pool and distorting unemployment/wages.
    /// Zero when the company is Active or has no seasonal profile.
    #[serde(default)]
    pub furloughed_workers_count: f64,

    /// Phase 48: CEO VIP ID (references the global VIP registry).
    /// When None, no CEO is tracked in the VIP registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ceo_vip_id: Option<String>,

    /// Phase 55: Earnings per share (net profit / shares_count).
    /// Computed each turn after process_company. Used for P/E ratio.
    #[serde(default)]
    pub eps: f64,
    /// Phase 55: Price-to-earnings ratio (share_price / eps).
    /// Computed each turn. 0.0 if eps <= 0 or shares_count == 0.
    #[serde(default)]
    pub pe_ratio: f64,
    /// Phase 55: Dividend yield (annualized dividends / market cap).
    /// Computed each turn from aggregated_stats.total_dividends.
    #[serde(default)]
    pub dividend_yield: f64,
    /// Phase 55: Opening share price for the current turn (first trade price).
    /// Set during securities matching. 0.0 if no trades occurred.
    #[serde(default)]
    pub open_price: f64,
    /// Phase 55: Closing share price for the current turn (last trade price).
    /// Set during securities matching. Falls back to share_price if no trades.
    #[serde(default)]
    pub close_price: f64,

    /// AI & Stability Audit (Pillar 4B): Proto-learning ledger tracking the
    /// ROI outcome of past actions. Applies penalty weights to future decisions
    /// of the same type if past actions led to declining ROI.
    #[serde(default)]
    pub action_ledger: ActionLedger,

    /// Any additional company fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

impl Company {
    /// Create a new company with essential parameters.
    ///
    /// # Arguments
    /// * `id` - Company identifier
    /// * `name` - Company name
    /// * `sector` - GDP sector
    /// * `legal_form` - Legal form of the company
    /// * `fixed_capital` - Fixed capital amount
    /// * `liquid_capital` - Liquid capital amount
    /// * `worker_capacity` - Worker capacity
    ///
    /// # Returns
    /// A new Company instance with boilerplate fields defaulted
    ///
    /// # Rules
    /// * liquid_capital is transferred to brokerage_account.cash
    /// * liquid_capital field is zeroed after transfer (prevents capital cloning)
    /// * Use computed_liquid_capital() for runtime queries
    pub fn new(
        id: String,
        name: String,
        sector: Sector,
        legal_form: LegalForm,
        fixed_capital: f64,
        liquid_capital: f64,
        worker_capacity: u32,
    ) -> Self {
        let company_capital = fixed_capital + liquid_capital;
        let is_listed = legal_form.is_listed();
        
        // Create brokerage account and transfer liquid capital
        let brokerage_account = if liquid_capital > 0.0 {
            Some(crate::securities::BrokerageAccount {
                cash: liquid_capital,
                fx_balances: HashMap::new(),
                portfolio: BTreeMap::new(),
                pending_orders: BTreeMap::new(),
                frozen_cash: 0.0,
                is_frozen: false,
                margin_account: None,
                extra: HashMap::new(),
            })
        } else {
            None
        };
        
        Self {
            id,
            file_stem: String::new(),
            name,
            sector,
            region_id: String::new(),
            legal_form,
            state_share: 0.0,
            fixed_capital,
            liquid_capital: 0.0, // Zeroed after transfer to brokerage_account
            available_cash: 0.0,
            debit_cash: 0.0,
            credit_cash: 0.0,
            unfilled_bid_prices: std::collections::HashMap::new(),
            liabilities: 0.0,
            company_capital,
            shares_count: 0,
            share_price: 0.0,
            shareholders: ShareholderRegister::default(),
            price_history: Vec::new(),
            financial_history: Vec::new(),
            safety_level: 0.5,
            union_id: None,
            building_ids: Vec::new(),
            scale_factor: 1,
            worker_capacity,
            is_national_champion: false,
            is_listed,
            owners: BTreeMap::new(),
            free_float: 0.0,
            aggregated_stats: AggregatedStats::default(),
            bank_type: None,
            balance_sheet: None,
            loan_margin: None,
            brokerage_account,
            primary_bank_id: None,
            outstanding_loan_bank_id: None,
            fund_type: None,
            fund_ledger: None,
            temporary_disruption_modifier: 0.0,
            target_fte_demand: worker_capacity,
            offered_wage_per_fte: 0.0,
            prev_offered_wage_per_fte: 0.0,
            wage_arrears: 0.0,
            severance_arrears: 0.0,
            furlough_turns_accumulated: 0,
            productivity_penalty: 0.0,
            target_wage: 0.0,
            is_striking: false,
            fulfilled_fte: 0,
            prev_fulfilled_fte: 0,
            physical_fte_demand: worker_capacity,
            is_in_receivership: false,
            agricultural_profile: None,
            rd_budget: 0.0,
            patents: Vec::new(),
            licensed_methods: Vec::new(),
            information_quality: None,
            shadow_employment: None,
            pending_expansion: None,
            blueprints: Vec::new(),
            licensed_blueprints: Vec::new(),
            reputation_score: 50.0,
            donation_history: Vec::new(),
            is_dspw: false,
            consumer_loans: Vec::new(),
            annual_profit_accumulator: 0.0,
            seasonal_profile: None,
            furloughed_workers_count: 0.0,
            ceo_vip_id: None,
            eps: 0.0, pe_ratio: 0.0, dividend_yield: 0.0, open_price: 0.0, close_price: 0.0,
            action_ledger: ActionLedger::default(),
            extra: Map::new(),
        }
    }
    
    /// Compute liquid capital from brokerage account (runtime query).
    ///
    /// # Returns
    /// * brokerage_account.cash if exists, else 0.0
    ///
    /// # Rules
    /// * Use this instead of the liquid_capital field for runtime queries
    /// * liquid_capital field is zeroed after transfer to prevent cloning
    pub fn computed_liquid_capital(&self) -> f64 {
        self.brokerage_account.as_ref().map(|b| b.cash).unwrap_or(0.0)
    }

    /// Phase 87+: Operational cash available for payroll and short-term obligations.
    /// This is the actual cash source used by the labor market for wage payment
    /// (brokerage_account.cash when present, otherwise available_cash).
    /// Distress and furlough checks MUST use this, not `liquid_capital` (which
    /// is a capital reserve reduced by seed-inventory deductions).
    pub fn operational_cash(&self) -> f64 {
        self.available_cash.max(0.0)
            + self.brokerage_account.as_ref().map(|b| b.cash.max(0.0)).unwrap_or(0.0)
    }

    /// AI & Stability Audit (Pillar 4A): Moving average of net profit over
    /// the last `window` entries in `financial_history`.
    ///
    /// Returns 0.0 if there is insufficient history. This smooths 1-turn
    /// shocks (e.g., a bad harvest turn) so the corporate AI doesn't panic-fire
    /// workers or immediately restructure based on a single bad data point.
    ///
    /// # Arguments
    /// * `window` - Number of recent entries to average (e.g., 3 for a 3-turn
    ///   moving average = 1.5 months of data)
    ///
    /// # Returns
    /// The average net profit over the window, or 0.0 if no history exists.
    pub fn moving_avg_net_profit(&self, window: usize) -> f64 {
        let history = &self.financial_history;
        if history.is_empty() {
            return 0.0;
        }
        let start = history.len().saturating_sub(window);
        let records = &history[start..];
        let sum: f64 = records
            .iter()
            .filter_map(|r| {
                if let Value::Object(map) = r {
                    map.get("net_profit")?.as_f64()
                } else {
                    None
                }
            })
            .sum();
        sum / records.len().max(1) as f64
    }
}

impl Borrower for Company {
    fn id(&self) -> &str {
        &self.id
    }
    
    fn liquid_capital(&self) -> f64 {
        self.liquid_capital
    }
    
    fn fixed_capital(&self) -> f64 {
        self.fixed_capital
    }
    
    fn liabilities(&self) -> f64 {
        self.liabilities
    }
    
    fn computed_liquid_capital(&self) -> f64 {
        self.brokerage_account.as_ref().map(|b| b.cash).unwrap_or(0.0)
    }
}



/// Cluster metadata for a building (`"cluster_info"`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct ClusterInfo {
    /// Region identifier (`"region_id"`).
    #[serde(default)]
    pub region_id: String,
    /// Cluster scale factor (`"scale_factor"`).
    #[serde(default)]
    pub scale_factor: u32,
    /// Sector (`"sector"`).
    #[serde(default)]
    pub sector: Sector,
    /// Owner company id (`"owner_id"`).
    #[serde(default)]
    pub owner_id: String,
    /// Any additional cluster fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// The active production method stored on a building (`"active_method"`).
///
/// This is the concrete, runtime method selected for the building. Inputs and
/// outputs are strictly typed as [`Commodity`] keys.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct ActiveProductionMethod {
    /// Year this method becomes available (`"year"`).
    #[serde(default)]
    pub year: u32,
    /// Expert labor ratio (`"experts_ratio"`).
    #[serde(default)]
    pub experts_ratio: f64,
    /// Skilled labor ratio (`"skilled_ratio"`).
    #[serde(default)]
    pub skilled_ratio: f64,
    /// Basic labor ratio (`"basic_ratio"`).
    #[serde(default)]
    pub basic_ratio: f64,
    /// Efficiency multiplier (`"efficiency"`).
    #[serde(default = "default_efficiency")]
    pub efficiency: f64,
    /// Per-1000-worker inputs consumed (`"inputs"`).
    #[serde(default)]
    pub inputs: BTreeMap<Commodity, f64>,
    /// Per-1000-worker outputs produced (`"outputs"`).
    #[serde(default)]
    pub outputs: BTreeMap<Commodity, f64>,
    /// The three chosen method names (`"aktywne_metody"`).
    #[serde(default)]
    pub active_methods: ProductionMethodChoice,
    /// Phase 19A: Blueprint id applied to this building's blueprint-eligible
    /// outputs. When set, produced outputs carry the blueprint's quality
    /// (→ `InventoryCohort` in Phase 19C) and the building pays blueprint
    /// royalties. None = legacy behavior (flat aggregate inventory, no quality).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_blueprint: Option<String>,
    /// Phase 74: Thermal efficiency (0.0–1.0) for energy production methods.
    /// Fraction of fuel calorific energy converted to useful Energy/Heat output.
    /// 0.0 for non-energy methods (default). Used by `process_building_cycle()`
    /// to dynamically compute fuel consumption from required energy output
    /// based on the actual `calorific_value_mj_per_unit()` of input fuels.
    #[serde(default)]
    pub thermal_efficiency: f64,
    /// Phase 79: Round-trip storage efficiency (0.0-1.0) for energy storage methods.
    /// Fraction of input Energy recovered as output Energy. 0.0 for non-storage
    /// methods (default). Used by `process_building_cycle()` to enforce strict
    /// conservation: `output_energy = input_energy * storage_efficiency`.
    #[serde(default)]
    pub storage_efficiency: f64,
    /// Phase 82: Smog emission factor (smog units per unit of fuel/input consumed).
    /// Physical constant from combustion chemistry. 0.0 for non-emitting methods.
    /// Used by `compute_smog_for_region()` to calculate air pollution.
    #[serde(default)]
    pub emission_factor: f64,
    /// Phase 83 (PATCH 3): Biological hazard factor — pathogenic mass per unit
    /// of water consumed. Mirrors `ProductionMethod.biohazard_factor`.
    #[serde(default)]
    pub biohazard_factor: f64,
    /// Phase 83 (PARADIGM SHIFT): Output water quality for water treatment
    /// methods. 0.0 = no water treatment. Mirrors `ProductionMethod.output_water_quality`.
    #[serde(default)]
    pub output_water_quality: f64,
    /// Phase 83 (PARADIGM SHIFT): Discharge water quality for wastewater
    /// treatment methods. 0.0 = no wastewater treatment. Mirrors
    /// `ProductionMethod.discharge_quality`.
    #[serde(default)]
    pub discharge_quality: f64,
    /// Any additional method fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

fn default_efficiency() -> f64 {
    1.0
}

/// A single building / production site (`building`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Building {
    /// Building identifier (`"id"`).
    #[serde(default)]
    pub id: String,
    /// Building name / kind (`"name"`), e.g. `Cement Plant`.
    #[serde(default)]
    pub name: String,
    /// Owner company id (`"owner_id"`).
    #[serde(default)]
    pub owner_id: String,
    /// Year built (`"year_built"`).
    #[serde(default)]
    pub year_built: u32,
    /// GDP sector (`"gdp_sector"`).
    #[serde(default)]
    pub sector: Sector,
    /// Worker capacity (`"worker_capacity"`).
    #[serde(default)]
    pub worker_capacity: u32,
    /// Current employment (`"current_employment"`).
    #[serde(default)]
    pub current_employment: u32,
    /// Cash reserve of the building (`"reserve"`).
    #[serde(default)]
    pub reserve: f64,
    /// Active production method (`"active_method"`).
    #[serde(default)]
    pub active_method: ActiveProductionMethod,
    /// Accidents in the last year (`"accidents_last_year"`).
    #[serde(default)]
    pub accidents_last_year: u32,
    /// Whether the building is on strike (`"strike"`).
    #[serde(default)]
    pub strike: bool,
    /// Cluster scale factor (`"scale_factor"`).
    #[serde(default)]
    pub scale_factor: u32,
    /// Building construction capacity (`"building_capacity"`).
    #[serde(default)]
    pub building_capacity: u32,
    /// Region id (`"region_id"`).
    #[serde(default)]
    pub region_id: String,
    /// Cluster metadata (`"cluster_info"`).
    #[serde(default)]
    pub cluster_info: ClusterInfo,
    /// Last turn production by commodity (`"last_production"`).
    #[serde(default)]
    pub last_production: BTreeMap<Commodity, f64>,
    /// Last turn profit (`"last_profit"`).
    #[serde(default)]
    pub last_profit: f64,
    /// Emergency Stabilization: Last turn fulfillment ratio (0.0–1.0).
    /// Set by `execute_production_cycle` to indicate what fraction of the BOM
    /// inputs were physically available. Used by the corporate AI to distinguish
    /// temporary raw-material distress from structural bankruptcy.
    #[serde(default)]
    pub last_fulfillment_ratio: f64,
    /// Building condition (0.0-1.0), degrades over time, restored by maintenance.
    #[serde(default = "default_building_condition")]
    pub condition: f64,
    /// Phase 19B: Fixed-asset cohorts (machinery/vehicles) installed in this
    /// building. Empty = manual mode (capacity from labor only, pre-Phase-19
    /// behavior). Cohorts are aggregated by blueprint+acquire turn+condition
    /// (never per-item) for RAM predictability — see `economy/fixed_assets.rs`.
    #[serde(default)]
    pub fixed_assets: Vec<crate::economy::fixed_assets::FixedAssetCohort>,
    /// Whether this building is a protected heritage site.
    #[serde(default)]
    pub is_heritage_site: bool,
    /// Experience level, e.g. for military bases (`"experience_level"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experience_level: Option<f64>,
    /// Aggregate production/employment statistics for the cluster.
    #[serde(default)]
    pub aggregated_stats: AggregatedStats,
    /// Phase 4: Physical commodity inventory (inputs + outputs) at this building site.
    #[serde(default)]
    pub inventory: BTreeMap<Commodity, f64>,
    /// Phase 4: Maximum inventory capacity (tons units).
    #[serde(default = "default_building_inventory_capacity")]
    pub inventory_capacity: f64,
    /// Phase 7: Active construction project on this site (None if operational).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_project: Option<crate::construction::ConstructionProject>,
    /// Phase 81 Wave 2: Pending consumption-method upgrade (lighting/heating/
    /// ventilation). Distinct from `active_project` (building construction/
    /// expansion). Only one method upgrade at a time. The active method string
    /// ONLY flips when `is_complete()` returns true (Flaw 2 correction).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_method_upgrade: Option<crate::construction::UpgradeProject>,
    /// Phase 82B: Active emission control method (e.g., "None", "Wet Scrubber",
    /// "Baghouse Filter", "FGD"). Upgradable independently of production method.
    /// Applied to heavy industry, heating plants, and power plants.
    #[serde(default)]
    pub active_emission_control: String,
    /// Phase 84: Landfill state (None for non-landfill buildings).
    /// Replaces the legacy `LandfillData` with typed `Commodity` keys and
    /// a hard capacity stop (LOGISTICAL BOUND 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub landfill_state: Option<crate::utilities::waste_grid::LandfillState>,
    /// Phase 21A: Linked geological deposit ID (formation_id + "/" + commodity key).
    /// None for non-mining buildings or mining buildings without a deposit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deposit_id: Option<String>,
    /// Phase 22B: Accumulated structural defect (0.0 = sound, 1.0 = catastrophic).
    /// Hidden from auction listings — inherited by distressed-asset buyers.
    /// Reduces efficiency and increases collapse risk.
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub structural_defect: f64,
    /// Phase 24A.9: Land footprint in hectares occupied by this building.
    /// Used for land conservation during demolition — when a building is
    /// demolished, these hectares are returned to the regional land inventory.
    /// Default 0.0 for legacy buildings (no land reclamation on demolition).
    #[serde(default, skip_serializing_if = "is_zero_f64")]
    pub land_hectares: f64,
    /// Any additional building fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

fn default_building_inventory_capacity() -> f64 {
    10000.0
}

fn default_building_condition() -> f64 {
    1.0
}

/// Serde helper: returns true if the f64 is zero (for `skip_serializing_if`).
fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

/// Default reputation score for new companies (neutral start).
fn default_reputation_score() -> f64 {
    50.0
}

impl Building {
    /// Create a new building with essential parameters.
    ///
    /// # Arguments
    /// * `id` - Building identifier
    /// * `owner_id` - Owner company id
    /// * `sector` - GDP sector
    /// * `worker_capacity` - Worker capacity
    ///
    /// # Returns
    /// A new Building instance with boilerplate fields defaulted
    pub fn new(id: String, owner_id: String, sector: Sector, worker_capacity: u32) -> Self {
        Self {
            id,
            name: String::new(),
            owner_id,
            year_built: 0,
            sector,
            worker_capacity,
            current_employment: 0,
            reserve: 0.0,
            active_method: ActiveProductionMethod::default(),
            accidents_last_year: 0,
            strike: false,
            scale_factor: 1,
            building_capacity: 0,
            region_id: String::new(),
            cluster_info: ClusterInfo::default(),
            last_production: BTreeMap::new(),
            last_profit: 0.0,
            last_fulfillment_ratio: 1.0,
            condition: 1.0,
            is_heritage_site: false,
            experience_level: None,
            aggregated_stats: AggregatedStats::default(),
            inventory: BTreeMap::new(),
            inventory_capacity: default_building_inventory_capacity(),
            active_project: None,
            pending_method_upgrade: None,
            active_emission_control: String::new(),
            landfill_state: None,
            deposit_id: None,
            fixed_assets: Vec::new(),
            structural_defect: 0.0,
            land_hectares: 0.0,
            extra: Map::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    /// Validates that the current English JSON schema correctly populates
    /// the typed `LegalForm` and `union_id` fields.
    #[test]
    fn company_english_schema_validation() {
        let english_json = r#"{
            "id": "COMP-001",
            "name": "Modern Consortium",
            "sector": "heavy_industry",
            "region_id": "R-1",
            "fixed_capital": 1000000.0,
            "liquid_capital": 500000.0,
            "liabilities": 0.0,
            "company_capital": 1500000.0,
            "shares_count": 1000,
            "share_price": 1500.0,
            "safety_level": 0.5,
            "union_id": "UNION-001",
            "worker_capacity": 500,
            "is_national_champion": true,
            "is_listed": false
        }"#;
        let company: Company = serde_json::from_str(english_json).expect("English JSON must deserialize");
        assert_eq!(company.id, "COMP-001");
        assert!(company.is_national_champion);
        assert!(!company.is_listed);
        assert_eq!(company.union_id, Some("UNION-001".to_string()));
    }
}

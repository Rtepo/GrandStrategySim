//! Ownership-driven corporate strategy and IPO evaluation.
//!
//! This module implements the `CorporateStrategy` trait for each [`LegalForm`]
//! and the market-driven `IpoStrategy` used by family businesses and
//! cooperatives to decide whether to go public.

use crate::economy::market::MarketSignal;
use crate::entities::legal_form::{CooperativeData, FamilyBusinessData, JointStockData, LegalForm, LegalFormTransition, LegalTransition, TransitionContext};
use crate::entities::{ActiveProductionMethod, Company};
use crate::registries::enums::Sector;
use crate::state::macro_data::LaborMarket;
use crate::state::treasury::{SectorShare, StockMarket};
use crate::state::Country;
use crate::corporate::market_behavior::MarketBehaviorModifiers;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// All signals a company observes before deciding its turn.
#[derive(Debug, Clone)]
pub struct CorporateDecisionCtx<'a> {
    /// The company being evaluated.
    pub company: &'a Company,
    /// The country the company belongs to.
    pub country: &'a Country,
    /// The company's GDP sector.
    pub sector: Sector,
    /// Sector-level share and runtime data.
    pub sector_share: &'a SectorShare,
    /// Market signal produced after market clearing.
    pub market_signal: &'a MarketSignal,
    /// Representative corporate credit rate.
    pub bank_credit_rate: f64,
    /// National stock market.
    pub stock_market: &'a StockMarket,
    /// National labor market.
    pub labor_market: &'a LaborMarket,
    /// In-game year.
    pub year: u32,
    /// Gross profit from owned buildings this turn.
    pub gross_profit: f64,
    /// Net profit after overhead, interest and tax.
    pub net_profit: f64,
    /// Phase 57: Trait-driven behavior modifiers (no raw trait string checks).
    pub behavior_modifiers: MarketBehaviorModifiers,
}

/// Actions a company can choose.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum CorporateAction {
    /// Expand capacity using the given finance source.
    Expand {
        /// Amount of capital to invest in fixed assets.
        investment: f64,
        /// Number of new workers to hire.
        new_workers: u32,
        /// Source of financing for the investment.
        finance: FinanceSource,
    },
    /// Restructure: lay off workers and/or write off fixed capital.
    Restructure {
        /// Number of workers to lay off.
        layoffs: u32,
        /// Amount of capital to write off.
        capital_write_off: f64,
    },
    /// Pay a dividend / patronage distribution.
    PayDividend {
        /// Total dividend amount to distribute.
        total: f64,
    },
    /// Switch to a more profitable production method.
    SwitchMethod {
        /// The new production method to adopt.
        method: ActiveProductionMethod,
    },
    /// Raise wages for the workforce.
    RaiseWages {
        /// Percentage bump in wages.
        bump: f64,
    },
    /// Cut wages for the workforce.
    CutWages {
        /// Percentage cut in wages.
        cut: f64,
    },
    /// Go public: float `shares_to_float` at `reserve_price`.
    Ipo {
        /// Number of shares to float on the market.
        shares_to_float: u64,
        /// Reserve price per share.
        reserve_price: f64,
    },
    /// Do nothing this turn.
    Idle,
    /// Phase 24A.9: Demolish a building and return its land to the region.
    Demolish {
        /// Building ID to demolish.
        building_id: String,
    },
    /// Phase 24A.9: Halt production at a building (temporary shutdown).
    HaltProduction {
        /// Building ID to halt.
        building_id: String,
    },
}

/// Source of expansion financing.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum FinanceSource {
    /// Reinvest retained earnings.
    Internal,
    /// Borrow from commercial banks.
    BankLoan(f64),
    /// Issue corporate bonds.
    BondIssue(f64),
    /// Capital raised by an IPO.
    IpoProceeds(f64),
}

/// Phase 55: Outcome of a board vote on a CEO-proposed action.
#[derive(Debug, Clone, PartialEq)]
pub enum BoardDecision {
    /// Board approves the proposed action — CEO may proceed.
    Approve,
    /// Board blocks the proposed action — CEO must take `Idle` or `Restructure`.
    Block,
    /// Board fires the CEO — a new CEO must be appointed externally.
    FireCeo,
}

/// Phase 55: Evaluate a board's response to a CEO-proposed action.
///
/// The board votes based on each member's `loyalty_to_ceo` and the nature
/// of the proposed action. A simple majority is required to approve.
/// If the average loyalty is below 0.3 AND the company is unprofitable,
/// the board fires the CEO.
///
/// # Arguments
/// * `board_members` - The board seats of the joint-stock company.
/// * `proposed_action` - The action the CEO wants to take.
/// * `is_profitable` - Whether the company was profitable this turn.
///
/// # Returns
/// The board's decision: `Approve`, `Block`, or `FireCeo`.
pub fn evaluate_board_conflict(
    board_members: &[crate::entities::legal_form::BoardSeat],
    proposed_action: &CorporateAction,
    is_profitable: bool,
) -> BoardDecision {
    if board_members.is_empty() {
        // No board — CEO has unchecked power.
        return BoardDecision::Approve;
    }

    let avg_loyalty: f64 = board_members.iter().map(|s| s.loyalty_to_ceo).sum::<f64>()
        / board_members.len() as f64;

    // Fire CEO if loyalty is critically low and company is bleeding.
    if avg_loyalty < 0.3 && !is_profitable {
        return BoardDecision::FireCeo;
    }

    // Each member votes: loyalty > 0.5 = approve, < 0.5 = block.
    // Expansions and IPOs require higher loyalty (risk-averse board).
    let is_risky = matches!(
        proposed_action,
        CorporateAction::Expand { .. } | CorporateAction::Ipo { .. }
    );

    let threshold = if is_risky { 0.6 } else { 0.5 };

    let approve_votes = board_members
        .iter()
        .filter(|s| s.loyalty_to_ceo >= threshold)
        .count();

    if approve_votes * 2 > board_members.len() {
        BoardDecision::Approve
    } else {
        BoardDecision::Block
    }
}

/// Trait for all company strategies.  Concrete ownership forms implement
/// this with their own behavioural rules.
pub trait CorporateStrategy {
    /// Choose the action for this turn.
    fn decide(&self, ctx: &CorporateDecisionCtx) -> CorporateAction;

    /// Evaluate whether the company should go public.
    fn evaluate_ipo(&self, ctx: &CorporateDecisionCtx) -> Option<CorporateAction>;

    /// Evaluate dividend / patronage distribution.
    fn evaluate_dividend(&self, ctx: &CorporateDecisionCtx) -> CorporateAction;

    /// Evaluate expansion or restructuring.
    fn evaluate_expansion(&self, ctx: &CorporateDecisionCtx) -> CorporateAction;

    /// Evaluate production method switching.
    fn evaluate_production_method(&self, ctx: &CorporateDecisionCtx) -> CorporateAction;

    /// Evaluate inventory disposal for economic rationality (Phase 5.5).
    fn evaluate_inventory_disposal(&self, ctx: &CorporateDecisionCtx) -> Vec<DisposalOrder>;
}

impl CorporateStrategy for LegalForm {
    fn decide(&self, ctx: &CorporateDecisionCtx) -> CorporateAction {
        if let Some(ipo) = self.evaluate_ipo(ctx) {
            return ipo;
        }

        if is_distressed(ctx) {
            return self.evaluate_restructure(ctx);
        }

        // Prioritize expansion over dividends to match original behavior
        let expansion = self.evaluate_expansion(ctx);
        if !matches!(expansion, CorporateAction::Idle) {
            return expansion;
        }

        let dividend = self.evaluate_dividend(ctx);
        if let CorporateAction::PayDividend { total } = dividend {
            if total > 0.0 && ctx.company.liquid_capital > ctx.company.fixed_capital * 0.05 {
                return dividend;
            }
        }

        CorporateAction::Idle
    }

    fn evaluate_ipo(&self, ctx: &CorporateDecisionCtx) -> Option<CorporateAction> {
        match self {
            LegalForm::FamilyBusiness(data) => evaluate_family_ipo(data, ctx),
            LegalForm::Cooperative(data) => evaluate_cooperative_ipo(data, ctx),
            _ => None,
        }
    }

    fn evaluate_dividend(&self, ctx: &CorporateDecisionCtx) -> CorporateAction {
        match self {
            LegalForm::FamilyBusiness(data) => {
                let total = ctx.net_profit * (1.0 - data.family_retained_share).max(0.0);
                CorporateAction::PayDividend { total: total.max(0.0) }
            }
            LegalForm::JointStockCompany(data) => {
                let ratio = dividend_payout_ratio(data);
                let total = ctx.net_profit * ratio;
                CorporateAction::PayDividend { total: total.max(0.0) }
            }
            LegalForm::Cooperative(_data) => {
                let total = ctx.net_profit * 0.5;
                CorporateAction::PayDividend { total: total.max(0.0) }
            }
            _ => CorporateAction::PayDividend { total: 0.0 },
        }
    }

    fn evaluate_expansion(&self, ctx: &CorporateDecisionCtx) -> CorporateAction {
        match self {
            LegalForm::FamilyBusiness(_) => family_expansion(ctx),
            LegalForm::JointStockCompany(_) => joint_stock_expansion(ctx),
            LegalForm::Cooperative(_) => cooperative_expansion(ctx),
            LegalForm::MutualAidCircle(_) => mutual_aid_expansion(ctx),
            LegalForm::Consortium(_) => CorporateAction::Idle,
            LegalForm::Latifundium(_) => CorporateAction::Idle, // Latifundia expand via land acquisition, not capital
            LegalForm::MunicipalCompany(_) => CorporateAction::Idle, // Municipal companies expand via municipal budget, not capital
            LegalForm::StateMonopoly(_) => CorporateAction::Idle, // State monopolies expand via state budget, not capital
            LegalForm::HousingCommunity(_) => CorporateAction::Idle, // Housing communities expand via member contributions, not capital
            LegalForm::HousingCooperative(_) => CorporateAction::Idle, // Housing cooperatives expand via member contributions, not capital
            LegalForm::StrategicReserveAgency(_) => CorporateAction::Idle, // Strategic Reserve Agency expands via state budget, not capital
            LegalForm::LogisticsCompany(_) => logistics_expansion(ctx), // Phase 29: ROI-driven warehouse expansion
            LegalForm::NonProfit(_) => CorporateAction::Idle, // Non-profits don't pursue corporate expansion
        }
    }

    fn evaluate_production_method(&self, ctx: &CorporateDecisionCtx) -> CorporateAction {
        evaluate_method_switch(ctx)
    }

    fn evaluate_inventory_disposal(&self, ctx: &CorporateDecisionCtx) -> Vec<DisposalOrder> {
        evaluate_inventory_disposal(ctx)
    }
}

/// Disposal order for strategic inventory management (Phase 5.5).
#[derive(Debug, Clone)]
pub struct DisposalOrder {
    /// Batch ID to dispose
    pub batch_id: String,
    /// Warehouse ID where batch is stored
    pub warehouse_id: String,
    /// Reason for disposal
    pub reason: DisposalReason,
}

/// Reason for inventory disposal (Phase 5.5).
#[derive(Debug, Clone, PartialEq)]
pub enum DisposalReason {
    /// Storage costs exceed market value
    UnprofitableStorage,
    /// Batch approaching expiration
    ApproachingExpiration,
    /// State intervention expected
    StateInterventionExpected,
}

/// Evaluate inventory disposal decisions for economic rationality (Phase 5.5).
///
/// # Arguments
/// * `ctx` - Corporate decision context
///
/// # Returns
/// * Vector of disposal orders for batches that should be scrapped
///
/// # Rules
/// * If accumulated_fees + transport_cost > market_price AND no state buyout expected
/// * Producer voluntarily scraps batch to stop bleeding
/// * Producer pays accumulated fees to LogisticsCompany
/// * AI hard rule: Never dispose if state intervention is imminent
fn evaluate_inventory_disposal(ctx: &CorporateDecisionCtx) -> Vec<DisposalOrder> {
    let mut disposal_orders = Vec::new();
    
    // Phase 13: Requires warehouse batch tracking access
    
    // Example logic (when batches are accessible):
    // for batch in company.owned_batches.iter() {
    //     let current_market_price = get_market_price(batch.commodity, ctx);
    //     let projected_transport_cost = estimate_transport_cost(batch);
    //     let total_cost = batch.accumulated_fees + projected_transport_cost;
    //     
    //     let should_dispose = total_cost > current_market_price
    //         && !state_intervention_expected(batch.commodity, ctx)
    //         && company.liquid_capital > batch.accumulated_fees;
    //     
    //     if should_dispose {
    //         disposal_orders.push(DisposalOrder {
    //             batch_id: batch.id,
    //             warehouse_id: batch.warehouse_id,
    //             reason: DisposalReason::UnprofitableStorage,
    //         });
    //     }
    // }
    
    disposal_orders
}

/// Evaluate whether to switch production methods based on gross margin comparison.
///
/// This implements the gross-margin-based AI logic for adaptive production methods.
/// Companies will switch to alternative methods if they offer significantly better
/// profitability (15% improvement) with reasonable payback period (20 turns).
///
/// # Arguments
/// * `ctx` - Corporate decision context with market prices and company data
///
/// # Returns
/// * `SwitchMethod` action if a profitable alternative exists, `Idle` otherwise
fn evaluate_method_switch(ctx: &CorporateDecisionCtx) -> CorporateAction {
    // For Phase 1, we use a simplified approach: check if the company has buildings
    // and evaluate method switching based on market conditions
    // Phase 13: Requires per-building method evaluation from company registry
    
    // Find alternative methods from registry for this building type
    // For Phase 1, we implement a simplified version with hardcoded synthetic alternatives
    // We create a dummy current method for evaluation
    let dummy_current = ActiveProductionMethod {
        year: ctx.year,
        inputs: std::collections::BTreeMap::new(),
        outputs: std::collections::BTreeMap::new(),
        experts_ratio: 0.1,
        skilled_ratio: 0.3,
        basic_ratio: 0.6,
        efficiency: 1.0,
        active_methods: crate::state::treasury::ProductionMethodChoice::default(),
        active_blueprint: None,
        extra: serde_json::Map::new(),
    };
    
    let alternatives = find_alternative_methods(&dummy_current, ctx.year);
    
    for alt_method in alternatives {
        // Calculate projected Gross Margin for alternative method
        let alt_gm = calculate_gross_margin(
            &alt_method,
            &ctx.market_signal.prices,
            ctx.country.macro_indicators.average_wage,
        );
        
        // Calculate switch cost (equipment, retraining, downtime)
        let switch_cost = calculate_switch_cost(&dummy_current, &alt_method);
        
        // Switch only if alternative is profitable AND switch cost is justified
        // For Phase 1, we use a simpler threshold: positive gross margin with reasonable payback
        if alt_gm > 0.0 {
            let payback_turns = switch_cost / alt_gm.max(0.01);
            
            if payback_turns <= 20.0 {
                return CorporateAction::SwitchMethod {
                    method: alt_method,
                };
            }
        }
    }
    CorporateAction::Idle
}

/// Calculate projected Gross Margin for a production method.
///
/// Gross Margin = Expected Revenue - Expected Input Costs - Wages
///
/// # Arguments
/// * `method` - The production method to evaluate
/// * `market_prices` - Current market prices for commodities
/// * `base_wage` - National average wage
///
/// # Returns
/// * Gross margin per 1000 workers
fn calculate_gross_margin(
    method: &ActiveProductionMethod,
    market_prices: &std::collections::HashMap<crate::registries::enums::Commodity, f64>,
    base_wage: f64,
) -> f64 {
    let wage_multiplier = method.experts_ratio * 3.0 + method.skilled_ratio * 2.0 + method.basic_ratio;
    let wages_per_1k = wage_multiplier * base_wage;
    
    // Calculate input costs using current market prices
    let input_costs: f64 = method.inputs.iter()
        .map(|(commodity, amount_per_1k)| {
            let price = market_prices.get(commodity).copied().unwrap_or(100.0);
            amount_per_1k * price
        })
        .sum();
    
    // Calculate output revenue using current market prices
    let output_revenue: f64 = method.outputs.iter()
        .map(|(commodity, amount_per_1k)| {
            let price = market_prices.get(commodity).copied().unwrap_or(100.0);
            amount_per_1k * price
        })
        .sum();
    
    // Gross margin per 1000 workers
    output_revenue - input_costs - wages_per_1k
}

/// Calculate one-time cost to switch production methods.
///
/// # Arguments
/// * `current` - Current production method
/// * `alternative` - Alternative production method being considered
///
/// # Returns
/// * Total switch cost in currency units
fn calculate_switch_cost(
    current: &ActiveProductionMethod,
    alternative: &ActiveProductionMethod,
) -> f64 {
    // Equipment replacement cost (simplified: 10% of fixed capital)
    let equipment_cost = 10000.0;  // Placeholder - should reference building.fixed_capital
    
    // Worker retraining cost (based on labor ratio differences)
    let labor_diff = (current.experts_ratio - alternative.experts_ratio).abs()
        + (current.skilled_ratio - alternative.skilled_ratio).abs()
        + (current.basic_ratio - alternative.basic_ratio).abs();
    let retraining_cost = labor_diff * 5000.0;
    
    // Downtime cost (estimated 2 turns of lost production)
    let downtime_cost = 2000.0;  // Placeholder
    
    equipment_cost + retraining_cost + downtime_cost
}

/// Find alternative production methods for a given method.
///
/// For Phase 1, this returns hardcoded synthetic alternatives (e.g., Coal to Liquids).
/// Future phases will query the production methods registry.
///
/// # Arguments
/// * `current_method` - The current production method
/// * `year` - Current game year (for technology availability)
///
/// # Returns
/// * Vector of alternative production methods
fn find_alternative_methods(
    current_method: &ActiveProductionMethod,
    year: u32,
) -> Vec<ActiveProductionMethod> {
    let mut alternatives = Vec::new();
    
    // Coal to Energy synthetic fuel production (available from year 2000)
    if year >= 2000 {
        // Create synthetic coal-to-energy method
        let mut synthetic_method = current_method.clone();
        
        // Add coal input
        synthetic_method.inputs.insert(
            crate::registries::enums::Commodity::HardCoal,
            100.0,
        );
        
        // Add energy output (synthetic fuel)
        synthetic_method.outputs.insert(
            crate::registries::enums::Commodity::Energy,
            60.0,
        );
        
        alternatives.push(synthetic_method);
    }
    
    alternatives
}

/// Calculate administrative overhead penalty for companies requiring office space
///
/// # Arguments
/// * `legal_form` - The company's legal form
/// * `company_id` - The company's unique identifier
/// * `region_id` - The region where the company operates
///
/// # Returns
/// * Administrative overhead penalty (0.0-1.0), where 0.3 = 30% efficiency penalty
///
/// # Rules
/// * JointStockCompany and StateMonopoly require leased office space
/// * If no office space is leased, apply 30% efficiency penalty
/// * Small businesses (FamilyBusiness, Cooperative, MutualAidCircle) do not require dedicated office space
pub fn calculate_administrative_overhead(
    legal_form: &crate::entities::legal_form::LegalForm,
    company_id: &str,
    region_id: &str,
) -> f64 {
    match legal_form {
        crate::entities::legal_form::LegalForm::JointStockCompany(_) 
        | crate::entities::legal_form::LegalForm::StateMonopoly(_) => {
            // Check if company has leased office space
            // Phase 13: Requires company registry for office lease checking
            let has_office = check_office_lease(company_id, region_id);
            if !has_office {
                0.3 // 30% efficiency penalty
            } else {
                0.0
            }
        }
        _ => 0.0, // Small businesses don't require dedicated office space
    }
}

/// Placeholder function to check if a company has leased office space
///
/// # Arguments
/// * `company_id` - The company's unique identifier
/// * `region_id` - The region where the company operates
///
/// # Returns
/// * true if the company has leased office space, false otherwise
fn check_office_lease(company_id: &str, region_id: &str) -> bool {
    // Phase 13: Requires CommercialInventory access for lease checking
    let _ = (company_id, region_id);
    false // Placeholder: assume no office space leased
}

impl LegalForm {
    fn evaluate_restructure(&self, ctx: &CorporateDecisionCtx) -> CorporateAction {
        if ctx.company.company_capital < 0.0 {
            // Phase 37: Even bankrupt companies are capped at 25% capacity
            // reduction per turn (was 100%). Full liquidation now takes
            // ~4 turns, preventing instant GDP collapse cascades.
            let layoffs = (ctx.company.worker_capacity / 4).max(1);
            CorporateAction::Restructure {
                layoffs,
                capital_write_off: ctx.company.fixed_capital * 0.25,
            }
        } else {
            // Phase 37: Cap layoffs to 10% per turn (was 50%).
            // Cooperatives cap at 12.5% (was 25%).
            let layoffs = match self {
                LegalForm::Cooperative(_) => (ctx.company.worker_capacity / 8).max(1),
                _ => (ctx.company.worker_capacity / 10).max(1),
            };
            CorporateAction::Restructure {
                layoffs,
                capital_write_off: 0.0,
            }
        }
    }
}

/// IPO decision engine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IpoStrategy {
    /// Minimum years of profitable history before an IPO is considered.
    pub min_profit_history: usize,
    /// Minimum sector PMI for an IPO.
    pub min_sector_pmi: f64,
    /// Minimum stock-market confidence to float shares.
    pub min_stock_confidence: f64,
}

impl IpoStrategy {
    /// Evaluates whether an IPO is advisable this turn.
    pub fn evaluate(&self, ctx: &CorporateDecisionCtx) -> Option<CorporateAction> {
        if ctx.company.financial_history.len() < self.min_profit_history {
            return None;
        }

        let desired_investment = ctx.company.desired_investment(ctx.market_signal);
        let internal_cap = ctx.company.liquid_capital + ctx.company.retained_earnings();
        let capital_gap = (desired_investment - internal_cap).max(0.0);

        if capital_gap <= 0.0 {
            return None;
        }

        let pmi = ctx.market_signal.sector_outlook(ctx.sector);
        if pmi < self.min_sector_pmi {
            return None;
        }

        let confidence = ctx.stock_market.confidence;
        if confidence < self.min_stock_confidence {
            return None;
        }

        let index = ctx.stock_market.index;
        if index <= 0.0 {
            return None;
        }

        let reserve_price = ctx.company.reserve_price();
        let shares_to_float = ((capital_gap / reserve_price) as u64).max(100_000);

        Some(CorporateAction::Ipo {
            shares_to_float,
            reserve_price,
        })
    }
}

impl Company {
    /// Desired investment based on the company capital and market outlook.
    pub fn desired_investment(&self, market_signal: &MarketSignal) -> f64 {
        let outlook = market_signal.sector_outlook(self.sector);
        let base = self.company_capital.max(0.0) * 0.2;
        // Phase 29: Minimum 10% investment rate even in downturns — companies
        // still invest for replacement and maintenance regardless of PMI.
        base * (outlook / 50.0).max(0.1)
    }

    /// Sum of the last three net-profit records.
    pub fn retained_earnings(&self) -> f64 {
        let mut total = 0.0;
        let start = self.financial_history.len().saturating_sub(3);
        for record in self.financial_history.iter().skip(start) {
            if let Some(Value::Object(map)) = Some(record) {
                if let Some(Value::Number(n)) = map.get("zysk_netto") {
                    total += n.as_f64().unwrap_or(0.0);
                }
            }
        }
        total
    }

    /// Reserve price per share for an IPO.
    fn reserve_price(&self) -> f64 {
        let capital = self.company_capital.max(1.0);
        let shares = self.shares_count.max(1);
        capital / shares as f64
    }
}

fn evaluate_family_ipo(data: &FamilyBusinessData, ctx: &CorporateDecisionCtx) -> Option<CorporateAction> {
    if ctx.company.company_capital < 10_000_000.0 {
        return None;
    }
    if data.family_retained_share < 0.30 || data.family_retained_share > 0.95 {
        return None;
    }

    IpoStrategy {
        min_profit_history: 2,
        min_sector_pmi: 55.0,
        min_stock_confidence: 60.0,
    }
    .evaluate(ctx)
}

fn evaluate_cooperative_ipo(data: &CooperativeData, ctx: &CorporateDecisionCtx) -> Option<CorporateAction> {
    if data.member_count < 500 {
        return None;
    }
    if ctx.company.company_capital < 5_000_000.0 {
        return None;
    }

    IpoStrategy {
        min_profit_history: 2,
        min_sector_pmi: 50.0,
        min_stock_confidence: 50.0,
    }
    .evaluate(ctx)
}

fn is_distressed(ctx: &CorporateDecisionCtx) -> bool {
    ctx.company.company_capital < 0.0
        || (ctx.net_profit < 0.0 && ctx.company.liquid_capital == 0.0)
}

/// Phase 37: Cap new worker hiring to 20% of current capacity per turn.
/// Small companies (<5 workers) can add up to 5 to allow initial scaling.
fn cap_new_workers(company: &Company, raw: u32) -> u32 {
    let cap = (company.worker_capacity as f32 * 0.20) as u32 + 1;
    raw.min(cap.max(5))
}

fn family_expansion(ctx: &CorporateDecisionCtx) -> CorporateAction {
    // Phase 29: GDP thresholds eradicated. A profitable company invests
    // based on its own gross_profit and company_capital, not national GDP.
    if ctx.gross_profit > 0.0 && ctx.company.company_capital > 0.0 {
        let investment = ctx.gross_profit * 0.30;
        let new_workers = cap_new_workers(ctx.company, ((ctx.gross_profit / 1_000.0) as u32).max(1));
        CorporateAction::Expand {
            investment,
            new_workers,
            finance: FinanceSource::Internal,
        }
    } else {
        CorporateAction::Idle
    }
}

fn cooperative_expansion(ctx: &CorporateDecisionCtx) -> CorporateAction {
    // Phase 29: GDP thresholds eradicated. A profitable cooperative invests
    // based on its own gross_profit and company_capital, not national GDP.
    if ctx.gross_profit > 0.0 && ctx.company.company_capital > 0.0 {
        let investment = ctx.gross_profit * 0.20;
        let new_workers = cap_new_workers(ctx.company, ((ctx.gross_profit / 1_000.0) as u32).max(1));
        CorporateAction::Expand {
            investment,
            new_workers,
            finance: FinanceSource::Internal,
        }
    } else {
        CorporateAction::Idle
    }
}

fn mutual_aid_expansion(ctx: &CorporateDecisionCtx) -> CorporateAction {
    if ctx.company.company_capital > 0.0 && ctx.gross_profit > 0.0 {
        let investment = ctx.gross_profit * 0.10;
        let new_workers = cap_new_workers(ctx.company, ((ctx.gross_profit / 1_000.0) as u32).max(1));
        CorporateAction::Expand {
            investment,
            new_workers,
            finance: FinanceSource::Internal,
        }
    } else {
        CorporateAction::Idle
    }
}

fn joint_stock_expansion(ctx: &CorporateDecisionCtx) -> CorporateAction {
    let desired = ctx.company.desired_investment(ctx.market_signal);
    let internal = ctx.company.liquid_capital;

    if desired <= internal {
        // Phase 29: Increased from 0.30 to 0.50 for higher investment velocity.
        let raw = ((desired / 1_000.0) as u32).max(1);
        CorporateAction::Expand {
            investment: desired * 0.50,
            new_workers: cap_new_workers(ctx.company, raw),
            finance: FinanceSource::Internal,
        }
    } else {
        let loan = (desired - internal).min(max_credit(ctx.company, ctx.bank_credit_rate));
        let raw = ((desired / 1_000.0) as u32).max(1);
        CorporateAction::Expand {
            investment: internal + loan,
            new_workers: cap_new_workers(ctx.company, raw),
            finance: FinanceSource::BankLoan(loan),
        }
    }
}

/// Phase 29: ROI-driven warehouse expansion for logistics companies.
///
/// Logistics companies build warehouses based on potential profit, NOT
/// utilization percentages. The trigger is the aggregate overflow fees
/// currently being paid (or lost to perishability) by manufacturing
/// companies in the company's region. If demand for storage is high
/// (companies are bleeding cash to overflow fees), the logistics company
/// expands to capture that revenue.
fn logistics_expansion(ctx: &CorporateDecisionCtx) -> CorporateAction {
    if ctx.company.company_capital <= 0.0 || ctx.gross_profit <= 0.0 {
        return CorporateAction::Idle;
    }

    // Read regional overflow fees from country.regional_overflow_fees
    let region_overflow_fees: f64 = ctx
        .country
        .regional_overflow_fees
        .get(&ctx.company.region_id)
        .copied()
        .unwrap_or(0.0);

    if region_overflow_fees <= 0.0 {
        return CorporateAction::Idle;
    }

    // ROI calculation: projected storage revenue vs warehouse construction cost.
    // The logistics company can expect to capture a portion of the overflow fees
    // by providing warehouse capacity. Use 50% capture rate as a conservative estimate.
    let projected_storage_revenue = region_overflow_fees * 0.5;
    let investment = ctx.gross_profit * 0.30;
    let warehouse_cost = investment.max(10_000.0);

    // Payback threshold: warehouse should pay for itself within 20 turns
    let payback_turns = warehouse_cost / projected_storage_revenue.max(1.0);
    if payback_turns > 20.0 {
        return CorporateAction::Idle;
    }

    let new_workers = cap_new_workers(ctx.company, ((investment / 5_000.0) as u32).max(1));
    CorporateAction::Expand {
        investment,
        new_workers,
        finance: FinanceSource::Internal,
    }
}

fn dividend_payout_ratio(data: &JointStockData) -> f64 {
    let board = data.board_independence.clamp(0.0, 1.0);
    0.3 + board * 0.3
}

fn max_credit(company: &Company, bank_credit_rate: f64) -> f64 {
    if bank_credit_rate >= 0.20 {
        0.0
    } else {
        company.company_capital.max(0.0) * 0.5
    }
}

/// Attempts to apply the legal-form transition implied by an `Ipo` action.
///
/// Returns `Some(new_legal_form)` with injected capital if the transition
/// succeeds, or `None` if it is rejected.
pub fn try_apply_ipo(
    company: &Company,
    legal_form: &LegalForm,
    _shares_to_float: u64,
    _reserve_price: f64,
    ctx: &CorporateDecisionCtx,
) -> Option<LegalForm> {
    let transition = match legal_form {
        LegalForm::FamilyBusiness(_) => LegalTransition::FamilyBusinessToJointStockCompany,
        LegalForm::Cooperative(_) => LegalTransition::CooperativeToJointStockCompany,
        _ => return None,
    };

    let sector_pmi = ctx.market_signal.sector_outlook(ctx.sector);
    let transition_ctx = TransitionContext {
        company,
        sector_pmi,
        stock_confidence: ctx.stock_market.confidence,
        market_signal: ctx.market_signal,
        private_capital_pool: ctx.country.budget.private_capital,
        bank_credit_rate: ctx.bank_credit_rate,
    };

    legal_form
        .clone()
        .try_transition(transition, &transition_ctx)
        .ok()
}

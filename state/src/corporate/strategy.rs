//! Ownership-driven corporate strategy and IPO evaluation.
//!
//! This module implements the `CorporateStrategy` trait for each [`LegalForm`]
//! and the market-driven `IpoStrategy` used by family businesses and
//! cooperatives to decide whether to go public.

use crate::corporate::market_behavior::MarketBehaviorModifiers;
use crate::economy::market::MarketSignal;
use crate::entities::legal_form::{
    CooperativeData, FamilyBusinessData, JointStockData, LegalForm, LegalFormTransition,
    LegalTransition, TransitionContext,
};
use crate::entities::{ActiveProductionMethod, Company};
use crate::registries::enums::Sector;
use crate::state::macro_data::LaborMarket;
use crate::state::treasury::{SectorShare, StockMarket};
use crate::state::Country;
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
    /// Emergency Stabilization: Average production fulfillment ratio this turn
    /// (0.0 = no inputs available, 1.0 = full inputs). Used by the corporate AI
    /// to distinguish temporary raw-material distress from structural bankruptcy.
    pub avg_fulfillment_ratio: f64,
    /// Phase 88: Current global turn (for agricultural grace hardcap computation).
    pub current_turn: u32,
    /// Phase 95: All buildings in the country (for blueprint design evaluation).
    pub buildings: &'a [crate::entities::Building],
}

/// Actions a company can choose.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
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
    /// Emergency Stabilization: Furlough a fraction of the workforce.
    /// Workers are temporarily moved to `furloughed_workers_count` (retained
    /// by the company, excluded from active labor clearing) and re-instated
    /// when conditions improve without recruitment cost. Used during temporary
    /// cash/raw-material distress instead of permanent layoffs.
    Furlough {
        /// Number of FTE to furlough (moved from fulfilled_fte to furloughed_workers_count).
        fte_count: u32,
        /// Fraction of normal wage paid to furloughed workers (0.0–1.0).
        /// Set by labor law; 0.0 means no pay during furlough.
        wage_fraction: f64,
    },
    /// Phase 93: Fund a geological survey to search for hidden Rare/UltraRare
    /// veins in a region. The company chooses a `target_depth` (the maximum
    /// depth it is willing to scan). If the actual vein's depth exceeds this,
    /// discovery fails (fog-of-war: the company cannot know the real depth
    /// before discovery). The survey cost is paid to the State Treasury.
    GeologicalSurvey {
        /// The region to survey.
        region_id: String,
        /// The commodity to search for.
        commodity: crate::registries::enums::Commodity,
        /// The company-chosen search depth target in meters.
        target_depth: f64,
    },
    /// Phase 95: Design a new product blueprint (commercial engineering).
    /// The company spends `available_cash` (NOT `rd_budget`) to design a
    /// blueprint for a blueprint-eligible commodity. The design fee is paid
    /// to the State Treasury as a patent/certification fee (double-entry).
    DesignBlueprint {
        /// The commodity this blueprint will produce.
        output_commodity: crate::registries::enums::Commodity,
        /// The base technology ID for this blueprint.
        base_tech: crate::registries::tech_tree::TechId,
        /// The method slot this blueprint targets.
        required_slot: crate::registries::production_methods::MethodSlot,
    },
    /// R4.2: Transform the company's legal form (e.g., family business to
    /// cooperative, mutual-aid circle to cooperative). Unlike Ipo, this
    /// action handles non-IPO transitions including buyouts.
    TransformLegalForm {
        /// The target legal transition.
        transition: crate::entities::legal_form::LegalTransition,
        /// Buyout amount for the prior owner (if applicable). For family-
        /// business-to-cooperative, this is the cash paid to the family
        /// for their ownership stake. Zero for transitions without a buyout.
        buyout_amount: f64,
    },
    /// Phase E.10: Steal a competitor's patented technology via espionage
    /// or reverse engineering. Only chosen if the company cannot afford
    /// the license fee.
    StealIP {
        /// Technology ID to steal.
        tech_id: crate::registries::tech_tree::TechId,
        /// Target company ID (the patent holder).
        target_company_id: String,
        /// Method of IP theft.
        method: crate::entities::IPTheftMethod,
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

    let avg_loyalty: f64 =
        board_members.iter().map(|s| s.loyalty_to_ceo).sum::<f64>() / board_members.len() as f64;

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

    /// R4.4: Evaluate whether the company should transform its legal form.
    /// Returns `Some(TransformLegalForm)` if a transformation is warranted,
    /// or `None` if no transformation should occur this turn.
    fn evaluate_transform(&self, ctx: &CorporateDecisionCtx) -> Option<CorporateAction>;
}

impl CorporateStrategy for LegalForm {
    fn decide(&self, ctx: &CorporateDecisionCtx) -> CorporateAction {
        if let Some(ipo) = self.evaluate_ipo(ctx) {
            return ipo;
        }

        // R4.4: Evaluate legal-form transformation (family business to
        // cooperative, mutual-aid circle to cooperative). This is checked
        // before distress/expansion because transformation changes the
        // company's fundamental structure.
        if let Some(transform) = self.evaluate_transform(ctx) {
            return transform;
        }

        if is_distressed(ctx) {
            // Emergency Stabilization: If the distress is temporary (raw-material
            // shortage or cash-flow issue, NOT structural bankruptcy), prefer
            // furlough over permanent layoffs. Furloughed workers are retained
            // and can be re-instated without recruitment cost.
            if let Some(furlough) = evaluate_furlough(ctx) {
                return furlough;
            }
            return self.evaluate_restructure(ctx);
        }

        // Phase 93: Mining companies may fund geological surveys to discover
        // hidden Rare/UltraRare veins. Evaluated before expansion because
        // finding a new deposit is a prerequisite for expansion.
        if ctx.company.sector == Sector::Mining {
            let survey = evaluate_geological_survey(ctx);
            if !matches!(survey, CorporateAction::Idle) {
                return survey;
            }
        }

        // Phase 95: Industrial companies may design product blueprints.
        // Evaluated before expansion because a new blueprint can improve
        // the profitability of existing production capacity.
        if ctx.company.sector == Sector::HeavyIndustry
            || ctx.company.sector == Sector::LightIndustry
        {
            let blueprint = evaluate_blueprint_design(ctx);
            if !matches!(blueprint, CorporateAction::Idle) {
                return blueprint;
            }
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
                CorporateAction::PayDividend {
                    total: total.max(0.0),
                }
            }
            LegalForm::JointStockCompany(data) => {
                let ratio = dividend_payout_ratio(data);
                let total = ctx.net_profit * ratio;
                CorporateAction::PayDividend {
                    total: total.max(0.0),
                }
            }
            LegalForm::Cooperative(_data) => {
                let total = ctx.net_profit * 0.5;
                CorporateAction::PayDividend {
                    total: total.max(0.0),
                }
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
            LegalForm::Guild(_) => CorporateAction::Idle, // Phase 85: Guilds expand via member recruitment, not capital
        }
    }

    fn evaluate_production_method(&self, ctx: &CorporateDecisionCtx) -> CorporateAction {
        evaluate_method_switch(ctx)
    }

    fn evaluate_inventory_disposal(&self, ctx: &CorporateDecisionCtx) -> Vec<DisposalOrder> {
        evaluate_inventory_disposal(ctx)
    }

    fn evaluate_transform(&self, ctx: &CorporateDecisionCtx) -> Option<CorporateAction> {
        match self {
            LegalForm::FamilyBusiness(data) => {
                // R4.3: Family business mutualizes into a cooperative when:
                // 1. The family retains <= 70% (workers have significant stake).
                // 2. The company has enough workers to form a viable cooperative.
                // 3. Sector PMI is moderate (not booming — family would IPO instead;
                //    not bust — company would restructure instead).
                // 4. The transformation is economically rational: the buyout cost
                //    (family's share * company_capital) must be affordable from
                //    the company's own cash.
                let avg_wage = ctx.country.macro_indicators.average_wage;
                let min_workers = (avg_wage / 10.0).max(50.0) as u32;
                if ctx.company.worker_capacity < min_workers {
                    return None;
                }
                if data.family_retained_share > 0.70 {
                    return None;
                }
                let sector_pmi = ctx.market_signal.sector_outlook(ctx.sector);
                if sector_pmi < 45.0 || sector_pmi > 65.0 {
                    return None; // Too distressed or too booming
                }

                // R4.3: Compute buyout amount = family_retained_share * company_capital.
                // This is the cash paid to the family for their ownership stake.
                let buyout_amount = ctx.company.company_capital.max(0.0) * data.family_retained_share;

                // Check that the company can afford the buyout from its own cash.
                let available = ctx
                    .company
                    .brokerage_account
                    .as_ref()
                    .map(|ba| ba.cash.max(0.0))
                    .unwrap_or(ctx.company.available_cash.max(0.0));
                if available < buyout_amount {
                    return None; // Cannot afford the buyout
                }

                Some(CorporateAction::TransformLegalForm {
                    transition: LegalTransition::FamilyBusinessToCooperative,
                    buyout_amount,
                })
            }
            LegalForm::MutualAidCircle(data) => {
                // R4.4: Mutual aid circle transitions to a cooperative when:
                // 1. It has enough members (scaled by average_wage).
                // 2. It has a sufficient common fund.
                // 3. Sector PMI is favorable.
                let avg_wage = ctx.country.macro_indicators.average_wage;
                let min_members = (avg_wage / 20.0).max(50.0) as u32;
                if data.member_count < min_members {
                    return None;
                }
                let sector_pmi = ctx.market_signal.sector_outlook(ctx.sector);
                if sector_pmi < 45.0 {
                    return None;
                }
                // No buyout — the common fund becomes the cooperative's patronage pool.
                Some(CorporateAction::TransformLegalForm {
                    transition: LegalTransition::MutualAidCircleToCooperative,
                    buyout_amount: 0.0,
                })
            }
            _ => None,
        }
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
fn evaluate_inventory_disposal(_ctx: &CorporateDecisionCtx) -> Vec<DisposalOrder> {
    let disposal_orders = Vec::new();

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
        ..Default::default()
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
                return CorporateAction::SwitchMethod { method: alt_method };
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
    market_prices: &rustc_hash::FxHashMap<crate::registries::enums::Commodity, f64>,
    base_wage: f64,
) -> f64 {
    let wage_multiplier =
        method.experts_ratio * 3.0 + method.skilled_ratio * 2.0 + method.basic_ratio;
    let wages_per_1k = wage_multiplier * base_wage;

    // Calculate input costs using current market prices
    let input_costs: f64 = method
        .inputs
        .iter()
        .map(|(commodity, amount_per_1k)| {
            let price = market_prices.get(commodity).copied().unwrap_or(100.0);
            amount_per_1k * price
        })
        .sum();

    // Calculate output revenue using current market prices
    let output_revenue: f64 = method
        .outputs
        .iter()
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
    let equipment_cost = 10000.0; // Placeholder - should reference building.fixed_capital

    // Worker retraining cost (based on labor ratio differences)
    let labor_diff = (current.experts_ratio - alternative.experts_ratio).abs()
        + (current.skilled_ratio - alternative.skilled_ratio).abs()
        + (current.basic_ratio - alternative.basic_ratio).abs();
    let retraining_cost = labor_diff * 5000.0;

    // Downtime cost (estimated 2 turns of lost production)
    let downtime_cost = 2000.0; // Placeholder

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
        synthetic_method
            .inputs
            .insert(crate::registries::enums::Commodity::HardCoal, 100.0);

        // Add energy output (synthetic fuel)
        synthetic_method
            .outputs
            .insert(crate::registries::enums::Commodity::Energy, 60.0);

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
                if let Some(Value::Number(n)) = map.get("net_profit") {
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

fn evaluate_family_ipo(
    data: &FamilyBusinessData,
    ctx: &CorporateDecisionCtx,
) -> Option<CorporateAction> {
    // R6.1: Scale threshold by average_wage (inflation-proof).
    let avg_wage = ctx.country.macro_indicators.average_wage;
    let min_capital = avg_wage * 2000.0;
    if ctx.company.company_capital < min_capital {
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

fn evaluate_cooperative_ipo(
    data: &CooperativeData,
    ctx: &CorporateDecisionCtx,
) -> Option<CorporateAction> {
    // R6.3: Scale member threshold and capital threshold by average_wage.
    let avg_wage = ctx.country.macro_indicators.average_wage;
    let min_members = (avg_wage / 20.0).max(50.0) as u32;
    if data.member_count < min_members {
        return None;
    }
    let min_capital = avg_wage * 1000.0;
    if ctx.company.company_capital < min_capital {
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
    // AI & Stability Audit (Pillar 1C + 4A): Broadened distress detection to
    // include raw-material shortage and payroll-coverage threshold. Uses a
    // 3-turn moving average of net profit instead of single-turn value to
    // prevent 1-turn panic firings.
    // Phase 87+: Uses operational_cash() (actual payroll cash source) instead
    // of liquid_capital (capital reserve reduced by seed-inventory deductions).
    let avg_profit = ctx.company.moving_avg_net_profit(3);
    let payroll = ctx.company.offered_wage_per_fte * ctx.company.fulfilled_fte as f64;
    ctx.company.company_capital < 0.0
        || (avg_profit < 0.0 && ctx.company.operational_cash() < payroll * 2.0)
        || ctx.avg_fulfillment_ratio < 0.1
}

/// Phase 88: Determines whether a company is within its material-shortage
/// grace period, during which material-shortage furloughs are suppressed.
///
/// For AGRICULTURE companies: The grace remains active until the company
/// records its first non-zero revenue in `financial_history` (meaning it
/// successfully sold its first harvest), OR a hardcap of 24 turns (1 year)
/// passes since `founded_turn`. This prevents premature furloughs when crops
/// are still growing and no harvest has been sold yet. The revenue-based check
/// is robust against multi-batch scenarios — even if a side-batch harvests
/// early, the grace only expires when actual revenue is recorded.
///
/// Phase 89: Extended to heavy CAPEX industrial sectors (Mining, HeavyIndustry,
/// LightIndustry, Energy, Construction). These sectors have long ramp-up periods
/// before their first sale clears through B2B/B2C. They receive a 12-turn
/// hardcap (half the agriculture 24-turn hardcap, since industrial sales cycles
/// are shorter than agricultural harvest cycles). The same revenue-based check
/// applies — grace expires when first non-zero revenue is recorded.
///
/// For other sectors: The grace is the original Turn 1 check — active only
/// while `financial_history` is empty (no completed production cycle).
fn is_within_material_shortage_grace(company: &Company, current_turn: u32) -> bool {
    match company.sector {
        Sector::Agriculture => {
            // Hardcap: 24 turns (1 year) since founding.
            if current_turn.saturating_sub(company.founded_turn) >= 24 {
                return false;
            }
            // Check if the company has recorded its first non-zero revenue.
            let has_nonzero_revenue = company.financial_history.iter().any(|record| {
                record
                    .get("revenue")
                    .and_then(|v| v.as_f64())
                    .map(|r| r > 0.0)
                    .unwrap_or(false)
            });
            if has_nonzero_revenue {
                return false; // First harvest sold — grace expires
            }
            true // Still waiting for first harvest sale — grace active
        }
        Sector::Mining
        | Sector::HeavyIndustry
        | Sector::LightIndustry
        | Sector::Energy
        | Sector::Construction => {
            // Phase 89: 12-turn hardcap for industrial sectors.
            if current_turn.saturating_sub(company.founded_turn) >= 12 {
                return false;
            }
            // Same revenue-based check as agriculture.
            let has_nonzero_revenue = company.financial_history.iter().any(|record| {
                record
                    .get("revenue")
                    .and_then(|v| v.as_f64())
                    .map(|r| r > 0.0)
                    .unwrap_or(false)
            });
            if has_nonzero_revenue {
                return false; // First sale cleared — grace expires
            }
            true // Still waiting for first sale — grace active
        }
        // Other sectors: Turn 1 grace (no financial history yet)
        _ => company.financial_history.is_empty(),
    }
}

/// Emergency Stabilization: Evaluate whether a distressed company should
/// furlough workers instead of firing them. Furlough is preferred when:
/// - The company is NOT structurally bankrupt (company_capital >= 0.0)
/// - There is a temporary cash shortage OR raw-material shortage
/// - The company has workers to furlough
///
/// Returns `Some(Furlough)` if furlough is appropriate, `None` if the company
/// should proceed to permanent restructuring/liquidation.
fn evaluate_furlough(ctx: &CorporateDecisionCtx) -> Option<CorporateAction> {
    // Structurally bankrupt companies must restructure, not furlough.
    if ctx.company.company_capital < 0.0 {
        return None;
    }

    // No workers to furlough.
    if ctx.company.fulfilled_fte == 0 {
        return None;
    }

    // Determine the nature of the distress:
    // - Raw-material shortage: fulfillment_ratio < 0.1 (can't produce)
    //   Phase 88: Grace period is now revenue-and-hardcap-aware. For agriculture
    //   companies, grace remains active until the first non-zero revenue is
    //   recorded (first harvest sold) OR 24 turns (1 year) since founding.
    //   This prevents premature furloughs when crops are still growing.
    //   For non-agriculture: Turn 1 grace (no financial history yet).
    // - Cash-flow distress: can't cover 2 turns of payroll
    //   Phase 87+: Uses operational_cash() (actual payroll cash source).
    let wage_per_fte = ctx.company.offered_wage_per_fte.max(1.0);
    let total_payroll = ctx.company.fulfilled_fte as f64 * wage_per_fte;
    let cash_shortage = ctx.company.operational_cash() < total_payroll * 2.0;
    let material_shortage = ctx.avg_fulfillment_ratio < 0.1
        && !is_within_material_shortage_grace(ctx.company, ctx.current_turn);

    if !cash_shortage && !material_shortage {
        return None;
    }

    // Calculate furlough count:
    // - If material shortage: furlough proportionally to the shortage
    //   (e.g., 90% shortage → furlough 90% of workers)
    // - If cash shortage: furlough enough to bring payroll within cash budget
    let furlough_count = if material_shortage {
        let shortage_fraction = 1.0 - ctx.avg_fulfillment_ratio;
        ((ctx.company.fulfilled_fte as f64) * shortage_fraction).ceil() as u32
    } else {
        // Cash shortage: furlough enough to bring payroll within budget
        let affordable_fte = (ctx.company.operational_cash() / wage_per_fte).floor() as u32;
        ctx.company.fulfilled_fte.saturating_sub(affordable_fte)
    };

    // Clamp: can't furlough more than we have, and furlough at least 1.
    let furlough_count = furlough_count.min(ctx.company.fulfilled_fte).max(1);

    if furlough_count == 0 {
        return None;
    }

    // wage_fraction = 0.0 (no pay during furlough — era-appropriate, no UI).
    // Future labor law mechanics can increase this.
    Some(CorporateAction::Furlough {
        fte_count: furlough_count,
        wage_fraction: 0.0,
    })
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
        // AI & Stability Audit (Pillar 4B): Apply proto-learning penalty weight.
        // If past expansions led to declining ROI, reduce investment accordingly.
        // If weight > 0.8, skip expansion entirely (the company has "learned"
        // that expansion is counterproductive in current conditions).
        let expansion_weight = ctx.company.action_ledger.weight_for("Expand");
        if expansion_weight > 0.8 {
            return CorporateAction::Idle;
        }
        let investment = ctx.gross_profit * 0.30 * (1.0 - expansion_weight);
        let new_workers =
            cap_new_workers(ctx.company, ((ctx.gross_profit / 1_000.0) as u32).max(1));
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
        let expansion_weight = ctx.company.action_ledger.weight_for("Expand");
        if expansion_weight > 0.8 {
            return CorporateAction::Idle;
        }
        let investment = ctx.gross_profit * 0.20 * (1.0 - expansion_weight);
        let new_workers =
            cap_new_workers(ctx.company, ((ctx.gross_profit / 1_000.0) as u32).max(1));
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
        let expansion_weight = ctx.company.action_ledger.weight_for("Expand");
        if expansion_weight > 0.8 {
            return CorporateAction::Idle;
        }
        let investment = ctx.gross_profit * 0.10 * (1.0 - expansion_weight);
        let new_workers =
            cap_new_workers(ctx.company, ((ctx.gross_profit / 1_000.0) as u32).max(1));
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
    let expansion_weight = ctx.company.action_ledger.weight_for("Expand");
    if expansion_weight > 0.8 {
        return CorporateAction::Idle;
    }
    let desired = ctx.company.desired_investment(ctx.market_signal) * (1.0 - expansion_weight);
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

/// Phase 93: Evaluate whether a mining company should fund a geological survey
/// to discover hidden Rare/UltraRare veins.
///
/// A mining company should consider a survey when:
/// - It has cash above a safety buffer (can afford the sunk cost).
/// - Its current deposit is depleting (current_reserves / total_reserves < 0.5)
///   or output quality is falling.
/// - It is in the Mining sector.
///
/// The AI chooses `target_depth` based on its current method year's
/// `max_depth_for_method_year` (it knows what depth its technology can
/// realistically exploit).
///
/// Returns `CorporateAction::GeologicalSurvey` if a survey is warranted,
/// or `CorporateAction::Idle` otherwise.
fn evaluate_geological_survey(ctx: &CorporateDecisionCtx) -> CorporateAction {
    // Only mining companies survey.
    if ctx.company.sector != Sector::Mining {
        return CorporateAction::Idle;
    }

    // Must have positive cash above a safety buffer.
    // The safety buffer is 6 months of wages (TURNS_PER_YEAR / 4 turns).
    let payroll = ctx.company.fulfilled_fte as f64 * ctx.company.offered_wage_per_fte;
    let safety_buffer = payroll * 6.0;
    let available = ctx
        .company
        .brokerage_account
        .as_ref()
        .map(|ba| ba.cash.max(0.0))
        .unwrap_or(ctx.company.available_cash.max(0.0));

    if available <= safety_buffer * 2.0 {
        // Not enough cash to risk on a survey.
        return CorporateAction::Idle;
    }

    // Determine the company's region.
    let region_id = ctx.company.region_id.clone();

    // Check if the company has a depleting deposit.
    // We check the company's buildings' deposit_id and look up the vein
    // via the country's geological_formations (legacy) since we don't have
    // Planet access in this context. If the deposit is depleting, survey.
    //
    // Since we don't have direct access to the Planet here, we use a simpler
    // heuristic: if the company's fulfillment ratio is low (indicating
    // declining production, possibly due to depletion), consider surveying.
    if ctx.avg_fulfillment_ratio > 0.5 {
        // Production is still healthy — no urgent need to survey.
        return CorporateAction::Idle;
    }

    // Choose target_depth based on the current year's technology.
    // The company surveys to the depth its technology can exploit.
    let target_depth = crate::economy::production::geology::max_depth_for_method_year(ctx.year);

    // Choose a commodity to search for: prefer Rare/UltraRare commodities
    // that the company's region is likely to have. Since we don't have Planet
    // access, we cycle through Rare/UltraRare commodities deterministically
    // based on the current turn.
    let rare_commodities = [
        crate::registries::enums::Commodity::Uranium,
        crate::registries::enums::Commodity::Gold,
        crate::registries::enums::Commodity::Silver,
        crate::registries::enums::Commodity::Tin,
    ];
    let commodity = rare_commodities[ctx.current_turn as usize % rare_commodities.len()];

    CorporateAction::GeologicalSurvey {
        region_id,
        commodity,
        target_depth,
    }
}

/// Phase 95: Evaluate whether the company should design a new product blueprint.
///
/// # Rules
/// * Only HeavyIndustry and LightIndustry companies design blueprints.
/// * Only if the company has fewer than `max_blueprints_per_company` blueprints.
/// * Only if the company has a patented or licensed Commercial tech.
/// * Only if `available_cash >= compute_blueprint_design_cost(sector, average_wage)`.
/// * Only if the company has operational capacity (`fulfilled_fte > 0`).
/// * Returns `DesignBlueprint` with the highest-margin eligible commodity, or `Idle`.
fn evaluate_blueprint_design(ctx: &CorporateDecisionCtx) -> CorporateAction {
    use crate::registries::enums::Commodity;

    // Only industrial sectors design blueprints.
    if ctx.company.sector != Sector::HeavyIndustry && ctx.company.sector != Sector::LightIndustry {
        return CorporateAction::Idle;
    }

    // Must have operational capacity.
    if ctx.company.fulfilled_fte == 0 {
        return CorporateAction::Idle;
    }

    // Must have at least one patent or licensed method (Commercial tech).
    if ctx.company.patents.is_empty() && ctx.company.licensed_methods.is_empty() {
        return CorporateAction::Idle;
    }

    // Compute dynamic design cost.
    let average_wage = ctx.country.macro_indicators.average_wage.max(1.0);
    let design_cost = crate::economy::generative_goods_config::compute_blueprint_design_cost(
        ctx.company.sector,
        average_wage,
        &ctx.country.generative_goods_config,
    );

    // Must have available_cash (NOT rd_budget) for the design fee.
    let available = ctx
        .company
        .brokerage_account
        .as_ref()
        .map(|ba| ba.cash.max(0.0))
        .unwrap_or(ctx.company.available_cash.max(0.0));

    // Keep a safety buffer: 6 months of payroll.
    let payroll = ctx.company.fulfilled_fte as f64 * ctx.company.offered_wage_per_fte;
    let safety_buffer = payroll * 6.0;
    if available <= safety_buffer + design_cost {
        return CorporateAction::Idle;
    }

    // Choose the output commodity: pick the first blueprint-eligible commodity
    // from the company's first building's outputs.
    // This is a simplification — a full implementation would evaluate all
    // eligible commodities and pick the highest-margin one.
    let building = ctx.buildings.iter().find(|b| b.owner_id == ctx.company.id);

    let output_commodity = if let Some(b) = building {
        // Find the first blueprint-eligible output.
        b.active_method
            .outputs
            .keys()
            .find(|c| c.is_blueprint_eligible())
            .copied()
    } else {
        None
    };

    let output_commodity = match output_commodity {
        Some(c) => c,
        None => {
            // Fallback: use sector-default blueprint-eligible commodities.
            match ctx.company.sector {
                Sector::HeavyIndustry => Commodity::IndustrialMachinery,
                Sector::LightIndustry => Commodity::Cars,
                _ => return CorporateAction::Idle,
            }
        }
    };

    // Use the first patent's tech_id as the base tech.
    let base_tech = ctx
        .company
        .patents
        .first()
        .map(|p| p.tech_id.clone())
        .or_else(|| {
            ctx.company
                .licensed_methods
                .first()
                .map(|lm| lm.tech_id.clone())
        });

    let base_tech = match base_tech {
        Some(t) => t,
        None => return CorporateAction::Idle,
    };

    CorporateAction::DesignBlueprint {
        output_commodity,
        base_tech,
        required_slot: crate::registries::production_methods::MethodSlot::Production,
    }
}

fn dividend_payout_ratio(data: &JointStockData) -> f64 {
    let board = data.board_independence.clamp(0.0, 1.0);
    0.3 + board * 0.3
}

/// R5.2: Update board independence dynamically based on CEO performance.
///
/// Independent boards push for higher payout ratios. When the CEO delivers
/// strong profits, board loyalty increases (independence decreases). When
/// the CEO underperforms, independence increases (board becomes more hostile).
///
/// # Arguments
/// * `data` - The JSC data with board_independence to update.
/// * `is_profitable` - Whether the company was profitable this turn.
/// * `profit_margin` - Net profit / revenue (clamped to [-1, 1]).
pub fn update_board_independence(
    data: &mut crate::entities::legal_form::JointStockData,
    is_profitable: bool,
    profit_margin: f64,
) {
    // R5.2: Independence drifts toward 1.0 when CEO underperforms,
    // toward 0.4 when CEO overperforms (never fully dependent).
    let target = if is_profitable {
        0.4 + (1.0 - profit_margin.clamp(0.0, 1.0)) * 0.3
    } else {
        0.8
    };
    // Mean-reversion toward target at 5% per turn
    let rate = 0.05;
    data.board_independence += (target - data.board_independence) * rate;
    data.board_independence = data.board_independence.clamp(0.0, 1.0);
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
        average_wage: ctx.country.macro_indicators.average_wage,
    };

    legal_form
        .clone()
        .try_transition(transition, &transition_ctx)
        .ok()
}

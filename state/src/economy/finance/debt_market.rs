//! Advanced debt market — wholesale securities, retail savings bonds, and
//! sovereign default mechanics.
//!
//! This module implements Pillar III of the Phase 8 blueprint: a full debt
//! market with wholesale tradable `TreasurySecurity` instruments held by
//! banks/funds, retail non-tradable `SavingsBond` instruments held by citizens,
//! a secondary market for wholesale trading, and sovereign default mechanics
//! with arrears capitalization, credit rating crashes, and primary market
//! lockout.

use crate::state::macro_data::{annual_to_per_turn_rate, TURNS_PER_YEAR};
use crate::state::Country;
use serde::{Deserialize, Serialize};

// ============================================================================
// CREDIT RATING
// ============================================================================

/// Sovereign credit rating (Moody's-style scale).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum CreditRating {
    /// Highest investment grade.
    Aaa,
    /// High quality, investment grade.
    Aa1,
    /// High quality, investment grade.
    Aa2,
    /// High quality, investment grade.
    Aa3,
    /// Upper-medium investment grade.
    A1,
    /// Upper-medium investment grade.
    A2,
    /// Upper-medium investment grade.
    A3,
    /// Lower-medium investment grade.
    Baa1,
    /// Lower-medium investment grade.
    Baa2,
    /// Lower-medium investment grade.
    #[default]
    Baa3,
    /// Speculative grade (non-investment).
    Ba1,
    /// Speculative grade.
    Ba2,
    /// Speculative grade.
    Ba3,
    /// Highly speculative.
    Caa1,
    /// Highly speculative.
    Caa2,
    /// Highly speculative.
    Caa3,
    /// Extremely speculative.
    Ca,
    /// Default.
    C,
}

impl CreditRating {
    /// Returns the ordinal index (0 = Aaa, 17 = C).
    pub fn ordinal(self) -> usize {
        match self {
            CreditRating::Aaa => 0,
            CreditRating::Aa1 => 1,
            CreditRating::Aa2 => 2,
            CreditRating::Aa3 => 3,
            CreditRating::A1 => 4,
            CreditRating::A2 => 5,
            CreditRating::A3 => 6,
            CreditRating::Baa1 => 7,
            CreditRating::Baa2 => 8,
            CreditRating::Baa3 => 9,
            CreditRating::Ba1 => 10,
            CreditRating::Ba2 => 11,
            CreditRating::Ba3 => 12,
            CreditRating::Caa1 => 13,
            CreditRating::Caa2 => 14,
            CreditRating::Caa3 => 15,
            CreditRating::Ca => 16,
            CreditRating::C => 17,
        }
    }

    /// Returns the rating at the given ordinal, clamped to valid range.
    pub fn from_ordinal(n: usize) -> Self {
        let all = [
            CreditRating::Aaa,
            CreditRating::Aa1,
            CreditRating::Aa2,
            CreditRating::Aa3,
            CreditRating::A1,
            CreditRating::A2,
            CreditRating::A3,
            CreditRating::Baa1,
            CreditRating::Baa2,
            CreditRating::Baa3,
            CreditRating::Ba1,
            CreditRating::Ba2,
            CreditRating::Ba3,
            CreditRating::Caa1,
            CreditRating::Caa2,
            CreditRating::Caa3,
            CreditRating::Ca,
            CreditRating::C,
        ];
        all[n.min(all.len() - 1)]
    }

    /// Downgrades the rating by `n` notches, clamped at `C`.
    pub fn downgrade(self, n: usize) -> Self {
        Self::from_ordinal(self.ordinal() + n)
    }

    /// Upgrades the rating by `n` notches, clamped at `Aaa`.
    pub fn upgrade(self, n: usize) -> Self {
        Self::from_ordinal(self.ordinal().saturating_sub(n))
    }
}

// ============================================================================
// TREASURY SECURITY (WHOLESALE)
// ============================================================================

/// Type of wholesale treasury security.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum TreasurySecurityType {
    /// Short-term, zero-coupon, discount (Treasury bills).
    #[default]
    TreasuryBill,
    /// Long-term, coupon-bearing (Obligacje).
    TreasuryBond,
}

/// Coupon payment frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum CouponFrequency {
    /// Coupon paid each turn.
    #[default]
    EveryTurn,
    /// Coupon paid once per year (every 4 turns).
    Annual,
    /// Interest compounded, paid at maturity.
    CapitalizedAtMaturity,
}

/// Type of entity holding a wholesale treasury security.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum SecurityHolderType {
    /// Commercial bank.
    CommercialBank,
    /// Universal bank.
    UniversalBank,
    /// Investment bank.
    InvestmentBank,
    /// Open-end fund (FIO).
    OpenEndFund,
    /// Closed-end fund (FIZ).
    ClosedEndFund,
    /// Hedge fund.
    HedgeFund,
    /// Central bank.
    CentralBank,
    /// Foreign entity.
    ForeignEntity,
    /// Retail savings bond (non-tradable, held by citizens via B2C window).
    #[default]
    RetailSavingsBond,
    /// Phase 38: DSPW primary dealer (bank that purchased at auction).
    PrimaryDealer,
}

/// A single holder of a treasury security.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SecurityHolder {
    /// Entity ID (company_id, fund_id, or "CITIZEN:region:class").
    pub entity_id: String,
    /// Type of holder.
    pub holder_type: SecurityHolderType,
    /// Face value held.
    pub quantity: f64,
    /// Price paid at acquisition.
    pub purchase_price: f64,
}

/// A wholesale treasury security (T-Bill or T-Bond).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TreasurySecurity {
    /// Unique security ID (e.g. "TBILL-2024-001").
    pub id: String,
    /// Security type (bill or bond).
    pub security_type: TreasurySecurityType,
    /// Principal at maturity (face value).
    pub face_value: f64,
    /// Price at issuance (discount for T-Bills).
    pub issue_price: f64,
    /// Turn of issuance.
    pub issue_turn: u32,
    /// Turns until maturity.
    pub maturity_turns: u32,
    /// Turns remaining until maturity.
    pub turns_remaining: u32,
    /// Coupon rate (0 for zero-coupon T-Bills).
    pub coupon_rate: f64,
    /// Coupon payment frequency.
    pub coupon_frequency: CouponFrequency,
    /// Whether this is an inflation-indexed bond.
    pub is_inflation_indexed: bool,
    /// All current holders.
    pub holders: Vec<SecurityHolder>,
    /// Last turn a coupon was paid.
    pub last_coupon_turn: u32,
    /// Whether this security has matured.
    pub is_matured: bool,
    /// Phase 38: Whether this security is unpurchased auction inventory
    /// awaiting DSPW primary dealer settlement. When true, the treasury
    /// has not yet received cash — the security sits in limbo until a
    /// DSPW bank pulls it from inventory.
    #[serde(default)]
    pub is_auction_inventory: bool,
}

// ============================================================================
// SAVINGS BOND (RETAIL)
// ============================================================================

/// A non-tradable retail savings bond held by citizens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SavingsBond {
    /// Unique bond ID.
    pub id: String,
    /// Amount citizen deposited (principal).
    pub face_value: f64,
    /// Fixed interest rate offered to retail.
    pub interest_rate: f64,
    /// Turn of issuance.
    pub issue_turn: u32,
    /// Turns until maturity (typically 16-40 = 4-10 years).
    pub maturity_turns: u32,
    /// Turns remaining until maturity.
    pub turns_remaining: u32,
    /// Whether this bond is inflation-indexed.
    pub is_inflation_indexed: bool,
    /// Holder key: "region_id:class_name" identifying the citizen group.
    pub holder_key: String,
    /// Unpaid interest capitalized (see default mechanics).
    pub arrears: f64,
}

// ============================================================================
// SECONDARY MARKET
// ============================================================================

/// Type of debt order on the secondary market.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum DebtOrderType {
    /// Buy order.
    #[default]
    Buy,
    /// Sell order.
    Sell,
}

/// A buy or sell order for a treasury security on the secondary market.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DebtOrder {
    /// Security ID being traded.
    pub security_id: String,
    /// Entity placing the order.
    pub entity_id: String,
    /// Order type (buy or sell).
    pub order_type: DebtOrderType,
    /// Quantity (face value) to trade.
    pub quantity: f64,
    /// Price as fraction of face value (0.95 = 95%).
    pub price: f64,
}

/// State of the wholesale secondary market for treasury securities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SecondaryMarketState {
    /// Active buy orders.
    pub buy_orders: Vec<DebtOrder>,
    /// Active sell orders.
    pub sell_orders: Vec<DebtOrder>,
    /// Last turn the market was cleared.
    pub last_clearing_turn: u32,
    /// Market-determined yield from last clearing.
    pub last_yield: f64,
}

// ============================================================================
// DEFAULT EVENT
// ============================================================================

/// Record of a sovereign default event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DefaultEvent {
    /// Turn the default occurred.
    pub turn: u32,
    /// Amount that could not be paid.
    pub unpaid_amount: f64,
    /// Security IDs affected by the default.
    pub security_ids_affected: Vec<String>,
    /// Credit rating before the default.
    pub rating_before: CreditRating,
    /// Credit rating after the default.
    pub rating_after: CreditRating,
}

// ============================================================================
// DEBT MARKET
// ============================================================================

/// The complete debt market for a country.
///
/// Placed on `Country` as `debt_market: DebtMarket`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DebtMarket {
    /// Outstanding wholesale (tradable) securities.
    pub outstanding_securities: Vec<TreasurySecurity>,
    /// Outstanding retail (non-tradable) savings bonds.
    pub retail_bonds: Vec<SavingsBond>,
    /// Bank company IDs that are designated primary dealers (DSPW).
    pub primary_dealers: Vec<String>,
    /// Whether the DSPW mechanism is enabled.
    pub dspw_enabled: bool,
    /// Secondary market state for wholesale securities.
    pub secondary_market: SecondaryMarketState,
    /// Sum of all outstanding principals (wholesale + retail).
    pub total_outstanding_debt: f64,
    /// Weighted average interest rate across all outstanding debt.
    pub weighted_avg_interest_rate: f64,
    /// Current sovereign credit rating.
    pub credit_rating: CreditRating,
    /// History of default events.
    pub default_history: Vec<DefaultEvent>,
    /// True when in arrears default — no new wholesale issuance possible.
    pub is_locked_out_of_primary: bool,
    /// Cumulative unpaid interest capitalized as arrears.
    pub total_arrears: f64,
}

impl DebtMarket {
    /// Recalculates aggregate debt metrics from individual securities.
    pub fn recalculate(&mut self) {
        let wholesale: f64 = self
            .outstanding_securities
            .iter()
            .filter(|s| !s.is_matured)
            .flat_map(|s| s.holders.iter().map(|h| h.quantity))
            .sum();
        let retail: f64 = self
            .retail_bonds
            .iter()
            .map(|b| b.face_value + b.arrears)
            .sum();
        self.total_outstanding_debt = wholesale + retail;

        // Weighted average interest rate
        let total = self.total_outstanding_debt;
        if total > 0.0 {
            let w_sum: f64 = self
                .outstanding_securities
                .iter()
                .filter(|s| !s.is_matured)
                .flat_map(|s| s.holders.iter().map(|h| h.quantity * s.coupon_rate))
                .sum::<f64>()
                + self
                    .retail_bonds
                    .iter()
                    .map(|b| b.face_value * b.interest_rate)
                    .sum::<f64>();
            self.weighted_avg_interest_rate = w_sum / total;
        }
    }
}

// ============================================================================
// PRIMARY MARKET: WHOLESALE DEBT ISSUANCE
// ============================================================================

/// Issues treasury securities to cover a fiscal deficit.
///
/// # Arguments
/// * `country` - Mutable country state.
/// * `debt_market` - Mutable debt market.
/// * `amount_needed` - Deficit amount to raise.
/// * `current_turn` - Current turn number.
///
/// # Rules
/// * If `is_locked_out_of_primary` is true, **no issuance occurs**.
/// * Without DSPW: traditional auction among eligible institutional holders.
/// * With DSPW: designated primary dealers are legally obligated to absorb
///   the entire issue at a price discount: `dspw_issue_price = market_clearing_price * 0.995`.
///   This price discount means DSPWs earn a higher yield (inverse price-yield relationship).
/// * Cash flows: `bank.brokerage_account.cash → treasury.liquid_reserves`.
/// * Securities credited to `bank.balance_sheet.securities`.
pub fn issue_treasury_securities(country: &mut Country, amount_needed: f64, current_turn: u32) {
    if country.debt_market.is_locked_out_of_primary || amount_needed <= 0.0 {
        return;
    }

    // Determine security type: short-term T-Bill for turn-level deficit
    let is_short_term = amount_needed < country.budget.gdp * 0.05;
    let maturity_turns = if is_short_term { 4 } else { 20 };

    // Phase 36: Sovereign bond yields are now a spread over the CB reference rate.
    // The credit spread reflects default risk based on debt-to-GDP ratio.
    let cb_reference_rate = country.central_bank.interest_rates.reference_rate;
    let debt_to_gdp = if country.budget.gdp > 0.0 {
        country.debt_market.total_outstanding_debt / country.budget.gdp
    } else {
        0.0
    };
    // Credit spread: 0.5% base + 1% per 50% of debt-to-GDP, capped at 5%
    let credit_spread = (0.005 + (debt_to_gdp * 0.02)).min(0.05);

    // Phase 67: Reputation-based interest rate penalty.
    // Low global reputation increases sovereign borrowing costs — bad-faith
    // actors (treaty violators) pay a risk premium reflecting lower sovereign trust.
    let reputation_config = crate::international::reputation::ReputationConfig::default();
    let reputation_penalty = country
        .global_reputation
        .debt_interest_penalty(&reputation_config);

    let sovereign_yield = cb_reference_rate + credit_spread + reputation_penalty;

    let coupon_rate = if is_short_term {
        0.0 // T-Bills are zero-coupon
    } else {
        sovereign_yield.max(0.01).min(0.20) // Floor at 1%, cap at 20%
    };

    // Calculate issue price using the sovereign yield
    let market_yield = sovereign_yield.max(0.01);
    let base_price = if is_short_term {
        // T-Bill: issue_price = face_value / (1 + yield * turns/4)
        1.0 / (1.0 + market_yield * maturity_turns as f64 / 4.0)
    } else {
        // T-Bond: approximately par
        1.0 / (1.0 + market_yield * 0.1)
    };

    let issue_price =
        if country.debt_market.dspw_enabled && !country.debt_market.primary_dealers.is_empty() {
            // DSPW: mandatory price discount (0.5% below market clearing price)
            // This means DSPWs pay less, earning a higher yield
            base_price * 0.995
        } else {
            base_price
        };

    // Phase 38: DSPW Reversed Transaction Flow.
    // When DSPW is enabled and primary dealers exist, securities are created
    // as unpurchased "Auction Inventory" — the treasury does NOT receive cash
    // yet. The DSPW banks will pull-purchase from this inventory during
    // process_banking_turn (which has access to the companies slice).
    // When DSPW is NOT enabled, fall back to the citizen-savings pathway.
    let has_dspw =
        country.debt_market.dspw_enabled && !country.debt_market.primary_dealers.is_empty();

    let security_id = format!(
        "{}-{}-{:04}",
        if is_short_term { "TBILL" } else { "TBOND" },
        current_turn,
        country.debt_market.outstanding_securities.len() + 1
    );

    if has_dspw {
        // Create auction inventory — no cash changes hands yet.
        // The dspw_auction_settlement step in turn.rs will handle the purchase.
        let actual_amount = amount_needed;
        country
            .debt_market
            .outstanding_securities
            .push(TreasurySecurity {
                id: security_id,
                security_type: if is_short_term {
                    TreasurySecurityType::TreasuryBill
                } else {
                    TreasurySecurityType::TreasuryBond
                },
                face_value: actual_amount,
                issue_price,
                issue_turn: current_turn,
                maturity_turns,
                turns_remaining: maturity_turns,
                coupon_rate,
                coupon_frequency: if is_short_term {
                    CouponFrequency::CapitalizedAtMaturity
                } else {
                    CouponFrequency::Annual
                },
                is_inflation_indexed: false,
                holders: Vec::new(), // Empty — awaiting DSPW purchase
                last_coupon_turn: current_turn,
                is_matured: false,
                is_auction_inventory: true,
            });
        country.debt_market.recalculate();
        return;
    }

    // Fallback: No DSPW dealers — use citizen savings as buyer capacity.
    // Phase 31: STRICT DOUBLE-ENTRY — deduct from citizen savings, credit treasury.
    let total_capacity = country.budget.citizen_savings.max(0.0) * 0.05; // Citizens allocate up to 5% of savings

    if total_capacity <= 0.0 {
        return;
    }

    let actual_amount = amount_needed.min(total_capacity);
    let security_id = format!(
        "{}-{}-{:04}",
        if is_short_term { "TBILL" } else { "TBOND" },
        current_turn,
        country.debt_market.outstanding_securities.len() + 1
    );

    let mut holders = Vec::new();
    let total_raised = actual_amount * issue_price;

    // Phase 31: Record as citizen holder (not CentralBank) since we're
    // using citizen savings as the real buyer capacity.
    holders.push(SecurityHolder {
        entity_id: "CITIZEN_AGGREGATE".to_string(),
        holder_type: SecurityHolderType::RetailSavingsBond,
        quantity: actual_amount,
        purchase_price: total_raised,
    });

    if total_raised > 0.0 {
        // Phase 31: Double-entry — deduct from citizen savings, credit treasury.
        country.budget.citizen_savings -= total_raised;
        country.budget.liquid_reserves += total_raised;
        country
            .debt_market
            .outstanding_securities
            .push(TreasurySecurity {
                id: security_id,
                security_type: if is_short_term {
                    TreasurySecurityType::TreasuryBill
                } else {
                    TreasurySecurityType::TreasuryBond
                },
                face_value: actual_amount,
                issue_price,
                issue_turn: current_turn,
                maturity_turns,
                turns_remaining: maturity_turns,
                coupon_rate,
                coupon_frequency: if is_short_term {
                    CouponFrequency::CapitalizedAtMaturity
                } else {
                    CouponFrequency::Annual
                },
                is_inflation_indexed: false,
                holders,
                last_coupon_turn: current_turn,
                is_matured: false,
                is_auction_inventory: false,
            });
        country.debt_market.recalculate();
    }
}

// ============================================================================
// RETAIL MARKET: SAVINGS BONDS (B2C WINDOW)
// ============================================================================

/// Clears retail savings bond purchases during Phase 6.5 (B2C).
///
/// # Arguments
/// * `country` - Mutable country state.
/// * `debt_market` - Mutable debt market.
/// * `current_turn` - Current turn number.
///
/// # Rules
/// * Citizens buy non-tradable SavingsBonds from a Treasury window.
/// * Cash flows: `region.class_demographics.savings → treasury.liquid_reserves`.
/// * No iteration over individual citizens — operates at aggregate demographic class level.
/// * **Causality**: Cash raised here funds the NEXT turn's budget (Turn X+1),
///   not the current turn's spending.
pub fn clear_savings_bonds_b2c(country: &mut Country, current_turn: u32) {
    // Set retail rate: 1-2% above wholesale yield to incentivize participation
    let wholesale_yield = country.debt_market.weighted_avg_interest_rate.max(0.03);
    let retail_rate = wholesale_yield + 0.015;

    // Maximum issuance cap per turn: 2% of GDP
    let max_issuance = country.budget.gdp * 0.02;
    if max_issuance <= 0.0 {
        return;
    }

    // Aggregate citizen savings across all regions
    let total_citizen_savings: f64 = country
        .regions
        .iter()
        .flat_map(|r| {
            r.class_demographics
                .rural_classes
                .values()
                .chain(r.class_demographics.urban_classes.values())
        })
        .map(|cd| cd.savings)
        .sum();

    if total_citizen_savings <= 0.0 {
        return;
    }

    // Citizens allocate up to 5% of their savings to savings bonds
    let potential_demand = total_citizen_savings * 0.05;
    let actual_issuance = potential_demand.min(max_issuance);

    if actual_issuance <= 0.0 {
        return;
    }

    // Deduct from citizen savings pro-rata across regions and classes
    let mut total_absorbed = 0.0;
    for region in &mut country.regions {
        for (class_name, cd) in region
            .class_demographics
            .rural_classes
            .iter_mut()
            .chain(region.class_demographics.urban_classes.iter_mut())
        {
            if cd.savings <= 0.0 {
                continue;
            }
            let share = cd.savings / total_citizen_savings;
            let purchase = actual_issuance * share;
            if purchase > cd.savings {
                continue;
            }
            cd.savings -= purchase;
            total_absorbed += purchase;

            // Create savings bond record
            let holder_key = format!("{}:{}", region.id, class_name);
            country.debt_market.retail_bonds.push(SavingsBond {
                id: format!(
                    "SB-{}-{:04}",
                    current_turn,
                    country.debt_market.retail_bonds.len() + 1
                ),
                face_value: purchase,
                interest_rate: retail_rate,
                issue_turn: current_turn,
                maturity_turns: 24, // 6 years default
                turns_remaining: 24,
                is_inflation_indexed: false,
                holder_key,
                arrears: 0.0,
            });
        }
    }

    if total_absorbed > 0.0 {
        // Cash flows to treasury — funds NEXT turn's budget
        country.budget.liquid_reserves += total_absorbed;
        country.debt_market.recalculate();
    }
}

// ============================================================================
// DEBT SERVICE
// ============================================================================

/// Processes debt service for all outstanding securities and retail bonds.
///
/// # Arguments
/// * `country` - Mutable country state.
/// * `debt_market` - Mutable debt market.
/// * `current_turn` - Current turn number.
///
/// # Rules
/// * For each wholesale `TreasurySecurity`:
///   - Coupon payment (if due): `treasury.liquid_reserves → holder.brokerage_account.cash`.
///   - Inflation-indexed bonds: principal indexed by `(1 + inflation_rate)` before coupon.
///   - Principal repayment at maturity.
/// * For each retail `SavingsBond`:
///   - Interest payment: `treasury.liquid_reserves → region.class_demographics.savings`.
///   - Principal repayment at maturity.
/// * If `treasury.liquid_reserves < total_due`: partial pro-rata payment,
///   unpaid interest capitalized as arrears, credit rating crashes by 3 notches,
///   primary market lockout triggered.
pub fn process_debt_service(
    country: &mut Country,
    companies: &mut [crate::entities::Company],
    current_turn: u32,
) {
    let inflation_rate = country.macro_indicators.inflation / 100.0;
    let mut total_due = 0.0_f64;
    let mut payments: Vec<(String, f64)> = Vec::new(); // (entity_id, amount)

    // Process wholesale securities
    let mut matured_indices = Vec::new();
    for (sec_idx, security) in country
        .debt_market
        .outstanding_securities
        .iter_mut()
        .enumerate()
    {
        if security.is_matured {
            continue;
        }

        // Decrement turns remaining
        if security.turns_remaining > 0 {
            security.turns_remaining -= 1;
        }

        // Coupon payment check (Phase 74: fix frequency — Annual = every 24 turns)
        let coupon_due = match security.coupon_frequency {
            CouponFrequency::EveryTurn => true,
            CouponFrequency::Annual => {
                (current_turn - security.last_coupon_turn) >= TURNS_PER_YEAR as u32
            }
            CouponFrequency::CapitalizedAtMaturity => security.turns_remaining == 0,
        };

        if coupon_due && security.coupon_rate > 0.0 {
            for holder in &mut security.holders {
                // Inflation-indexed: adjust principal first
                let adjusted_principal = if security.is_inflation_indexed {
                    let new_principal = holder.quantity * (1.0 + inflation_rate);
                    holder.quantity = new_principal;
                    new_principal
                } else {
                    holder.quantity
                };

                // Phase 74: Compound interest calculation
                let interest = match security.coupon_frequency {
                    CouponFrequency::EveryTurn => {
                        // Per-turn compound rate
                        adjusted_principal * annual_to_per_turn_rate(security.coupon_rate)
                    }
                    CouponFrequency::Annual => {
                        // Annual coupon paid once per year — full annual rate is correct
                        adjusted_principal * security.coupon_rate
                    }
                    CouponFrequency::CapitalizedAtMaturity => {
                        // Compound over the full maturity period
                        let maturity_years = security.maturity_turns as f64 / TURNS_PER_YEAR as f64;
                        adjusted_principal
                            * ((1.0 + security.coupon_rate).powf(maturity_years) - 1.0)
                    }
                };

                total_due += interest;
                payments.push((holder.entity_id.clone(), interest));
            }
            security.last_coupon_turn = current_turn;
        }

        // Principal repayment at maturity
        if security.turns_remaining == 0 {
            for holder in &security.holders {
                total_due += holder.quantity;
                payments.push((holder.entity_id.clone(), holder.quantity));
            }
            matured_indices.push(sec_idx);
        }
    }

    // Process retail savings bonds
    let mut matured_retail_indices = Vec::new();
    for (bond_idx, bond) in country.debt_market.retail_bonds.iter_mut().enumerate() {
        if bond.turns_remaining > 0 {
            bond.turns_remaining -= 1;
        }

        // Interest payment (annual — Phase 74: fix to every 24 turns, not 4)
        if (current_turn - bond.issue_turn).is_multiple_of(TURNS_PER_YEAR as u32)
            && bond.turns_remaining > 0
        {
            let adjusted_principal = if bond.is_inflation_indexed {
                let new_principal = bond.face_value * (1.0 + inflation_rate);
                bond.face_value = new_principal;
                new_principal
            } else {
                bond.face_value
            };

            // Annual coupon paid once per year — full annual rate is correct
            let interest = adjusted_principal * bond.interest_rate;
            total_due += interest;
            // Retail interest goes to citizen savings
            payments.push((format!("RETAIL:{}", bond.holder_key), interest));
        }

        // Principal repayment at maturity
        if bond.turns_remaining == 0 {
            total_due += bond.face_value;
            payments.push((format!("RETAIL:{}", bond.holder_key), bond.face_value));
            matured_retail_indices.push(bond_idx);
        }
    }

    // Check for sovereign default
    let available = country.budget.liquid_reserves;
    if total_due > 0.0 && available < total_due {
        // Partial pro-rata payment
        let payment_ratio = available / total_due;
        country.budget.liquid_reserves = 0.0;

        // Capitalize unpaid interest as arrears
        let unpaid = total_due - available;
        country.debt_market.total_arrears += unpaid;
        country.debt_market.is_locked_out_of_primary = true;

        // Credit rating crash by 3 notches
        let rating_before = country.debt_market.credit_rating;
        country.debt_market.credit_rating = country.debt_market.credit_rating.downgrade(3);

        // Record default event
        country.debt_market.default_history.push(DefaultEvent {
            turn: current_turn,
            unpaid_amount: unpaid,
            security_ids_affected: country
                .debt_market
                .outstanding_securities
                .iter()
                .filter(|s| !s.is_matured)
                .map(|s| s.id.clone())
                .collect(),
            rating_before,
            rating_after: country.debt_market.credit_rating,
        });

        // Capitalize unpaid portion into holder principals (wholesale)
        // Phase 74: Use compound per-turn rate for capitalization
        for security in &mut country.debt_market.outstanding_securities {
            if security.is_matured {
                continue;
            }
            for holder in &mut security.holders {
                let per_turn_rate = annual_to_per_turn_rate(security.coupon_rate);
                let holder_due = holder.quantity * per_turn_rate;
                let unpaid_holder = holder_due * (1.0 - payment_ratio);
                holder.quantity += unpaid_holder;
            }
        }

        // Capitalize unpaid retail interest into bond arrears
        // Phase 74: Use compound per-turn rate for capitalization
        for bond in &mut country.debt_market.retail_bonds {
            let per_turn_rate = annual_to_per_turn_rate(bond.interest_rate);
            let interest = bond.face_value * per_turn_rate;
            let unpaid_interest = interest * (1.0 - payment_ratio);
            bond.arrears += unpaid_interest;
        }
    } else if total_due > 0.0 {
        // Full payment
        country.budget.liquid_reserves -= total_due;

        // Credit rating recovery: +1 notch per turn of full payment
        if country.debt_market.credit_rating.ordinal() > 0
            && country.debt_market.total_arrears == 0.0
        {
            country.debt_market.credit_rating = country.debt_market.credit_rating.upgrade(1);
        }
    }

    // Phase 24A.2: Credit holders with their payments (previously a black hole —
    // the treasury debited liquid_reserves but no holder was ever credited).
    // payment_ratio is 1.0 for full payment, < 1.0 for partial (pro-rata).
    let payment_ratio = if total_due > 0.0 {
        (available.min(total_due)) / total_due
    } else {
        1.0
    };
    let cb_id = country.central_bank.id.clone();
    for (entity_id, amount) in &payments {
        let actual_credit = amount * payment_ratio;
        if actual_credit <= 0.0 {
            continue;
        }
        if let Some(key) = entity_id.strip_prefix("RETAIL:") {
            // Retail savings bond: credit citizen savings.
            // entity_id format: "RETAIL:region_id:class_name"
            if let Some(colon_pos) = key.find(':') {
                let region_id = &key[..colon_pos];
                let class_name = &key[colon_pos + 1..];
                for region in &mut country.regions {
                    if region.id == region_id {
                        // Try rural classes first, then urban
                        if let Some(class) =
                            region.class_demographics.rural_classes.get_mut(class_name)
                        {
                            class.savings += actual_credit;
                        } else if let Some(class) =
                            region.class_demographics.urban_classes.get_mut(class_name)
                        {
                            class.savings += actual_credit;
                        }
                        break;
                    }
                }
            }
        } else if *entity_id == cb_id {
            // Central bank holder: CB receives payment from treasury.
            // Since CB profits are remitted to treasury, credit back to budget.
            // This is an internal public-sector transfer (net zero money mass).
            country.budget.liquid_reserves += actual_credit;
        } else {
            // Commercial bank or fund holder: credit the company.
            if let Some(company) = companies.iter_mut().find(|c| c.id == *entity_id) {
                if let Some(ref mut bs) = company.balance_sheet {
                    // Bank holder: credit reserves (principal repayment + interest income)
                    bs.reserves_at_central_bank += actual_credit;
                } else if let Some(ref mut ba) = company.brokerage_account {
                    // Fund/institutional holder: credit brokerage cash
                    ba.cash += actual_credit;
                } else {
                    // Fallback: credit available_cash
                    company.available_cash += actual_credit;
                }
            }
            // Foreign entity holders: money leaves the system (no credit needed).
        }
    }

    // Remove matured securities
    for idx in matured_indices.iter().rev() {
        country.debt_market.outstanding_securities[*idx].is_matured = true;
    }
    country
        .debt_market
        .outstanding_securities
        .retain(|s| !s.is_matured);

    // Remove matured retail bonds
    country
        .debt_market
        .retail_bonds
        .retain(|b| b.turns_remaining > 0);

    country.debt_market.recalculate();
}

// ============================================================================
// ARREARS CLEARANCE
// ============================================================================

/// Clears arrears when the treasury has positive reserves.
///
/// # Arguments
/// * `country` - Mutable country state.
/// * `debt_market` - Mutable debt market.
///
/// # Rules
/// * When `treasury.liquid_reserves > 0` and `total_arrears > 0`, the Treasury
///   must prioritize arrears repayment.
/// * Once `total_arrears == 0`, `is_locked_out_of_primary` is reset to `false`.
pub fn clear_arrears(country: &mut Country) {
    if country.debt_market.total_arrears <= 0.0 {
        return;
    }

    let available = country.budget.liquid_reserves;
    if available <= 0.0 {
        return;
    }

    let payment = available.min(country.debt_market.total_arrears);
    country.budget.liquid_reserves -= payment;
    country.debt_market.total_arrears -= payment;

    if country.debt_market.total_arrears <= 0.0 {
        country.debt_market.total_arrears = 0.0;
        country.debt_market.is_locked_out_of_primary = false;
    }
}

// ============================================================================
// SECONDARY MARKET CLEARING
// ============================================================================

/// Clears the wholesale secondary debt market.
///
/// # Arguments
/// * `debt_market` - Mutable debt market.
/// * `current_turn` - Current turn number.
///
/// # Rules
/// * Match buy and sell orders by price (highest buy >= lowest sell).
/// * Execute trades: buyer cash → seller cash, update holder records.
/// * Calculate market yield from clearing prices.
/// * Update `debt_market.secondary_market.last_yield`.
pub fn clear_secondary_debt_market(debt_market: &mut DebtMarket, _current_turn: u32) {
    let sm = &mut debt_market.secondary_market;

    // Sort buy orders descending by price
    sm.buy_orders.sort_by(|a, b| {
        b.price
            .partial_cmp(&a.price)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Sort sell orders ascending by price
    sm.sell_orders.sort_by(|a, b| {
        a.price
            .partial_cmp(&b.price)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut clearing_price = 0.0_f64;
    let mut total_quantity = 0.0_f64;

    let mut buy_idx = 0;
    let mut sell_idx = 0;

    while buy_idx < sm.buy_orders.len() && sell_idx < sm.sell_orders.len() {
        let buy = &sm.buy_orders[buy_idx];
        let sell = &sm.sell_orders[sell_idx];

        if buy.price < sell.price {
            break;
        }

        let trade_qty = buy.quantity.min(sell.quantity);
        let trade_price = (buy.price + sell.price) / 2.0;

        clearing_price = trade_price;
        total_quantity += trade_qty;

        // Update security holder records
        if let Some(security) = debt_market
            .outstanding_securities
            .iter_mut()
            .find(|s| s.id == buy.security_id)
        {
            // Decrease seller quantity, increase buyer quantity
            if let Some(seller_holder) = security
                .holders
                .iter_mut()
                .find(|h| h.entity_id == sell.entity_id)
            {
                seller_holder.quantity -= trade_qty;
            }
            if let Some(buyer_holder) = security
                .holders
                .iter_mut()
                .find(|h| h.entity_id == buy.entity_id)
            {
                buyer_holder.quantity += trade_qty;
                buyer_holder.purchase_price = trade_price * trade_qty;
            } else {
                security.holders.push(SecurityHolder {
                    entity_id: buy.entity_id.clone(),
                    holder_type: SecurityHolderType::CommercialBank,
                    quantity: trade_qty,
                    purchase_price: trade_price * trade_qty,
                });
            }
        }

        sm.buy_orders[buy_idx].quantity -= trade_qty;
        sm.sell_orders[sell_idx].quantity -= trade_qty;

        if sm.buy_orders[buy_idx].quantity < 1e-9 {
            buy_idx += 1;
        }
        if sm.sell_orders[sell_idx].quantity < 1e-9 {
            sell_idx += 1;
        }
    }

    // Remove filled orders
    sm.buy_orders.retain(|o| o.quantity >= 1e-9);
    sm.sell_orders.retain(|o| o.quantity >= 1e-9);

    // Calculate market yield from clearing price
    if clearing_price > 0.0 && total_quantity > 0.0 {
        sm.last_yield = (1.0 - clearing_price) / clearing_price;
    }

    debt_market.recalculate();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credit_rating_downgrade() {
        assert_eq!(CreditRating::Aaa.downgrade(3), CreditRating::Aa3);
        assert_eq!(CreditRating::Baa3.downgrade(3), CreditRating::Ba3);
        assert_eq!(CreditRating::C.downgrade(5), CreditRating::C);
    }

    #[test]
    fn test_credit_rating_upgrade() {
        assert_eq!(CreditRating::Caa3.upgrade(3), CreditRating::Ba3);
        assert_eq!(CreditRating::Aaa.upgrade(1), CreditRating::Aaa);
    }

    #[test]
    fn test_debt_market_default_state() {
        let dm = DebtMarket::default();
        assert_eq!(dm.credit_rating, CreditRating::Baa3);
        assert!(!dm.is_locked_out_of_primary);
        assert_eq!(dm.total_arrears, 0.0);
    }

    #[test]
    fn test_issue_treasury_securities_lockout() {
        let mut country = Country::mock_for_tests();
        country.debt_market.is_locked_out_of_primary = true;

        issue_treasury_securities(&mut country, 1000.0, 1);

        assert!(country.debt_market.outstanding_securities.is_empty());
        assert_eq!(country.budget.liquid_reserves, 0.0);
    }

    #[test]
    fn test_clear_arrears() {
        let mut country = Country::mock_for_tests();
        country.budget.liquid_reserves = 500.0;
        country.debt_market.total_arrears = 1000.0;
        country.debt_market.is_locked_out_of_primary = true;

        clear_arrears(&mut country);

        assert_eq!(country.budget.liquid_reserves, 0.0);
        assert!((country.debt_market.total_arrears - 500.0).abs() < 1e-6);
        assert!(country.debt_market.is_locked_out_of_primary); // Still locked, arrears remain

        // Clear remaining arrears
        country.budget.liquid_reserves = 600.0;
        clear_arrears(&mut country);

        assert!((country.debt_market.total_arrears).abs() < 1e-6);
        assert!(!country.debt_market.is_locked_out_of_primary);
        assert!((country.budget.liquid_reserves - 100.0).abs() < 1e-6);
    }
}

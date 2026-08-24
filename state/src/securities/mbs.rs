//! Mortgage-Backed Securities (MBS) module for securitization.
//!
//! This module implements Phase D.5 MBS structures:
//! - MBS tranches (Senior, Mezzanine, Junior)
//! - Loss waterfall mechanics
//! - Yield calculation with servicing spread

use serde::{Deserialize, Serialize};
use serde_json::Map;

/// Tranche priority in loss waterfall.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Copy)]

#[derive(Default)]
pub enum TranchePriority {
    /// Senior tranche - absorbs losses last (appears AAA).

    #[default]
    Senior,
    /// Mezzanine tranche - absorbs losses after Junior.

    Mezzanine,
    /// Junior/Equity tranche - absorbs losses first (highest risk).

    Junior,
}


/// Single tranche of an MBS.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct MbsTranche {
    /// Tranche ID.
    #[serde(default)]
    pub id: String,
    
    /// Priority level for loss absorption.
    #[serde(default)]
    pub priority: TranchePriority,
    
    /// Notional value of this tranche.
    #[serde(default)]
    pub notional: f64,
    
    /// Current outstanding balance (decreases as loans amortize).
    #[serde(default)]
    pub outstanding_balance: f64,
    
    /// Yield paid to tranche holders (weighted avg of loans - servicing spread).
    #[serde(default)]
    pub yield_rate: f64,
    
    /// Current tranche value (mark-to-market based on underlying loan performance).
    #[serde(default)]
    pub market_value: f64,
    
    /// Owner of this tranche (fund_id, bank_id, etc.).
    #[serde(default)]
    pub owner_id: String,
    
    /// Any additional tranche fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

/// Mortgage-Backed Security - Pooled loans securitized into tranches.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct MortgageBackedSecurity {
    /// MBS ID.
    #[serde(default)]
    pub id: String,
    
    /// Originating bank (created the SPV).
    #[serde(default)]
    pub originator_bank_id: String,
    
    /// SPV Company ID holding the underlying loans.
    #[serde(default)]
    pub spv_id: String,
    
    /// Underlying loan IDs (from BankBalanceSheet.loans_issued).
    #[serde(default)]
    pub underlying_loan_ids: Vec<String>,
    
    /// Tranches (Senior, Mezzanine, Junior).
    #[serde(default)]
    pub tranches: Vec<MbsTranche>,
    
    /// Servicing spread paid to originator (e.g., 0.5%).
    #[serde(default)]
    pub servicing_spread: f64,
    
    /// Current weighted average yield of underlying loans.
    #[serde(default)]
    pub weighted_avg_loan_rate: f64,
    
    /// Total notional of all underlying loans.
    #[serde(default)]
    pub total_underlying_notional: f64,
    
    /// Current default rate of underlying loans.
    #[serde(default)]
    pub current_default_rate: f64,
    
    /// Any additional MBS fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

impl MortgageBackedSecurity {
    /// Calculate tranche yield: weighted avg of underlying loans - servicing spread.
    ///
    /// # Arguments
    /// * `priority` - Tranche priority level
    /// * `weighted_avg_loan_rate` - Weighted average rate of underlying loans
    /// * `servicing_spread` - Servicing spread paid to originator
    ///
    /// # Returns
    /// Tranche yield rate
    ///
    /// # Rules
    /// - Senior tranche gets full yield (lowest risk)
    /// - Mezzanine gets yield + risk premium
    /// - Junior gets yield + high risk premium (to compensate for first-loss position)
    pub fn calculate_tranche_yield(
        priority: TranchePriority,
        weighted_avg_loan_rate: f64,
        servicing_spread: f64,
    ) -> f64 {
        let base_yield = weighted_avg_loan_rate - servicing_spread;
        
        match priority {
            TranchePriority::Senior => base_yield, // No premium (appears safe)
            TranchePriority::Mezzanine => base_yield + 0.02, // 2% risk premium
            TranchePriority::Junior => base_yield + 0.05, // 5% risk premium (high yield bait)
        }
    }
    
    /// Distribute losses from underlying loan defaults to tranches.
    /// Loss waterfall: Junior absorbs 100% first, then Mezzanine, then Senior.
    ///
    /// # Arguments
    /// * `total_loss` - Total loss from underlying loan defaults
    ///
    /// # Rules
    /// - Loss waterfall: Junior first, then Mezzanine, then Senior
    /// - Tranche outstanding_balance reduced by absorbed loss
    /// - Market value marked to outstanding_balance
    pub fn distribute_losses(&mut self, total_loss: f64) {
        let mut remaining_loss = total_loss;
        
        // Sort tranches by priority (Junior first, then Mezzanine, then Senior)
        let tranche_order = vec![TranchePriority::Junior, TranchePriority::Mezzanine, TranchePriority::Senior];
        
        for priority in tranche_order {
            if remaining_loss <= 0.0 {
                break;
            }
            
            if let Some(tranche) = self.tranches.iter_mut().find(|t| t.priority == priority) {
                let absorbable = tranche.outstanding_balance.min(remaining_loss);
                tranche.outstanding_balance -= absorbable;
                tranche.market_value = tranche.outstanding_balance; // Mark-to-market
                remaining_loss -= absorbable;
            }
        }
    }
}

/// Securitize eligible loans from a bank's balance sheet into an MBS.
///
/// # Arguments
/// * `bank` - Mutable originating bank company
/// * `mbs_pool` - Mutable vector of all MBS (new MBS appended here)
/// * `exchange` - Mutable stock exchange (Ask orders submitted here)
/// * `config` - Securities market config with tranche fractions and servicing spread
/// * `current_turn` - Current turn number
///
/// # Returns
/// `Some(mbs_id)` if securitization occurred, `None` if no eligible loans
///
/// # Rules
/// * NO MAGIC CASH: loans are moved off balance sheet, not duplicated
/// * Only non-securitized, non-pledged, current-status loans are eligible
/// * Bank's loans_issued securitized flag set to true (loans remain for servicing)
/// * Tranches created at configured fractions (senior/mezzanine/junior)
/// * Originating bank retains Junior tranche (first-loss position)
/// * Senior and Mezzanine tranches submitted as Ask orders on exchange
/// * Servicing spread paid to originating bank for ongoing administration
pub fn securitize_loans(
    bank: &mut crate::entities::Company,
    mbs_pool: &mut Vec<MortgageBackedSecurity>,
    exchange: &mut crate::securities::exchange::StockExchange,
    config: &crate::securities::config::SecuritiesMarketConfig,
    current_turn: u32,
) -> Option<String> {
    let balance_sheet = bank.balance_sheet.as_mut()?;

    // Collect eligible loan IDs and total notional
    let eligible: Vec<(String, f64, f64)> = balance_sheet.loans_issued.iter()
        .filter(|l| !l.securitized && l.pledged_to_covered_bond.is_none() && l.outstanding_balance > 0.0)
        .map(|l| (l.id.clone(), l.outstanding_balance, l.interest_rate))
        .collect();

    if eligible.is_empty() {
        return None;
    }

    let total_notional: f64 = eligible.iter().map(|(_, bal, _)| bal).sum();
    if total_notional <= 0.0 {
        return None;
    }

    let weighted_avg_rate: f64 = {
        let total_rate_weighted: f64 = eligible.iter().map(|(_, bal, rate)| bal * rate).sum();
        if total_notional > 0.0 { total_rate_weighted / total_notional } else { 0.0 }
    };

    let mbs_id = format!("MBS-{}-{}", bank.id, current_turn);

    // Mark loans as securitized
    for (loan_id, _, _) in &eligible {
        if let Some(loan) = balance_sheet.loans_issued.iter_mut().find(|l| &l.id == loan_id) {
            loan.securitized = true;
        }
    }

    // Create tranches
    let senior_notional = total_notional * config.mbs_senior_fraction;
    let mezzanine_notional = total_notional * config.mbs_mezzanine_fraction;
    let junior_notional = total_notional * config.mbs_junior_fraction;

    let senior_yield = MortgageBackedSecurity::calculate_tranche_yield(
        TranchePriority::Senior, weighted_avg_rate, config.mbs_servicing_spread,
    );
    let mezzanine_yield = MortgageBackedSecurity::calculate_tranche_yield(
        TranchePriority::Mezzanine, weighted_avg_rate, config.mbs_servicing_spread,
    );
    let junior_yield = MortgageBackedSecurity::calculate_tranche_yield(
        TranchePriority::Junior, weighted_avg_rate, config.mbs_servicing_spread,
    );

    let tranches = vec![
        MbsTranche {
            id: format!("{}-senior", mbs_id),
            priority: TranchePriority::Senior,
            notional: senior_notional,
            outstanding_balance: senior_notional,
            yield_rate: senior_yield,
            market_value: senior_notional,
            owner_id: String::new(),
            extra: Default::default(),
        },
        MbsTranche {
            id: format!("{}-mezzanine", mbs_id),
            priority: TranchePriority::Mezzanine,
            notional: mezzanine_notional,
            outstanding_balance: mezzanine_notional,
            yield_rate: mezzanine_yield,
            market_value: mezzanine_notional,
            owner_id: String::new(),
            extra: Default::default(),
        },
        MbsTranche {
            id: format!("{}-junior", mbs_id),
            priority: TranchePriority::Junior,
            notional: junior_notional,
            outstanding_balance: junior_notional,
            yield_rate: junior_yield,
            market_value: junior_notional,
            owner_id: bank.id.clone(),
            extra: Default::default(),
        },
    ];

    let mbs = MortgageBackedSecurity {
        id: mbs_id.clone(),
        originator_bank_id: bank.id.clone(),
        spv_id: format!("SPV-{}", mbs_id),
        underlying_loan_ids: eligible.iter().map(|(id, _, _)| id.clone()).collect(),
        tranches,
        servicing_spread: config.mbs_servicing_spread,
        weighted_avg_loan_rate: weighted_avg_rate,
        total_underlying_notional: total_notional,
        current_default_rate: 0.0,
        extra: Default::default(),
    };

    // Submit Ask orders for Senior and Mezzanine tranches on exchange
    for tranche in &mbs.tranches {
        if tranche.priority == TranchePriority::Junior {
            continue;
        }
        let instrument_id = format!("MBS:{}:{:?}", mbs.id, tranche.priority).to_lowercase();
        let ask_order = crate::securities::exchange::Order::new_sell(
            format!("MBS-ASK-{}", tranche.id),
            bank.id.clone(),
            instrument_id.clone(),
            crate::securities::exchange::InstrumentType::MbsTranche {
                mbs_id: mbs.id.clone(),
                priority: tranche.priority,
            },
            1,
            tranche.outstanding_balance,
            current_turn + 10,
        );
        let book = exchange.order_book.entry(instrument_id).or_default();
        if let Some(pos) = book.asks.iter().position(|(p, _)| *p == tranche.outstanding_balance) {
            book.asks[pos].1.push(ask_order);
        } else {
            book.asks.push((tranche.outstanding_balance, vec![ask_order]));
            book.asks.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        }
        book.best_ask = book.asks.first().map(|(p, _)| *p).unwrap_or(0.0);
    }

    mbs_pool.push(mbs);
    Some(mbs_id)
}

/// Process MBS turn: pay coupons to tranche holders from originating bank.
///
/// # Arguments
/// * `mbs_pool` - Mutable slice of all MBS
/// * `companies` - Mutable slice of all companies (for bank debit and owner credit)
/// * `current_turn` - Current turn number
///
/// # Rules
/// * Coupon = outstanding_balance * yield_rate (per turn)
/// * Originating bank is DEBITED (reserves_at_central_bank -= coupon)
/// * Tranche owners are CREDITED (brokerage cash += coupon)
/// * Coupon payments do NOT reduce tranche outstanding_balance (principal stays)
/// * outstanding_balance only decreases via amortization or default losses
/// * If bank cannot pay full coupon, pays what it can (partial default)
pub fn process_mbs_turn(
    mbs_pool: &mut [MortgageBackedSecurity],
    companies: &mut [crate::entities::Company],
    _current_turn: u32,
) {
    for mbs in mbs_pool.iter_mut() {
        for tranche in &mut mbs.tranches {
            if tranche.outstanding_balance <= 0.0 || tranche.owner_id.is_empty() {
                continue;
            }

            let coupon = tranche.outstanding_balance * tranche.yield_rate;
            if coupon <= 0.0 {
                continue;
            }

            // Debit originating bank
            let mut actual_coupon = coupon;
            let bank_id = mbs.originator_bank_id.clone();
            if let Some(bank) = companies.iter_mut().find(|c| c.id == bank_id) {
                if let Some(ref mut bs) = bank.balance_sheet {
                    let available = bs.reserves_at_central_bank;
                    actual_coupon = coupon.min(available);
                    bs.reserves_at_central_bank -= actual_coupon;
                }
            }

            // Credit tranche owner
            if actual_coupon > 0.0 {
                let owner_id = tranche.owner_id.clone();
                if let Some(owner) = companies.iter_mut().find(|c| c.id == owner_id) {
                    if let Some(ref mut acct) = owner.brokerage_account {
                        acct.cash += actual_coupon;
                    }
                }
            }
        }

        // Process amortization: reduce outstanding_balance proportionally
        let amortization = mbs.total_underlying_notional * mbs.weighted_avg_loan_rate * 0.1;
        if amortization > 0.0 {
            let mut remaining_amort = amortization;
            let tranche_order = vec![TranchePriority::Senior, TranchePriority::Mezzanine, TranchePriority::Junior];
            for priority in tranche_order {
                if remaining_amort <= 0.0 {
                    break;
                }
                if let Some(tranche) = mbs.tranches.iter_mut().find(|t| t.priority == priority) {
                    let reducible = tranche.outstanding_balance.min(remaining_amort);
                    tranche.outstanding_balance -= reducible;
                    tranche.market_value = tranche.outstanding_balance;
                    remaining_amort -= reducible;
                }
            }
        }
    }
}

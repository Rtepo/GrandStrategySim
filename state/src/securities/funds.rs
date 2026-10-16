//! Institutional investor funds module.
//!
//! This module implements FundType enum and FundLedger struct for
//! institutional investors like FIO, FIZ, hedge funds, and ETFs.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::entities::Company;
use crate::registries::enums::Sector;
use crate::securities::config::SecuritiesMarketConfig;
use crate::securities::covered_bonds::CoveredBond;
use crate::securities::exchange::{InstrumentType, Order, StockExchange};
use crate::securities::mbs::MortgageBackedSecurity;
use crate::society::geography::Region;

/// Type of institutional fund.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]

pub enum FundType {
    /// FIO - Open-End Investment Fund (Fundusz Inwestycyjny Otwarty).
    OpenEndInvestmentFund,

    /// FIZ - Closed-End Investment Fund (Closed-End Investment Fund).
    ClosedEndInvestmentFund,

    /// Hedge Fund (high-risk, high-leverage strategies).
    HedgeFund,

    /// ETF - Exchange Traded Fund.
    ExchangeTradedFund,

    /// Mutual Fund (traditional diversified portfolio).
    MutualFund,
}

impl FundType {
    /// R6.4: Returns true if this is a passive/index-tracking fund.
    /// ETFs are passive — they track an index and charge lower fees.
    pub fn is_passive(&self) -> bool {
        matches!(self, FundType::ExchangeTradedFund)
    }

    /// R6.4: Returns true if this is an actively-managed fund.
    /// FIO/FIZ, hedge funds, and mutual funds use active stock picking.
    pub fn is_active(&self) -> bool {
        !self.is_passive()
    }

    /// R6.4: Returns the default management fee rate for this fund type.
    /// Passive funds charge 0.5%, active funds charge 2.0%.
    pub fn default_management_fee_rate(&self) -> f64 {
        if self.is_passive() {
            0.005 // 0.5% for ETFs
        } else {
            0.02 // 2.0% for active funds
        }
    }
}

/// Detailed ledger for fund operations and holdings.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct FundLedger {
    /// Net Asset Value per share.
    pub nav_per_share: f64,

    /// Total shares outstanding.
    pub shares_outstanding: u64,

    /// Management fee (percentage of AUM).
    pub management_fee: f64,

    /// Performance fee (percentage of profits above benchmark).
    pub performance_fee: f64,

    /// Leverage ratio (for hedge funds).
    pub leverage_ratio: f64,

    /// Investment mandate restrictions.
    pub investment_mandate: InvestmentMandate,

    /// Liquidity provision to AMM pools.
    pub liquidity_provision: BTreeMap<String, f64>,

    /// Resurrection Phase 2: Unit holder registry — maps contributor_id to units held.
    #[serde(default)]
    pub unit_holders: BTreeMap<String, u64>,

    /// Phase 36: Sovereign/treasury bond holdings with strict double-entry tracking.
    /// Each holding records the security ID, face value, purchase price, coupon rate,
    /// maturity, and ownership. This prevents the "magic asset increase" bug where
    /// fund NAV rose without debiting cash or crediting a counterparty.
    #[serde(default)]
    pub bond_holdings: Vec<FundBondHolding>,

    /// Phase 57: Fund manager VIP ID (for trait-driven behavior).
    #[serde(default)]
    pub fund_manager_vip_id: Option<String>,

    /// Equity-market fix: instrument_id (`EQUITY:<company_id>`) → shares held,
    /// synced from the fund's brokerage portfolio each turn by
    /// `charge_fund_fees`. This is the test/UI-readable count of fund equity
    /// positions (`funds_equity_holdings_count > 0` checks `len()` here) —
    /// the brokerage `portfolio` map already tracked lots, but nothing rolled
    /// them up into a plain per-instrument share count.
    #[serde(default)]
    pub equity_holdings: BTreeMap<String, u64>,
}

/// Phase 36: A fund's holding of a sovereign/treasury bond.
///
/// Records the full economic terms of the bond purchase so that coupon
/// payments, maturity redemptions, and NAV calculations can be performed
/// with strict double-entry accuracy.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct FundBondHolding {
    /// The treasury security ID (matches `TreasurySecurity.id`).
    #[serde(default)]
    pub security_id: String,
    /// Face value purchased.
    #[serde(default)]
    pub face_value: f64,
    /// Price paid (cash debited from fund).
    #[serde(default)]
    pub purchase_price: f64,
    /// Coupon rate (e.g., 0.04 for 4%).
    #[serde(default)]
    pub coupon_rate: f64,
    /// Maturity turn (when principal is repaid).
    #[serde(default)]
    pub maturity_turn: u32,
    /// Last turn a coupon was paid.
    #[serde(default)]
    pub last_coupon_turn: u32,
    /// Whether the holding has matured and been redeemed.
    #[serde(default)]
    pub redeemed: bool,
}

/// Investment mandate restrictions for institutional funds.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]

pub struct InvestmentMandate {
    /// Maximum position size in single company (percentage).
    pub max_position_size: f64,

    /// Allowed sectors.
    pub allowed_sectors: Vec<Sector>,

    /// Minimum liquidity requirement.
    pub min_liquidity: f64,
}

/// Result of a fund capital collection for one contributor.
#[derive(Debug, Clone)]
pub struct FundSubscription {
    /// Fund company ID.
    pub fund_id: String,
    /// Contributor ID (region/class key or company_id).
    pub contributor_id: String,
    /// Cash amount collected.
    pub amount: f64,
    /// Fund units issued to contributor.
    pub units_issued: u64,
}

/// Collect capital from contributors into funds, issuing fund units (NOT theft).
///
/// # Arguments
/// * `funds` - Mutable slice of companies that are funds
/// * `regions` - Mutable slice of regions (for ClassDemographics.savings)
/// * `companies` - Mutable slice of all companies (for hedge fund contributors)
/// * `config` - Securities market config with subscription rates
/// * `current_turn` - Current turn number
///
/// # Returns
/// Vector of `FundSubscription` records for audit trail
///
/// # Rules
/// * FIO/ETF: collect from ClassDemographics.savings across all regions
/// * FIZ: collect from aristocracy/wealthy classes at 2x rate
/// * Hedge Fund: collect from wealthy companies at 0.5x rate
/// * NAV = Total Fund Value / shares_outstanding (AUM = cash + portfolio at market)
/// * First subscription: NAV = 1.0 (par value)
/// * Double-entry: savings -= amount, fund.cash += amount, shares += units, unit_holders += units
pub fn collect_fund_capital(
    funds: &mut [Company],
    regions: &mut [Region],
    companies: &mut [Company],
    config: &SecuritiesMarketConfig,
    _current_turn: u32,
    avg_wage: f64,
) -> Vec<FundSubscription> {
    let mut subscriptions = Vec::new();
    // Phase 94: Track total collected per fund for bank-reserve sync (M0).
    // Citizen savings ↓ (M0) must be offset by bank reserves ↑ (M0).
    let mut bank_syncs: Vec<(String, f64)> = Vec::new();

    // Equity-market fix: the equity universe AND price map come from the
    // union of both slices. The turn wiring currently puts every company
    // into `funds` and leaves `companies` empty (split_at_mut(len)), so any
    // NAV valuation or candidate scan over `companies` alone sees nothing.
    let equity_prices = equity_price_map(funds, companies);

    for fund in funds.iter_mut() {
        let ft = match &fund.fund_type {
            Some(ft) => ft.clone(),
            None => continue,
        };
        let ledger = match &mut fund.fund_ledger {
            Some(l) => l,
            None => continue,
        };

        // Phase 94: Track total collected for bank-reserve sync (M0).
        let mut total_collected: f64 = 0.0;
        // Calculate current NAV using AUM (cash + portfolio at market prices)
        let fund_cash = fund
            .brokerage_account
            .as_ref()
            .map(|b| b.cash)
            .unwrap_or(0.0);
        // Clone bond holdings to avoid borrow conflict with `ledger`
        let bond_holdings_value: f64 = ledger
            .bond_holdings
            .iter()
            .filter(|h| !h.redeemed)
            .map(|h| h.face_value)
            .sum();
        let portfolio_value =
            calculate_portfolio_value(&fund.brokerage_account, &equity_prices, &None)
                + bond_holdings_value;
        // Frozen cash is still the fund's money (encumbered by open bids) —
        // excluding it understates AUM and drives NAV artificially low.
        let frozen_cash = fund
            .brokerage_account
            .as_ref()
            .map(|b| b.frozen_cash)
            .unwrap_or(0.0);
        let total_fund_value = fund_cash + frozen_cash + portfolio_value;

        let nav_per_share = if ledger.shares_outstanding > 0 {
            total_fund_value / ledger.shares_outstanding as f64
        } else {
            1.0 // Initial par value for first subscription
        };

        let subscription_rate = match ft {
            FundType::OpenEndInvestmentFund | FundType::MutualFund => config.fund_subscription_rate,
            FundType::ClosedEndInvestmentFund => config.fund_subscription_rate * 2.0,
            FundType::HedgeFund => config.fund_subscription_rate * 0.5,
            FundType::ExchangeTradedFund => config.fund_subscription_rate * 0.8,
        };

        match ft {
            FundType::OpenEndInvestmentFund
            | FundType::MutualFund
            | FundType::ExchangeTradedFund
            | FundType::ClosedEndInvestmentFund => {
                // Collect from ClassDemographics.savings across all regions
                for region in regions.iter_mut() {
                    // Iterate rural classes
                    for (class_key, class) in &mut region.class_demographics.rural_classes {
                        let contributor_id = format!("{}:rural:{}", region.id, class_key);
                        let subscription_amount = class.savings * subscription_rate;
                        if subscription_amount <= 0.0 {
                            continue;
                        }

                        // FIZ only targets wealthy classes (savings_per_capita > 500)
                        if ft == FundType::ClosedEndInvestmentFund
                            && class.savings_per_capita < 500.0
                        {
                            continue;
                        }

                        // Double-entry (cash side): savings -= amount, fund.cash += amount
                        class.savings -= subscription_amount;
                        if let Some(ref mut acct) = fund.brokerage_account {
                            acct.cash += subscription_amount;
                        }
                        total_collected += subscription_amount;

                        // Double-entry (equity side): issue units.
                        // Guard the f64→u64 cast: `as` saturates to u64::MAX
                        // on huge quotients (a collapsed NAV divides cash by
                        // a near-zero number), which is exactly the
                        // "1.8e19 shares outstanding" defect. Clamp to a
                        // finite, non-negative value first.
                        let units_issued = if nav_per_share.is_finite() && nav_per_share > 0.0 {
                            let raw = subscription_amount / nav_per_share;
                            if raw.is_finite() && raw > 0.0 {
                                raw.min(u64::MAX as f64) as u64
                            } else {
                                0
                            }
                        } else {
                            0
                        };
                        ledger.shares_outstanding =
                            ledger.shares_outstanding.saturating_add(units_issued);
                        {
                            let e = ledger
                                .unit_holders
                                .entry(contributor_id.clone())
                                .or_insert(0);
                            *e = e.saturating_add(units_issued);
                        }

                        subscriptions.push(FundSubscription {
                            fund_id: fund.id.clone(),
                            contributor_id,
                            amount: subscription_amount,
                            units_issued,
                        });
                    }
                    // Iterate urban classes
                    for (class_key, class) in &mut region.class_demographics.urban_classes {
                        let contributor_id = format!("{}:urban:{}", region.id, class_key);
                        let subscription_amount = class.savings * subscription_rate;
                        if subscription_amount <= 0.0 {
                            continue;
                        }

                        // FIZ only targets wealthy classes (savings_per_capita > 500)
                        if ft == FundType::ClosedEndInvestmentFund
                            && class.savings_per_capita < 500.0
                        {
                            continue;
                        }

                        // Double-entry (cash side): savings -= amount, fund.cash += amount
                        class.savings -= subscription_amount;
                        if let Some(ref mut acct) = fund.brokerage_account {
                            acct.cash += subscription_amount;
                        }
                        total_collected += subscription_amount;

                        // Double-entry (equity side): issue units.
                        // Guard the f64→u64 cast: `as` saturates to u64::MAX
                        // on huge quotients (a collapsed NAV divides cash by
                        // a near-zero number), which is exactly the
                        // "1.8e19 shares outstanding" defect. Clamp to a
                        // finite, non-negative value first.
                        let units_issued = if nav_per_share.is_finite() && nav_per_share > 0.0 {
                            let raw = subscription_amount / nav_per_share;
                            if raw.is_finite() && raw > 0.0 {
                                raw.min(u64::MAX as f64) as u64
                            } else {
                                0
                            }
                        } else {
                            0
                        };
                        ledger.shares_outstanding =
                            ledger.shares_outstanding.saturating_add(units_issued);
                        {
                            let e = ledger
                                .unit_holders
                                .entry(contributor_id.clone())
                                .or_insert(0);
                            *e = e.saturating_add(units_issued);
                        }

                        subscriptions.push(FundSubscription {
                            fund_id: fund.id.clone(),
                            contributor_id,
                            amount: subscription_amount,
                            units_issued,
                        });
                    }
                }
            }
            FundType::HedgeFund => {
                // Collect from wealthy companies (high computed_liquid_capital)
                for company in companies.iter_mut() {
                    if company.id == fund.id {
                        continue;
                    }
                    let liquid = company.computed_liquid_capital();
                    if liquid <= 0.0 {
                        continue;
                    }
                    let subscription_amount = liquid * subscription_rate;
                    if subscription_amount <= 0.0 {
                        continue;
                    }

                    // Double-entry (cash side): company.cash -= amount, fund.cash += amount
                    if let Some(ref mut company_acct) = company.brokerage_account {
                        if company_acct.cash < subscription_amount {
                            continue;
                        }
                        company_acct.cash -= subscription_amount;
                    } else {
                        continue;
                    }
                    if let Some(ref mut fund_acct) = fund.brokerage_account {
                        fund_acct.cash += subscription_amount;
                    }
                    total_collected += subscription_amount;

                    // Double-entry (equity side): issue units.
                    // Same f64→u64 guard as the retail classes above —
                    // a near-zero NAV otherwise saturates to u64::MAX.
                    let units_issued = if nav_per_share.is_finite() && nav_per_share > 0.0 {
                        let raw = subscription_amount / nav_per_share;
                        if raw.is_finite() && raw > 0.0 {
                            raw.min(u64::MAX as f64) as u64
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    ledger.shares_outstanding =
                        ledger.shares_outstanding.saturating_add(units_issued);
                    {
                        let e = ledger
                            .unit_holders
                            .entry(company.id.clone())
                            .or_insert(0);
                        *e = e.saturating_add(units_issued);
                    }

                    subscriptions.push(FundSubscription {
                        fund_id: fund.id.clone(),
                        contributor_id: company.id.clone(),
                        amount: subscription_amount,
                        units_issued,
                    });
                }
            }
        }

        // Update NAV after all collections
        let fund_cash = fund
            .brokerage_account
            .as_ref()
            .map(|b| b.cash)
            .unwrap_or(0.0);
        let bond_holdings_value: f64 = ledger
            .bond_holdings
            .iter()
            .filter(|h| !h.redeemed)
            .map(|h| h.face_value)
            .sum();
        let portfolio_value =
            calculate_portfolio_value(&fund.brokerage_account, &equity_prices, &None)
                + bond_holdings_value;
        let frozen_cash = fund
            .brokerage_account
            .as_ref()
            .map(|b| b.frozen_cash)
            .unwrap_or(0.0);
        let total_fund_value = fund_cash + frozen_cash + portfolio_value;
        if ledger.shares_outstanding > 0 {
            let nav = total_fund_value / ledger.shares_outstanding as f64;
            if nav.is_finite() {
                ledger.nav_per_share = nav;
            }
        }
        // Phase 94: Record total collected for bank-reserve sync (M0).
        if total_collected > 0.0 {
            bank_syncs.push((fund.id.clone(), total_collected));
        }
    }

    // Equity-market fix: hedge-fund contributors can also live inside the
    // `funds` slice itself — the turn wiring passes ALL companies as `funds`
    // and leaves `companies` empty (split_at_mut(len)), which previously
    // starved hedge funds of contributors entirely. Pair indices inside the
    // same slice; `companies` (when non-empty under a corrected wiring) is
    // still scanned by the in-loop branch above, and the two slices are
    // disjoint by construction so no contributor is charged twice.
    let hedge_idx: Vec<usize> = funds
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            matches!(f.fund_type, Some(FundType::HedgeFund)) && f.fund_ledger.is_some()
        })
        .map(|(i, _)| i)
        .collect();
    for fi in hedge_idx {
        // NAV snapshot on the pre-collection state — same formula as the
        // in-loop branch (par value 1.0 for the first subscription).
        let nav_per_share = {
            let fund = &funds[fi];
            let ledger = fund.fund_ledger.as_ref().unwrap();
            let fund_cash = fund
                .brokerage_account
                .as_ref()
                .map(|b| b.cash + b.frozen_cash)
                .unwrap_or(0.0);
            let bond_value: f64 = ledger
                .bond_holdings
                .iter()
                .filter(|h| !h.redeemed)
                .map(|h| h.face_value)
                .sum();
            let pv = calculate_portfolio_value(&fund.brokerage_account, &equity_prices, &None);
            let total = fund_cash + bond_value + pv;
            if ledger.shares_outstanding > 0 {
                total / ledger.shares_outstanding as f64
            } else {
                1.0
            }
        };
        let subscription_rate = config.fund_subscription_rate * 0.5;
        let mut collected = 0.0_f64;
        for ci in 0..funds.len() {
            if ci == fi {
                continue;
            }
            // Never drain another fund's AUM as a "contributor".
            if funds[ci].fund_type.is_some() {
                continue;
            }
            let (fund, contributor) = pair_mut(funds, fi, ci);
            let liquid = contributor.computed_liquid_capital();
            if liquid <= 0.0 {
                continue;
            }
            let subscription_amount = liquid * subscription_rate;
            if subscription_amount <= 0.0 {
                continue;
            }
            let contributor_acct = match contributor.brokerage_account.as_mut() {
                Some(a) => a,
                None => continue,
            };
            if contributor_acct.cash < subscription_amount {
                continue;
            }
            // Double-entry: contributor cash → fund cash.
            contributor_acct.cash -= subscription_amount;
            if let Some(ref mut fund_acct) = fund.brokerage_account {
                fund_acct.cash += subscription_amount;
            }
            collected += subscription_amount;
            let units_issued = if nav_per_share.is_finite() && nav_per_share > 0.0 {
                let raw = subscription_amount / nav_per_share;
                if raw.is_finite() && raw > 0.0 {
                    raw.min(u64::MAX as f64) as u64
                } else {
                    0
                }
            } else {
                0
            };
            if let Some(ref mut ledger) = fund.fund_ledger {
                ledger.shares_outstanding =
                    ledger.shares_outstanding.saturating_add(units_issued);
                let e = ledger
                    .unit_holders
                    .entry(contributor.id.clone())
                    .or_insert(0);
                *e = e.saturating_add(units_issued);
            }
            subscriptions.push(FundSubscription {
                fund_id: fund.id.clone(),
                contributor_id: contributor.id.clone(),
                amount: subscription_amount,
                units_issued,
            });
        }
        // Restrike NAV for this hedge fund after its collections.
        if collected > 0.0 {
            let fund = &mut funds[fi];
            let ledger = fund.fund_ledger.as_mut().unwrap();
            let fund_cash = fund
                .brokerage_account
                .as_ref()
                .map(|b| b.cash + b.frozen_cash)
                .unwrap_or(0.0);
            let bond_value: f64 = ledger
                .bond_holdings
                .iter()
                .filter(|h| !h.redeemed)
                .map(|h| h.face_value)
                .sum();
            let pv = calculate_portfolio_value(&fund.brokerage_account, &equity_prices, &None);
            let total = fund_cash + bond_value + pv;
            if ledger.shares_outstanding > 0 {
                let nav = total / ledger.shares_outstanding as f64;
                if nav.is_finite() {
                    ledger.nav_per_share = nav;
                }
            }
            bank_syncs.push((fund.id.clone(), collected));
        }
    }

    // Phase 94: Second pass — sync bank reserves for fund credits (M0).
    // Citizen savings ↓ (M0) is offset by bank reserves ↑ (M0).
    for (fund_id, amount) in &bank_syncs {
        crate::economy::transfer_settler::sync_bank_credit_by_company_id(funds, fund_id, *amount);
    }

    // R8.5: Enroll eligible citizens in tax-advantaged retirement accounts.
    // Eligible: working-age population (proxy: classes with allocated_fte > 0).
    // TaxFreeGrowth: contribution limit = avg_wage * 120
    // TaxDeferred: contribution limit = avg_wage * 60
    // Contributions are tracked per-class for PIT deduction (R8.6).
    if avg_wage > 0.0 {
        let tax_free_growth_limit = avg_wage * 120.0;
        let tax_deferred_limit = avg_wage * 60.0;
        for region in regions.iter_mut() {
            for class in region.class_demographics.rural_classes.values_mut() {
                // Only working-age classes with income participate
                if class.allocated_fte > 0.0 && class.savings > 0.0 {
                    // Split: 60% to TaxFreeGrowth, 40% to TaxDeferred
                    let eligible_amount = class.savings.min(tax_free_growth_limit + tax_deferred_limit);
                    let tax_deferred_contribution = eligible_amount * 0.4;
                    // Track TaxDeferred contributions for PIT deduction (R8.6)
                    class.tax_advantaged_contributions_this_year += tax_deferred_contribution;
                }
            }
            for class in region.class_demographics.urban_classes.values_mut() {
                if class.allocated_fte > 0.0 && class.savings > 0.0 {
                    let eligible_amount = class.savings.min(tax_free_growth_limit + tax_deferred_limit);
                    let tax_deferred_contribution = eligible_amount * 0.4;
                    class.tax_advantaged_contributions_this_year += tax_deferred_contribution;
                }
            }
        }
    }

    subscriptions
}

/// Build a `company_id → share_price` map from every company slice in
/// scope. The turn wiring currently passes ALL companies as the `funds`
/// slice (the `companies` argument arrives empty — see `split_at_mut(len)`
/// in turn.rs), so the equity universe and NAV pricing MUST come from the
/// union of both slices or the fund never sees a listed stock.
fn equity_price_map(
    funds: &[Company],
    companies: &[Company],
) -> std::collections::HashMap<String, f64> {
    funds
        .iter()
        .chain(companies.iter())
        .map(|c| (c.id.clone(), c.share_price))
        .collect()
}

/// Disjoint mutable access to two elements of the same slice
/// (`(&mut s[i], &mut s[j])`), used when hedge-fund contributors live in
/// the same slice as the fund itself under the current turn wiring.
fn pair_mut<T>(slice: &mut [T], i: usize, j: usize) -> (&mut T, &mut T) {
    debug_assert!(i != j);
    if i < j {
        let (l, r) = slice.split_at_mut(j);
        (&mut l[i], &mut r[0])
    } else {
        let (l, r) = slice.split_at_mut(i);
        (&mut r[0], &mut l[j])
    }
}

/// Calculate portfolio value at current market prices.
///
/// # Arguments
/// * `brokerage_account` - Optional reference to brokerage account
/// * `equity_prices` - `company_id → share_price` map (see `equity_price_map`)
/// * `fund_ledger` - Optional fund ledger (for bond holdings, Phase 36)
///
/// # Returns
/// Total market value of portfolio holdings
fn calculate_portfolio_value(
    brokerage_account: &Option<crate::securities::BrokerageAccount>,
    equity_prices: &std::collections::HashMap<String, f64>,
    fund_ledger: &Option<FundLedger>,
) -> f64 {
    let acct = match brokerage_account {
        Some(a) => a,
        None => return 0.0,
    };

    let mut total = 0.0;
    for (instrument_id, lots) in &acct.portfolio {
        let qty: u64 = lots.iter().map(|l| l.quantity).sum();
        if qty == 0 {
            continue;
        }
        if let Some(company_id) = instrument_id.strip_prefix("EQUITY:") {
            if let Some(price) = equity_prices.get(company_id) {
                total += price * qty as f64;
            }
        }
        // MBS and bond portfolio values would use their respective market prices
        // For now, use face value (outstanding_balance) as approximation
    }

    // Phase 36: Include sovereign bond holdings at face value
    if let Some(ledger) = fund_ledger {
        for holding in &ledger.bond_holdings {
            if !holding.redeemed {
                total += holding.face_value;
            }
        }
    }

    total
}

/// Phase 36: Purchase a treasury security on the primary market with strict
/// double-entry accounting.
///
/// # Flow
/// 1. Check fund has sufficient cash (brokerage_account.cash >= purchase_price).
/// 2. Debit fund cash: `brokerage_account.cash -= purchase_price`.
/// 3. Credit Treasury: `country.budget.liquid_reserves += purchase_price`.
/// 4. Add a `SecurityHolder` entry to the treasury security.
/// 5. Record a `FundBondHolding` in the fund ledger.
///
/// # Arguments
/// * `fund` - Mutable fund company making the purchase.
/// * `country` - Mutable country (for treasury credit and debt market access).
/// * `security_id` - ID of the treasury security to buy.
/// * `face_value` - Face value to purchase.
/// * `purchase_price` - Cash price to pay.
/// * `current_turn` - Current turn (for logging).
///
/// # Returns
/// `true` if the purchase succeeded, `false` if rejected (insufficient cash,
/// security not found, or fund has no ledger/brokerage account).
pub fn fund_purchase_treasury_bond(
    fund: &mut Company,
    country: &mut crate::state::Country,
    security_id: &str,
    face_value: f64,
    purchase_price: f64,
    current_turn: u32,
) -> bool {
    // Validate fund has a ledger and brokerage account
    let ledger = match fund.fund_ledger.as_mut() {
        Some(l) => l,
        None => return false,
    };
    let brokerage = match fund.brokerage_account.as_mut() {
        Some(b) => b,
        None => return false,
    };

    // Strict double-entry check: reject if insufficient liquidity
    if brokerage.cash < purchase_price || purchase_price <= 0.0 || face_value <= 0.0 {
        return false;
    }

    // Find the treasury security in the debt market
    let security = match country
        .debt_market
        .outstanding_securities
        .iter_mut()
        .find(|s| s.id == security_id)
    {
        Some(s) => s,
        None => return false,
    };

    // Debit fund cash
    brokerage.cash -= purchase_price;

    // Credit Treasury liquid reserves
    country.budget.liquid_reserves += purchase_price;

    // Add fund as a holder of this security
    use crate::economy::debt_market::{SecurityHolder, SecurityHolderType};
    security.holders.push(SecurityHolder {
        entity_id: fund.id.clone(),
        holder_type: SecurityHolderType::RetailSavingsBond,
        quantity: face_value,
        purchase_price,
    });

    // Record the holding in the fund ledger
    ledger.bond_holdings.push(FundBondHolding {
        security_id: security_id.to_string(),
        face_value,
        purchase_price,
        coupon_rate: security.coupon_rate,
        maturity_turn: security.issue_turn + security.maturity_turns,
        last_coupon_turn: current_turn,
        redeemed: false,
    });

    true
}

/// Phase 36: Process coupon payments for all fund bond holdings.
///
/// For each holding, accrue coupon interest since the last coupon turn and
/// credit the fund's brokerage cash account. The Treasury is debited.
///
/// # Arguments
/// * `fund` - Mutable fund company.
/// * `country` - Mutable country (for treasury debit).
/// * `current_turn` - Current turn.
///
/// # Returns
/// Total coupon interest paid this turn.
pub fn process_fund_coupon_payments(
    fund: &mut Company,
    country: &mut crate::state::Country,
    current_turn: u32,
) -> f64 {
    let ledger = match fund.fund_ledger.as_mut() {
        Some(l) => l,
        None => return 0.0,
    };
    let brokerage = match fund.brokerage_account.as_mut() {
        Some(b) => b,
        None => return 0.0,
    };

    let mut total_coupons = 0.0;
    for holding in ledger.bond_holdings.iter_mut() {
        if holding.redeemed || holding.coupon_rate <= 0.0 {
            continue;
        }
        // Annual coupon, prorated by turns elapsed (24 turns = 1 year)
        let turns_elapsed = current_turn.saturating_sub(holding.last_coupon_turn);
        if turns_elapsed == 0 {
            continue;
        }
        let annual_coupon = holding.face_value * holding.coupon_rate;
        let coupon_payment = annual_coupon * (turns_elapsed as f64 / 24.0);
        if coupon_payment <= 0.0 {
            continue;
        }

        // Double-entry: debit Treasury, credit fund cash
        country.budget.liquid_reserves -= coupon_payment;
        brokerage.cash += coupon_payment;
        holding.last_coupon_turn = current_turn;
        total_coupons += coupon_payment;
    }

    total_coupons
}

/// Snapshot of a listed company's tradeable-equity data, captured once per
/// turn (before the mutable fund loop) so every fund prices the same world.
#[derive(Debug, Clone)]
struct EquityCandidate {
    id: String,
    instrument_id: String,
    sector: Sector,
    share_price: f64,
    market_cap: f64,
    eps: f64,
    dividend_yield: f64,
    /// P/E when eps > 0, else the P/B fallback used by the original screen.
    pe_or_pb: f64,
    /// Book value per share (`company_capital / shares_count`).
    book_ps: f64,
}

impl EquityCandidate {
    fn from_company(c: &Company) -> Self {
        let book_ps = if c.shares_count > 0 {
            c.company_capital / c.shares_count as f64
        } else {
            0.0
        };
        let pe_or_pb = if c.eps > 0.0 && c.share_price > 0.0 && c.pe_ratio > 0.0 {
            c.pe_ratio
        } else if book_ps > 0.0 {
            c.share_price / book_ps
        } else {
            f64::INFINITY
        };
        EquityCandidate {
            id: c.id.clone(),
            instrument_id: format!("EQUITY:{}", c.id),
            sector: c.sector,
            share_price: c.share_price,
            market_cap: c.share_price * c.shares_count as f64,
            eps: c.eps,
            dividend_yield: c.dividend_yield,
            pe_or_pb,
            book_ps,
        }
    }
}

/// Submit fund orders to the exchange based on deterministic valuation scores.
///
/// # Arguments
/// * `funds` - Mutable slice of fund companies
/// * `exchange` - Mutable stock exchange to submit orders to
/// * `companies` - Slice of all companies (for valuation)
/// * `mbs_pool` - Slice of MBS structures
/// * `covered_bonds` - Slice of covered bonds
/// * `config` - Securities market config with valuation thresholds
/// * `current_turn` - Current turn number
///
/// # Rules
/// * Compute Valuation Score for each equity: P/E ratio, dividend yield
/// * Each fund targets `config.fund_equity_target_share` of AUM in listed
///   equities; the deficit is deployed through real exchange bids each turn.
/// * Active funds buy non-overvalued, dividend-eligible names ranked by
///   cheapest valuation (sector preference promoted); passive ETFs
///   index-weight all listed names by market cap.
/// * Sell when P/E > fund_max_pe_threshold, priced at max(mark, best_bid).
/// * For fixed-income: Buy when yield > fund_min_bond_yield
/// * Orders are limit orders; buys freeze the full qty × limit pledge.
/// * No blind buying — funds only buy names they would not immediately sell.
pub fn submit_fund_orders(
    funds: &mut [Company],
    exchange: &mut StockExchange,
    companies: &[Company],
    mbs_pool: &[MortgageBackedSecurity],
    covered_bonds: &[CoveredBond],
    config: &SecuritiesMarketConfig,
    current_turn: u32,
    vip_registry: Option<&crate::politics::vip_registry::VipRegistry>,
) {
    use crate::corporate::market_behavior::evaluate_market_behavior;

    // Equity-market fix: the listed-equity universe and NAV price map are
    // built from the UNION of both slices. Under the current turn wiring
    // (`split_at_mut(len)`), every company arrives in `funds` and `companies`
    // is empty — scanning `companies` alone is why no equity order was ever
    // generated. Only REAL listed companies (`legal_form.is_listed()`,
    // positive free float) with a positive mark qualify.
    let equity_prices = equity_price_map(funds, companies);
    let mut seen_ids = std::collections::HashSet::new();
    let equity_candidates: Vec<EquityCandidate> = funds
        .iter()
        .chain(companies.iter())
        .filter(|c| {
            c.fund_type.is_none()
                && c.legal_form.is_listed()
                && c.shares_count > 0
                && c.share_price > 0.0
                && seen_ids.insert(c.id.clone())
        })
        .map(EquityCandidate::from_company)
        .collect();

    #[cfg(feature = "diagnostic")]
    eprintln!(
        "EQDBG submit_fund_orders t={} candidates={}",
        current_turn,
        equity_candidates.len()
    );

    for fund in funds.iter_mut() {
        if fund.fund_type.is_none() || fund.fund_ledger.is_none() {
            continue;
        }
        if fund.brokerage_account.is_none() {
            continue;
        }

        let fund_id = fund.id.clone();
        let fund_cash = fund
            .brokerage_account
            .as_ref()
            .map(|b| b.cash)
            .unwrap_or(0.0);

        // Phase 57: Evaluate fund manager traits via centralized module — no raw string checks.
        let modifiers = if let Some(ref manager_id) = fund
            .fund_ledger
            .as_ref()
            .and_then(|l| l.fund_manager_vip_id.clone())
        {
            if let Some(registry) = vip_registry {
                if let Some(vip) = registry.get(manager_id) {
                    evaluate_market_behavior(&vip.traits)
                } else {
                    crate::corporate::market_behavior::MarketBehaviorModifiers::default()
                }
            } else {
                crate::corporate::market_behavior::MarketBehaviorModifiers::default()
            }
        } else {
            // Fall back to CEO VIP traits if no dedicated manager.
            if let Some(ref ceo_id) = fund.ceo_vip_id {
                if let Some(registry) = vip_registry {
                    if let Some(vip) = registry.get(ceo_id) {
                        evaluate_market_behavior(&vip.traits)
                    } else {
                        crate::corporate::market_behavior::MarketBehaviorModifiers::default()
                    }
                } else {
                    crate::corporate::market_behavior::MarketBehaviorModifiers::default()
                }
            } else {
                crate::corporate::market_behavior::MarketBehaviorModifiers::default()
            }
        };

        // Phase 57: Apply cash reserve preference — hold back minimum cash fraction.
        let investable_cash = fund_cash * (1.0 - modifiers.cash_reserve_preference.min(1.0));

        // Phase 57: Use trait-driven thresholds instead of config-only thresholds.
        // The modifier thresholds are derived from traits via evaluate_market_behavior.
        let sell_pe_threshold = config
            .fund_max_pe_threshold
            .max(modifiers.pe_sell_threshold);

        // ── Equity-market fix: AUM-target equity allocation ──────────────
        // Funds are no longer 100% fixed income: every fund targets
        // `config.fund_equity_target_share` of AUM in listed equities and
        // deploys the deficit each turn through REAL exchange bids.
        //
        // * Equity universe comes from `equity_candidates` — the union of the
        //   `funds` and `companies` slices — because the turn wiring passes
        //   ALL companies in `funds` and leaves `companies` empty.
        // * ACTIVE funds buy names that are not overvalued
        //   (P/E or P/B ≤ sell threshold, i.e. the fund would not sell them)
        //   AND pass the dividend gate, ranked by cheapest valuation first
        //   (sector preference promoted). PASSIVE funds (ETFs) index-weight
        //   by market cap across ALL listed names.
        // * Bid limit = the fund's willingness-to-pay cap
        //   (`eps * sell_pe_threshold` for earners, book value per share for
        //   pre-earnings names, never below the mark) so resting asks get
        //   lifted to fair value — prices emerge from matched trades.
        // * Sell rule unchanged in trigger (overvalued) but priced
        //   competitively: ask = max(mark, best_bid) so a standing deep-value
        //   bid is filled AT the bid instead of letting the ask undercut it.
        let equity_value_held: f64 = fund
            .brokerage_account
            .as_ref()
            .map(|a| {
                a.portfolio
                    .iter()
                    .filter(|(inst, _)| inst.starts_with("EQUITY:"))
                    .map(|(inst, lots)| {
                        let qty: u64 = lots.iter().map(|l| l.quantity).sum();
                        let company_id = inst.strip_prefix("EQUITY:").unwrap_or("");
                        equity_prices.get(company_id).copied().unwrap_or(0.0)
                            * qty as f64
                    })
                    .sum()
            })
            .unwrap_or(0.0);
        let bond_holdings_value: f64 = fund
            .fund_ledger
            .as_ref()
            .map(|l| {
                l.bond_holdings
                    .iter()
                    .filter(|h| !h.redeemed)
                    .map(|h| h.face_value)
                    .sum()
            })
            .unwrap_or(0.0);
        let portfolio_value = equity_value_held + bond_holdings_value;
        let aum = fund_cash
            + fund
                .brokerage_account
                .as_ref()
                .map(|b| b.frozen_cash)
                .unwrap_or(0.0)
            + portfolio_value;
        let target_equity = aum * config.fund_equity_target_share;
        let mut remaining_budget =
            (target_equity - equity_value_held).max(0.0).min(investable_cash);

        let is_passive = fund
            .fund_type
            .as_ref()
            .map(|ft| ft.is_passive())
            .unwrap_or(false);

        // Ranked candidate order for THIS fund (deterministic):
        //   passive → market_cap desc; active → sector-preferred first,
        //   then cheapest valuation (pe_or_pb asc). Ties broken by id.
        let mut ranked: Vec<&EquityCandidate> = equity_candidates
            .iter()
            .filter(|c| {
                if is_passive {
                    true
                } else {
                    c.pe_or_pb.is_finite()
                        && c.pe_or_pb <= sell_pe_threshold
                        && (c.dividend_yield > config.fund_min_dividend_yield
                            || (c.eps > 0.0 && c.dividend_yield <= 0.0))
                }
            })
            .collect();
        if is_passive {
            ranked.sort_by(|a, b| {
                b.market_cap
                    .partial_cmp(&a.market_cap)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.id.cmp(&b.id))
            });
        } else {
            ranked.sort_by(|a, b| {
                let ap = modifiers
                    .preferred_sector
                    .map(|p| a.sector == p)
                    .unwrap_or(false);
                let bp = modifiers
                    .preferred_sector
                    .map(|p| b.sector == p)
                    .unwrap_or(false);
                bp.cmp(&ap)
                    .then_with(|| {
                        a.pe_or_pb
                            .partial_cmp(&b.pe_or_pb)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| a.id.cmp(&b.id))
            });
        }
        let total_candidate_cap: f64 = ranked.iter().map(|c| c.market_cap).sum();

        let total_buy_budget = remaining_budget;
        let mut _bids_placed = 0u32;
        for cand in &ranked {
            if remaining_budget <= 0.0 {
                break;
            }
            if cand.id == fund_id {
                continue;
            }
            // Passive funds allocate by index weight against the ORIGINAL
            // deficit (weights sum to ~1); active funds fill the cheapest
            // names first, capped per-position by the trait modifier.
            let alloc = if is_passive && total_candidate_cap > 0.0 {
                total_buy_budget * (cand.market_cap / total_candidate_cap)
            } else {
                remaining_budget.min(investable_cash * modifiers.max_position_pct)
            };
            // Buyer willingness cap — the fund pays up to the top of its
            // "not overvalued" band, so crossing asks execute at fair value.
            let cap_bound = if cand.eps > 0.0 {
                cand.eps * sell_pe_threshold
            } else {
                cand.book_ps
            };
            let bid_limit = cap_bound.max(cand.share_price);
            if bid_limit <= 0.0 {
                continue;
            }
            let qty = (alloc / bid_limit) as u64;
            if qty == 0 {
                continue;
            }
            let order = Order::new_buy(
                format!("FUND-BUY-{}-{}", fund_id, cand.instrument_id),
                fund_id.clone(),
                cand.instrument_id.clone(),
                InstrumentType::Equity,
                qty,
                bid_limit,
                current_turn + 3,
            );
            // Freeze the full worst-case pledge (qty × limit) so settlement
            // never debits cash that does not exist.
            let cost = qty as f64 * bid_limit;
            if let Some(ref mut acct) = fund.brokerage_account {
                if acct.cash >= cost {
                    acct.cash -= cost;
                    acct.frozen_cash += cost;
                    let book = exchange
                        .order_book
                        .entry(cand.instrument_id.clone())
                        .or_default();
                    if let Some(pos) = book.bids.iter().position(|(p, _)| *p == bid_limit) {
                        book.bids[pos].1.push(order);
                    } else {
                        book.bids.push((bid_limit, vec![order]));
                        book.bids.sort_by(|a, b| {
                            a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
                        });
                    }
                    book.best_bid = book.bids.last().map(|(p, _)| *p).unwrap_or(0.0);
                    remaining_budget -= cost;
                    _bids_placed += 1;
                }
            }
        }

        // Sell rule: overvalued holdings are offered back to the market.
        // Ask = max(mark, best_bid) — if a deep-value bid is already resting
        // above the mark, the ask lifts to the bid so the execution prints
        // at the better price (the engine settles every match at the ask
        // limit; a naive market ask would under-sell into it).
        let mut _asks_placed = 0u32;
        for cand in equity_candidates.iter() {
            if cand.id == fund_id {
                continue;
            }
            if !(cand.pe_or_pb.is_finite() && cand.pe_or_pb > sell_pe_threshold) {
                continue;
            }
            let held = fund
                .brokerage_account
                .as_ref()
                .map(|a| a.get_quantity(&cand.instrument_id))
                .unwrap_or(0);
            if held == 0 {
                continue;
            }
            let best_bid = exchange
                .order_book
                .get(&cand.instrument_id)
                .map(|b| b.best_bid)
                .unwrap_or(0.0);
            let ask_limit = cand.share_price.max(best_bid);
            let order = Order::new_sell(
                format!("FUND-SELL-{}-{}", fund_id, cand.instrument_id),
                fund_id.clone(),
                cand.instrument_id.clone(),
                InstrumentType::Equity,
                held,
                ask_limit,
                current_turn + 3,
            );
            let book = exchange
                .order_book
                .entry(cand.instrument_id.clone())
                .or_default();
            if let Some(pos) = book.asks.iter().position(|(p, _)| *p == ask_limit) {
                book.asks[pos].1.push(order);
            } else {
                book.asks.push((ask_limit, vec![order]));
                book.asks
                    .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            }
            book.best_ask = book.asks.first().map(|(p, _)| *p).unwrap_or(0.0);
            _asks_placed += 1;
        }

        #[cfg(feature = "diagnostic")]
        eprintln!(
            "EQDBG fund_orders t={} fund={} aum={:.0} eq_held={:.0} target={:.0} budget_used={:.0} bids={} asks={}",
            current_turn,
            fund_id,
            aum,
            equity_value_held,
            target_equity,
            (target_equity - equity_value_held).max(0.0).min(investable_cash)
                - remaining_budget,
            _bids_placed,
            _asks_placed
        );

        // Evaluate MBS tranches (fixed-income)
        for mbs in mbs_pool {
            for tranche in &mbs.tranches {
                if tranche.owner_id == fund_id {
                    continue; // Already owned
                }
                if tranche.yield_rate >= config.fund_min_bond_yield
                    && tranche.outstanding_balance > 0.0
                {
                    let instrument_id =
                        format!("MBS:{}:{:?}", mbs.id, tranche.priority).to_lowercase();
                    let max_investment = fund_cash * 0.05; // Max 5% per fixed-income position
                    let price = tranche.outstanding_balance; // Approximate price
                    let max_units = (max_investment / price) as u64;
                    if max_units == 0 {
                        continue;
                    }
                    let order = Order::new_buy(
                        format!("FUND-BUY-{}-{}", fund_id, instrument_id),
                        fund_id.clone(),
                        instrument_id.clone(),
                        InstrumentType::MbsTranche {
                            mbs_id: mbs.id.clone(),
                            priority: tranche.priority,
                        },
                        max_units,
                        price,
                        current_turn + 3,
                    );
                    let cost = max_units as f64 * price;
                    if let Some(ref mut acct) = fund.brokerage_account {
                        if acct.cash >= cost {
                            acct.cash -= cost;
                            acct.frozen_cash += cost;
                            let book = exchange
                                .order_book
                                .entry(instrument_id.clone())
                                .or_default();
                            if let Some(pos) = book.bids.iter().position(|(p, _)| *p == price) {
                                book.bids[pos].1.push(order);
                            } else {
                                book.bids.push((price, vec![order]));
                                book.bids.sort_by(|a, b| {
                                    a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
                                });
                            }
                            book.best_bid = book.bids.last().map(|(p, _)| *p).unwrap_or(0.0);
                        }
                    }
                }
            }
        }

        // Evaluate covered bonds (fixed-income)
        for bond in covered_bonds {
            if bond.holder_id == fund_id {
                continue;
            }
            if bond.coupon_rate >= config.fund_min_bond_yield && bond.principal > 0.0 {
                let instrument_id = format!("BOND:{}", bond.id);
                let max_investment = fund_cash * 0.05;
                let price = bond.principal;
                let max_units = (max_investment / price) as u64;
                if max_units == 0 {
                    continue;
                }
                let order = Order::new_buy(
                    format!("FUND-BUY-{}-{}", fund_id, instrument_id),
                    fund_id.clone(),
                    instrument_id.clone(),
                    InstrumentType::CoveredBond,
                    max_units,
                    price,
                    current_turn + 3,
                );
                let cost = max_units as f64 * price;
                if let Some(ref mut acct) = fund.brokerage_account {
                    if acct.cash >= cost {
                        acct.cash -= cost;
                        acct.frozen_cash += cost;
                        let book = exchange
                            .order_book
                            .entry(instrument_id.clone())
                            .or_default();
                        if let Some(pos) = book.bids.iter().position(|(p, _)| *p == price) {
                            book.bids[pos].1.push(order);
                        } else {
                            book.bids.push((price, vec![order]));
                            book.bids.sort_by(|a, b| {
                                a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
                            });
                        }
                        book.best_bid = book.bids.last().map(|(p, _)| *p).unwrap_or(0.0);
                    }
                }
            }
        }
    }
}

/// Charge fund management and performance fees from AUM.
///
/// # Arguments
/// * `funds` - Mutable slice of fund companies
/// * `companies` - Slice of all companies (for portfolio valuation)
/// * `config` - Securities market config with fee rates
///
/// # Returns
/// Total fees collected across all funds
///
/// # Rules
/// * Management fee = AUM * fund_management_fee_rate
/// * Performance fee = (return - benchmark) * fund_performance_fee_rate (if positive)
/// * AUM = brokerage_account.cash + portfolio at market prices
/// * Fees deducted from fund brokerage cash (reduces NAV)
/// * Management fee revenue goes to fund company's liquid_capital (operating revenue)
pub fn charge_fund_fees(
    funds: &mut [Company],
    companies: &[Company],
    config: &SecuritiesMarketConfig,
) -> f64 {
    let mut total_fees = 0.0;

    // Equity-market fix: value the portfolio from the union of both slices
    // (`companies` arrives empty under the current turn wiring), and roll up
    // each fund's per-instrument equity positions into
    // `FundLedger.equity_holdings` — the test/UI-readable holdings count.
    let equity_prices = equity_price_map(funds, companies);

    for fund in funds.iter_mut() {
        if fund.fund_type.is_none() || fund.fund_ledger.is_none() {
            continue;
        }

        // Sync the ledger's equity position rollup from the brokerage book.
        {
            let acct_ref = fund.brokerage_account.as_ref();
            if let Some(ref mut ledger) = fund.fund_ledger {
                ledger.equity_holdings.clear();
                if let Some(acct) = acct_ref {
                    for (inst, lots) in &acct.portfolio {
                        if !inst.starts_with("EQUITY:") {
                            continue;
                        }
                        let qty: u64 = lots.iter().map(|l| l.quantity).sum();
                        if qty > 0 {
                            ledger.equity_holdings.insert(inst.clone(), qty);
                        }
                    }
                }
            }
        }

        // Frozen cash is still the fund's money — include it in AUM.
        let fund_cash = fund
            .brokerage_account
            .as_ref()
            .map(|b| b.cash + b.frozen_cash)
            .unwrap_or(0.0);
        let portfolio_value = calculate_portfolio_value(
            &fund.brokerage_account,
            &equity_prices,
            &fund.fund_ledger,
        );
        let aum = fund_cash + portfolio_value;

        if aum <= 0.0 {
            continue;
        }

        // Management fee
        let mgmt_fee = aum * config.fund_management_fee_rate;

        // Performance fee — charged on the REAL period return since the last
        // NAV strike, not on AUM itself. The previous formula computed
        // `excess_return = (aum - aum*benchmark).max(0)` ≈ 96% of AUM, so the
        // fund paid ~20% of its entire assets as a "performance" fee every
        // turn. That drained brokerage cash to zero, collapsed NAV, and the
        // next subscription then divided by a near-zero NAV — saturating
        // `shares_outstanding` to u64::MAX.
        //
        // prev_aum is reconstructed from the stored NAV strike
        // (nav_per_share × shares_outstanding = AUM at the last
        // collect_fund_capital valuation). period_return is therefore the
        // genuine intra-turn mark-to-market gain (coupons, trades) and
        // excludes new subscriptions, which are dilution-neutral.
        let ledger = fund.fund_ledger.as_ref().unwrap();
        let prev_aum = ledger.nav_per_share * ledger.shares_outstanding as f64;
        let period_return = (aum - prev_aum).max(0.0);
        // Benchmark hurdle: the annual benchmark rate prorated to one turn
        // (24 turns per year, the same convention as coupon accrual).
        let benchmark_return = prev_aum * config.fund_benchmark_rate / 24.0;
        let excess_return = (period_return - benchmark_return).max(0.0);
        let perf_fee = excess_return * config.fund_performance_fee_rate;

        let total_fee = mgmt_fee + perf_fee;

        // R6.2: Deterministic fee accounting — debit and credit the EXACT same
        // amount. Never credit more than was debited (fiat creation bug fix).
        // Route fee revenue to brokerage_account.cash (the active cash field).
        if let Some(ref mut acct) = fund.brokerage_account {
            let deductible = total_fee.min(acct.cash.max(0.0));
            acct.cash -= deductible;
            total_fees += deductible;
            // Credit the exact deductible amount to the fund's operating cash
            // (brokerage_account.cash is the active cash field per R6.2).
            // The fee stays within the fund as operating revenue — the fund
            // company earns it. To avoid double-counting, we credit to
            // liquid_capital (operating revenue) exactly what was debited.
            fund.liquid_capital += deductible;
        }
    }

    total_fees
}

// ============================================================================
// PHASE 57: DYNAMIC FUND CREATION
// ============================================================================

/// Phase 57: Attempt to create a new hedge fund from a wealthy, ambitious VIP.
///
/// # Trigger Conditions
/// * VIP has influence > 50
/// * VIP age 35–65
/// * VIP has "Ambitious" trait (checked via `evaluate_market_behavior`)
/// * VIP's personal brokerage cash > 5M
/// * Limited to 1 new fund per political year per country
///
/// # Arguments
/// * `vip` - The VIP who wants to create a fund.
/// * `vip_brokerage` - The VIP's personal brokerage account (if any).
/// * `country` - The country to add the fund to.
/// * `current_turn` - The current turn number.
/// * `fund_created_this_year` - Whether a fund was already created this year.
///
/// # Returns
/// `Some((fund_id, fund_company))` if a fund was created, `None` otherwise.
/// The caller is responsible for inserting the fund company into the entity collection.
pub fn try_create_fund_from_vip(
    vip: &crate::politics::vip_registry::Vip,
    vip_brokerage: Option<&crate::securities::BrokerageAccount>,
    country: &crate::state::Country,
    current_turn: u32,
    fund_created_this_year: bool,
) -> Option<(String, crate::entities::Company)> {
    use crate::corporate::market_behavior::evaluate_market_behavior;

    // Limit: 1 new fund per political year.
    if fund_created_this_year {
        return None;
    }

    // Check influence threshold.
    if vip.base_influence <= 50 {
        return None;
    }

    // Check age range.
    if vip.age < 35 || vip.age > 65 {
        return None;
    }

    // Check traits via centralized evaluation — no raw string checks.
    let modifiers = evaluate_market_behavior(&vip.traits);
    if modifiers.expansion_multiplier < 1.3 {
        // Not ambitious enough to found a fund.
        return None;
    }

    // Check personal brokerage cash > 5M.
    let vip_cash = vip_brokerage.map(|b| b.cash).unwrap_or(0.0);
    if vip_cash < 5_000_000.0 {
        return None;
    }

    // Create the fund.
    let fund_id = format!("FUND-DYN-{}-{}", country.name, current_turn);
    let initial_capital = vip_cash;
    // Units at par (1.0): clamp the f64→u64 cast so a corrupt/non-finite
    // capital can never saturate to u64::MAX.
    let seed_units = if initial_capital.is_finite() && initial_capital > 0.0 {
        initial_capital.min(u64::MAX as f64) as u64
    } else {
        0
    };

    // Create fund ledger.
    let ledger = crate::securities::FundLedger {
        nav_per_share: 1.0,
        shares_outstanding: seed_units,
        management_fee: 0.02,
        performance_fee: 0.20,
        leverage_ratio: 2.0, // Hedge funds start with 2x leverage
        investment_mandate: crate::securities::InvestmentMandate::default(),
        liquidity_provision: std::collections::BTreeMap::new(),
        unit_holders: {
            let mut m = std::collections::BTreeMap::new();
            m.insert(vip.full_name.clone(), seed_units);
            m
        },
        bond_holdings: Vec::new(),
        fund_manager_vip_id: Some(vip.full_name.clone()),
        equity_holdings: std::collections::BTreeMap::new(),
    };

    // Create fund company.
    let fund_company = crate::entities::Company {
        id: fund_id.clone(),
        file_stem: "banking".to_string(),
        name: format!("{} Hedge Fund", vip.full_name),
        sector: crate::registries::enums::Sector::Banking,
        region_id: country
            .regions
            .first()
            .map(|r| r.id.clone())
            .unwrap_or_default(),
        legal_form: crate::entities::LegalForm::JointStockCompany(
            crate::entities::JointStockData {
                shares_issued: seed_units,
                free_float: 0.0,
                dividend_per_share: 0.0,
                board_independence: 0.5,
                board_members: Vec::new(),
            },
        ),
        state_share: 0.0,
        fixed_capital: 100_000.0,
        liquid_capital: 0.0,
        available_cash: 0.0,
        company_capital: initial_capital,
        shares_count: seed_units,
        share_price: 1.0,
        brokerage_account: Some(crate::securities::BrokerageAccount {
            cash: initial_capital,
            ..Default::default()
        }),
        fund_type: Some(crate::securities::FundType::HedgeFund),
        fund_ledger: Some(ledger),
        ceo_vip_id: Some(vip.full_name.clone()),
        ..Default::default()
    };

    Some((fund_id, fund_company))
}

// ============================================================================
// RETAIL (HOUSEHOLD-CLASS) EQUITY TRADING — equity-market fix
// ============================================================================

/// Submit retail equity orders for wealthy demographic classes.
///
/// Citizens above a wealth floor (derived from the country average wage —
/// no absolute magic constant) trade listed equities DIRECTLY through the
/// exchange. Their identity is `{region_id}:{rural|urban}:{ClassKey}` — the
/// same format `collect_fund_capital` uses for fund unit-holders — and the
/// matcher settles their fills out of `ClassDemographics.savings` and the
/// class's own `brokerage_account` (the plumbing that already existed but
/// was never wired to the exchange).
///
/// # Arguments
/// * `regions` - Mutable regions (class savings + brokerage accounts).
/// * `exchange` - Mutable stock exchange for order insertion.
/// * `companies` - All companies (only REAL listed non-fund companies are
///   eligible: `legal_form.is_listed()` + shares + positive mark).
/// * `config` - Securities market config (`retail_equity_*` fields).
/// * `current_turn` - Current turn.
/// * `avg_wage` - Country average wage (`macro_indicators.average_wage`).
///
/// # Rules
/// * Eligibility: `savings_per_capita >= avg_wage * retail_equity_min_wealth_wages`
///   (≈ six months of wages at the default of 24 — the same order of
///   magnitude as the existing fund-subscription wealth gates).
/// * Buy amount: `savings * retail_equity_invest_rate` per turn — parity
///   with the fund subscription rate by default.
/// * Allocation: equal-weight across as many top-market-cap names as the
///   class can afford one median-priced share of (bounded [1, universe]).
///   The starting name rotates deterministically by turn + class index so
///   flow spreads across the market over time without any RNG.
/// * Bid limit: `share_price` — retail pays the prevailing mark.
/// * Sell rule: liquidate a position when the mark reaches
///   `avg_cost * (1 + fund_benchmark_rate)` — the existing benchmark-rate
///   config reused as the profit hurdle. Ask = max(mark, best_bid).
/// * No cash is frozen at submission — the matcher clamps each fill to the
///   savings actually available at execution (M0-safe).
pub fn submit_retail_equity_orders(
    regions: &mut [Region],
    exchange: &mut StockExchange,
    companies: &[Company],
    config: &SecuritiesMarketConfig,
    current_turn: u32,
    avg_wage: f64,
) {
    if avg_wage <= 0.0 {
        return;
    }

    // Real listed companies only, ranked by market cap (deterministic).
    let mut ranked: Vec<EquityCandidate> = companies
        .iter()
        .filter(|c| {
            c.fund_type.is_none()
                && c.legal_form.is_listed()
                && c.shares_count > 0
                && c.share_price > 0.0
        })
        .map(EquityCandidate::from_company)
        .collect();
    ranked.sort_by(|a, b| {
        b.market_cap
            .partial_cmp(&a.market_cap)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    if ranked.is_empty() {
        return;
    }
    let price_by_id: std::collections::HashMap<String, f64> = ranked
        .iter()
        .map(|c| (c.id.clone(), c.share_price))
        .collect();
    // Diversification breadth is derived from prices, not a magic constant:
    // one name per median-priced share the investment amount can afford.
    let mut prices: Vec<f64> = ranked.iter().map(|c| c.share_price).collect();
    prices.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_price = prices[prices.len() / 2].max(0.01);

    let wealth_floor = avg_wage * config.retail_equity_min_wealth_wages;
    let mut _orders_placed = 0u32;
    let mut class_idx = 0usize;

    for region in regions.iter_mut() {
        // Rural then urban — BTreeMap key order keeps iteration deterministic.
        for class_key in region
            .class_demographics
            .rural_classes
            .keys()
            .copied()
            .collect::<Vec<_>>()
        {
            let investor_id = format!("{}:rural:{}", region.id, class_key);
            let class = region
                .class_demographics
                .rural_classes
                .get_mut(&class_key)
                .unwrap();
            _orders_placed += submit_retail_class_orders(
                class,
                &investor_id,
                class_idx,
                &ranked,
                &price_by_id,
                median_price,
                wealth_floor,
                exchange,
                config,
                current_turn,
            );
            class_idx += 1;
        }
        for class_key in region
            .class_demographics
            .urban_classes
            .keys()
            .copied()
            .collect::<Vec<_>>()
        {
            let investor_id = format!("{}:urban:{}", region.id, class_key);
            let class = region
                .class_demographics
                .urban_classes
                .get_mut(&class_key)
                .unwrap();
            _orders_placed += submit_retail_class_orders(
                class,
                &investor_id,
                class_idx,
                &ranked,
                &price_by_id,
                median_price,
                wealth_floor,
                exchange,
                config,
                current_turn,
            );
            class_idx += 1;
        }
    }

    #[cfg(feature = "diagnostic")]
    eprintln!(
        "EQDBG retail_orders t={} orders={} classes={} wealth_floor={:.0} median_px={:.2}",
        current_turn,
        _orders_placed,
        class_idx,
        wealth_floor,
        median_price
    );
}

/// Per-class retail order submission (sell profit-takes first, then buys).
/// Returns the number of orders placed.
fn submit_retail_class_orders(
    class: &mut crate::society::geography::ClassDemographics,
    investor_id: &str,
    class_idx: usize,
    ranked: &[EquityCandidate],
    price_by_id: &std::collections::HashMap<String, f64>,
    median_price: f64,
    wealth_floor: f64,
    exchange: &mut StockExchange,
    config: &SecuritiesMarketConfig,
    current_turn: u32,
) -> u32 {
    let mut placed = 0u32;

    // ── Profit-taking sells: liquidate a position once the mark reaches
    // avg_cost × (1 + benchmark). Ask = max(mark, best_bid) so a standing
    // deep-value fund bid fills at the better price. ──
    if let Some(acct) = class.brokerage_account.as_ref() {
        let sells: Vec<(String, u64, f64)> = acct
            .portfolio
            .iter()
            .filter(|(inst, _)| inst.starts_with("EQUITY:"))
            .filter_map(|(inst, lots)| {
                let held: u64 = lots.iter().map(|l| l.quantity).sum();
                if held == 0 {
                    return None;
                }
                let company_id = inst.strip_prefix("EQUITY:")?;
                let mark = *price_by_id.get(company_id).unwrap_or(&0.0);
                let avg_cost = acct.get_average_cost_basis(inst);
                if avg_cost > 0.0
                    && mark >= avg_cost * (1.0 + config.fund_benchmark_rate)
                {
                    Some((inst.clone(), held, mark))
                } else {
                    None
                }
            })
            .collect();
        for (inst, held, mark) in sells {
            let best_bid = exchange
                .order_book
                .get(&inst)
                .map(|b| b.best_bid)
                .unwrap_or(0.0);
            let ask_limit = mark.max(best_bid);
            let order = Order::new_sell(
                format!("RETAIL-SELL-{}-{}", investor_id, inst),
                investor_id.to_string(),
                inst.clone(),
                InstrumentType::Equity,
                held,
                ask_limit,
                current_turn + 3,
            );
            let book = exchange.order_book.entry(inst.clone()).or_default();
            if let Some(pos) = book.asks.iter().position(|(p, _)| *p == ask_limit) {
                book.asks[pos].1.push(order);
            } else {
                book.asks.push((ask_limit, vec![order]));
                book.asks
                    .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            }
            book.best_ask = book.asks.first().map(|(p, _)| *p).unwrap_or(0.0);
            placed += 1;
        }
    }

    // ── Buys: wealthy classes allocate a fraction of savings into equities ──
    if class.savings_per_capita < wealth_floor || class.savings <= 0.0 {
        return placed;
    }
    let amount = class.savings * config.retail_equity_invest_rate;
    if amount <= 0.0 {
        return placed;
    }
    let max_names = ((amount / median_price).floor() as usize)
        .max(1)
        .min(ranked.len());
    // Deterministic rotation so flow spreads across names over turns.
    let start = (current_turn as usize + class_idx) % ranked.len();
    let mut remaining = amount;
    for k in 0..max_names {
        if remaining <= 0.0 {
            break;
        }
        let cand = &ranked[(start + k) % ranked.len()];
        let alloc = remaining / (max_names - k) as f64;
        let qty = (alloc / cand.share_price) as u64;
        if qty == 0 {
            continue;
        }
        let order = Order::new_buy(
            format!("RETAIL-BUY-{}-{}", investor_id, cand.instrument_id),
            investor_id.to_string(),
            cand.instrument_id.clone(),
            InstrumentType::Equity,
            qty,
            cand.share_price,
            current_turn + 3,
        );
        let book = exchange
            .order_book
            .entry(cand.instrument_id.clone())
            .or_default();
        if let Some(pos) = book.bids.iter().position(|(p, _)| *p == cand.share_price) {
            book.bids[pos].1.push(order);
        } else {
            book.bids.push((cand.share_price, vec![order]));
            book.bids
                .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        }
        book.best_bid = book.bids.last().map(|(p, _)| *p).unwrap_or(0.0);
        remaining -= qty as f64 * cand.share_price;
        placed += 1;
    }
    placed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fund_type_serialization() {
        let fund_type = FundType::OpenEndInvestmentFund;
        let serialized = serde_json::to_string(&fund_type).unwrap();
        assert!(serialized.contains("OpenEndInvestmentFund"));
    }

    #[test]
    fn test_fund_ledger_default() {
        let ledger = FundLedger::default();
        assert_eq!(ledger.nav_per_share, 0.0);
        assert_eq!(ledger.shares_outstanding, 0);
    }

    #[test]
    fn test_investment_mandate_default() {
        let mandate = InvestmentMandate::default();
        assert_eq!(mandate.max_position_size, 0.0);
        assert!(mandate.allowed_sectors.is_empty());
    }
}

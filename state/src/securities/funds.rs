//! Institutional investor funds module.
//!
//! This module implements FundType enum and FundLedger struct for
//! institutional investors like FIO, FIZ, hedge funds, and ETFs.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use serde_json::Value;

use crate::registries::enums::Sector;
use crate::entities::Company;
use crate::securities::config::SecuritiesMarketConfig;
use crate::securities::exchange::{StockExchange, Order, InstrumentType};
use crate::securities::mbs::MortgageBackedSecurity;
use crate::securities::covered_bonds::CoveredBond;
use crate::society::geography::Region;

/// Type of institutional fund.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename = "typ_funduszu")]
pub enum FundType {
    /// FIO - Open-End Investment Fund (Fundusz Inwestycyjny Otwarty).
    #[serde(rename = "FIO")]
    OpenEndInvestmentFund,
    
    /// FIZ - Closed-End Investment Fund (Fundusz Inwestycyjny Zamknięty).
    #[serde(rename = "FIZ")]
    ClosedEndInvestmentFund,
    
    /// Hedge Fund (high-risk, high-leverage strategies).
    #[serde(rename = "fundusz_zabezpieczający")]
    HedgeFund,
    
    /// ETF - Exchange Traded Fund.
    #[serde(rename = "ETF")]
    ExchangeTradedFund,
    
    /// Mutual Fund (traditional diversified portfolio).
    #[serde(rename = "fundusz_wzajemny")]
    MutualFund,
}

/// Detailed ledger for fund operations and holdings.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename = "księga_funduszu")]
pub struct FundLedger {
    /// Net Asset Value per share.
    #[serde(rename = "wartość_aktywa_netto")]
    pub nav_per_share: f64,

    /// Total shares outstanding.
    #[serde(rename = "akcje_w_obrocie")]
    pub shares_outstanding: u64,

    /// Management fee (percentage of AUM).
    #[serde(rename = "opłata_zarządzania")]
    pub management_fee: f64,

    /// Performance fee (percentage of profits above benchmark).
    #[serde(rename = "opłata_za_wyniki")]
    pub performance_fee: f64,

    /// Leverage ratio (for hedge funds).
    #[serde(rename = "dźwignia")]
    pub leverage_ratio: f64,

    /// Investment mandate restrictions.
    #[serde(rename = "mandat_inwestycyjny")]
    pub investment_mandate: InvestmentMandate,

    /// Liquidity provision to AMM pools.
    #[serde(rename = "dostarczanie_płynności")]
    pub liquidity_provision: BTreeMap<String, f64>,

    /// Resurrection Phase 2: Unit holder registry — maps contributor_id to units held.
    #[serde(rename = "posiadacze_jednostek", default)]
    pub unit_holders: BTreeMap<String, u64>,

    /// Phase 36: Sovereign/treasury bond holdings with strict double-entry tracking.
    /// Each holding records the security ID, face value, purchase price, coupon rate,
    /// maturity, and ownership. This prevents the "magic asset increase" bug where
    /// fund NAV rose without debiting cash or crediting a counterparty.
    #[serde(rename = "obligacje_skarbowe", default)]
    pub bond_holdings: Vec<FundBondHolding>,
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
#[serde(rename = "mandat_inwestycyjny")]
pub struct InvestmentMandate {
    /// Maximum position size in single company (percentage).
    #[serde(rename = "maksymalna_pozycja")]
    pub max_position_size: f64,
    
    /// Allowed sectors.
    #[serde(rename = "dozwolone_sektory")]
    pub allowed_sectors: Vec<Sector>,
    
    /// Minimum liquidity requirement.
    #[serde(rename = "minimalna_płynność")]
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
    current_turn: u32,
) -> Vec<FundSubscription> {
    let mut subscriptions = Vec::new();

    for fund in funds.iter_mut() {
        let ft = match &fund.fund_type {
            Some(ft) => ft.clone(),
            None => continue,
        };
        let ledger = match &mut fund.fund_ledger {
            Some(l) => l,
            None => continue,
        };

        // Calculate current NAV using AUM (cash + portfolio at market prices)
        let fund_cash = fund.brokerage_account.as_ref().map(|b| b.cash).unwrap_or(0.0);
        // Clone bond holdings to avoid borrow conflict with `ledger`
        let bond_holdings_value: f64 = ledger.bond_holdings.iter()
            .filter(|h| !h.redeemed)
            .map(|h| h.face_value)
            .sum();
        let portfolio_value = calculate_portfolio_value(&fund.brokerage_account, companies, &None) + bond_holdings_value;
        let total_fund_value = fund_cash + portfolio_value;

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
                        if ft == FundType::ClosedEndInvestmentFund && class.savings_per_capita < 500.0 {
                            continue;
                        }

                        // Double-entry (cash side): savings -= amount, fund.cash += amount
                        class.savings -= subscription_amount;
                        if let Some(ref mut acct) = fund.brokerage_account {
                            acct.cash += subscription_amount;
                        }

                        // Double-entry (equity side): issue units
                        let units_issued = if nav_per_share > 0.0 {
                            (subscription_amount / nav_per_share) as u64
                        } else {
                            0
                        };
                        ledger.shares_outstanding += units_issued;
                        *ledger.unit_holders.entry(contributor_id.clone()).or_insert(0) += units_issued;

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
                        if ft == FundType::ClosedEndInvestmentFund && class.savings_per_capita < 500.0 {
                            continue;
                        }

                        // Double-entry (cash side): savings -= amount, fund.cash += amount
                        class.savings -= subscription_amount;
                        if let Some(ref mut acct) = fund.brokerage_account {
                            acct.cash += subscription_amount;
                        }

                        // Double-entry (equity side): issue units
                        let units_issued = if nav_per_share > 0.0 {
                            (subscription_amount / nav_per_share) as u64
                        } else {
                            0
                        };
                        ledger.shares_outstanding += units_issued;
                        *ledger.unit_holders.entry(contributor_id.clone()).or_insert(0) += units_issued;

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

                    // Double-entry (equity side): issue units
                    let units_issued = if nav_per_share > 0.0 {
                        (subscription_amount / nav_per_share) as u64
                    } else {
                        0
                    };
                    ledger.shares_outstanding += units_issued;
                    *ledger.unit_holders.entry(company.id.clone()).or_insert(0) += units_issued;

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
        let fund_cash = fund.brokerage_account.as_ref().map(|b| b.cash).unwrap_or(0.0);
        let bond_holdings_value: f64 = ledger.bond_holdings.iter()
            .filter(|h| !h.redeemed)
            .map(|h| h.face_value)
            .sum();
        let portfolio_value = calculate_portfolio_value(&fund.brokerage_account, companies, &None) + bond_holdings_value;
        let total_fund_value = fund_cash + portfolio_value;
        if ledger.shares_outstanding > 0 {
            ledger.nav_per_share = total_fund_value / ledger.shares_outstanding as f64;
        }
    }

    subscriptions
}

/// Calculate portfolio value at current market prices.
///
/// # Arguments
/// * `brokerage_account` - Optional reference to brokerage account
/// * `companies` - Slice of all companies (for share price lookup)
/// * `fund_ledger` - Optional fund ledger (for bond holdings, Phase 36)
///
/// # Returns
/// Total market value of portfolio holdings
fn calculate_portfolio_value(
    brokerage_account: &Option<crate::securities::BrokerageAccount>,
    companies: &[Company],
    fund_ledger: &Option<FundLedger>,
) -> f64 {
    let acct = match brokerage_account {
        Some(a) => a,
        None => return 0.0,
    };

    let mut total = 0.0;
    for (instrument_id, qty) in &acct.portfolio {
        if instrument_id.starts_with("EQUITY:") {
            let company_id = &instrument_id[7..];
            if let Some(company) = companies.iter().find(|c| c.id == company_id) {
                total += company.share_price * *qty as f64;
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
    let security = match country.debt_market.outstanding_securities.iter_mut()
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
/// * Buy when P/E < fund_min_pe_threshold AND dividend_yield > fund_min_dividend_yield
/// * Sell when P/E > fund_max_pe_threshold
/// * For fixed-income: Buy when yield > fund_min_bond_yield
/// * Orders are limit orders at current market price
/// * No blind buying — funds only buy undervalued, sell overvalued
pub fn submit_fund_orders(
    funds: &mut [Company],
    exchange: &mut StockExchange,
    companies: &[Company],
    mbs_pool: &[MortgageBackedSecurity],
    covered_bonds: &[CoveredBond],
    config: &SecuritiesMarketConfig,
    current_turn: u32,
) {
    for fund in funds.iter_mut() {
        if fund.fund_type.is_none() || fund.fund_ledger.is_none() {
            continue;
        }
        if fund.brokerage_account.is_none() {
            continue;
        }

        let fund_id = fund.id.clone();
        let fund_cash = fund.brokerage_account.as_ref().map(|b| b.cash).unwrap_or(0.0);

        // Evaluate equities
        for company in companies {
            if company.id == fund_id {
                continue;
            }
            if !company.is_listed || company.shares_count == 0 || company.share_price <= 0.0 {
                continue;
            }

            // Compute P/B ratio (price-to-book) as valuation proxy
            let book_value_per_share = company.company_capital / company.shares_count as f64;
            let pb_ratio = if book_value_per_share > 0.0 {
                company.share_price / book_value_per_share
            } else {
                f64::INFINITY
            };

            // Compute dividend yield
            let dividend_yield = if company.share_price > 0.0 && company.shares_count > 0 {
                company.aggregated_stats.total_dividends / (company.share_price * company.shares_count as f64)
            } else {
                0.0
            };

            let instrument_id = format!("EQUITY:{}", company.id);

            // Valuation Score logic
            if pb_ratio < config.fund_min_pe_threshold && dividend_yield > config.fund_min_dividend_yield {
                // Undervalued: submit Buy order
                let max_investment = fund_cash * 0.1; // Max 10% of cash per position
                let max_shares = (max_investment / company.share_price) as u64;
                if max_shares == 0 {
                    continue;
                }
                let order = Order::new_buy(
                    format!("FUND-BUY-{}-{}", fund_id, instrument_id),
                    fund_id.clone(),
                    instrument_id.clone(),
                    InstrumentType::Equity,
                    max_shares,
                    company.share_price,
                    current_turn + 3,
                );
                // Freeze cash for the order
                let cost = max_shares as f64 * company.share_price;
                if let Some(ref mut acct) = fund.brokerage_account {
                    if acct.cash >= cost {
                        acct.cash -= cost;
                        acct.frozen_cash += cost;
                        // Insert into order book
                        let book = exchange.order_book.entry(instrument_id.clone()).or_default();
                        if let Some(pos) = book.bids.iter().position(|(p, _)| *p == company.share_price) {
                            book.bids[pos].1.push(order);
                        } else {
                            book.bids.push((company.share_price, vec![order]));
                            book.bids.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                        }
                        book.best_bid = book.bids.last().map(|(p, _)| *p).unwrap_or(0.0);
                    }
                }
            } else if pb_ratio > config.fund_max_pe_threshold {
                // Overvalued: submit Sell order if fund holds shares
                let held = fund.brokerage_account.as_ref()
                    .and_then(|a| a.portfolio.get(&instrument_id))
                    .copied()
                    .unwrap_or(0);
                if held == 0 {
                    continue;
                }
                let order = Order::new_sell(
                    format!("FUND-SELL-{}-{}", fund_id, instrument_id),
                    fund_id.clone(),
                    instrument_id.clone(),
                    InstrumentType::Equity,
                    held,
                    company.share_price,
                    current_turn + 3,
                );
                let book = exchange.order_book.entry(instrument_id.clone()).or_default();
                if let Some(pos) = book.asks.iter().position(|(p, _)| *p == company.share_price) {
                    book.asks[pos].1.push(order);
                } else {
                    book.asks.push((company.share_price, vec![order]));
                    book.asks.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                }
                book.best_ask = book.asks.first().map(|(p, _)| *p).unwrap_or(0.0);
            }
        }

        // Evaluate MBS tranches (fixed-income)
        for mbs in mbs_pool {
            for tranche in &mbs.tranches {
                if tranche.owner_id == fund_id {
                    continue; // Already owned
                }
                if tranche.yield_rate >= config.fund_min_bond_yield && tranche.outstanding_balance > 0.0 {
                    let instrument_id = format!("MBS:{}:{:?}", mbs.id, tranche.priority).to_lowercase();
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
                            let book = exchange.order_book.entry(instrument_id.clone()).or_default();
                            if let Some(pos) = book.bids.iter().position(|(p, _)| *p == price) {
                                book.bids[pos].1.push(order);
                            } else {
                                book.bids.push((price, vec![order]));
                                book.bids.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
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
                        let book = exchange.order_book.entry(instrument_id.clone()).or_default();
                        if let Some(pos) = book.bids.iter().position(|(p, _)| *p == price) {
                            book.bids[pos].1.push(order);
                        } else {
                            book.bids.push((price, vec![order]));
                            book.bids.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
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

    for fund in funds.iter_mut() {
        if fund.fund_type.is_none() || fund.fund_ledger.is_none() {
            continue;
        }

        let fund_cash = fund.brokerage_account.as_ref().map(|b| b.cash).unwrap_or(0.0);
        let portfolio_value = calculate_portfolio_value(&fund.brokerage_account, companies, &fund.fund_ledger);
        let aum = fund_cash + portfolio_value;

        if aum <= 0.0 {
            continue;
        }

        // Management fee
        let mgmt_fee = aum * config.fund_management_fee_rate;

        // Performance fee (simplified: if fund cash growth > benchmark rate)
        let ledger = fund.fund_ledger.as_ref().unwrap();
        let benchmark_return = aum * config.fund_benchmark_rate;
        let excess_return = (aum - benchmark_return).max(0.0);
        let perf_fee = excess_return * config.fund_performance_fee_rate;

        let total_fee = mgmt_fee + perf_fee;

        // Deduct from fund cash
        if let Some(ref mut acct) = fund.brokerage_account {
            let deductible = total_fee.min(acct.cash);
            acct.cash -= deductible;
            total_fees += deductible;
        }

        // Credit fee as operating revenue to fund company
        fund.liquid_capital += total_fee;
    }

    total_fees
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fund_type_serialization() {
        let fund_type = FundType::OpenEndInvestmentFund;
        let serialized = serde_json::to_string(&fund_type).unwrap();
        assert!(serialized.contains("FIO"));
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


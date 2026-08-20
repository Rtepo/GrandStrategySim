//! Securities market module for equity trading, brokerage accounts, and advanced financial instruments.
//! 
//! This module implements Phase D.4 of the market architecture:
//! - BrokerageAccount for closed-loop ownership tracking
//! - StockExchange with dual-liquidity (Order Book + AMM)
//! - KNF (Financial Supervision Authority) for regulatory oversight
//! - FundType and FundLedger for institutional investors
//! - BillOfLading for trade finance collateral
//! - CoveredBond for bank debt securities
//! 
//! Phase D.5 additions:
//! - MBS (Mortgage-Backed Securities) with tranches
//! - Derivatives (CDS, Futures)
//! - CCP (Central Counterparty Clearinghouse)

pub mod brokerage;
pub mod config;
pub mod exchange;
pub mod knf;
pub mod funds;
pub mod trade_finance;
pub mod covered_bonds;
pub mod mbs;
pub mod derivatives;
pub mod ccp;

pub use brokerage::{BrokerageAccount, MarginAccount, PositionLot};
pub use config::SecuritiesMarketConfig;
pub use exchange::{StockExchange, OrderBook, Order, LiquidityPool, Trade, CircuitBreaker, InstrumentType, MarketIndex, CommoditySpotMarket};
pub use knf::{KNF, AuditFinding, ViolationType, TradingHalt, HaltReason, FreezeReason};
pub use funds::{FundType, FundLedger, InvestmentMandate};
pub use trade_finance::{BillOfLading, BillStatus, WorkingCapitalLoan};
pub use covered_bonds::CoveredBond;
pub use mbs::{MortgageBackedSecurity, MbsTranche, TranchePriority};
pub use derivatives::{CreditDefaultSwap, FuturesContract, ReferenceEntity, FuturesUnderlying, FuturesPosition, ClearingMethod};
pub use ccp::{CentralCounterparty, CcpMember, MemberStatus, MarginRequirements};

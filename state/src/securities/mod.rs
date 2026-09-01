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
pub mod ccp;
pub mod config;
pub mod covered_bonds;
pub mod derivatives;
pub mod exchange;
pub mod funds;
pub mod knf;
pub mod mbs;
pub mod trade_finance;

pub use brokerage::{BrokerageAccount, MarginAccount, PositionLot};
pub use ccp::{CcpMember, CentralCounterparty, MarginRequirements, MemberStatus};
pub use config::SecuritiesMarketConfig;
pub use covered_bonds::CoveredBond;
pub use derivatives::{
    ClearingMethod, CreditDefaultSwap, FuturesContract, FuturesPosition, FuturesUnderlying,
    ReferenceEntity,
};
pub use exchange::{
    CircuitBreaker, CommoditySpotMarket, InstrumentType, LiquidityPool, MarketIndex, Order,
    OrderBook, StockExchange, Trade,
};
pub use funds::{FundLedger, FundType, InvestmentMandate};
pub use knf::{AuditFinding, FreezeReason, HaltReason, TradingHalt, ViolationType, KNF};
pub use mbs::{MbsTranche, MortgageBackedSecurity, TranchePriority};
pub use trade_finance::{BillOfLading, BillStatus, WorkingCapitalLoan};

//! International systems — diplomacy, trade and currency shocks.
//!
//! This module contains the global, cross-country mechanics that cannot be
//! handled inside a single [`crate::economy::CountryTurnCtx`].  The core entry point is
//! [`trade::balance_global_trade`], which uses the two-phase Collect-Then-Apply
//! mutation pattern so the Rust borrow checker can safely update all nations
//! from a shared global market snapshot.

pub mod diplomacy;
pub mod trade;

pub use diplomacy::{generate_diplomacy, process_diplomacy_turn};
pub use trade::{
    balance_global_trade, DiplomaticRelation, TradeBalanceResult, TradeDelta,
};

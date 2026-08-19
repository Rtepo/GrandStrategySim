//! Market subdirectory: market clearing, order books, and price history.
pub mod clearing;
pub mod market;
pub mod market_history;
pub mod order_book;

// Re-export contents of market.rs at the market/ module level so that
// `crate::economy::market::MarketSignal` continues to resolve.
pub use market::*;

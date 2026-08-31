//! Mergers and Acquisitions lifecycle (stub — Phase D scaffolding).
//!
//! This module will eventually implement organic, market-driven M&A:
//! - Regional sector saturation / distressed target / labor-pool rescue triggers.
//! - Two-pass delta-buffer execution with FreightCapacity-aware inventory transfer.
//! - Strict double-entry cash and liability assumption via `LoanRef`.
//!
//! For the current build, the public entrypoint is a no-op so the turn loop
//! compiles and the tombstone fields introduced in Phase A are exercised.

use crate::economy::market::MarketSignal;
use crate::entities::{Building, Company};
use crate::state::Country;

/// Process M&A for a single country in the turn loop.
///
/// Currently a no-op stub. The full implementation will scan region-sector pairs,
/// build `AcquisitionDelta`s, and apply them before `CompanyLifecycle` runs.
pub fn process_mergers_and_acquisitions(
    _companies: &mut [Company],
    _buildings: &mut [Building],
    _country: &mut Country,
    _year: u32,
    _market_signal: &MarketSignal,
    _current_turn: u32,
) {
    // Phase D: full M&A implementation pending.
}

//! Government simulation — tax collection and state spending.
//!
//! This module ports the treasury cycle from the Python engine's
//! `economy/macro/taxes.py` and `politics/core.py`: collecting income,
//! corporate and VAT revenues, then clearing state OPEX (state-building
//! maintenance, the Black Ops budget, and debt servicing).

pub mod kio;
pub mod treasury_ops;

pub use treasury_ops::{
    accumulate_storage_fees, apply_rationing_consequences, calculate_black_ops_budget,
    check_emergency_conditions, process_black_ops_funding, process_state_reserve_maintenance,
    process_storage_transactions, settle_periodic_storage_fees, settle_rot_fees,
};

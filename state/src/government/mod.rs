//! Government simulation — tax collection and state spending.
//!
//! This module ports the treasury cycle from the Python engine's
//! `economy/macro/taxes.py` and `politics/core.py`: collecting income,
//! corporate and VAT revenues, then clearing state OPEX (state-building
//! maintenance, the Black Ops budget, and debt servicing).

pub mod treasury_ops;
pub mod kio;

pub use treasury_ops::{
    settle_rot_fees,
    settle_periodic_storage_fees,
    check_emergency_conditions,
    apply_rationing_consequences,
    accumulate_storage_fees,
    process_storage_transactions,
    calculate_black_ops_budget,
    process_black_ops_funding,
    process_state_reserve_maintenance,
};

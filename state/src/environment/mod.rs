//! Phase 82: Environment module — smog, emissions, and air quality.
//!
//! This module handles localized air pollution (smog) from heating,
//! industrial, and power generation sources. Smog is computed as a
//! concentration (mass per area), not raw mass, ensuring that rural
//! regions with the same absolute emissions have much lower smog than
//! dense urban regions (CORRECTION 7: Concentration Fallacy).

pub mod smog;

pub use smog::{LocalPollutionState, compute_smog_for_region, distribute_smog_to_parcels};

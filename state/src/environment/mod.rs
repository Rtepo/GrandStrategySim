//! Phase 82: Environment module — smog, emissions, and air quality.
//!
//! This module handles localized air pollution (smog) from heating,
//! industrial, and power generation sources. Smog is computed as a
//! concentration (mass per area), not raw mass, ensuring that rural
//! regions with the same absolute emissions have much lower smog than
//! dense urban regions (CORRECTION 7: Concentration Fallacy).
//!
//! Phase 18E: Extended with urban park pollution reduction, happiness
//! boost, and ecological tax assessment based on pollution proximity.

pub mod smog;
pub mod parks;

pub use smog::{compute_smog_for_region, distribute_smog_to_parcels, LocalPollutionState};
pub use parks::{
    apply_urban_park_pollution_reduction,
    apply_urban_park_happiness_boost,
    assess_ecological_tax_by_pollution_proximity,
    ParkEnvironmentConfig,
};

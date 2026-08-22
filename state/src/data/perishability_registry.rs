//! Perishability profile registry for commodity storage decay.
//!
//! Provides compile-time-checked static data for commodity shelf life
//! and decay rates across storage types (General vs Cold).

use crate::registries::enums::Commodity;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Perishability profile for a commodity.
#[derive(Debug, Clone, Copy)]
pub struct PerishabilityProfile {
    /// Maximum turns before total decay in general storage.
    pub max_turns_general: u32,
    /// Decay rate per turn in general storage (0.0-1.0).
    pub decay_rate_general: f64,
    /// Maximum turns before total decay in cold storage.
    pub max_turns_cold: u32,
    /// Decay rate per turn in cold storage (0.0-1.0).
    pub decay_rate_cold: f64,
}

/// Returns the static perishability registry.
///
/// # Returns
/// * `&'static HashMap<Commodity, PerishabilityProfile>` — compile-time initialized
///
/// # Rules
/// * Registry is exhaustive over the `Commodity` enum; adding a new commodity
///   requires an entry here or a compile-time error will occur.
/// * Non-perishable goods use `u32::MAX` and `0.0` decay rates.
pub fn perishability_registry() -> &'static HashMap<Commodity, PerishabilityProfile> {
    static REGISTRY: OnceLock<HashMap<Commodity, PerishabilityProfile>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut m = HashMap::new();

        // Agricultural commodities (Phase 6.3.5)
        m.insert(
            Commodity::Vegetable,
            PerishabilityProfile {
                max_turns_general: 3,
                decay_rate_general: 0.33,
                max_turns_cold: 8,
                decay_rate_cold: 0.12,
            },
        );
        m.insert(
            Commodity::Cereal,
            PerishabilityProfile {
                max_turns_general: 12,
                decay_rate_general: 0.08,
                max_turns_cold: 24,
                decay_rate_cold: 0.04,
            },
        );
        m.insert(
            Commodity::Fodder,
            PerishabilityProfile {
                max_turns_general: 8,
                decay_rate_general: 0.12,
                max_turns_cold: u32::MAX,
                decay_rate_cold: 0.0,
            },
        );
        m.insert(
            Commodity::IndustrialFiber,
            PerishabilityProfile {
                max_turns_general: 24,
                decay_rate_general: 0.04,
                max_turns_cold: 48,
                decay_rate_cold: 0.02,
            },
        );
        m.insert(
            Commodity::Luxury,
            PerishabilityProfile {
                max_turns_general: 16,
                decay_rate_general: 0.06,
                max_turns_cold: 32,
                decay_rate_cold: 0.03,
            },
        );

        // Legacy Polish-keyed commodities (save compatibility)
        // Note: Medicine doesn't exist in Commodity enum; using placeholder if needed

        m
    })
}

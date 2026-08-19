//! Government economic interventions for price controls and subsidies.
//!
//! This module defines price interventions (caps, floors, subsidies) that
//! are applied at order submission (caps/floors) and settlement (subsidies).

use crate::registries::enums::Commodity;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Price intervention for a specific commodity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PriceIntervention {
    /// Commodity to intervene on.
    pub commodity: Commodity,
    /// Maximum allowed price (None = no cap). Applied at order submission.
    pub price_cap: Option<f64>,
    /// Minimum guaranteed price (None = no floor). Applied at order submission.
    pub price_floor: Option<f64>,
    /// Per-unit subsidy to buyers (None = no subsidy). Applied at settlement.
    pub buyer_subsidy: Option<f64>,
    /// Per-unit subsidy to sellers (None = no subsidy). Applied at settlement.
    pub seller_subsidy: Option<f64>,
}

/// Economic policy containing all price interventions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct EconomicPolicy {
    /// Price interventions by commodity.
    pub price_interventions: HashMap<Commodity, PriceIntervention>,
}

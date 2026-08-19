//! Policy parameters for financial operations (eliminates magic numbers).
//!
//! This module implements Phase D.5 policy structures for:
//! - Central Bank policy (QE, REPO haircuts)
//! - KNF policy (derivative fines, margin ratios)

use serde::{Deserialize, Serialize};
use serde_json::Map;

/// Central Bank policy parameters (eliminates magic numbers).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct CentralBankPolicy {
    /// REPO haircut for Treasury Bonds (e.g., 0.02 for 2%).
    #[serde(default)]
    pub repo_haircut_treasury: f64,
    
    /// REPO haircut for Senior MBS (e.g., 0.20 for 20%).
    #[serde(default)]
    pub repo_haircut_mbs: f64,
    
    /// QE budget as percentage of GDP (e.g., 0.05 for 5%).
    #[serde(default)]
    pub qe_gdp_percentage: f64,
    
    /// KNF volatility threshold to trigger QE (e.g., 0.8 for 80%).
    #[serde(default)]
    pub qe_volatility_threshold: f64,
    
    /// Any additional CB policy fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

/// KNF policy parameters (eliminates magic numbers).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct KnfPolicy {
    /// OTC derivative fine rate as percentage of notional (e.g., 0.02 for 2%).
    #[serde(default)]
    pub otc_fine_rate: f64,
    
    /// Initial margin ratio for derivatives (e.g., 0.10 for 10%).
    #[serde(default)]
    pub initial_margin_ratio: f64,
    
    /// Maintenance margin ratio (e.g., 0.05 for 5%).
    #[serde(default)]
    pub maintenance_margin_ratio: f64,
    
    /// Any additional KNF policy fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

/// Bankruptcy policy parameters (eliminates magic numbers).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct BankruptcyPolicy {
    /// Maximum turns an asset can sit in auction pool before nationalization.
    #[serde(default)]
    pub auction_max_turns: u32,
    
    /// Fire-sale discount for initial auction listing (e.g., 0.5 for -50%).
    #[serde(default)]
    pub fire_sale_discount: f64,
    
    /// Rescue nationalization discount (e.g., 0.1 for -90%).
    #[serde(default)]
    pub rescue_nationalization_discount: f64,
    
    /// Privatization queue markup (e.g., 1.0 for book value, 1.2 for +20%).
    #[serde(default)]
    pub privatization_markup: f64,
    
    /// Any additional bankruptcy policy fields.
    #[serde(flatten, default)]
    pub extra: Map<String, serde_json::Value>,
}

impl BankruptcyPolicy {
    /// Create default bankruptcy policy with standard values.
    pub fn with_defaults() -> Self {
        Self {
            auction_max_turns: 4,
            fire_sale_discount: 0.5,
            rescue_nationalization_discount: 0.1,
            privatization_markup: 1.0,
            extra: Map::new(),
        }
    }
}

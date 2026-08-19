//! Shared currency zones (Python `ctx.waluty`).
//!
//! Each entry in `waluty.json` represents a currency union or single national
//! currency.  Currency data is stored on [`crate::state::GameState`] so that the global
//! trade balancer can apply exchange-rate shocks in the two-phase mutation
//! pass.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Exchange-rate policy of a currency zone.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct CurrencyPolicy {
    /// Exchange-rate regime, e.g. `Fluid` or `Fixed`.
    #[serde(default)]
    pub regime: String,
    /// Target exchange rate for a pegged regime.
    #[serde(default)]
    pub target: f64,
    /// Any additional policy fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

/// A currency zone shared by one or more countries.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Currency {
    /// Three-letter currency prefix / code.
    #[serde(default)]
    pub prefix: String,
    /// Exchange rate vs the global numeraire.
    #[serde(default = "default_exchange_rate")]
    pub exchange_rate: f64,
    /// Exchange-rate policy.
    #[serde(default)]
    pub policy: CurrencyPolicy,
    /// Member countries.
    #[serde(default)]
    pub members: Vec<String>,
    /// Quantitative-easing / tightening volume this turn.
    #[serde(default)]
    pub qe_volume: f64,
    /// Last central-bank message.
    #[serde(default)]
    pub last_message: String,
    /// Any additional currency fields.
    #[serde(flatten, default)]
    pub extra: Map<String, Value>,
}

fn default_exchange_rate() -> f64 {
    1.0
}

/// Calculate cross rate between two currencies via IEU.
/// 
/// # Arguments
/// * `from_currency_rate` - Exchange rate of source currency vs IEU
/// * `to_currency_rate` - Exchange rate of target currency vs IEU
/// 
/// # Returns
/// Cross rate (how much target currency = 1 source currency)
/// 
/// # Rules
/// - IEU is the absolute reference point (value = 1.0)
/// - Formula: (from_currency_rate / IEU) * (IEU / to_currency_rate)
/// - Simplified: to_currency_rate / from_currency_rate (INVERTED to prevent infinite wealth glitch)
/// - Prevents combinatorial explosion: N currencies require N rates to IEU, not N*(N-1)/2 cross-pairs
/// 
/// # Example
/// - PLN/IEU = 4.0 (1 IEU = 4 PLN)
/// - USD/IEU = 0.8 (1 IEU = 0.8 USD)
/// - PLN/USD = 0.8 / 4.0 = 0.2 (1 PLN = 0.2 USD) - INVERTED to prevent infinite wealth glitch
pub fn calculate_cross_rate(from_currency_rate: f64, to_currency_rate: f64) -> f64 {
    if from_currency_rate <= 0.0 {
        return f64::INFINITY; // Division by zero protection
    }
    to_currency_rate / from_currency_rate
}

/// Converts an amount from one currency to another via IEU.
/// 
/// # Arguments
/// * `amount` - Amount in source currency
/// * `from_currency_rate` - Exchange rate of source currency vs IEU
/// * `to_currency_rate` - Exchange rate of target currency vs IEU
/// 
/// # Returns
/// Equivalent amount in target currency
/// 
/// # Rules
/// - Formula: amount * (to_currency_rate / from_currency_rate)
/// - Example: 10 PLN * (0.8 / 4) = 2 USD (correct conversion)
pub fn convert_currency(amount: f64, from_currency_rate: f64, to_currency_rate: f64) -> f64 {
    amount * calculate_cross_rate(from_currency_rate, to_currency_rate)
}

//! Phase 57: Centralized VIP trait → market behavior modifier mapping.
//!
//! This module is the SINGLE source of truth for trait → behavior mapping.
//! All market and corporate behavior functions consume [`MarketBehaviorModifiers`],
//! never raw trait strings. No other module may inspect raw trait strings
//! for market/corporate behavior decisions.
//!
//! # Trait → Modifier Mapping
//!
//! | Trait | Modifiers Affected |
//! |-------|--------------------|
//! | Paranoid | `dip_sell_threshold *= 0.33`, `cash_reserve_preference = 0.40` |
//! | Ambitious | `expansion_multiplier *= 1.5`, `max_position_pct *= 2.0`, `leverage_tolerance *= 1.3` |
//! | Corrupt | `fraud_probability = 0.15`, `profit_diversion_rate = 0.05–0.15` |
//! | Charismatic | `share_price_premium = +0.20`, `subscription_attractiveness = 2.0` |
//! | Conservative | `expansion_multiplier *= 0.7`, `pe_buy_threshold *= 0.5`, `turnover_rate *= 0.5` |
//! | Incompetent | `benchmark_tracking_error = 0.05`, `method_switch_aggression *= 0.5` |
//! | Loyal | `turnover_rate *= 0.3`, `dip_sell_threshold = 1.0` (never panic sells) |
//! | Reformer | `method_switch_aggression *= 2.0`, `rd_investment_multiplier *= 1.5` |
//! | Populist | `wage_modifier *= 1.15`, `dividend_payout_modifier *= 0.8` |
//! | Cruel | `wage_modifier *= 0.85` |
//! | Pious | `profit_diversion_rate` donated to charity instead of personal account |
//! | Militarist | Sector preference for Armaments (filter in fund order selection) |
//! | Diplomatic | Reduces union strike risk (handled in labor module) |

use serde::{Deserialize, Serialize};

/// Strongly-typed modifiers derived from VIP traits.
///
/// All market and corporate behavior functions consume this struct,
/// never raw trait strings. This is the centralized bounded-rationality
/// representation of a VIP's market behavior tendencies.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketBehaviorModifiers {
    // ── Risk & Position Sizing ──
    /// Risk tolerance multiplier (1.0 = baseline, >1 = aggressive, <1 = cautious).
    pub risk_tolerance: f64,
    /// Max % of cash per position (baseline 0.10 = 10%).
    pub max_position_pct: f64,
    /// Minimum cash fraction to hold (baseline 0.0).
    pub cash_reserve_preference: f64,

    // ── Trading Triggers ──
    /// Drop % that triggers panic sell (baseline 0.15 = 15%).
    pub dip_sell_threshold: f64,
    /// Turnover rate multiplier (1.0 = normal, <1 = low turnover, >1 = high turnover).
    pub turnover_rate: f64,
    /// Max P/E to buy at (baseline from config, typically 25.0).
    pub pe_buy_threshold: f64,
    /// Min P/E to sell at (baseline from config, typically 5.0).
    pub pe_sell_threshold: f64,

    // ── Corporate Strategy ──
    /// Scales expansion investment (baseline 1.0).
    pub expansion_multiplier: f64,
    /// Max acceptable leverage ratio (baseline 1.0).
    pub leverage_tolerance: f64,
    /// Scales dividend payout (baseline 1.0).
    pub dividend_payout_modifier: f64,
    /// Scales offered wages (baseline 1.0).
    pub wage_modifier: f64,
    /// Scales R&D spending (baseline 1.0).
    pub rd_investment_multiplier: f64,
    /// How aggressively to switch production methods (baseline 1.0).
    pub method_switch_aggression: f64,

    // ── Market Price Effects ──
    /// Premium/discount applied to share price (baseline 0.0).
    pub share_price_premium: f64,

    // ── Fraud & Corruption ──
    /// Per-turn probability of fraud (baseline 0.0).
    pub fraud_probability: f64,
    /// Fraction of profit diverted to personal account (baseline 0.0).
    pub profit_diversion_rate: f64,

    // ── Fund-Specific ──
    /// Multiplier on fund subscription rate (baseline 1.0).
    pub subscription_attractiveness: f64,
    /// Expected underperformance vs benchmark (baseline 0.0).
    pub benchmark_tracking_error: f64,

    // ── Sector Preferences ──
    /// Preferred sector for investment (None = no preference).
    pub preferred_sector: Option<crate::registries::enums::Sector>,

    // ── Special Behaviors ──
    /// If true, diverted profits go to charity instead of personal account (Pious trait).
    pub diverts_to_charity: bool,
}

impl Default for MarketBehaviorModifiers {
    fn default() -> Self {
        MarketBehaviorModifiers {
            // ── Risk & Position Sizing ──
            risk_tolerance: 1.0,
            max_position_pct: 0.10,
            cash_reserve_preference: 0.0,

            // ── Trading Triggers ──
            dip_sell_threshold: 0.15,
            turnover_rate: 1.0,
            pe_buy_threshold: 25.0,
            pe_sell_threshold: 5.0,

            // ── Corporate Strategy ──
            expansion_multiplier: 1.0,
            leverage_tolerance: 1.0,
            dividend_payout_modifier: 1.0,
            wage_modifier: 1.0,
            rd_investment_multiplier: 1.0,
            method_switch_aggression: 1.0,

            // ── Market Price Effects ──
            share_price_premium: 0.0,

            // ── Fraud & Corruption ──
            fraud_probability: 0.0,
            profit_diversion_rate: 0.0,

            // ── Fund-Specific ──
            subscription_attractiveness: 1.0,
            benchmark_tracking_error: 0.0,

            // ── Sector Preferences ──
            preferred_sector: None,

            // ── Special Behaviors ──
            diverts_to_charity: false,
        }
    }
}

/// Centralized evaluation: traits → [`MarketBehaviorModifiers`].
///
/// This is the SINGLE source of truth for trait → behavior mapping.
/// No other module may inspect raw trait strings for market/corporate behavior.
///
/// # Arguments
/// * `traits` - Slice of trait string IDs from a VIP's `traits` field.
///
/// # Returns
/// A [`MarketBehaviorModifiers`] struct with all modifier fields set based on
/// the combined effect of all traits. Multiple traits accumulate multiplicatively.
pub fn evaluate_market_behavior(traits: &[String]) -> MarketBehaviorModifiers {
    let mut mods = MarketBehaviorModifiers::default();

    for trait_id in traits {
        apply_trait(&mut mods, trait_id);
    }

    // Also check main_trait if it's not already in the traits list.
    // (The caller should include main_trait in the traits slice, but we
    // handle the case where it's separate by having the caller pass all traits.)

    mods
}

/// Apply a single trait's modifiers to the accumulator.
///
/// This is the canonical mapping — no other function may perform trait string checks.
fn apply_trait(mods: &mut MarketBehaviorModifiers, trait_id: &str) {
    // Normalize: lowercase, trim for case-insensitive matching.
    let t = trait_id.to_lowercase();

    match t.as_str() {
        "paranoid" => {
            mods.dip_sell_threshold *= 0.33;
            mods.cash_reserve_preference = mods.cash_reserve_preference.max(0.40);
            mods.risk_tolerance *= 0.7;
        }
        "ambitious" => {
            mods.expansion_multiplier *= 1.5;
            mods.max_position_pct *= 2.0;
            mods.leverage_tolerance *= 1.3;
            mods.risk_tolerance *= 1.3;
        }
        "corrupt" => {
            mods.fraud_probability = mods.fraud_probability.max(0.15);
            // Profit diversion rate scales with how corrupt they are.
            mods.profit_diversion_rate = (mods.profit_diversion_rate + 0.10).min(0.15);
        }
        "charismatic" => {
            mods.share_price_premium += 0.20;
            mods.subscription_attractiveness *= 2.0;
        }
        "conservative" => {
            mods.expansion_multiplier *= 0.7;
            mods.pe_buy_threshold *= 0.5;
            mods.turnover_rate *= 0.5;
            mods.risk_tolerance *= 0.8;
            mods.cash_reserve_preference = mods.cash_reserve_preference.max(0.20);
        }
        "incompetent" => {
            mods.benchmark_tracking_error = mods.benchmark_tracking_error.max(0.05);
            mods.method_switch_aggression *= 0.5;
            mods.risk_tolerance *= 0.9;
        }
        "loyal" => {
            mods.turnover_rate *= 0.3;
            // 1.0 means a 100% drop is needed to trigger panic sell — effectively never.
            mods.dip_sell_threshold = 1.0;
        }
        "reformer" => {
            mods.method_switch_aggression *= 2.0;
            mods.rd_investment_multiplier *= 1.5;
        }
        "populist" => {
            mods.wage_modifier *= 1.15;
            mods.dividend_payout_modifier *= 0.8;
        }
        "cruel" => {
            mods.wage_modifier *= 0.85;
        }
        "pious" => {
            // Diverted profits go to charity instead of personal account.
            mods.diverts_to_charity = true;
            // Pious VIPs are less likely to commit fraud.
            mods.fraud_probability *= 0.5;
        }
        "militarist" => {
            mods.preferred_sector = Some(crate::registries::enums::Sector::ArmamentsIndustry);
        }
        "diplomatic" => {
            // Diplomatic trait reduces union strike risk (handled in labor module).
            // No direct market behavior modifier, but slightly reduces turnover.
            mods.turnover_rate *= 0.8;
        }
        _ => {
            // Unknown trait — no modifier applied.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_modifiers() {
        let mods = MarketBehaviorModifiers::default();
        assert_eq!(mods.risk_tolerance, 1.0);
        assert_eq!(mods.max_position_pct, 0.10);
        assert_eq!(mods.dip_sell_threshold, 0.15);
        assert_eq!(mods.fraud_probability, 0.0);
        assert_eq!(mods.expansion_multiplier, 1.0);
    }

    #[test]
    fn test_ambitious_trait() {
        let mods = evaluate_market_behavior(&["Ambitious".to_string()]);
        assert!((mods.expansion_multiplier - 1.5).abs() < 1e-6);
        assert!((mods.max_position_pct - 0.20).abs() < 1e-6);
        assert!((mods.leverage_tolerance - 1.3).abs() < 1e-6);
    }

    #[test]
    fn test_corrupt_trait() {
        let mods = evaluate_market_behavior(&["Corrupt".to_string()]);
        assert!((mods.fraud_probability - 0.15).abs() < 1e-6);
        assert!(mods.profit_diversion_rate > 0.0);
        assert!(mods.profit_diversion_rate <= 0.15);
    }

    #[test]
    fn test_paranoid_trait() {
        let mods = evaluate_market_behavior(&["Paranoid".to_string()]);
        assert!((mods.dip_sell_threshold - 0.15 * 0.33).abs() < 1e-6);
        assert!((mods.cash_reserve_preference - 0.40).abs() < 1e-6);
    }

    #[test]
    fn test_loyal_trait_never_panics() {
        let mods = evaluate_market_behavior(&["Loyal".to_string()]);
        assert_eq!(mods.dip_sell_threshold, 1.0);
        assert!((mods.turnover_rate - 0.3).abs() < 1e-6);
    }

    #[test]
    fn test_conservative_trait() {
        let mods = evaluate_market_behavior(&["Conservative".to_string()]);
        assert!((mods.expansion_multiplier - 0.7).abs() < 1e-6);
        assert!((mods.pe_buy_threshold - 12.5).abs() < 1e-6);
        assert!((mods.turnover_rate - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_charismatic_trait() {
        let mods = evaluate_market_behavior(&["Charismatic".to_string()]);
        assert!((mods.share_price_premium - 0.20).abs() < 1e-6);
        assert!((mods.subscription_attractiveness - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_multiple_traits_accumulate() {
        let mods = evaluate_market_behavior(&[
            "Ambitious".to_string(),
            "Corrupt".to_string(),
        ]);
        // Ambitious: expansion *= 1.5, Corrupt: no expansion change
        assert!((mods.expansion_multiplier - 1.5).abs() < 1e-6);
        // Corrupt: fraud_probability = 0.15
        assert!((mods.fraud_probability - 0.15).abs() < 1e-6);
        // Ambitious: max_position *= 2.0
        assert!((mods.max_position_pct - 0.20).abs() < 1e-6);
    }

    #[test]
    fn test_pious_trait_diverts_to_charity() {
        let mods = evaluate_market_behavior(&["Pious".to_string()]);
        assert!(mods.diverts_to_charity);
        assert!((mods.fraud_probability - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_militarist_sector_preference() {
        let mods = evaluate_market_behavior(&["Militarist".to_string()]);
        assert_eq!(
            mods.preferred_sector,
            Some(crate::registries::enums::Sector::ArmamentsIndustry)
        );
    }

    #[test]
    fn test_unknown_trait_no_effect() {
        let mods = evaluate_market_behavior(&["NonExistentTrait".to_string()]);
        assert_eq!(mods, MarketBehaviorModifiers::default());
    }

    #[test]
    fn test_empty_traits_returns_default() {
        let mods = evaluate_market_behavior(&[]);
        assert_eq!(mods, MarketBehaviorModifiers::default());
    }

    #[test]
    fn test_case_insensitive_matching() {
        let mods_lower = evaluate_market_behavior(&["ambitious".to_string()]);
        let mods_upper = evaluate_market_behavior(&["AMBITIOUS".to_string()]);
        let mods_mixed = evaluate_market_behavior(&["AmBiTiOuS".to_string()]);
        assert_eq!(mods_lower, mods_upper);
        assert_eq!(mods_lower, mods_mixed);
    }
}

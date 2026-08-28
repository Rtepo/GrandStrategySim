//! Geography and demographics configuration (Phase 86.5A).
//!
//! Extracts CRITICAL magic numbers from `society/geography.rs` into a
//! serializable config struct.

use serde::{Deserialize, Serialize};

/// Configuration for geography, demographics, and resource pricing.
///
/// Replaces hardcoded magic numbers in `geography.rs` with configurable values.
/// Seed savings and base resource prices are nominal values that should be
/// scaled by `effective_wage` (clamped to subsistence) at usage sites.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeographyConfig {
    // ── Savings Seeds ──
    /// Free peasant savings seed (fiat per capita, scaled by effective_wage).
    #[serde(default = "default_free_peasant_savings_seed")]
    pub free_peasant_savings_seed: f64,

    /// Middle class savings seed (fiat per capita, scaled by effective_wage).
    #[serde(default = "default_middle_class_savings_seed")]
    pub middle_class_savings_seed: f64,

    // ── Resource Base Prices ──
    /// Base price for rock/stone resources (fiat per ton, scaled by effective_wage).
    #[serde(default = "default_rock_base_price")]
    pub rock_base_price: f64,

    /// Default base price for unmapped resources (fiat per ton).
    #[serde(default = "default_resource_fallback_price")]
    pub resource_fallback_price: f64,

    /// Geological reserve multiplier (reserves = gdp * multiplier * 1000).
    #[serde(default = "default_geological_reserve_multiplier")]
    pub geological_reserve_multiplier: f64,

    /// Geological reserve unit scale (e.g., 1000.0 for per-thousand GDP).
    #[serde(default = "default_geological_reserve_scale")]
    pub geological_reserve_scale: f64,

    // ── Economic Status Thresholds ──
    /// GDP per capita threshold for Prosperous status.
    #[serde(default = "default_prosperous_gdp_threshold")]
    pub prosperous_gdp_threshold: f64,

    /// GDP per capita threshold for Struggling status.
    #[serde(default = "default_struggling_gdp_threshold")]
    pub struggling_gdp_threshold: f64,

    // ── Distance ──
    /// Default distance for land borders (km).
    #[serde(default = "default_land_border_distance")]
    pub land_border_distance: f64,

    // ── Resource Extraction ──
    /// Minimum extraction cost (fiat per ton).
    #[serde(default = "default_min_extraction_cost")]
    pub min_extraction_cost: f64,

    /// Maximum extraction cost (fiat per ton).
    #[serde(default = "default_max_extraction_cost")]
    pub max_extraction_cost: f64,

    // ── Construction ──
    /// Hectares cleared per turn during land preparation.
    #[serde(default = "default_hectares_per_turn")]
    pub hectares_per_turn: f64,

    // ── Mental Health ──
    /// Baseline mental health (0-100 scale).
    #[serde(default = "default_baseline_mental_health")]
    pub baseline_mental_health: f64,
}

fn default_free_peasant_savings_seed() -> f64 { 100.0 }
fn default_middle_class_savings_seed() -> f64 { 1000.0 }
fn default_rock_base_price() -> f64 { 100.0 }
fn default_resource_fallback_price() -> f64 { 100.0 }
fn default_geological_reserve_multiplier() -> f64 { 1.0 }
fn default_geological_reserve_scale() -> f64 { 1000.0 }
fn default_prosperous_gdp_threshold() -> f64 { 1000.0 }
fn default_struggling_gdp_threshold() -> f64 { 100.0 }
fn default_land_border_distance() -> f64 { 100.0 }
fn default_min_extraction_cost() -> f64 { 10.0 }
fn default_max_extraction_cost() -> f64 { 100.0 }
fn default_hectares_per_turn() -> f64 { 1000.0 }
fn default_baseline_mental_health() -> f64 { 70.0 }

impl Default for GeographyConfig {
    fn default() -> Self {
        GeographyConfig {
            free_peasant_savings_seed: default_free_peasant_savings_seed(),
            middle_class_savings_seed: default_middle_class_savings_seed(),
            rock_base_price: default_rock_base_price(),
            resource_fallback_price: default_resource_fallback_price(),
            geological_reserve_multiplier: default_geological_reserve_multiplier(),
            geological_reserve_scale: default_geological_reserve_scale(),
            prosperous_gdp_threshold: default_prosperous_gdp_threshold(),
            struggling_gdp_threshold: default_struggling_gdp_threshold(),
            land_border_distance: default_land_border_distance(),
            min_extraction_cost: default_min_extraction_cost(),
            max_extraction_cost: default_max_extraction_cost(),
            hectares_per_turn: default_hectares_per_turn(),
            baseline_mental_health: default_baseline_mental_health(),
        }
    }
}

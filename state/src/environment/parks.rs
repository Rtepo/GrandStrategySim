//! Phase 18E: Urban park pollution reduction, happiness boost, and
//! ecological tax assessment based on pollution proximity.
//!
//! ## Urban Park Effects
//!
//! Urban parks provide localized environmental benefits:
//! - **Pollution reduction**: Trees and vegetation absorb particulate matter
//!   and other pollutants. The reduction scales by park area and ecological
//!   health.
//! - **Happiness boost**: Access to green space improves citizen well-being.
//!   The boost scales by visitor count and park quality.
//!
//! ## Ecological Tax Assessment
//!
//! Industrial firms near protected areas pay ecological taxes proportional
//! to their pollution output and proximity to the protected zone. This
//! implements the "polluter pays" principle:
//! - Firms closer to protected areas pay higher taxes
//! - Firms with higher pollution output pay higher taxes
//! - Tax revenue funds the adjacent protected area

use crate::politics::conservation::{BufferZone, UrbanPark};
use serde::{Deserialize, Serialize};

/// Phase 18E: Configuration for park environment effects.
///
/// All multipliers are configuration-driven (Rule 2: no magic numbers).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParkEnvironmentConfig {
    /// Pollution absorption rate per hectare of urban park (0.0-1.0).
    /// Higher = more pollution absorbed per hectare.
    #[serde(default = "default_pollution_absorption_rate")]
    pub pollution_absorption_rate: f64,

    /// Happiness boost per visitor (0.0-1.0).
    /// Scales by average_wage to remain inflation-proof.
    #[serde(default = "default_happiness_boost_per_visitor")]
    pub happiness_boost_per_visitor: f64,

    /// Maximum pollution reduction per urban park (caps absorption).
    #[serde(default = "default_max_pollution_reduction")]
    pub max_pollution_reduction: f64,

    /// Ecological tax base rate per hectare of industrial land.
    /// Scales by average_wage for inflation-proofness.
    #[serde(default = "default_ecological_tax_base_rate")]
    pub ecological_tax_base_rate: f64,

    /// Pollution proximity multiplier: firms closer to protected areas
    /// pay proportionally more tax. 1.0 = no proximity effect.
    #[serde(default = "default_pollution_proximity_multiplier")]
    pub pollution_proximity_multiplier: f64,

    /// Buffer zone pollution threshold above which ecological tax applies.
    #[serde(default = "default_buffer_pollution_threshold")]
    pub buffer_pollution_threshold: f64,
}

fn default_pollution_absorption_rate() -> f64 { 0.05 }
fn default_happiness_boost_per_visitor() -> f64 { 0.01 }
fn default_max_pollution_reduction() -> f64 { 10.0 }
fn default_ecological_tax_base_rate() -> f64 { 0.002 }
fn default_pollution_proximity_multiplier() -> f64 { 2.0 }
fn default_buffer_pollution_threshold() -> f64 { 0.3 }

impl Default for ParkEnvironmentConfig {
    fn default() -> Self {
        Self {
            pollution_absorption_rate: default_pollution_absorption_rate(),
            happiness_boost_per_visitor: default_happiness_boost_per_visitor(),
            max_pollution_reduction: default_max_pollution_reduction(),
            ecological_tax_base_rate: default_ecological_tax_base_rate(),
            pollution_proximity_multiplier: default_pollution_proximity_multiplier(),
            buffer_pollution_threshold: default_buffer_pollution_threshold(),
        }
    }
}

/// Phase 18E: Apply urban park pollution reduction to a region's smog level.
///
/// Urban parks absorb pollution proportional to their area and ecological
/// health. The reduction is subtracted from the region's smog_level.
///
/// # Arguments
/// * `smog_level` - Current smog level (0.0-100.0)
/// * `urban_parks` - All urban parks in the region
/// * `config` - Park environment configuration
///
/// # Returns
/// New smog level after pollution reduction (clamped at 0.0)
pub fn apply_urban_park_pollution_reduction(
    smog_level: f64,
    urban_parks: &[UrbanPark],
    config: &ParkEnvironmentConfig,
) -> f64 {
    let total_reduction: f64 = urban_parks
        .iter()
        .map(|park| {
            let reduction = park.pollution_reduction_factor
                * park.total_area
                * park.ecological_health
                * config.pollution_absorption_rate;
            reduction.min(config.max_pollution_reduction)
        })
        .sum();

    (smog_level - total_reduction).max(0.0)
}

/// Phase 18E: Apply urban park happiness boost to a region.
///
/// Returns the aggregate happiness boost from all urban parks in a region.
/// The boost scales by visitor count and park ecological health.
///
/// # Arguments
/// * `urban_parks` - All urban parks in the region
/// * `average_wage` - Current average wage (for inflation-proof scaling)
/// * `config` - Park environment configuration
///
/// # Returns
/// Total happiness boost (additive to region happiness, 0.0+)
pub fn apply_urban_park_happiness_boost(
    urban_parks: &[UrbanPark],
    average_wage: f64,
    config: &ParkEnvironmentConfig,
) -> f64 {
    urban_parks
        .iter()
        .map(|park| {
            let visitor_count = park.last_turn_visitor_count;
            let boost = config.happiness_boost_per_visitor
                * visitor_count
                * park.ecological_health;
            // Scale by average_wage fraction (inflation-proof)
            boost * (average_wage / 1000.0).max(0.1).min(10.0)
        })
        .sum()
}

/// Phase 18E: Assess ecological tax for a buffer zone based on pollution
/// proximity.
///
/// The ecological tax scales by:
/// - Industrial area in the buffer zone
/// - Current pollution level (polluter pays)
/// - Average wage (inflation-proof)
/// - Proximity multiplier (firms closer to protected areas pay more)
///
/// # Arguments
/// * `buffer_zone` - Buffer zone with industrial area and pollution data
/// * `average_wage` - Current average wage
/// * `config` - Park environment configuration
///
/// # Returns
/// Ecological tax amount (in currency units)
pub fn assess_ecological_tax_by_pollution_proximity(
    buffer_zone: &BufferZone,
    average_wage: f64,
    config: &ParkEnvironmentConfig,
) -> f64 {
    if buffer_zone.industrial_area <= 0.0 {
        return 0.0;
    }

    // Base tax: industrial area × base rate × average_wage
    let base_tax = buffer_zone.industrial_area
        * config.ecological_tax_base_rate
        * average_wage.max(1.0);

    // Pollution multiplier: higher pollution = higher tax
    let pollution_factor = if buffer_zone.pollution_level > config.buffer_pollution_threshold {
        1.0 + (buffer_zone.pollution_level - config.buffer_pollution_threshold)
            * config.pollution_proximity_multiplier
    } else {
        1.0
    };

    base_tax * pollution_factor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urban_park_pollution_reduction() {
        let config = ParkEnvironmentConfig::default();
        let park = UrbanPark {
            id: "test".to_string(),
            name: "Test Park".to_string(),
            country: "TestCountry".to_string(),
            region_id: "reg1".to_string(),
            micro_region_id: "micro1".to_string(),
            total_area: 10.0,
            ecological_health: 0.8,
            management_cost: 0.2,
            visitor_capacity: 100.0,
            last_turn_visitor_count: 80.0,
            entry_fee_per_visitor: 0.0,
            pollution_reduction_factor: 0.05,
            happiness_boost_per_visitor: 0.01,
            annexed_parcel_ids: Vec::new(),
            funding_balance: 0.0,
        };

        let new_smog = apply_urban_park_pollution_reduction(50.0, &[park], &config);
        assert!(new_smog < 50.0, "Pollution should decrease");
        assert!(new_smog >= 0.0, "Pollution should not go negative");
    }

    #[test]
    fn test_urban_park_happiness_boost() {
        let config = ParkEnvironmentConfig::default();
        let park = UrbanPark {
            id: "test".to_string(),
            name: "Test Park".to_string(),
            country: "TestCountry".to_string(),
            region_id: "reg1".to_string(),
            micro_region_id: "micro1".to_string(),
            total_area: 10.0,
            ecological_health: 0.8,
            management_cost: 0.2,
            visitor_capacity: 100.0,
            last_turn_visitor_count: 80.0,
            entry_fee_per_visitor: 0.0,
            pollution_reduction_factor: 0.05,
            happiness_boost_per_visitor: 0.01,
            annexed_parcel_ids: Vec::new(),
            funding_balance: 0.0,
        };

        let boost = apply_urban_park_happiness_boost(&[park], 1000.0, &config);
        assert!(boost > 0.0, "Happiness boost should be positive");
    }

    #[test]
    fn test_ecological_tax_zero_industrial_area() {
        let config = ParkEnvironmentConfig::default();
        let buffer = BufferZone {
            id: "bz1".to_string(),
            name: "Test Buffer".to_string(),
            country: "TestCountry".to_string(),
            region_id: "reg1".to_string(),
            total_area: 100.0,
            industrial_area: 0.0,
            ecological_tax_per_hectare: 0.0,
            protected_area_id: "np1".to_string(),
            protected_area_type: "national_park".to_string(),
            parcel_ids: Vec::new(),
            pollution_level: 0.5,
        };

        let tax = assess_ecological_tax_by_pollution_proximity(&buffer, 1000.0, &config);
        assert_eq!(tax, 0.0, "Tax should be zero with no industrial area");
    }

    #[test]
    fn test_ecological_tax_polluter_pays() {
        let config = ParkEnvironmentConfig::default();
        let buffer_low_pollution = BufferZone {
            id: "bz1".to_string(),
            name: "Low Pollution Buffer".to_string(),
            country: "TestCountry".to_string(),
            region_id: "reg1".to_string(),
            total_area: 100.0,
            industrial_area: 50.0,
            ecological_tax_per_hectare: 0.0,
            protected_area_id: "np1".to_string(),
            protected_area_type: "national_park".to_string(),
            parcel_ids: Vec::new(),
            pollution_level: 0.1, // Below threshold
        };

        let buffer_high_pollution = BufferZone {
            id: "bz2".to_string(),
            name: "High Pollution Buffer".to_string(),
            country: "TestCountry".to_string(),
            region_id: "reg1".to_string(),
            total_area: 100.0,
            industrial_area: 50.0,
            ecological_tax_per_hectare: 0.0,
            protected_area_id: "np1".to_string(),
            protected_area_type: "national_park".to_string(),
            parcel_ids: Vec::new(),
            pollution_level: 0.8, // Above threshold
        };

        let tax_low = assess_ecological_tax_by_pollution_proximity(
            &buffer_low_pollution,
            1000.0,
            &config,
        );
        let tax_high = assess_ecological_tax_by_pollution_proximity(
            &buffer_high_pollution,
            1000.0,
            &config,
        );

        assert!(
            tax_high > tax_low,
            "High pollution should pay more tax (polluter pays)"
        );
    }
}

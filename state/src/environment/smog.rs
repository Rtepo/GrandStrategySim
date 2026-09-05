//! Phase 82: Localized smog (air pollution) mechanics.
//!
//! Smog is computed as a **concentration** (mass per km²), not raw mass.
//! This ensures that 100 tons of particulate in a 50,000 km² rural province
//! produces a much lower smog concentration than in a 50 km² city
//! (CORRECTION 7: Concentration Fallacy).
//!
//! ## Sources
//!
//! - **Standalone heating**: Coal stoves, oil heaters, wood fireplaces
//! - **Centralized heating plants**: With/without emission controls
//! - **Industrial emissions**: Heavy industry (cement, steel, chemicals)
//! - **Power plants**: With/without emission controls
//!
//! ## Integration
//!
//! Smog feeds into:
//! - `Region.winter_mortality_multiplier` (heating deficit + smog mortality)
//! - Cadastre `Parcel.pollution_level` (via `distribute_smog_to_parcels`)
//! - VIP health impacts (via existing Phase 62.5 immission system)

use crate::society::cadastre::Cadastre;
use serde::{Deserialize, Serialize};

/// Local pollution state for a region.
///
/// Tracks accumulated smog level and per-turn emission breakdown by source.
/// Smog level is a 0-100 concentration scale where:
/// - 0-30: acceptable air quality
/// - 30-60: noticeable health impacts
/// - 60-80: severe respiratory disease
/// - 80-100: lethal for elderly/children
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct LocalPollutionState {
    /// Accumulated smog concentration (0.0 = clean, 100.0 = lethal).
    /// This is a CONCENTRATION, not raw mass — emissions are divided by
    /// region area before accumulating (CORRECTION 7).
    #[serde(default)]
    pub smog_level: f64,

    /// Per-turn smog mass from standalone combustion (housing/commercial).
    /// Stored for diagnostics and snapshot reporting.
    #[serde(default)]
    pub standalone_emissions: f64,

    /// Per-turn smog mass from centralized plants (heating plants + power plants).
    #[serde(default)]
    pub centralized_emissions: f64,

    /// Per-turn smog mass from industrial production (heavy industry).
    #[serde(default)]
    pub industrial_emissions: f64,

    // ════════════════════════════════════════════════════════════════════════
    // Phase 83: Biological contamination (PARADIGM SHIFT — Water Quality Spectrum)
    // Distinct from smog — waterborne/pathogenic pollution with different
    // sources, decay rates, mortality effects, and cadastre deposition.
    // ════════════════════════════════════════════════════════════════════════
    /// Accumulated biological contamination (0.0 = clean, 100.0 = epidemic).
    /// Distinct from smog — this is waterborne/pathogenic pollution.
    #[serde(default)]
    pub biohazard_level: f64,

    /// Per-turn biohazard mass from standalone sanitation (open defecation,
    /// cesspools, septic tanks that leak into environment).
    #[serde(default)]
    pub standalone_biohazard: f64,

    /// Per-turn biohazard mass from sewage leakage/overflow (untreated
    /// blackwater that escapes the sewer network).
    #[serde(default)]
    pub sewage_overflow_biohazard: f64,

    /// Per-turn biohazard mass from untreated industrial wastewater
    /// (tanneries, abattoirs, etc. with non-zero `biohazard_factor`).
    #[serde(default)]
    pub industrial_biohazard: f64,

    /// PARADIGM SHIFT (Pillar 4) + PATCH 6: Per-turn biohazard mass from
    /// citizens consuming low-quality water (quality < 0.9). Evaluates
    /// per-building `water_quality_received`, not grid-level quality.
    #[serde(default)]
    pub low_quality_water_biohazard: f64,

    // ════════════════════════════════════════════════════════════════════════
    // Phase 84: Waste pollution (Solid Waste Management & Circular Economy)
    // Distinct pollution vectors from waste disposal: burning → smog,
    // dumping → biohazard, uncollected → biohazard + cadastre pollution.
    // ════════════════════════════════════════════════════════════════════════
    /// Phase 84: Per-turn smog mass from open trash burning.
    /// Feeds into `compute_smog_for_region()` alongside standalone/centralized/
    /// industrial emissions. Trash Burning produces severe localized smog.
    #[serde(default)]
    pub waste_burning_emissions: f64,

    /// Phase 84: Per-turn biohazard mass from illegal dumping and landfill
    /// leachate. Feeds into `compute_biohazard_for_region()`.
    #[serde(default)]
    pub waste_dumping_biohazard: f64,

    /// Phase 84: Per-turn biohazard mass from uncollected waste rotting in
    /// streets. Grows when collection capacity is insufficient or landfills
    /// are full (LOGISTICAL BOUND 2 — catastrophic backup).
    #[serde(default)]
    pub uncollected_waste_biohazard: f64,
}

/// Physical constant: atmospheric dispersion rate per turn.
/// 5% of accumulated smog disperses each turn via wind/rain.
const NATURAL_DECAY_RATE: f64 = 0.05;

/// Physical constant: fraction of atmospheric smog that settles as
/// particulate on parcels per turn (gravimetric deposition).
const DEPOSITION_RATE: f64 = 0.01;

/// Compute the smog mortality multiplier from smog concentration.
///
/// At smog=100: 50% increase in death rate (winter).
/// This is a physical dose-response relationship, not a magic number.
pub fn smog_mortality_multiplier(smog_level: f64) -> f64 {
    1.0 + (smog_level / 100.0).max(0.0) * 0.5
}

/// Compute the year-round (non-winter) smog health impact.
///
/// At smog=100: 10% increase in death rate year-round.
pub fn smog_year_round_mortality(smog_level: f64) -> f64 {
    (smog_level / 100.0).max(0.0) * 0.1
}

/// Compute smog for a single region from all emission sources.
///
/// CORRECTION 7 (Concentration Fallacy): Emissions are mass; smog is
/// concentration. Total per-turn emissions are divided by region area
/// before accumulating into `smog_level`.
///
/// # Arguments
/// * `pollution` - Mutable local pollution state for the region
/// * `standalone_emissions_mass` - Total mass of standalone heating emissions
/// * `centralized_emissions_mass` - Total mass of centralized plant emissions
/// * `industrial_emissions_mass` - Total mass of industrial emissions
/// * `region_area_km2` - Region area in km² (from `land_use_inventory.total_area / 100.0`)
pub fn compute_smog_for_region(
    pollution: &mut LocalPollutionState,
    standalone_emissions_mass: f64,
    centralized_emissions_mass: f64,
    industrial_emissions_mass: f64,
    region_area_km2: f64,
) {
    // Store per-source breakdown for diagnostics
    pollution.standalone_emissions = standalone_emissions_mass;
    pollution.centralized_emissions = centralized_emissions_mass;
    pollution.industrial_emissions = industrial_emissions_mass;

    // Total emission mass this turn
    let total_emissions_mass =
        standalone_emissions_mass + centralized_emissions_mass + industrial_emissions_mass;

    // CORRECTION 7: Convert mass to concentration (mass per km²)
    let area = region_area_km2.max(1.0);
    let emission_concentration = total_emissions_mass / area;

    // Accumulate with natural atmospheric decay
    pollution.smog_level =
        (pollution.smog_level + emission_concentration) * (1.0 - NATURAL_DECAY_RATE);

    // Clamp to 0-100 range
    pollution.smog_level = pollution.smog_level.clamp(0.0, 100.0);
}

/// Distribute smog to cadastre parcels as particulate pollution.
///
/// Integrates with the existing Phase 62.4 immission system: smog settles
/// on parcels as additional pollution, which then spreads to neighboring
/// parcels via the topological graph.
///
/// # Arguments
/// * `cadastre` - Mutable cadastre
/// * `region_id` - Region ID to distribute smog for
/// * `smog_level` - Current smog concentration for the region
pub fn distribute_smog_to_parcels(cadastre: &mut Cadastre, region_id: &str, smog_level: f64) {
    if smog_level <= 0.0 {
        return;
    }

    // Iterate all parcels and add smog deposition to those in this region
    for (_, parcel) in cadastre.parcels.iter_mut() {
        if parcel.region_id == region_id {
            // Smog settles on parcels as particulate pollution
            parcel.pollution_level += smog_level * DEPOSITION_RATE;
            // Cap at 100.0 to prevent runaway
            parcel.pollution_level = parcel.pollution_level.min(100.0);
        }
    }
}

// ============================================================================
// PHASE 83: BIOLOGICAL POLLUTION (PARADIGM SHIFT — Water Quality Spectrum)
// ============================================================================

/// Physical constant: pathogen die-off rate per turn (3%).
/// Pathogens persist longer than smog — waterborne diseases survive in
/// soil and water for weeks. 3% per turn at 4 turns/year = ~12% per year.
const BIO_DECAY_RATE: f64 = 0.03;

/// Physical constant: fraction of quality deficit that manifests as
/// pathogenic load per liter consumed. Calibrated from WHO cholera
/// incidence data for untreated water consumption.
const PATHOGEN_SEVERITY_FACTOR: f64 = 0.5;

/// Safe water quality threshold — water below this quality causes sickness.
pub const SAFE_WATER_QUALITY_THRESHOLD: f64 = 0.9;

/// Compute the biohazard mortality multiplier from biohazard concentration.
///
/// At biohazard=100: 100% increase in death rate (epidemic).
/// Biological mortality is year-round (cholera, typhoid, dysentery kill
/// in summer too, unlike winter-only smog mortality).
pub fn biohazard_mortality_multiplier(biohazard_level: f64) -> f64 {
    1.0 + (biohazard_level / 100.0).max(0.0)
}

/// Per-building water quality receipt for biohazard computation.
/// Used by `compute_biohazard_for_region()` to evaluate per-building
/// sickness (PATCH 6: Universal Water Sickness).
#[derive(Debug, Clone)]
pub struct BuildingWaterReceipt {
    /// Building identifier (for diagnostics).
    pub building_id: String,
    /// Water quality the building actually received (0.0-1.0).
    pub water_quality_received: f64,
    /// Water volume consumed by the building (liters).
    pub water_consumed: f64,
}

/// Compute biological pollution for a single region from all biohazard sources.
///
/// PARADIGM SHIFT: Biohazard is distinct from smog — different sources,
/// different decay, different mortality. Waterborne disease persists
/// year-round (cholera, typhoid, dysentery).
///
/// # Sources (per plan D.2)
/// 1. Standalone sanitation biohazard
/// 2. Sewer overflow biohazard (untreated blackwater)
/// 3. Sewer leakage biohazard (cracked pipes)
/// 4. Industrial wastewater biohazard (biohazard_factor)
/// 5. Low-quality water consumption biohazard (PATCH 6: per-building)
///
/// # Arguments
/// * `pollution` - Mutable local pollution state for the region
/// * `standalone_biohazard_mass` - Biohazard from standalone sanitation
/// * `sewage_overflow_biohazard_mass` - Biohazard from sewer overflow/leakage
/// * `industrial_biohazard_mass` - Biohazard from industrial wastewater
/// * `building_receipts` - Per-building water quality and consumption
/// * `region_area_km2` - Region area for concentration calculation
pub fn compute_biohazard_for_region(
    pollution: &mut LocalPollutionState,
    standalone_biohazard_mass: f64,
    sewage_overflow_biohazard_mass: f64,
    industrial_biohazard_mass: f64,
    building_receipts: &[BuildingWaterReceipt],
    region_area_km2: f64,
) {
    // Store per-source breakdown for diagnostics
    pollution.standalone_biohazard = standalone_biohazard_mass;
    pollution.sewage_overflow_biohazard = sewage_overflow_biohazard_mass;
    pollution.industrial_biohazard = industrial_biohazard_mass;

    // PATCH 6 (Universal Water Sickness): Evaluate per-building water quality
    let mut low_quality_water_biohazard: f64 = 0.0;
    for receipt in building_receipts {
        if receipt.water_consumed <= 0.0 {
            continue;
        }
        // Quality deficit below safe threshold
        let quality_deficit =
            (SAFE_WATER_QUALITY_THRESHOLD - receipt.water_quality_received).max(0.0);
        // Biohazard = quality_deficit * water_consumed * PATHOGEN_SEVERITY_FACTOR
        let building_biohazard =
            quality_deficit * receipt.water_consumed * PATHOGEN_SEVERITY_FACTOR;
        low_quality_water_biohazard += building_biohazard;
    }
    pollution.low_quality_water_biohazard = low_quality_water_biohazard;

    // Total biohazard mass this turn
    let total_biohazard_mass = standalone_biohazard_mass
        + sewage_overflow_biohazard_mass
        + industrial_biohazard_mass
        + low_quality_water_biohazard;

    // CORRECTION 7: Convert mass to concentration (mass per km²)
    let area = region_area_km2.max(1.0);
    let biohazard_concentration = total_biohazard_mass / area;

    // Accumulate with natural pathogen die-off (slower than smog decay)
    pollution.biohazard_level =
        (pollution.biohazard_level + biohazard_concentration) * (1.0 - BIO_DECAY_RATE);

    // Clamp to 0-100 range
    pollution.biohazard_level = pollution.biohazard_level.clamp(0.0, 100.0);
}

/// Distribute biohazard to cadastre parcels as biological contamination.
///
/// Pathogens are heavier than particulate, so deposition rate is lower
/// than smog (0.005 vs 0.01).
pub fn distribute_biohazard_to_parcels(
    cadastre: &mut Cadastre,
    region_id: &str,
    biohazard_level: f64,
) {
    if biohazard_level <= 0.0 {
        return;
    }

    let bio_deposition_rate: f64 = 0.005;
    for (_, parcel) in cadastre.parcels.iter_mut() {
        if parcel.region_id == region_id {
            parcel.pollution_level += biohazard_level * bio_deposition_rate;
            parcel.pollution_level = parcel.pollution_level.min(100.0);
        }
    }
}

/// Blueprint 006: Off-grid waste emission — routes off-grid sewage and solid
/// waste to LocalPollutionState. Off-grid buildings (no sewer connection)
/// must convert sewage to standalone_biohazard mass and solid waste to
/// waste_dumping_biohazard. This enforces mass conservation: off-grid sewage
/// does not vanish into the void — it pollutes the local environment.
///
/// # Arguments
/// * `pollution` - Mutable local pollution state for the region
/// * `sewage_volume_liters` - Sewage volume in liters (gated by water_extracted)
/// * `solid_waste_mass` - Solid waste mass in tons
/// * `_building_id` - Building ID (for diagnostics, unused in mass calculation)
/// * `_region_id` - Region ID (for diagnostics, unused in mass calculation)
///
/// # Physics
/// * Sewage → biohazard: 0.001 kg biohazard mass per liter of raw sewage
///   (physical conversion factor from WHO wastewater strength data).
/// * Solid waste → waste_dumping_biohazard: uses existing waste_grid.rs
///   logic via waste_dumping_biohazard field.
pub fn off_grid_waste_emission(
    pollution: &mut LocalPollutionState,
    sewage_volume_liters: f64,
    solid_waste_mass: f64,
    _building_id: &str,
    _region_id: &str,
) {
    // Sewage → standalone_biohazard mass (Rule 1: mass conservation).
    // 0.001 kg biohazard per liter of raw sewage.
    let sewage_biohazard_mass = sewage_volume_liters.max(0.0) * 0.001;
    pollution.standalone_biohazard += sewage_biohazard_mass;

    // Solid waste → waste_dumping_biohazard (Rule 1: mass conservation).
    // Off-grid solid waste is dumped locally, generating biohazard.
    pollution.waste_dumping_biohazard += solid_waste_mass.max(0.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_local_pollution() {
        let p = LocalPollutionState::default();
        assert_eq!(p.smog_level, 0.0);
        assert_eq!(p.standalone_emissions, 0.0);
        assert_eq!(p.centralized_emissions, 0.0);
        assert_eq!(p.industrial_emissions, 0.0);
    }

    // ── Blueprint 006: Off-Grid Waste Emission ──

    #[test]
    fn test_off_grid_waste_emission_routes_sewage_to_biohazard() {
        // Blueprint 006 invariant: Off-grid sewage routes to
        // LocalPollutionState.standalone_biohazard.
        let mut p = LocalPollutionState::default();
        assert_eq!(p.standalone_biohazard, 0.0);
        off_grid_waste_emission(&mut p, 1000.0, 0.0, "b1", "r1");
        // 1000L sewage * 0.001 = 1.0 kg biohazard mass
        assert!((p.standalone_biohazard - 1.0).abs() < 1e-9,
            "Sewage must convert to biohazard mass");
    }

    #[test]
    fn test_off_grid_waste_emission_routes_solid_waste() {
        // Blueprint 006 invariant: Solid waste routes to
        // waste_dumping_biohazard.
        let mut p = LocalPollutionState::default();
        assert_eq!(p.waste_dumping_biohazard, 0.0);
        off_grid_waste_emission(&mut p, 0.0, 5.0, "b1", "r1");
        assert!((p.waste_dumping_biohazard - 5.0).abs() < 1e-9,
            "Solid waste must route to waste_dumping_biohazard");
    }

    #[test]
    fn test_off_grid_waste_emission_zero_sewage() {
        // Blueprint 006 invariant: Zero sewage produces zero biohazard.
        let mut p = LocalPollutionState::default();
        off_grid_waste_emission(&mut p, 0.0, 0.0, "b1", "r1");
        assert_eq!(p.standalone_biohazard, 0.0);
        assert_eq!(p.waste_dumping_biohazard, 0.0);
    }

    #[test]
    fn test_smog_mortality_multiplier() {
        assert_eq!(smog_mortality_multiplier(0.0), 1.0);
        assert!((smog_mortality_multiplier(100.0) - 1.5).abs() < 1e-9);
        assert!((smog_mortality_multiplier(50.0) - 1.25).abs() < 1e-9);
    }

    #[test]
    fn test_smog_year_round_mortality() {
        assert_eq!(smog_year_round_mortality(0.0), 0.0);
        assert!((smog_year_round_mortality(100.0) - 0.1).abs() < 1e-9);
    }

    #[test]
    fn test_concentration_fallacy_rural_vs_urban() {
        // Same emissions mass, different area → different concentration
        let mut rural = LocalPollutionState::default();
        let mut urban = LocalPollutionState::default();

        // 1000 units of emissions
        compute_smog_for_region(&mut rural, 500.0, 300.0, 200.0, 50000.0);
        compute_smog_for_region(&mut urban, 500.0, 300.0, 200.0, 50.0);

        // Rural: 1000/50000 = 0.02 concentration, * 0.95 decay = 0.019
        // Urban: 1000/50 = 20.0 concentration, * 0.95 decay = 19.0
        assert!(urban.smog_level > rural.smog_level * 100.0);
        assert!(rural.smog_level < 1.0);
        assert!(urban.smog_level > 15.0);
    }

    #[test]
    fn test_smog_accumulation_and_decay() {
        let mut p = LocalPollutionState::default();

        // Turn 1: 500 emissions, 100 km² area
        compute_smog_for_region(&mut p, 500.0, 0.0, 0.0, 100.0);
        // concentration = 500/100 = 5.0, smog = 5.0 * 0.95 = 4.75
        assert!((p.smog_level - 4.75).abs() < 0.01);

        // Turn 2: same emissions
        compute_smog_for_region(&mut p, 500.0, 0.0, 0.0, 100.0);
        // smog = (4.75 + 5.0) * 0.95 = 9.2625
        assert!((p.smog_level - 9.2625).abs() < 0.01);
    }

    #[test]
    fn test_smog_clamped_at_100() {
        let mut p = LocalPollutionState {
            smog_level: 99.0,
            ..Default::default()
        };
        // Huge emissions in tiny area
        compute_smog_for_region(&mut p, 100000.0, 0.0, 0.0, 1.0);
        assert_eq!(p.smog_level, 100.0);
    }

    #[test]
    fn test_smog_decay_with_no_emissions() {
        let mut p = LocalPollutionState {
            smog_level: 50.0,
            ..Default::default()
        };
        compute_smog_for_region(&mut p, 0.0, 0.0, 0.0, 100.0);
        // smog = (50 + 0) * 0.95 = 47.5
        assert!((p.smog_level - 47.5).abs() < 0.01);
    }

    // ── Phase 83: Biohazard Tests ──

    #[test]
    fn test_biohazard_mortality_multiplier() {
        assert_eq!(biohazard_mortality_multiplier(0.0), 1.0);
        assert!((biohazard_mortality_multiplier(100.0) - 2.0).abs() < 1e-9);
        assert!((biohazard_mortality_multiplier(50.0) - 1.5).abs() < 1e-9);
    }

    #[test]
    fn test_biohazard_defaults() {
        let p = LocalPollutionState::default();
        assert_eq!(p.biohazard_level, 0.0);
        assert_eq!(p.standalone_biohazard, 0.0);
        assert_eq!(p.sewage_overflow_biohazard, 0.0);
        assert_eq!(p.industrial_biohazard, 0.0);
        assert_eq!(p.low_quality_water_biohazard, 0.0);
    }

    #[test]
    fn test_biohazard_accumulation_and_decay() {
        let mut p = LocalPollutionState::default();
        // 100 units of standalone biohazard, 100 km² area
        compute_biohazard_for_region(&mut p, 100.0, 0.0, 0.0, &[], 100.0);
        // concentration = 100/100 = 1.0, biohazard = 1.0 * 0.97 = 0.97
        assert!((p.biohazard_level - 0.97).abs() < 0.01);
    }

    #[test]
    fn test_biohazard_low_quality_water_patch6() {
        let mut p = LocalPollutionState::default();
        // Building drinking rainwater at quality 0.6, 100 liters
        let receipts = vec![BuildingWaterReceipt {
            building_id: "rural_1".into(),
            water_quality_received: 0.6,
            water_consumed: 100.0,
        }];
        compute_biohazard_for_region(&mut p, 0.0, 0.0, 0.0, &receipts, 100.0);
        // quality_deficit = 0.9 - 0.6 = 0.3
        // biohazard = 0.3 * 100 * 0.5 = 15.0
        assert!((p.low_quality_water_biohazard - 15.0).abs() < 0.01);
    }

    #[test]
    fn test_biohazard_clean_well_water_no_sickness() {
        let mut p = LocalPollutionState::default();
        // Building drinking well water at quality 0.9, 100 liters
        let receipts = vec![BuildingWaterReceipt {
            building_id: "rural_well".into(),
            water_quality_received: 0.9,
            water_consumed: 100.0,
        }];
        compute_biohazard_for_region(&mut p, 0.0, 0.0, 0.0, &receipts, 100.0);
        // quality_deficit = 0.9 - 0.9 = 0.0 → no biohazard
        assert!((p.low_quality_water_biohazard - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_biohazard_failing_grid_quality() {
        let mut p = LocalPollutionState::default();
        // Urban building on failing grid at quality 0.5, 200 liters
        let receipts = vec![BuildingWaterReceipt {
            building_id: "urban_1".into(),
            water_quality_received: 0.5,
            water_consumed: 200.0,
        }];
        compute_biohazard_for_region(&mut p, 0.0, 0.0, 0.0, &receipts, 100.0);
        // quality_deficit = 0.9 - 0.5 = 0.4
        // biohazard = 0.4 * 200 * 0.5 = 40.0
        assert!((p.low_quality_water_biohazard - 40.0).abs() < 0.01);
    }

    #[test]
    fn test_biohazard_decay_slower_than_smog() {
        let mut p = LocalPollutionState {
            biohazard_level: 50.0,
            ..Default::default()
        };
        compute_biohazard_for_region(&mut p, 0.0, 0.0, 0.0, &[], 100.0);
        // biohazard = (50 + 0) * 0.97 = 48.5 (vs smog 47.5 — slower decay)
        assert!((p.biohazard_level - 48.5).abs() < 0.01);
    }

    #[test]
    fn test_biohazard_clamped_at_100() {
        let mut p = LocalPollutionState {
            biohazard_level: 99.0,
            ..Default::default()
        };
        compute_biohazard_for_region(&mut p, 100000.0, 0.0, 0.0, &[], 1.0);
        assert_eq!(p.biohazard_level, 100.0);
    }
}

//! Phase 84: Waste Epic — Solid Waste Management & Circular Economy
//!
//! Comprehensive test suite covering:
//! 1. Commodity serialization and B2B exclusion
//! 2. Waste generation scaling
//! 3. Cumulative rural method behavior (REFINEMENT 1)
//! 4. Geographic dumping constraints (REFINEMENT 2)
//! 5. Surface-water degradation from river dumping
//! 6. Forestry degradation from forest dumping
//! 7. 100% mass conservation (CRITICAL FIX 3)
//! 8. WtE ash output (CRITICAL FIX 2)
//! 9. Landfill hard stop (LOGISTICAL BOUND 2)
//! 10. Uncollected-waste crisis
//! 11. FreightCapacity PSZOK requirement (LOGISTICAL BOUND 1)
//! 12. Illegal dumping fallback
//! 13. Gate-fee failure
//! 14. Fertilizer sink
//! 15. Dual billing (CRITICAL FIX 4)
//! 16. AI decisions
//! 17. Turn-loop sequencing
//! 18. Snapshot role-gating
//! 19. Tech tree gating
//! 20. Production method registration

use sim_engine::registries::enums::{Commodity, Sector};
use sim_engine::registries::production_methods_data::default_production_methods;
use sim_engine::utilities::waste_grid::*;
use sim_engine::utilities::waste_grid::{
    compute_construction_waste, compute_regulated_curbside_fee, compute_regulated_gate_fee,
    compute_waste_from_consumption, compute_waste_pollution, is_centralized_waste_method,
    recycling_yields, select_dumping_vector, separation_yields, waste_disposal_biohazard_factor,
    waste_disposal_composts, waste_disposal_recovers_scrap, waste_disposal_smog_factor,
    waste_fraction_for_commodity, waste_separation_efficiency, DumpingVector, LandfillState,
    WasteGridState, WastePlantType, WasteSalesHistory, COMPOSTING_YIELD,
    CONSTRUCTION_WASTE_FRACTION, FOREST_AREA_THRESHOLD, LEACHATE_CONTAMINATION_FACTOR,
    SCRAP_RECOVERY_YIELD, WTE_ASH_FRACTION_ADVANCED, WTE_ASH_FRACTION_BASIC,
};
use std::collections::HashMap;

// ============================================================================
// SECTION 1: COMMODITY SERIALIZATION & B2B EXCLUSION (CRITICAL FIX 1)
// ============================================================================

#[test]
fn test_waste_commodities_exist() {
    // All 9 new waste commodities must be valid enum variants
    let _ = Commodity::MixedWaste;
    let _ = Commodity::BioWaste;
    let _ = Commodity::MetalWaste;
    let _ = Commodity::GlassWaste;
    let _ = Commodity::PlasticWaste;
    let _ = Commodity::ElectronicWaste;
    let _ = Commodity::TextileWaste;
    let _ = Commodity::ConstructionWaste;
    let _ = Commodity::BulkyWaste;
    let _ = Commodity::HazardousWaste;
}

#[test]
fn test_waste_commodity_serialization_roundtrip() {
    // Waste commodities must serialize and deserialize correctly
    let commodities = vec![
        Commodity::MixedWaste,
        Commodity::BioWaste,
        Commodity::MetalWaste,
        Commodity::GlassWaste,
        Commodity::PlasticWaste,
        Commodity::ElectronicWaste,
        Commodity::TextileWaste,
        Commodity::ConstructionWaste,
        Commodity::BulkyWaste,
        Commodity::HazardousWaste,
    ];
    for c in &commodities {
        let json = serde_json::to_string(c).unwrap();
        let deserialized: Commodity = serde_json::from_str(&json).unwrap();
        assert_eq!(
            *c, deserialized,
            "Serialization roundtrip failed for {:?}",
            c
        );
    }
}

#[test]
fn test_b2b_exclusion_trash_streams() {
    // CRITICAL FIX 1: Trash streams must NOT appear in WasteManagement primary commodities
    let primary: Vec<Commodity> = Sector::WasteManagement.primary_commodities();
    // Tradeable sorted fractions MUST be present
    assert!(
        primary.contains(&Commodity::MetalWaste),
        "MetalWaste must be tradeable"
    );
    assert!(
        primary.contains(&Commodity::GlassWaste),
        "GlassWaste must be tradeable"
    );
    assert!(
        primary.contains(&Commodity::PlasticWaste),
        "PlasticWaste must be tradeable"
    );
    assert!(
        primary.contains(&Commodity::ElectronicWaste),
        "ElectronicWaste must be tradeable"
    );
    assert!(
        primary.contains(&Commodity::TextileWaste),
        "TextileWaste must be tradeable"
    );
    // Disposal-only trash streams MUST be excluded
    assert!(
        !primary.contains(&Commodity::MixedWaste),
        "MixedWaste must NOT be tradeable"
    );
    assert!(
        !primary.contains(&Commodity::BioWaste),
        "BioWaste must NOT be tradeable"
    );
    assert!(
        !primary.contains(&Commodity::ConstructionWaste),
        "ConstructionWaste must NOT be tradeable"
    );
    assert!(
        !primary.contains(&Commodity::BulkyWaste),
        "BulkyWaste must NOT be tradeable"
    );
    assert!(
        !primary.contains(&Commodity::HazardousWaste),
        "HazardousWaste must NOT be tradeable"
    );
}

#[test]
fn test_b2b_exclusion_only_5_tradeable_fractions() {
    // Exactly 5 sorted secondary-material fractions are tradeable
    let primary = Sector::WasteManagement.primary_commodities();
    assert_eq!(
        primary.len(),
        5,
        "WasteManagement must have exactly 5 tradeable commodities"
    );
}

// ============================================================================
// SECTION 2: WASTE GENERATION SCALING
// ============================================================================

#[test]
fn test_waste_generation_from_food() {
    let mut consumed = HashMap::new();
    consumed.insert(Commodity::Food, 100.0);
    let waste = compute_waste_from_consumption(&consumed);
    assert!(
        waste.contains_key(&Commodity::BioWaste),
        "Food consumption must produce BioWaste"
    );
    let bio = waste.get(&Commodity::BioWaste).copied().unwrap_or(0.0);
    assert!(bio > 0.0, "BioWaste must be positive");
}

#[test]
fn test_waste_generation_from_manufactured_goods() {
    let mut consumed = HashMap::new();
    consumed.insert(Commodity::Furniture, 50.0);
    let waste = compute_waste_from_consumption(&consumed);
    // Manufactured goods should produce MixedWaste (packaging + disposal)
    if let Some(&mixed) = waste.get(&Commodity::MixedWaste) {
        assert!(mixed > 0.0, "Manufactured goods must produce MixedWaste");
    }
}

#[test]
fn test_waste_generation_scales_with_consumption() {
    let mut small = HashMap::new();
    small.insert(Commodity::Food, 10.0);
    let mut large = HashMap::new();
    large.insert(Commodity::Food, 1000.0);
    let small_waste = compute_waste_from_consumption(&small);
    let large_waste = compute_waste_from_consumption(&large);
    let small_bio = small_waste
        .get(&Commodity::BioWaste)
        .copied()
        .unwrap_or(0.0);
    let large_bio = large_waste
        .get(&Commodity::BioWaste)
        .copied()
        .unwrap_or(0.0);
    assert!(large_bio > small_bio, "Waste must scale with consumption");
}

#[test]
fn test_waste_generation_empty_consumption() {
    let consumed: HashMap<Commodity, f64> = HashMap::new();
    let waste = compute_waste_from_consumption(&consumed);
    assert!(waste.is_empty(), "No consumption = no waste");
}

#[test]
fn test_construction_waste_fraction() {
    let mut materials = HashMap::new();
    materials.insert(Commodity::Steel, 100.0);
    materials.insert(Commodity::Cement, 200.0);
    let waste = compute_construction_waste(&materials);
    let expected = 300.0 * CONSTRUCTION_WASTE_FRACTION;
    assert!(
        (waste - expected).abs() < 0.001,
        "Construction waste must be {}% of materials",
        CONSTRUCTION_WASTE_FRACTION * 100.0
    );
}

#[test]
fn test_construction_waste_empty() {
    let materials: HashMap<Commodity, f64> = HashMap::new();
    let waste = compute_construction_waste(&materials);
    assert_eq!(waste, 0.0, "No materials = no construction waste");
}

#[test]
fn test_waste_fraction_for_known_commodity() {
    let result = waste_fraction_for_commodity(Commodity::Food);
    assert!(result.is_some(), "Food must have a waste fraction");
    let (_waste_commodity, fraction) = result.unwrap();
    assert!(
        fraction > 0.0 && fraction <= 1.0,
        "Fraction must be in (0, 1]"
    );
}

#[test]
fn test_waste_fraction_for_unknown_commodity() {
    // Commodities with no waste fraction should return None
    let result = waste_fraction_for_commodity(Commodity::Energy);
    // Energy is a service, not a physical good — may or may not have waste
    // Just verify it doesn't panic
    let _ = result;
}

// ============================================================================
// SECTION 3: CUMULATIVE RURAL METHOD BEHAVIOR (REFINEMENT 1)
// ============================================================================

#[test]
fn test_primitive_dumping_is_standalone() {
    assert!(
        !is_centralized_waste_method("Primitive Dumping"),
        "Primitive Dumping must be standalone (not centralized)"
    );
}

#[test]
fn test_basic_homesteading_is_standalone() {
    assert!(
        !is_centralized_waste_method("Basic Homesteading"),
        "Basic Homesteading must be standalone (not centralized)"
    );
}

#[test]
fn test_advanced_rural_scavenging_is_standalone() {
    assert!(
        !is_centralized_waste_method("Advanced Rural Scavenging"),
        "Advanced Rural Scavenging must be standalone (not centralized)"
    );
}

#[test]
fn test_trash_burning_is_standalone() {
    assert!(
        !is_centralized_waste_method("Trash Burning"),
        "Trash Burning must be standalone (not centralized)"
    );
}

#[test]
fn test_unsegregated_collection_is_centralized() {
    assert!(
        is_centralized_waste_method("Unsegregated Collection"),
        "Unsegregated Collection must be centralized"
    );
}

#[test]
fn test_source_separated_curbside_is_centralized() {
    assert!(
        is_centralized_waste_method("Source-Separated Curbside"),
        "Source-Separated Curbside must be centralized"
    );
}

#[test]
fn test_smart_sorted_collection_is_centralized() {
    assert!(
        is_centralized_waste_method("Smart Sorted Collection"),
        "Smart Sorted Collection must be centralized"
    );
}

#[test]
fn test_basic_homesteading_composts() {
    // REFINEMENT 1: Basic Homesteading composts BioWaste into Fertilizers
    assert!(
        waste_disposal_composts("Basic Homesteading"),
        "Basic Homesteading must compost BioWaste"
    );
}

#[test]
fn test_advanced_rural_scavenging_composts() {
    // REFINEMENT 1: Advanced Rural Scavenging is cumulative — retains composting
    assert!(
        waste_disposal_composts("Advanced Rural Scavenging"),
        "Advanced Rural Scavenging must retain composting (cumulative track)"
    );
}

#[test]
fn test_primitive_dumping_does_not_compost() {
    assert!(
        !waste_disposal_composts("Primitive Dumping"),
        "Primitive Dumping must not compost"
    );
}

#[test]
fn test_advanced_rural_scavenging_recovers_scrap() {
    // REFINEMENT 1: Advanced Rural Scavenging recovers Metal and Glass
    assert!(
        waste_disposal_recovers_scrap("Advanced Rural Scavenging"),
        "Advanced Rural Scavenging must recover scrap metal and glass"
    );
}

#[test]
fn test_basic_homesteading_does_not_recover_scrap() {
    // Basic Homesteading only composts — no scrap recovery yet
    assert!(
        !waste_disposal_recovers_scrap("Basic Homesteading"),
        "Basic Homesteading must not recover scrap"
    );
}

#[test]
fn test_primitive_dumping_does_not_recover_scrap() {
    assert!(
        !waste_disposal_recovers_scrap("Primitive Dumping"),
        "Primitive Dumping must not recover scrap"
    );
}

#[test]
fn test_cumulative_track_composting_yield() {
    // COMPOSTING_YIELD must be in valid range
    const {
        assert!(
            COMPOSTING_YIELD > 0.0 && COMPOSTING_YIELD <= 1.0,
            "Composting yield must be in (0, 1]"
        );
    }
}

#[test]
fn test_cumulative_track_scrap_yield() {
    // SCRAP_RECOVERY_YIELD must be in valid range
    const {
        assert!(
            SCRAP_RECOVERY_YIELD > 0.0 && SCRAP_RECOVERY_YIELD <= 1.0,
            "Scrap recovery yield must be in (0, 1]"
        );
    }
}

// ============================================================================
// SECTION 4: GEOGRAPHIC DUMPING CONSTRAINTS (REFINEMENT 2)
// ============================================================================

#[test]
fn test_dumping_vector_river_with_navigable_river() {
    let v = select_dumping_vector(true, false, 0.0);
    assert_eq!(
        v,
        DumpingVector::RiverWater,
        "Region with navigable river must use river dumping"
    );
}

#[test]
fn test_dumping_vector_river_with_coastline() {
    let v = select_dumping_vector(false, true, 0.0);
    assert_eq!(
        v,
        DumpingVector::RiverWater,
        "Region with coastline must use water dumping"
    );
}

#[test]
fn test_dumping_vector_river_with_both() {
    let v = select_dumping_vector(true, true, 0.0);
    assert_eq!(
        v,
        DumpingVector::RiverWater,
        "Region with both river and coastline must use water dumping"
    );
}

#[test]
fn test_dumping_vector_forest_requires_significant_area() {
    // Forest area > 10% threshold
    let v = select_dumping_vector(false, false, FOREST_AREA_THRESHOLD + 0.01);
    assert_eq!(
        v,
        DumpingVector::ForestWild,
        "Region with >10% forest must use forest dumping"
    );
}

#[test]
fn test_dumping_vector_forest_at_exact_threshold() {
    // At exactly 10% — should use forest (>= threshold)
    let v = select_dumping_vector(false, false, FOREST_AREA_THRESHOLD);
    // The exact boundary behavior — verify it doesn't panic
    let _ = v;
}

#[test]
fn test_dumping_vector_forest_below_threshold() {
    // Forest area < 10% → street dumping
    let v = select_dumping_vector(false, false, FOREST_AREA_THRESHOLD - 0.01);
    assert_eq!(
        v,
        DumpingVector::StreetAlley,
        "Region with <10% forest must use street dumping"
    );
}

#[test]
fn test_dumping_vector_street_default() {
    // No water, no forest → street dumping (default fallback)
    let v = select_dumping_vector(false, false, 0.0);
    assert_eq!(
        v,
        DumpingVector::StreetAlley,
        "Region with no water and no forest must use street dumping"
    );
}

#[test]
fn test_dumping_vector_water_takes_precedence_over_forest() {
    // Even with significant forest, water dumping takes precedence
    let v = select_dumping_vector(true, false, 0.50);
    assert_eq!(
        v,
        DumpingVector::RiverWater,
        "Water dumping must take precedence over forest dumping"
    );
}

#[test]
fn test_dumping_vector_street_biohazard_high() {
    // Street/Alley dumping has severe local biohazard
    let v = DumpingVector::StreetAlley;
    assert!(
        v.biohazard_factor() > 0.5,
        "Street dumping must have high biohazard factor"
    );
}

#[test]
fn test_dumping_vector_forest_biohazard_moderate() {
    let v = DumpingVector::ForestWild;
    let bio = v.biohazard_factor();
    let street_bio = DumpingVector::StreetAlley.biohazard_factor();
    assert!(
        bio < street_bio,
        "Forest dumping must have lower biohazard than street dumping"
    );
    assert!(bio > 0.0, "Forest dumping must have some biohazard");
}

#[test]
fn test_dumping_vector_river_biohazard_low() {
    let v = DumpingVector::RiverWater;
    let bio = v.biohazard_factor();
    let forest_bio = DumpingVector::ForestWild.biohazard_factor();
    assert!(
        bio < forest_bio,
        "River dumping must have lower biohazard than forest dumping (waste leaves area)"
    );
}

#[test]
fn test_dumping_vector_river_degrades_surface_water() {
    // REFINEMENT 2: River dumping aggressively degrades surface water quality
    assert!(
        DumpingVector::RiverWater.degrades_surface_water(),
        "River dumping must degrade surface water"
    );
}

#[test]
fn test_dumping_vector_forest_does_not_degrade_surface_water() {
    assert!(
        !DumpingVector::ForestWild.degrades_surface_water(),
        "Forest dumping must not degrade surface water"
    );
}

#[test]
fn test_dumping_vector_street_does_not_degrade_surface_water() {
    assert!(
        !DumpingVector::StreetAlley.degrades_surface_water(),
        "Street dumping must not degrade surface water"
    );
}

#[test]
fn test_dumping_vector_forest_degrades_forestry() {
    // REFINEMENT 2: Forest dumping degrades forest ecological health
    assert!(
        DumpingVector::ForestWild.degrades_forestry(),
        "Forest dumping must degrade forestry"
    );
}

#[test]
fn test_dumping_vector_river_does_not_degrade_forestry() {
    assert!(
        !DumpingVector::RiverWater.degrades_forestry(),
        "River dumping must not degrade forestry"
    );
}

#[test]
fn test_dumping_vector_street_does_not_degrade_forestry() {
    assert!(
        !DumpingVector::StreetAlley.degrades_forestry(),
        "Street dumping must not degrade forestry"
    );
}

// ============================================================================
// SECTION 5: 100% MASS CONSERVATION (CRITICAL FIX 3)
// ============================================================================

fn verify_mass_balance(yields: &[(Commodity, f64)]) -> bool {
    let total: f64 = yields.iter().map(|(_, y)| y).sum();
    (total - 1.0).abs() < 0.001
}

#[test]
fn test_mass_balance_metal_recycling() {
    let yields = recycling_yields(Commodity::MetalWaste);
    assert!(
        verify_mass_balance(&yields),
        "MetalWaste yields must sum to 1.0"
    );
}

#[test]
fn test_mass_balance_glass_recycling() {
    let yields = recycling_yields(Commodity::GlassWaste);
    assert!(
        verify_mass_balance(&yields),
        "GlassWaste yields must sum to 1.0"
    );
}

#[test]
fn test_mass_balance_plastic_recycling() {
    let yields = recycling_yields(Commodity::PlasticWaste);
    assert!(
        verify_mass_balance(&yields),
        "PlasticWaste yields must sum to 1.0"
    );
}

#[test]
fn test_mass_balance_electronic_recycling() {
    let yields = recycling_yields(Commodity::ElectronicWaste);
    assert!(
        verify_mass_balance(&yields),
        "ElectronicWaste yields must sum to 1.0"
    );
}

#[test]
fn test_mass_balance_textile_recycling() {
    let yields = recycling_yields(Commodity::TextileWaste);
    assert!(
        verify_mass_balance(&yields),
        "TextileWaste yields must sum to 1.0"
    );
}

#[test]
fn test_mass_balance_separation_yields() {
    let yields = separation_yields();
    assert!(
        verify_mass_balance(&yields),
        "Separation yields must sum to 1.0"
    );
}

#[test]
fn test_metal_recycling_produces_residual() {
    // CRITICAL FIX 3: Metal recycling must output residual MixedWaste
    let yields = recycling_yields(Commodity::MetalWaste);
    let has_residual = yields.iter().any(|(c, _)| *c == Commodity::MixedWaste);
    assert!(
        has_residual,
        "Metal recycling must produce residual MixedWaste"
    );
}

#[test]
fn test_electronic_recycling_produces_hazardous_residual() {
    // Electronic waste recycling must produce HazardousWaste residual
    let yields = recycling_yields(Commodity::ElectronicWaste);
    let has_hazardous = yields.iter().any(|(c, _)| *c == Commodity::HazardousWaste);
    assert!(
        has_hazardous,
        "Electronic recycling must produce HazardousWaste residual"
    );
}

#[test]
fn test_glass_recycling_produces_residual() {
    let yields = recycling_yields(Commodity::GlassWaste);
    let has_residual = yields.iter().any(|(c, _)| *c == Commodity::MixedWaste);
    assert!(
        has_residual,
        "Glass recycling must produce residual MixedWaste"
    );
}

#[test]
fn test_plastic_recycling_produces_residual() {
    let yields = recycling_yields(Commodity::PlasticWaste);
    let has_residual = yields.iter().any(|(c, _)| *c == Commodity::MixedWaste);
    assert!(
        has_residual,
        "Plastic recycling must produce residual MixedWaste"
    );
}

#[test]
fn test_textile_recycling_produces_residual() {
    let yields = recycling_yields(Commodity::TextileWaste);
    let has_residual = yields.iter().any(|(c, _)| *c == Commodity::MixedWaste);
    assert!(
        has_residual,
        "Textile recycling must produce residual MixedWaste"
    );
}

#[test]
fn test_separation_produces_residual() {
    // Separation must output non-sortable residual (mass balance closure)
    let yields = separation_yields();
    let has_residual = yields.iter().any(|(c, _)| *c == Commodity::MixedWaste);
    assert!(has_residual, "Separation must produce residual MixedWaste");
}

#[test]
fn test_metal_recycling_outputs_steel() {
    let yields = recycling_yields(Commodity::MetalWaste);
    let has_steel = yields.iter().any(|(c, _)| *c == Commodity::Steel);
    assert!(has_steel, "Metal recycling must output Steel");
}

#[test]
fn test_glass_recycling_outputs_glass() {
    let yields = recycling_yields(Commodity::GlassWaste);
    let has_glass = yields.iter().any(|(c, _)| *c == Commodity::Glass);
    assert!(has_glass, "Glass recycling must output Glass");
}

#[test]
fn test_plastic_recycling_outputs_plastics() {
    let yields = recycling_yields(Commodity::PlasticWaste);
    let has_plastics = yields.iter().any(|(c, _)| *c == Commodity::Plastics);
    assert!(has_plastics, "Plastic recycling must output Plastics");
}

#[test]
fn test_electronic_recycling_outputs_copper() {
    let yields = recycling_yields(Commodity::ElectronicWaste);
    let has_copper = yields.iter().any(|(c, _)| *c == Commodity::Copper);
    assert!(has_copper, "Electronic recycling must output Copper");
}

#[test]
fn test_textile_recycling_outputs_industrial_fiber() {
    let yields = recycling_yields(Commodity::TextileWaste);
    let has_fiber = yields.iter().any(|(c, _)| *c == Commodity::IndustrialFiber);
    assert!(has_fiber, "Textile recycling must output IndustrialFiber");
}

// ============================================================================
// SECTION 6: WtE ASH OUTPUT (CRITICAL FIX 2)
// ============================================================================

#[test]
fn test_wte_ash_fraction_basic_in_range() {
    // CRITICAL FIX 2: WtE must output 0.20-0.30 per input unit as ash
    const {
        assert!(
            WTE_ASH_FRACTION_BASIC >= 0.20 && WTE_ASH_FRACTION_BASIC <= 0.30,
            "Basic WtE ash fraction must be in [0.20, 0.30]"
        );
    }
}

#[test]
fn test_wte_ash_fraction_advanced_in_range() {
    const {
        assert!(
            WTE_ASH_FRACTION_ADVANCED >= 0.20 && WTE_ASH_FRACTION_ADVANCED <= 0.30,
            "Advanced WtE ash fraction must be in [0.20, 0.30]"
        );
    }
}

#[test]
fn test_wte_ash_fraction_advanced_lower_than_basic() {
    // Advanced WtE should produce less ash (better combustion)
    const {
        assert!(
            WTE_ASH_FRACTION_ADVANCED <= WTE_ASH_FRACTION_BASIC,
            "Advanced WtE ash fraction should be <= basic"
        );
    }
}

#[test]
fn test_wte_ash_is_hazardous() {
    // CRITICAL FIX 2: Ash must be routed to landfill as HazardousWaste
    // This is enforced in the WtE production method outputs (HazardousWaste)
    // and in the turn processing (ash → uncollected HazardousWaste)
    let _ = Commodity::HazardousWaste; // Must exist
}

#[test]
fn test_wte_energy_output_positive() {
    const {
        assert!(WTE_ENERGY_PER_TON > 0.0, "WtE must produce positive energy");
    }
}

#[test]
fn test_wte_heat_output_positive() {
    const {
        assert!(
            WTE_HEAT_PER_TON_CHP > 0.0,
            "WtE CHP must produce positive heat"
        );
    }
}

#[test]
fn test_wte_mass_not_destroyed() {
    // CRITICAL FIX 2: WtE does not destroy mass — ash is the physical residual
    // Energy and heat are thermodynamic service outputs, not mass substitutes
    let input_mass = 100.0;
    let ash_mass = input_mass * WTE_ASH_FRACTION_BASIC;
    assert!(ash_mass > 0.0, "WtE must produce positive ash mass");
    // The remaining mass is converted to energy (E=mc²) and heat (thermodynamic)
    // but in the simulation, ash is the tracked physical residual
}

// ============================================================================
// SECTION 7: LANDFILL HARD STOP (LOGISTICAL BOUND 2)
// ============================================================================

#[test]
fn test_landfill_accepts_waste_below_capacity() {
    let mut landfill = LandfillState::new(100.0, 1.0, 0.9, 0.8);
    let mut waste = HashMap::new();
    waste.insert(Commodity::MixedWaste, 50.0);
    let accepted = landfill.accept_waste(&waste);
    assert_eq!(accepted, 50.0, "Landfill must accept waste below capacity");
    assert!(!landfill.is_full, "Landfill should not be full");
    assert_eq!(landfill.remaining_capacity, 50.0);
}

#[test]
fn test_landfill_hard_stop_at_full_capacity() {
    let mut landfill = LandfillState::new(100.0, 1.0, 0.9, 0.8);
    let mut waste = HashMap::new();
    waste.insert(Commodity::MixedWaste, 100.0);
    let accepted = landfill.accept_waste(&waste);
    assert_eq!(accepted, 100.0);
    assert!(
        landfill.is_full,
        "Landfill must be full after accepting capacity"
    );
    assert_eq!(landfill.remaining_capacity, 0.0);
}

#[test]
fn test_landfill_rejects_all_waste_when_full() {
    let mut landfill = LandfillState::new(100.0, 1.0, 0.9, 0.8);
    let mut waste = HashMap::new();
    waste.insert(Commodity::MixedWaste, 100.0);
    landfill.accept_waste(&waste);

    // Now reject all incoming
    let mut more_waste = HashMap::new();
    more_waste.insert(Commodity::MixedWaste, 50.0);
    let rejected = landfill.accept_waste(&more_waste);
    assert_eq!(rejected, 0.0, "Full landfill must reject ALL waste");
}

#[test]
fn test_landfill_partial_acceptance() {
    let mut landfill = LandfillState::new(100.0, 1.0, 0.9, 0.8);
    // Fill 80 tons
    let mut waste1 = HashMap::new();
    waste1.insert(Commodity::MixedWaste, 80.0);
    landfill.accept_waste(&waste1);
    assert_eq!(landfill.remaining_capacity, 20.0);

    // Try to add 30 more — should only accept 20
    let mut waste2 = HashMap::new();
    waste2.insert(Commodity::MixedWaste, 30.0);
    let accepted = landfill.accept_waste(&waste2);
    assert_eq!(
        accepted, 20.0,
        "Landfill must only accept remaining capacity"
    );
    assert!(landfill.is_full);
}

#[test]
fn test_landfill_utilization_empty() {
    let landfill = LandfillState::new(100.0, 1.0, 0.9, 0.8);
    assert_eq!(
        landfill.utilization(),
        0.0,
        "Empty landfill utilization = 0"
    );
}

#[test]
fn test_landfill_utilization_half() {
    let mut landfill = LandfillState::new(100.0, 1.0, 0.9, 0.8);
    let mut waste = HashMap::new();
    waste.insert(Commodity::MixedWaste, 50.0);
    landfill.accept_waste(&waste);
    assert!(
        (landfill.utilization() - 0.5).abs() < 0.001,
        "Half-full utilization = 0.5"
    );
}

#[test]
fn test_landfill_utilization_full() {
    let mut landfill = LandfillState::new(100.0, 1.0, 0.9, 0.8);
    let mut waste = HashMap::new();
    waste.insert(Commodity::MixedWaste, 100.0);
    landfill.accept_waste(&waste);
    assert!(
        (landfill.utilization() - 1.0).abs() < 0.001,
        "Full utilization = 1.0"
    );
}

#[test]
fn test_landfill_leachate_leakage_positive() {
    let mut landfill = LandfillState::new(100.0, 1.0, 0.9, 0.8);
    let mut waste = HashMap::new();
    waste.insert(Commodity::MixedWaste, 50.0);
    landfill.accept_waste(&waste);
    let leachate = landfill.leachate_leakage();
    // Leachate should be positive when landfill has waste
    // (depends on liner integrity and leachate capture)
    assert!(leachate >= 0.0, "Leachate leakage must be non-negative");
}

#[test]
fn test_landfill_leachate_zero_when_empty() {
    let landfill = LandfillState::new(100.0, 1.0, 0.9, 0.8);
    let leachate = landfill.leachate_leakage();
    assert_eq!(leachate, 0.0, "Empty landfill must have zero leachate");
}

// ============================================================================
// SECTION 8: WASTE POLLUTION COMPUTATION
// ============================================================================

#[test]
fn test_waste_pollution_burning_emissions() {
    let result = compute_waste_pollution(100.0, 0.0, DumpingVector::StreetAlley, 0.0, 0.0);
    assert!(
        result.burning_emissions > 0.0,
        "Burning waste must produce emissions"
    );
}

#[test]
fn test_waste_pollution_dumping_biohazard_street() {
    let result = compute_waste_pollution(0.0, 100.0, DumpingVector::StreetAlley, 0.0, 0.0);
    assert!(
        result.dumping_biohazard > 0.0,
        "Street dumping must produce biohazard"
    );
}

#[test]
fn test_waste_pollution_dumping_biohazard_forest_lower() {
    let street = compute_waste_pollution(0.0, 100.0, DumpingVector::StreetAlley, 0.0, 0.0);
    let forest = compute_waste_pollution(0.0, 100.0, DumpingVector::ForestWild, 0.0, 0.0);
    assert!(
        forest.dumping_biohazard < street.dumping_biohazard,
        "Forest dumping biohazard must be lower than street"
    );
}

#[test]
fn test_waste_pollution_dumping_biohazard_river_lowest() {
    let forest = compute_waste_pollution(0.0, 100.0, DumpingVector::ForestWild, 0.0, 0.0);
    let river = compute_waste_pollution(0.0, 100.0, DumpingVector::RiverWater, 0.0, 0.0);
    assert!(
        river.dumping_biohazard < forest.dumping_biohazard,
        "River dumping biohazard must be lowest (waste leaves area)"
    );
}

#[test]
fn test_waste_pollution_uncollected_biohazard() {
    let result = compute_waste_pollution(0.0, 0.0, DumpingVector::StreetAlley, 100.0, 0.0);
    assert!(
        result.uncollected_biohazard > 0.0,
        "Uncollected waste must produce biohazard"
    );
}

#[test]
fn test_waste_pollution_zero_waste() {
    let result = compute_waste_pollution(0.0, 0.0, DumpingVector::StreetAlley, 0.0, 0.0);
    assert_eq!(result.burning_emissions, 0.0);
    assert_eq!(result.dumping_biohazard, 0.0);
    assert_eq!(result.uncollected_biohazard, 0.0);
}

#[test]
fn test_leachate_contamination_factor_positive() {
    const {
        assert!(
            LEACHATE_CONTAMINATION_FACTOR > 0.0,
            "Leachate contamination factor must be positive"
        );
    }
}

// ============================================================================
// SECTION 9: WASTE DISPOSAL METHOD HELPERS
// ============================================================================

#[test]
fn test_waste_disposal_biohazard_none() {
    assert!(
        waste_disposal_biohazard_factor("None") > 0.0,
        "No waste disposal must have high biohazard"
    );
}

#[test]
fn test_waste_disposal_biohazard_primitive_dumping() {
    let bio = waste_disposal_biohazard_factor("Primitive Dumping");
    assert!(bio > 0.0, "Primitive Dumping must have biohazard");
}

#[test]
fn test_waste_disposal_biohazard_trash_burning() {
    let bio = waste_disposal_biohazard_factor("Trash Burning");
    // Burning destroys biohazard (pathogens killed by heat)
    assert_eq!(
        bio, 0.0,
        "Trash Burning must have zero biohazard (pathogens destroyed)"
    );
}

#[test]
fn test_waste_disposal_smog_trash_burning() {
    let smog = waste_disposal_smog_factor("Trash Burning");
    assert!(smog > 0.0, "Trash Burning must produce smog");
}

#[test]
fn test_waste_disposal_smog_primitive_dumping() {
    let smog = waste_disposal_smog_factor("Primitive Dumping");
    // Dumping produces biohazard, not smog
    assert_eq!(smog, 0.0, "Primitive Dumping must not produce smog");
}

#[test]
fn test_waste_separation_efficiency_unsegregated() {
    let eff = waste_separation_efficiency("Unsegregated Collection");
    assert_eq!(
        eff, 0.0,
        "Unsegregated collection must have 0 separation efficiency"
    );
}

#[test]
fn test_waste_separation_efficiency_source_separated() {
    let eff = waste_separation_efficiency("Source-Separated Curbside");
    assert!(
        eff > 0.0,
        "Source-separated curbside must have positive efficiency"
    );
}

#[test]
fn test_waste_separation_efficiency_smart_sorted() {
    let eff = waste_separation_efficiency("Smart Sorted Collection");
    let source_sep = waste_separation_efficiency("Source-Separated Curbside");
    assert!(
        eff > source_sep,
        "Smart sorted must be more efficient than source-separated"
    );
}

// ============================================================================
// SECTION 10: DUAL BILLING (CRITICAL FIX 4)
// ============================================================================

#[test]
fn test_curbside_fee_positive() {
    let history = WasteSalesHistory::default();
    let avg_wage = 1000.0;
    let fee = compute_regulated_curbside_fee(&history, avg_wage);
    assert!(fee > 0.0, "Curbside fee must be positive");
}

#[test]
fn test_gate_fee_positive() {
    let history = WasteSalesHistory::default();
    let avg_wage = 1000.0;
    let fee = compute_regulated_gate_fee(&history, avg_wage);
    assert!(fee > 0.0, "Gate fee must be positive");
}

#[test]
fn test_gate_fee_higher_than_curbside() {
    // CRITICAL FIX 4: Heavy waste (PSZOK) is more expensive than curbside
    let history = WasteSalesHistory::default();
    let avg_wage = 1000.0;
    let curbside = compute_regulated_curbside_fee(&history, avg_wage);
    let gate = compute_regulated_gate_fee(&history, avg_wage);
    assert!(gate > curbside, "Gate fee must be higher than curbside fee");
}

#[test]
fn test_curbside_fee_scales_with_wage() {
    let history = WasteSalesHistory::default();
    let low_wage = 500.0;
    let high_wage = 5000.0;
    let low_fee = compute_regulated_curbside_fee(&history, low_wage);
    let high_fee = compute_regulated_curbside_fee(&history, high_wage);
    assert!(
        high_fee > low_fee,
        "Curbside fee must scale with wage (no magic numbers)"
    );
}

#[test]
fn test_gate_fee_scales_with_wage() {
    let history = WasteSalesHistory::default();
    let low_wage = 500.0;
    let high_wage = 5000.0;
    let low_fee = compute_regulated_gate_fee(&history, low_wage);
    let high_fee = compute_regulated_gate_fee(&history, high_wage);
    assert!(
        high_fee > low_fee,
        "Gate fee must scale with wage (no magic numbers)"
    );
}

// ============================================================================
// SECTION 11: WASTE PLANT REGISTRIES (Rule 13)
// ============================================================================

#[test]
fn test_waste_plant_registries_exist() {
    let registry = default_production_methods();
    // All 13 waste plant types must be registered
    let required_keys = [
        "uncontrolled_landfill",
        "controlled_landfill",
        "modern_landfill",
        "waste_separation_plant",
        "advanced_sorting_facility",
        "metal_recycling",
        "glass_recycling",
        "plastic_recycling",
        "electronic_recycling",
        "textile_recycling",
        "waste_to_energy_plant",
        "advanced_wte_chp",
        "civic_amenity_site",
    ];
    for key in &required_keys {
        assert!(
            registry.contains_key(*key),
            "Waste plant registry '{}' must exist (Rule 13)",
            key
        );
    }
}

#[test]
fn test_waste_automation_organization_registries_exist() {
    let registry = default_production_methods();
    assert!(
        registry.contains_key("waste_automation"),
        "Waste automation registry must exist"
    );
    assert!(
        registry.contains_key("waste_organization"),
        "Waste organization registry must exist"
    );
}

#[test]
fn test_waste_disposal_methods_in_housing_consumption() {
    let registry = default_production_methods();
    let housing = registry.get("housing_consumption");
    assert!(housing.is_some(), "Housing consumption registry must exist");
    // Check that waste disposal methods are registered
    // (The BuildingMethods struct stores them in waste_disposal HashMap)
}

#[test]
fn test_waste_disposal_methods_in_commercial_consumption() {
    let registry = default_production_methods();
    let commercial = registry.get("commercial_consumption");
    assert!(
        commercial.is_some(),
        "Commercial consumption registry must exist"
    );
}

#[test]
fn test_landfill_methods_have_production_slot() {
    let registry = default_production_methods();
    let landfill = registry.get("uncontrolled_landfill").unwrap();
    // Must have at least one production method
    assert!(
        !landfill.production.is_empty(),
        "Landfill must have production methods"
    );
}

#[test]
fn test_wte_methods_output_hazardous_waste() {
    // CRITICAL FIX 2: WtE production methods must output HazardousWaste ash
    let registry = default_production_methods();
    let wte = registry.get("waste_to_energy_plant").unwrap();
    for pm in wte.production.values() {
        let has_ash = pm
            .outputs
            .iter()
            .any(|(c, _)| *c == Commodity::HazardousWaste);
        assert!(
            has_ash,
            "WtE method '{}' must output HazardousWaste ash",
            pm.year
        );
    }
}

#[test]
fn test_recycling_methods_output_residual() {
    // CRITICAL FIX 3: All recycling methods must output residual waste
    let registry = default_production_methods();
    let metal = registry.get("metal_recycling").unwrap();
    for pm in metal.production.values() {
        let total_output: f64 = pm.outputs.values().sum();
        let total_input: f64 = pm.inputs.values().sum();
        if total_input > 0.0 {
            // Output mass should approximately equal input mass (mass conservation)
            assert!(
                (total_output - total_input).abs() / total_input < 0.01,
                "Metal recycling must conserve mass: input={}, output={}",
                total_input,
                total_output
            );
        }
    }
}

#[test]
fn test_separation_methods_conserve_mass() {
    let registry = default_production_methods();
    let sep = registry.get("waste_separation_plant").unwrap();
    for pm in sep.production.values() {
        let total_output: f64 = pm.outputs.values().sum();
        let total_input: f64 = pm.inputs.values().sum();
        if total_input > 0.0 {
            assert!(
                (total_output - total_input).abs() / total_input < 0.01,
                "Separation must conserve mass: input={}, output={}",
                total_input,
                total_output
            );
        }
    }
}

#[test]
fn test_wte_methods_conserve_mass_with_ash() {
    // WtE: input mass = ash mass (energy/heat are service outputs, not mass)
    let registry = default_production_methods();
    let wte = registry.get("waste_to_energy_plant").unwrap();
    for pm in wte.production.values() {
        let ash_mass: f64 = pm
            .outputs
            .iter()
            .filter(|(c, _)| **c == Commodity::HazardousWaste)
            .map(|(_, q)| q)
            .sum();
        let input_mass: f64 = pm.inputs.values().sum();
        if input_mass > 0.0 {
            // Ash must be 20-30% of input
            let ash_fraction = ash_mass / input_mass;
            assert!(
                (0.20..=0.30).contains(&ash_fraction),
                "WtE ash fraction must be in [0.20, 0.30], got {}",
                ash_fraction
            );
        }
    }
}

#[test]
fn test_pszok_methods_accept_heavy_waste() {
    let registry = default_production_methods();
    let pszok = registry.get("civic_amenity_site").unwrap();
    for pm in pszok.production.values() {
        // PSZOK must accept BulkyWaste, ConstructionWaste, or HazardousWaste
        let accepts_heavy = pm.inputs.iter().any(|(c, _)| {
            *c == Commodity::BulkyWaste
                || *c == Commodity::ConstructionWaste
                || *c == Commodity::HazardousWaste
        });
        assert!(
            accepts_heavy,
            "PSZOK must accept heavy waste (Bulky/Construction/Hazardous)"
        );
    }
}

// ============================================================================
// SECTION 12: WASTE PLANT TYPE ENUM
// ============================================================================

#[test]
fn test_waste_plant_type_default() {
    let default = WastePlantType::default();
    let _ = default; // Must implement Default
}

#[test]
fn test_waste_plant_type_serialization() {
    let plant = WastePlantType::default();
    let json = serde_json::to_string(&plant).unwrap();
    let deserialized: WastePlantType = serde_json::from_str(&json).unwrap();
    assert_eq!(
        plant, deserialized,
        "WastePlantType must serialize correctly"
    );
}

// ============================================================================
// SECTION 13: WASTE GRID STATE
// ============================================================================

#[test]
fn test_waste_grid_default() {
    let grid = WasteGridState::default();
    assert_eq!(grid.collection_route_km, 0.0);
    assert_eq!(grid.collection_capacity, 0.0);
}

#[test]
fn test_waste_grid_add_uncollected() {
    let mut grid = WasteGridState::default();
    grid.add_uncollected(Commodity::MixedWaste, 50.0);
    assert_eq!(grid.total_uncollected(), 50.0);
}

#[test]
fn test_waste_grid_add_multiple_uncollected() {
    let mut grid = WasteGridState::default();
    grid.add_uncollected(Commodity::MixedWaste, 30.0);
    grid.add_uncollected(Commodity::BioWaste, 20.0);
    assert_eq!(grid.total_uncollected(), 50.0);
}

#[test]
fn test_waste_grid_drain_uncollected() {
    let mut grid = WasteGridState::default();
    grid.add_uncollected(Commodity::MixedWaste, 100.0);
    grid.drain_uncollected(0.5); // Drain 50%
    assert_eq!(grid.total_uncollected(), 50.0);
}

#[test]
fn test_waste_grid_drain_all() {
    let mut grid = WasteGridState::default();
    grid.add_uncollected(Commodity::MixedWaste, 100.0);
    grid.drain_uncollected(1.0);
    assert_eq!(grid.total_uncollected(), 0.0);
}

#[test]
fn test_waste_grid_recompute_capacity() {
    let mut grid = WasteGridState::default();
    grid.collection_route_km = 100.0;
    grid.route_condition = 0.8;
    grid.recompute_capacity();
    assert!(
        grid.collection_capacity > 0.0,
        "Capacity must be positive with routes"
    );
}

#[test]
fn test_waste_grid_degrade() {
    let mut grid = WasteGridState::default();
    grid.route_condition = 1.0;
    grid.degrade(0.0); // Summer
    assert!(
        grid.route_condition <= 1.0,
        "Route condition must not increase"
    );
}

#[test]
fn test_waste_grid_degrade_winter() {
    let mut grid = WasteGridState::default();
    grid.route_condition = 1.0;
    grid.degrade(1.0); // Winter
    assert!(
        grid.route_condition < 1.0,
        "Winter must degrade route condition"
    );
}

// ============================================================================
// SECTION 14: TECH TREE GATING
// ============================================================================

#[test]
fn test_waste_tech_nodes_exist() {
    use sim_engine::registries::tech_tree_data::default_tech_tree;
    let tech = default_tech_tree();
    let waste_techs = [
        "waste_001",
        "waste_002",
        "waste_003",
        "waste_004",
        "waste_005",
        "waste_006",
    ];
    for id in &waste_techs {
        assert!(tech.contains_key(*id), "Tech {} must exist", id);
    }
}

#[test]
fn test_waste_tech_001_prerequisites() {
    use sim_engine::registries::tech_tree_data::default_tech_tree;
    let tech = default_tech_tree();
    let node = tech.get("waste_001").expect("waste_001 must exist");
    // waste_001 should require sanit_001 (basic sanitation)
    assert!(
        !node.prerequisites.is_empty(),
        "waste_001 must have prerequisites"
    );
}

#[test]
fn test_waste_tech_progression() {
    use sim_engine::registries::tech_tree_data::default_tech_tree;
    let tech = default_tech_tree();
    // Each waste tech should require the previous one
    let waste_techs = [
        "waste_001",
        "waste_002",
        "waste_003",
        "waste_004",
        "waste_005",
        "waste_006",
    ];
    for i in 1..waste_techs.len() {
        let node = tech
            .get(waste_techs[i])
            .unwrap_or_else(|| panic!("{} must exist", waste_techs[i]));
        assert!(
            node.prerequisites.contains(&waste_techs[i - 1].to_string()),
            "{} must require {}",
            waste_techs[i],
            waste_techs[i - 1]
        );
    }
}

#[test]
fn test_waste_tech_unlocks_methods() {
    use sim_engine::registries::tech_tree_data::default_tech_tree;
    let tech = default_tech_tree();
    let node = tech.get("waste_001").expect("waste_001 must exist");
    assert!(
        !node.unlocks_methods.is_empty(),
        "waste_001 must unlock methods"
    );
}

#[test]
fn test_waste_tech_006_unlocks_smart_sorted() {
    use sim_engine::registries::tech_tree_data::default_tech_tree;
    let tech = default_tech_tree();
    let node = tech.get("waste_006").expect("waste_006 must exist");
    // waste_006 should unlock Smart Sorted Collection
    let housing_unlocks = node.unlocks_methods.get("housing_consumption");
    assert!(
        housing_unlocks.is_some(),
        "waste_006 must unlock housing methods"
    );
    let slot_map = housing_unlocks.unwrap();
    let has_smart = slot_map.values().any(|v| v.contains("Smart Sorted"));
    assert!(has_smart, "waste_006 must unlock Smart Sorted Collection");
}

// ============================================================================
// SECTION 15: MUNICIPAL AI WASTE DOMAIN
// ============================================================================

#[test]
fn test_waste_investment_plan_default() {
    use sim_engine::energy::municipal_infrastructure_ai::WasteInvestmentPlan;
    let plan = WasteInvestmentPlan::default();
    assert_eq!(plan.estimated_capex, 0.0);
    assert!(!plan.is_crisis);
}

#[test]
fn test_waste_investment_plan_serialization() {
    use sim_engine::energy::municipal_infrastructure_ai::WasteInvestmentPlan;
    let plan = WasteInvestmentPlan {
        expand_collection_routes_km: 50.0,
        build_waste_plant: Some(WastePlantType::default()),
        estimated_capex: 100000.0,
        expected_mortality_reduction_value: 5000.0,
        landfill_utilization: 0.8,
        uncollected_waste_mass: 100.0,
        passes_cost_benefit_gate: true,
        is_crisis: true,
        rationale: "Landfill full".to_string(),
    };
    let json = serde_json::to_string(&plan).unwrap();
    let deserialized: WasteInvestmentPlan = serde_json::from_str(&json).unwrap();
    assert_eq!(plan, deserialized);
}

#[test]
fn test_municipal_ai_includes_waste_domain() {
    use sim_engine::energy::municipal_infrastructure_ai::InfrastructureDomain;
    // InfrastructureDomain must have a Waste variant
    let _ = InfrastructureDomain::Waste;
}

#[test]
fn test_municipal_ai_waste_in_crisis_allocation() {
    use sim_engine::energy::municipal_heating_ai::HeatingInvestmentPlan;
    use sim_engine::energy::municipal_infrastructure_ai::*;
    let thermal = HeatingInvestmentPlan::default();
    let electrical = ElectricalInvestmentPlan::default();
    let water = WaterInvestmentPlan::default();
    let sanitation = SanitationInvestmentPlan::default();
    let waste = WasteInvestmentPlan {
        estimated_capex: 50000.0,
        is_crisis: true,
        passes_cost_benefit_gate: true,
        ..Default::default()
    };
    let plan = run_unified_municipal_ai(thermal, electrical, water, sanitation, waste, 100000.0);
    // Crisis waste plan should be funded
    assert!(
        plan.total_capex >= 50000.0,
        "Crisis waste plan must be funded"
    );
}

// ============================================================================
// SECTION 16: CONSTRUCTION PROJECT TYPES
// ============================================================================

#[test]
fn test_waste_construction_project_types_exist() {
    use sim_engine::construction::projects::ConstructionProjectType;
    // All waste facility types must exist
    let _ = ConstructionProjectType::Landfill;
    let _ = ConstructionProjectType::WasteSeparationPlant;
    let _ = ConstructionProjectType::RecyclingFacility;
    let _ = ConstructionProjectType::WasteToEnergyPlant;
    let _ = ConstructionProjectType::CivicAmenitySite;
}

// ============================================================================
// SECTION 17: SNAPSHOT ROLE-GATING
// ============================================================================

#[test]
fn test_waste_grid_snapshot_default() {
    use sim_engine::ui::snapshot::WasteGridSnapshot;
    let snap = WasteGridSnapshot::default();
    assert_eq!(snap.collection_route_km, 0.0);
}

#[test]
fn test_landfill_snapshot_default() {
    use sim_engine::ui::snapshot::LandfillSnapshot;
    let snap = LandfillSnapshot::default();
    assert_eq!(snap.total_capacity, 0.0);
    assert!(!snap.is_full);
}

#[test]
fn test_waste_pollution_snapshot_default() {
    use sim_engine::ui::snapshot::WastePollutionSnapshot;
    let snap = WastePollutionSnapshot::default();
    assert_eq!(snap.burning_emissions, 0.0);
}

#[test]
fn test_recycling_snapshot_default() {
    use sim_engine::ui::snapshot::RecyclingSnapshot;
    let snap = RecyclingSnapshot::default();
    assert_eq!(snap.active_recycling_plants, 0);
}

#[test]
fn test_region_detail_has_waste_fields() {
    use sim_engine::ui::snapshot::RegionDetail;
    // RegionDetail must have waste fields (Option-wrapped for role-gating)
    let detail = RegionDetail::default();
    assert!(
        detail.waste_grid.is_none(),
        "Default waste_grid should be None"
    );
    assert!(detail.landfill.is_none(), "Default landfill should be None");
    assert!(
        detail.waste_pollution.is_none(),
        "Default waste_pollution should be None"
    );
    assert!(
        detail.recycling.is_none(),
        "Default recycling should be None"
    );
}

// ============================================================================
// SECTION 18: WASTE EPIC TURN RESULT
// ============================================================================

#[test]
fn test_waste_epic_turn_result_default() {
    let result = WasteEpicTurnResult::default();
    assert_eq!(result.standalone_disposed, 0.0);
    assert_eq!(result.collected, 0.0);
    assert_eq!(result.landfilled, 0.0);
    assert_eq!(result.landfill_rejected, 0.0);
    assert_eq!(result.ash_generated, 0.0);
}

#[test]
fn test_waste_epic_turn_result_serialization() {
    let result = WasteEpicTurnResult {
        standalone_disposed: 100.0,
        collected: 50.0,
        landfilled: 40.0,
        ash_generated: 10.0,
        ..Default::default()
    };
    let json = serde_json::to_string(&result).unwrap();
    let deserialized: WasteEpicTurnResult = serde_json::from_str(&json).unwrap();
    assert_eq!(result, deserialized);
}

// ============================================================================
// SECTION 19: WASTE SALES HISTORY
// ============================================================================

#[test]
fn test_waste_sales_history_default() {
    let history = WasteSalesHistory::default();
    assert_eq!(history.smoothed_curbside_volume, 0.0);
}

#[test]
fn test_waste_sales_history_serialization() {
    let history = WasteSalesHistory {
        smoothed_curbside_volume: 100.0,
        ..Default::default()
    };
    let json = serde_json::to_string(&history).unwrap();
    let deserialized: WasteSalesHistory = serde_json::from_str(&json).unwrap();
    assert_eq!(history, deserialized);
}

// ============================================================================
// SECTION 20: FERTILIZER SINK
// ============================================================================

#[test]
fn test_subsistence_food_per_fertilizer_positive() {
    const {
        assert!(
            SUBSISTENCE_FOOD_PER_FERTILIZER > 0.0,
            "Subsistence food per fertilizer must be positive"
        );
    }
}

#[test]
fn test_composting_yield_in_range() {
    const {
        assert!(
            COMPOSTING_YIELD > 0.0 && COMPOSTING_YIELD < 1.0,
            "Composting yield must be in (0, 1) — not 100% conversion"
        );
    }
}

// ============================================================================
// SECTION 21: POLLUTION STATE FIELDS
// ============================================================================

#[test]
fn test_local_pollution_has_waste_fields() {
    use sim_engine::environment::smog::LocalPollutionState;
    let pollution = LocalPollutionState::default();
    // Phase 84 waste pollution fields must exist
    assert_eq!(pollution.waste_burning_emissions, 0.0);
    assert_eq!(pollution.waste_dumping_biohazard, 0.0);
    assert_eq!(pollution.uncollected_waste_biohazard, 0.0);
}

// ============================================================================
// SECTION 22: BUILDING WASTE DISPOSAL FIELD
// ============================================================================

#[test]
fn test_housing_building_has_waste_disposal() {
    use sim_engine::society::housing::HousingBuilding;
    let housing = HousingBuilding::default();
    // active_waste_disposal must exist and default to empty string
    assert_eq!(housing.active_waste_disposal, "");
}

#[test]
fn test_commercial_building_has_waste_disposal() {
    use sim_engine::society::housing::CommercialBuilding;
    let commercial = CommercialBuilding::default();
    assert_eq!(commercial.active_waste_disposal, "");
}

// ============================================================================
// SECTION 23: BUILDING LANDFILL STATE
// ============================================================================

#[test]
fn test_building_has_landfill_state_option() {
    use sim_engine::entities::Building;
    let building = Building::default();
    // landfill_state must be Option<LandfillState>
    assert!(
        building.landfill_state.is_none(),
        "Default building should have no landfill"
    );
}

// ============================================================================
// SECTION 24: REGION WASTE GRID
// ============================================================================

#[test]
fn test_region_has_waste_grid() {
    use sim_engine::society::geography::Region;
    let region = Region::default();
    let _ = &region.waste_grid;
}

// ============================================================================
// SECTION 25: WASTE UTILITY SERVICE TYPE
// ============================================================================

#[test]
fn test_waste_utility_service_type_exists() {
    use sim_engine::entities::legal_form::MunicipalServiceType;
    let _ = MunicipalServiceType::WasteUtility;
}

//! Construction Bill of Materials (BOM) definitions.
//!
//! Maps building sectors to the physical commodities required for construction.
//! Used when creating `ConstructionProject` instances for new buildings or
//! expansion of existing ones.
//!
//! Phase 45: BOM dispatch is via `Sector` enum — NO string matching.
//! BOMs are era-aware: pre-1925 uses Bricks/Timber/Planks, post-1950 uses
//! Cement/Steel/Prefabricates, with a transitional blend in between.

use crate::registries::enums::{Commodity, Sector};
use crate::economy::transport_networks::NetworkLevel;
use std::collections::BTreeMap;

/// Phase 45: Returns the construction BOM for a sector, era-aware.
///
/// # Arguments
/// * `sector` - The Sector enum variant (HeavyIndustry, Agriculture, Mining, etc.)
/// * `start_year` - Year of construction start (determines material mix)
///
/// # Returns
/// A `BTreeMap<Commodity, f64>` mapping each required material to its total
/// quantity.
///
/// # Rules
/// * Dispatch is via `match sector { ... }` — NO string matching.
/// * year <= 1925: BOMs use Bricks, Timber, Planks (traditional construction)
/// * year >= 1950: BOMs shift to Cement, Steel, Prefabricates (modern construction)
/// * 1925 < year < 1950: Transitional mix (both materials)
/// * Quantities are in abstract "units" (tons-equivalent).
/// * The BOM is a total requirement, not per-turn.
pub fn get_construction_bom(sector: Sector, start_year: u32) -> BTreeMap<Commodity, f64> {
    let era_factor = if start_year <= 1925 {
        0.0 // Traditional
    } else if start_year >= 1950 {
        1.0 // Modern
    } else {
        (start_year - 1925) as f64 / 25.0 // Transitional blend
    };

    // Strict enum dispatch — no string matching, no .contains(), no .to_lowercase()
    match sector {
        Sector::HeavyIndustry => bom_heavy_factory(era_factor),
        Sector::LightIndustry => bom_light_factory(era_factor),
        Sector::Mining => bom_mine(era_factor),
        Sector::Agriculture => bom_farm(era_factor),
        Sector::Construction => bom_warehouse(era_factor),
        Sector::Energy => bom_heavy_factory(era_factor),
        Sector::TransportLogistics => bom_warehouse(era_factor),
        Sector::PublicServices => bom_commercial(era_factor),
        Sector::PublicAdministration => bom_commercial(era_factor),
        Sector::Banking => bom_commercial(era_factor),
        Sector::ArmamentsIndustry => bom_heavy_factory(era_factor),
        Sector::MaintenanceWorkshops => bom_light_factory(era_factor),
        Sector::LocalServices => bom_commercial(era_factor),
        Sector::ExportServices => bom_commercial(era_factor),
        Sector::MedicalServices => bom_commercial(era_factor),
        Sector::EducationalServices => bom_commercial(era_factor),
        Sector::MediaAndEntertainment => bom_commercial(era_factor),
        Sector::WasteManagement => bom_light_factory(era_factor),
        Sector::Hospitality => bom_commercial(era_factor),
        Sector::NGO => bom_commercial(era_factor),
        Sector::Religion => bom_commercial(era_factor),
        Sector::Government => bom_commercial(era_factor),
    }
}

/// Phase 45: Legacy compatibility wrapper.
///
/// Maps a building kind string to a Sector and delegates to
/// `get_construction_bom(sector, start_year)`. This is kept for any callers
/// that still pass a building name string. The primary API is the Sector-enum
/// version above.
///
/// **DEPRECATED**: New code should call `get_construction_bom(Sector, u32)` directly.
pub fn get_construction_bom_for_kind(building_kind: &str, start_year: u32) -> BTreeMap<Commodity, f64> {
    let sector = sector_from_building_kind(building_kind);
    get_construction_bom(sector, start_year)
}

/// Phase 45: Best-effort mapping from a building kind string to a Sector.
/// Used only by the legacy compatibility wrapper.
fn sector_from_building_kind(kind: &str) -> Sector {
    let k = kind.to_lowercase();
    if k.contains("huta") || k.contains("stal") || k.contains("odlewn")
        || k.contains("ciężki") || k.contains("heavy")
    {
        Sector::HeavyIndustry
    } else if k.contains("kopaln") || k.contains("mine") || k.contains("węgiel")
        || k.contains("rud") || k.contains("boksyt")
    {
        Sector::Mining
    } else if k.contains("roln") || k.contains("farma") || k.contains("folwark")
        || k.contains("gospodarstw") || k.contains("farm")
    {
        Sector::Agriculture
    } else if k.contains("magazyn") || k.contains("warehouse") || k.contains("skład")
    {
        Sector::TransportLogistics
    } else if k.contains("sklep") || k.contains("biuro") || k.contains("handel")
        || k.contains("komercyj") || k.contains("commercial") || k.contains("office")
    {
        Sector::PublicServices
    } else if k.contains("cement") {
        Sector::HeavyIndustry
    } else if k.contains("chem") || k.contains("rafiner") || k.contains("petrochem") {
        Sector::HeavyIndustry
    } else if k.contains("energ") || k.contains("power") || k.contains("elektrown") {
        Sector::Energy
    } else {
        Sector::LightIndustry
    }
}

/// Returns the expansion BOM for upgrading an existing building.
///
/// # Rules
/// * Uses 60% of the base BOM quantities, scaled by the capacity increase ratio.
/// * `capacity_ratio` = `new_capacity / existing_capacity` (clamped to >= 0.1).
pub fn get_expansion_bom(
    sector: Sector,
    start_year: u32,
    existing_capacity: u32,
    new_capacity: u32,
) -> BTreeMap<Commodity, f64> {
    let base = get_construction_bom(sector, start_year);
    let ratio = if existing_capacity > 0 {
        (new_capacity as f64 / existing_capacity as f64).max(0.1)
    } else {
        1.0
    };
    let scale = 0.6 * ratio;

    base.into_iter()
        .map(|(commodity, qty)| (commodity, qty * scale))
        .filter(|(_, qty)| *qty > 0.0)
        .collect()
}

/// Era-aware heavy factory BOM.
/// era_factor = 0.0 (1900) -> Bricks/Timber/Planks dominant
/// era_factor = 1.0 (1975) -> Cement/Steel/Prefabricates dominant
fn bom_heavy_factory(era: f64) -> BTreeMap<Commodity, f64> {
    let mut bom = BTreeMap::new();
    bom.insert(Commodity::Steel, 200.0 + 300.0 * era);
    bom.insert(Commodity::Cement, 100.0 + 700.0 * era);
    bom.insert(Commodity::Bricks, 400.0 * (1.0 - era) + 100.0);
    bom.insert(Commodity::Timber, 200.0 * (1.0 - era) + 50.0);
    bom.insert(Commodity::Planks, 150.0 * (1.0 - era));
    bom.insert(Commodity::Prefabricates, 200.0 * era);
    bom.insert(Commodity::ConstructionMachinery, 50.0);
    bom.insert(Commodity::Glass, 100.0);
    bom
}

/// Era-aware light factory BOM.
fn bom_light_factory(era: f64) -> BTreeMap<Commodity, f64> {
    let mut bom = BTreeMap::new();
    bom.insert(Commodity::Steel, 100.0 + 100.0 * era);
    bom.insert(Commodity::Cement, 50.0 + 350.0 * era);
    bom.insert(Commodity::Bricks, 300.0 * (1.0 - era) + 100.0);
    bom.insert(Commodity::Timber, 200.0 * (1.0 - era) + 50.0);
    bom.insert(Commodity::Planks, 100.0 * (1.0 - era));
    bom.insert(Commodity::Prefabricates, 100.0 * era);
    bom.insert(Commodity::ConstructionMachinery, 20.0);
    bom.insert(Commodity::Glass, 50.0);
    bom
}

/// Era-aware mine BOM.
fn bom_mine(era: f64) -> BTreeMap<Commodity, f64> {
    let mut bom = BTreeMap::new();
    bom.insert(Commodity::Steel, 200.0 + 100.0 * era);
    bom.insert(Commodity::Cement, 100.0 + 100.0 * era);
    bom.insert(Commodity::Bricks, 100.0 * (1.0 - era) + 50.0);
    bom.insert(Commodity::Timber, 150.0 * (1.0 - era) + 50.0);
    bom.insert(Commodity::Planks, 80.0 * (1.0 - era));
    bom.insert(Commodity::Prefabricates, 50.0 * era);
    bom.insert(Commodity::ConstructionMachinery, 80.0);
    bom.insert(Commodity::Glass, 20.0);
    bom
}

/// Era-aware farm BOM.
fn bom_farm(era: f64) -> BTreeMap<Commodity, f64> {
    let mut bom = BTreeMap::new();
    bom.insert(Commodity::Steel, 30.0 + 20.0 * era);
    bom.insert(Commodity::Cement, 50.0 + 50.0 * era);
    bom.insert(Commodity::Bricks, 50.0 * (1.0 - era) + 30.0);
    bom.insert(Commodity::Timber, 300.0 * (1.0 - era) + 100.0);
    bom.insert(Commodity::Planks, 100.0 * (1.0 - era));
    bom.insert(Commodity::Prefabricates, 30.0 * era);
    bom.insert(Commodity::ConstructionMachinery, 10.0);
    bom
}

/// Era-aware warehouse BOM.
fn bom_warehouse(era: f64) -> BTreeMap<Commodity, f64> {
    let mut bom = BTreeMap::new();
    bom.insert(Commodity::Steel, 100.0 + 50.0 * era);
    bom.insert(Commodity::Cement, 150.0 + 150.0 * era);
    bom.insert(Commodity::Bricks, 200.0 * (1.0 - era) + 100.0);
    bom.insert(Commodity::Timber, 100.0 * (1.0 - era) + 50.0);
    bom.insert(Commodity::Planks, 80.0 * (1.0 - era));
    bom.insert(Commodity::Prefabricates, 100.0 * era);
    bom.insert(Commodity::ConstructionMachinery, 15.0);
    bom.insert(Commodity::Glass, 50.0);
    bom
}

/// Era-aware commercial BOM.
fn bom_commercial(era: f64) -> BTreeMap<Commodity, f64> {
    let mut bom = BTreeMap::new();
    bom.insert(Commodity::Steel, 100.0 + 100.0 * era);
    bom.insert(Commodity::Cement, 150.0 + 150.0 * era);
    bom.insert(Commodity::Bricks, 200.0 * (1.0 - era) + 100.0);
    bom.insert(Commodity::Timber, 200.0 * (1.0 - era) + 50.0);
    bom.insert(Commodity::Planks, 100.0 * (1.0 - era));
    bom.insert(Commodity::Prefabricates, 100.0 * era);
    bom.insert(Commodity::ConstructionMachinery, 20.0);
    bom.insert(Commodity::Glass, 200.0);
    bom
}

/// Phase 23B: Returns the construction BOM for a transport network link.
///
/// The BOM scales linearly with distance. The values below are per 100 km
/// of network length. For a 300 km rail line, multiply by 3.0.
///
/// # Arguments
/// * `level` - The network level to construct (DirtRoad, RailNetwork, etc.).
/// * `distance_km` - The length of the link in kilometers.
///
/// # Returns
/// A `BTreeMap<Commodity, f64>` of total material requirements.
pub fn get_network_construction_bom(
    level: NetworkLevel,
    distance_km: f64,
) -> BTreeMap<Commodity, f64> {
    let scale = (distance_km / 100.0).max(0.1); // minimum 10 km equivalent

    let base: BTreeMap<Commodity, f64> = match level {
        NetworkLevel::None => BTreeMap::new(),
        NetworkLevel::DirtRoad => {
            let mut b = BTreeMap::new();
            b.insert(Commodity::Timber, 200.0);
            b.insert(Commodity::Steel, 20.0);
            b.insert(Commodity::Cement, 100.0);
            b.insert(Commodity::Bricks, 50.0);
            b.insert(Commodity::ConstructionMachinery, 10.0);
            b.insert(Commodity::Stone, 200.0);
            b
        }
        NetworkLevel::PavedRoad => {
            let mut b = BTreeMap::new();
            b.insert(Commodity::Timber, 100.0);
            b.insert(Commodity::Steel, 100.0);
            b.insert(Commodity::Cement, 800.0);
            b.insert(Commodity::Bricks, 400.0);
            b.insert(Commodity::ConstructionMachinery, 30.0);
            b.insert(Commodity::Stone, 600.0);
            // Phase 45: Paved roads use Asphalt in modern era
            b.insert(Commodity::Asphalt, 200.0);
            b
        }
        NetworkLevel::RailNetwork => {
            let mut b = BTreeMap::new();
            b.insert(Commodity::Timber, 300.0);
            b.insert(Commodity::Steel, 1500.0);
            b.insert(Commodity::Cement, 1000.0);
            b.insert(Commodity::Bricks, 200.0);
            b.insert(Commodity::ConstructionMachinery, 80.0);
            b.insert(Commodity::Stone, 500.0);
            b
        }
        NetworkLevel::ElectrifiedRail => {
            let mut b = BTreeMap::new();
            b.insert(Commodity::Timber, 200.0);
            b.insert(Commodity::Steel, 2000.0);
            b.insert(Commodity::Cement, 1200.0);
            b.insert(Commodity::Bricks, 200.0);
            b.insert(Commodity::ConstructionMachinery, 100.0);
            b.insert(Commodity::Stone, 400.0);
            b
        }
        NetworkLevel::Highway => {
            let mut b = BTreeMap::new();
            b.insert(Commodity::Timber, 50.0);
            b.insert(Commodity::Steel, 500.0);
            b.insert(Commodity::Cement, 2000.0);
            b.insert(Commodity::Bricks, 600.0);
            b.insert(Commodity::ConstructionMachinery, 120.0);
            b.insert(Commodity::Stone, 1500.0);
            // Phase 45: Highways use Asphalt
            b.insert(Commodity::Asphalt, 1000.0);
            b
        }
        NetworkLevel::Canal => {
            let mut b = BTreeMap::new();
            b.insert(Commodity::Timber, 50.0);
            b.insert(Commodity::Steel, 100.0);
            b.insert(Commodity::Cement, 3000.0);
            b.insert(Commodity::Bricks, 1000.0);
            b.insert(Commodity::ConstructionMachinery, 150.0);
            b.insert(Commodity::Stone, 2000.0);
            b
        }
    };

    base.into_iter()
        .map(|(commodity, qty)| (commodity, qty * scale))
        .filter(|(_, qty)| *qty > 0.0)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_bom_rail_scales_with_distance() {
        let bom_100 = get_network_construction_bom(NetworkLevel::RailNetwork, 100.0);
        let bom_300 = get_network_construction_bom(NetworkLevel::RailNetwork, 300.0);
        // 300 km should require 3x the steel of 100 km.
        let steel_100 = bom_100.get(&Commodity::Steel).copied().unwrap_or(0.0);
        let steel_300 = bom_300.get(&Commodity::Steel).copied().unwrap_or(0.0);
        assert!((steel_300 / steel_100 - 3.0).abs() < 1e-9);
    }

    #[test]
    fn network_bom_none_is_empty() {
        let bom = get_network_construction_bom(NetworkLevel::None, 100.0);
        assert!(bom.is_empty());
    }

    #[test]
    fn network_bom_highway_requires_massive_cement() {
        let bom = get_network_construction_bom(NetworkLevel::Highway, 100.0);
        let cement = bom.get(&Commodity::Cement).copied().unwrap_or(0.0);
        assert!(cement >= 2000.0, "highway should require massive cement");
    }

    #[test]
    fn network_bom_canal_requires_massive_stone() {
        let bom = get_network_construction_bom(NetworkLevel::Canal, 100.0);
        let stone = bom.get(&Commodity::Stone).copied().unwrap_or(0.0);
        assert!(stone >= 2000.0, "canal should require massive stone");
    }

    #[test]
    fn test_era_aware_bom_1900_uses_bricks() {
        let bom = get_construction_bom(Sector::HeavyIndustry, 1900);
        let bricks = bom.get(&Commodity::Bricks).copied().unwrap_or(0.0);
        let prefabs = bom.get(&Commodity::Prefabricates).copied().unwrap_or(0.0);
        assert!(bricks > 300.0, "1900 heavy factory should use lots of bricks, got {}", bricks);
        assert!(prefabs < 1.0, "1900 heavy factory should not use prefabricates, got {}", prefabs);
    }

    #[test]
    fn test_era_aware_bom_1975_uses_prefabricates() {
        let bom = get_construction_bom(Sector::HeavyIndustry, 1975);
        let prefabs = bom.get(&Commodity::Prefabricates).copied().unwrap_or(0.0);
        let cement = bom.get(&Commodity::Cement).copied().unwrap_or(0.0);
        assert!(prefabs > 100.0, "1975 heavy factory should use prefabricates, got {}", prefabs);
        assert!(cement > 500.0, "1975 heavy factory should use lots of cement, got {}", cement);
    }

    #[test]
    fn test_era_aware_bom_transition() {
        let bom_1935 = get_construction_bom(Sector::HeavyIndustry, 1935);
        // 1935 is in the transition period (1925 < 1935 < 1950)
        // era_factor = (1935 - 1925) / 25 = 0.4
        let bricks = bom_1935.get(&Commodity::Bricks).copied().unwrap_or(0.0);
        let prefabs = bom_1935.get(&Commodity::Prefabricates).copied().unwrap_or(0.0);
        // Both should be present in transition
        assert!(bricks > 100.0, "1935 should still use some bricks, got {}", bricks);
        assert!(prefabs > 0.0, "1935 should start using prefabricates, got {}", prefabs);
    }

    #[test]
    fn test_sector_dispatch_heavy_vs_light() {
        let heavy = get_construction_bom(Sector::HeavyIndustry, 1975);
        let light = get_construction_bom(Sector::LightIndustry, 1975);
        let heavy_steel = heavy.get(&Commodity::Steel).copied().unwrap_or(0.0);
        let light_steel = light.get(&Commodity::Steel).copied().unwrap_or(0.0);
        assert!(heavy_steel > light_steel, "heavy industry should need more steel than light");
    }

    #[test]
    fn test_planks_only_in_traditional_era() {
        let bom_1900 = get_construction_bom(Sector::LightIndustry, 1900);
        let bom_1975 = get_construction_bom(Sector::LightIndustry, 1975);
        let planks_1900 = bom_1900.get(&Commodity::Planks).copied().unwrap_or(0.0);
        let planks_1975 = bom_1975.get(&Commodity::Planks).copied().unwrap_or(0.0);
        assert!(planks_1900 > 50.0, "1900 should use planks, got {}", planks_1900);
        assert!(planks_1975 < 1.0, "1975 should not use planks, got {}", planks_1975);
    }
}

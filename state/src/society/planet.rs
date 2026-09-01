//! Phase 87+: Top-Down Planetary Generation and Geological Vein System.
//!
//! Replaces the flat `generate_geological_formations` system with a
//! planet-wide `GeologicalVein` architecture. Veins are generated globally
//! with rarity tiers (UltraRare to Ubiquitous), assigned to lat/lon points,
//! and mapped to overlapping regions. This ensures resource scarcity is
//! physically coherent rather than randomly scattered.
//!
//! # Architecture
//! - `Planet` is stored on `GameState` (global, not per-country).
//! - Each `GeologicalVein` has a `RarityTier` controlling global count and reserves.
//! - Veins that overlap the same region and commodity are merged via `composite_id`.
//! - Regions query the `Planet` for veins intersecting their territory.

use crate::registries::enums::Commodity;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

/// Rarity tier for geological veins. Controls global vein count and reserve size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RarityTier {
    /// Max 4-8 globally. Uranium, Diamonds, Gold. High value, tiny reserves.
    UltraRare,
    /// Max 8-15 globally. Silver, Platinum, Tin. High value, small reserves.
    Rare,
    /// Max 10-20 globally. Copper, Zinc, Bauxite. Medium value, medium reserves.
    Uncommon,
    /// 12-25 globally. Iron, HardCoal, BrownCoal, Stone, Sand. Low value, large reserves.
    AbundantIndustrial,
    /// 20-40 globally. Limestone, Peat, Gravel. Very low value, huge reserves.
    Ubiquitous,
}

impl RarityTier {
    /// Target vein count range for this tier (min, max).
    fn count_range(&self) -> (usize, usize) {
        match self {
            RarityTier::UltraRare => (4, 8),
            RarityTier::Rare => (8, 15),
            RarityTier::Uncommon => (10, 20),
            RarityTier::AbundantIndustrial => (12, 25),
            RarityTier::Ubiquitous => (20, 40),
        }
    }

    /// Reserve size range for a single vein of this tier (min, max) in tons.
    fn reserve_range(&self) -> (f64, f64) {
        match self {
            RarityTier::UltraRare => (1_000_000.0, 10_000_000.0),
            RarityTier::Rare => (5_000_000.0, 50_000_000.0),
            RarityTier::Uncommon => (20_000_000.0, 200_000_000.0),
            RarityTier::AbundantIndustrial => (100_000_000.0, 1_000_000_000.0),
            RarityTier::Ubiquitous => (500_000_000.0, 5_000_000_000.0),
        }
    }

    /// Commodities available at this rarity tier.
    fn commodities(&self) -> &[Commodity] {
        match self {
            RarityTier::UltraRare => &[
                Commodity::Uranium,
                Commodity::Gold,
            ],
            RarityTier::Rare => &[
                Commodity::Silver,
                Commodity::Tin,
            ],
            RarityTier::Uncommon => &[
                Commodity::Copper,
                Commodity::Zinc,
                Commodity::Bauxite,
            ],
            RarityTier::AbundantIndustrial => &[
                Commodity::Iron,
                Commodity::HardCoal,
                Commodity::BrownCoal,
                Commodity::Stone,
                Commodity::Sand,
            ],
            RarityTier::Ubiquitous => &[
                Commodity::Limestone,
                Commodity::Peat,
                Commodity::Gravel,
            ],
        }
    }
}

/// Phase 88: Generate a deterministic human-readable name for a geological vein.
/// Uses the commodity name and a geographic descriptor derived from the vein
/// ID counter (ensuring deterministic names across reloads).
fn generate_vein_name(commodity: Commodity, id_counter: usize) -> String {
    let commodity_str = format!("{:?}", commodity);
    // Deterministic geographic descriptor based on ID counter.
    // This produces stable names like "Northern Iron Range", "Southern Coal Basin".
    let descriptors = [
        ("Northern", "Range"),
        ("Southern", "Basin"),
        ("Eastern", "Belt"),
        ("Western", "District"),
        ("Central", "Formation"),
        ("Highland", "Deposit"),
        ("Lowland", "Field"),
        ("Coastal", "Zone"),
    ];
    let (direction, suffix) = descriptors[id_counter % descriptors.len()];
    format!("{} {} {}", direction, commodity_str, suffix)
}

/// A geological vein — a coherent deposit of a single commodity spanning
/// one or more regions. Veins are the authoritative source of geological
/// resources for mining and power-plant generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeologicalVein {
    /// Unique vein identifier (e.g., "VEIN-001").
    pub id: String,
    /// If this vein was merged with others, the composite ID referencing all parents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composite_id: Option<String>,
    /// Phase 88: Human-readable name for UI display (e.g., "Silesian Coal Basin").
    pub name: String,
    /// The commodity this vein contains.
    pub commodity: Commodity,
    /// Rarity tier controlling scarcity.
    pub rarity_tier: RarityTier,
    /// Total original reserves (tons).
    pub total_reserves: f64,
    /// Current remaining reserves (tons). Decreases as mines extract.
    pub current_reserves: f64,
    /// (lat, lon) points defining the vein's geographic extent.
    pub cells: Vec<(f64, f64)>,
    /// Region IDs that this vein overlaps.
    pub overlapping_regions: Vec<String>,
    /// Extraction cost multiplier (1.0 = average, higher = harder to extract).
    pub extraction_cost: f64,
    /// Ore quality (0.0-1.0). Higher = more valuable per ton.
    pub quality: f64,
    /// Average depth in meters (affects extraction cost).
    pub depth: f64,
    /// Whether this vein has been discovered by geological survey.
    /// Undiscovered veins are invisible to unauthorized observers (Rule 11).
    #[serde(default)]
    pub discovered: bool,
}

/// A cell on the planet grid, mapping a lat/lon point to a region.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanetCell {
    /// Latitude (-90.0 to 90.0).
    pub lat: f64,
    /// Longitude (-180.0 to 180.0).
    pub lon: f64,
    /// Region ID occupying this cell, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region_id: Option<String>,
}

/// The planet — a global structure holding all geological veins and the
/// lat/lon grid mapping regions to planetary coordinates.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Planet {
    /// Grid cells mapping lat/lon to region IDs.
    #[serde(default)]
    pub grid_cells: Vec<PlanetCell>,
    /// All geological veins on the planet.
    #[serde(default)]
    pub veins: Vec<GeologicalVein>,
}

impl Planet {
    /// Generate the planet's geological veins.
    ///
    /// # Arguments
    /// * `regions` - All regions with their `coord_x`/`coord_y` mapped to lat/lon.
    ///   The mapping uses the region's coordinates to determine which veins overlap.
    /// * `rng` - Random number generator for deterministic generation.
    pub fn generate_veins<R: Rng>(
        &mut self,
        regions: &[(String, f64, f64)], // (region_id, lat, lon)
        rng: &mut R,
    ) {
        let mut vein_id_counter = 1;
        let tiers = [
            RarityTier::UltraRare,
            RarityTier::Rare,
            RarityTier::Uncommon,
            RarityTier::AbundantIndustrial,
            RarityTier::Ubiquitous,
        ];

        for &tier in &tiers {
            let (min_count, max_count) = tier.count_range();
            let count = rng.gen_range(min_count..=max_count);
            let commodities = tier.commodities();
            let (min_reserves, max_reserves) = tier.reserve_range();

            for _ in 0..count {
                let commodity = commodities[rng.gen_range(0..commodities.len())];
                let lat = rng.gen_range(-80.0..80.0);
                let lon = rng.gen_range(-170.0..170.0);
                let total_reserves = rng.gen_range(min_reserves..max_reserves);
                let quality = rng.gen_range(0.3..1.0);
                let depth = rng.gen_range(50.0..2000.0);
                let extraction_cost = 1.0 + (depth / 1000.0) + (1.0 - quality) * 0.5;

                // Find overlapping regions (within ~10 degrees lat/lon).
                let overlapping_regions: Vec<String> = regions
                    .iter()
                    .filter(|(_, r_lat, r_lon)| {
                        let dlat = (r_lat - lat).abs();
                        let dlon = (r_lon - lon).abs();
                        dlat < 10.0 && dlon < 10.0
                    })
                    .map(|(id, _, _)| id.clone())
                    .collect();

                let vein_id = format!("VEIN-{:04}", vein_id_counter);
                let vein_name = generate_vein_name(commodity, vein_id_counter);
                vein_id_counter += 1;

                self.veins.push(GeologicalVein {
                    id: vein_id,
                    composite_id: None,
                    name: vein_name,
                    commodity,
                    rarity_tier: tier,
                    total_reserves,
                    current_reserves: total_reserves,
                    cells: vec![(lat, lon)],
                    overlapping_regions,
                    extraction_cost,
                    quality,
                    depth,
                    discovered: false,
                });
            }
        }

        // Merge overlapping veins of the same commodity in the same region.
        self.merge_overlapping_veins();
    }

    /// Merge veins of the same commodity that overlap the same region.
    /// Creates a composite ID and sums their reserves.
    fn merge_overlapping_veins(&mut self) {
        // Group veins by (commodity, region) to find merge candidates.
        let mut merge_groups: std::collections::HashMap<(Commodity, String), Vec<usize>> =
            std::collections::HashMap::new();

        for (idx, vein) in self.veins.iter().enumerate() {
            for region_id in &vein.overlapping_regions {
                merge_groups
                    .entry((vein.commodity, region_id.clone()))
                    .or_default()
                    .push(idx);
            }
        }

        // For groups with more than one vein, assign composite IDs.
        // The veins remain separate structs but share a composite_id for
        // reference. Reserve summation happens at query time.
        let mut composite_counter = 1;
        for indices in merge_groups.values() {
            if indices.len() < 2 {
                continue;
            }
            let composite_id = format!("COMPOSITE-{:04}", composite_counter);
            composite_counter += 1;
            for &idx in indices {
                self.veins[idx].composite_id = Some(composite_id.clone());
            }
        }
    }

    /// Phase 89: Mark base industrial veins in populated regions as discovered.
    ///
    /// Base industrial commodities (AbundantIndustrial + Ubiquitous + Uncommon tiers)
    /// are auto-discovered during world generation because they are surface-visible
    /// deposits that any settled civilization would know about. Rare and UltraRare
    /// veins (gold, silver, uranium, etc.) remain hidden — they require geological
    /// survey and represent exploration gameplay.
    ///
    /// Only veins overlapping at least one populated region are discovered.
    /// Unpopulated regions (e.g., polar, desert) keep their veins hidden.
    ///
    /// # Arguments
    /// * `populated_region_ids` - Set of region IDs with population > 0.
    pub fn discover_base_industrial_veins(&mut self, populated_region_ids: &std::collections::HashSet<String>) {
        for vein in &mut self.veins {
            // Rare and UltraRare veins stay hidden — they require geological survey.
            if vein.rarity_tier == RarityTier::UltraRare || vein.rarity_tier == RarityTier::Rare {
                continue;
            }
            // AbundantIndustrial, Ubiquitous, and Uncommon tiers are base industrial.
            // Only discover if the vein overlaps at least one populated region.
            let overlaps_populated = vein.overlapping_regions.iter()
                .any(|r| populated_region_ids.contains(r));
            if overlaps_populated {
                vein.discovered = true;
            }
        }
    }

    /// Phase 90: Ensure each populated region has at least one vein for each
    /// AbundantIndustrial and Ubiquitous commodity. This fixes the Limestone
    /// monoculture by guaranteeing diverse base industrial deposits.
    ///
    /// This is NOT magic spawning — it represents the geological reality that
    /// any settled region has surface-visible deposits of common industrial
    /// minerals (Iron, Coal, Stone, Sand, Limestone, etc.). Rare and UltraRare
    /// veins remain hidden and are not affected by this method.
    ///
    /// Phase 93 (Geology Remediation): The old modulo-based hash sizing
    /// produced cloned reserve values across regions (e.g. Peat always
    /// spawning at exactly 1.22B). This is replaced by a per-region-commodity
    /// seeded PRNG (`StdRng::seed_from_u64`) with heavy-tailed distributions
    /// so deposits fluctuate radically between regions while remaining
    /// fully deterministic and reproducible.
    ///
    /// # Arguments
    /// * `populated_regions` - Slice of (region_id, lat, lon) tuples for
    ///   regions with population > 0.
    /// * `_rng` - Unused (kept for API compatibility); per-vein randomness
    ///   is seeded deterministically from the region-commodity hash.
    pub fn ensure_base_industrial_veins_per_region<R: Rng>(
        &mut self,
        populated_regions: &[(String, f64, f64)],
        _rng: &mut R,
    ) {
        // Phase 92: Geographic vein diversity — not every region gets every
        // commodity. Split into ubiquitous (construction materials, ~80%
        // presence) and industrial (Iron/Coal, ~35% presence). Use a
        // deterministic hash of (region_id, commodity) to decide presence,
        // ensuring reproducibility and geographic diversity.
        //
        // This creates distinct regional resource profiles: some regions are
        // mining hubs (Iron + Coal), others have only construction materials,
        // and some are resource-poor. The market transports resources via
        // freight, creating trade flows.

        /// Deterministic hash for (region_id, commodity) → u64.
        fn region_commodity_hash(region_id: &str, commodity: Commodity) -> u64 {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            region_id.hash(&mut hasher);
            commodity.hash(&mut hasher);
            hasher.finish()
        }

        /// Deterministic hash for (region_id, commodity, salt) → u64.
        /// Used to seed independent randomness streams for reserves, quality,
        /// and depth so they are statistically independent.
        fn region_commodity_salt_hash(region_id: &str, commodity: Commodity, salt: u64) -> u64 {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            region_id.hash(&mut hasher);
            commodity.hash(&mut hasher);
            salt.hash(&mut hasher);
            hasher.finish()
        }

        // Ubiquitous commodities: present in ~80% of regions.
        let ubiquitous: &[(Commodity, RarityTier)] = &[
            (Commodity::Stone, RarityTier::AbundantIndustrial),
            (Commodity::Sand, RarityTier::AbundantIndustrial),
            (Commodity::Limestone, RarityTier::Ubiquitous),
            (Commodity::Peat, RarityTier::Ubiquitous),
            (Commodity::Gravel, RarityTier::Ubiquitous),
        ];

        // Industrial commodities: present in ~35% of regions each.
        let industrial: &[(Commodity, RarityTier)] = &[
            (Commodity::Iron, RarityTier::AbundantIndustrial),
            (Commodity::HardCoal, RarityTier::AbundantIndustrial),
            (Commodity::BrownCoal, RarityTier::AbundantIndustrial),
        ];

        for (region_id, lat, lon) in populated_regions {
            // Process ubiquitous commodities (80% presence threshold).
            for &(commodity, tier) in ubiquitous {
                let already_has = self.veins.iter().any(|v| {
                    v.commodity == commodity
                        && v.overlapping_regions.iter().any(|r| r == region_id)
                });
                if already_has {
                    continue;
                }

                // Deterministic presence check: hash % 100 < 80 → present
                let h = region_commodity_hash(region_id, commodity);
                if h % 100 >= 80 {
                    continue; // This region doesn't have this commodity
                }

                // Phase 93: Seed independent PRNGs for reserves, quality, depth.
                // Heavy-tailed distributions produce radical variance between
                // regions while remaining fully deterministic.
                let (min_reserves, max_reserves) = tier.reserve_range();

                // Reserves: heavy-tailed via powf(0.4) — most deposits small,
                // occasional bonanzas. Full range [min, max] is accessible.
                let mut reserves_rng =
                    StdRng::seed_from_u64(region_commodity_salt_hash(region_id, commodity, 1));
                let reserve_uniform = reserves_rng.gen::<f64>(); // 0.0–1.0
                let reserve_factor = reserve_uniform.powf(0.4); // heavy-tailed
                let total_reserves = min_reserves + (max_reserves - min_reserves) * reserve_factor;

                // Quality: independent stream. Range 0.1–1.0 with heavy tail
                // toward lower quality (most deposits are mediocre).
                let mut quality_rng =
                    StdRng::seed_from_u64(region_commodity_salt_hash(region_id, commodity, 2));
                let quality_uniform = quality_rng.gen::<f64>();
                let quality = 0.1 + quality_uniform.powf(0.6) * 0.9; // 0.1–1.0

                // Depth: independent stream. Range 20–2500m with heavy tail
                // toward shallower deposits (most are near-surface).
                let mut depth_rng =
                    StdRng::seed_from_u64(region_commodity_salt_hash(region_id, commodity, 3));
                let depth_uniform = depth_rng.gen::<f64>();
                let depth = 20.0 + depth_uniform.powf(0.5) * 2480.0; // 20–2500

                let extraction_cost = 1.0 + (depth / 1000.0) + (1.0 - quality) * 0.5;

                let vein_id = format!("VEIN-{:04}", self.veins.len() + 1);
                let vein_name = generate_vein_name(commodity, self.veins.len() + 1);

                self.veins.push(GeologicalVein {
                    id: vein_id,
                    composite_id: None,
                    name: vein_name,
                    commodity,
                    rarity_tier: tier,
                    total_reserves,
                    current_reserves: total_reserves,
                    cells: vec![(*lat, *lon)],
                    overlapping_regions: vec![region_id.clone()],
                    extraction_cost,
                    quality,
                    depth,
                    discovered: false,
                });
            }

            // Process industrial commodities (35% presence threshold).
            for &(commodity, tier) in industrial {
                let already_has = self.veins.iter().any(|v| {
                    v.commodity == commodity
                        && v.overlapping_regions.iter().any(|r| r == region_id)
                });
                if already_has {
                    continue;
                }

                // Deterministic presence check: hash % 100 < 35 → present
                let h = region_commodity_hash(region_id, commodity);
                if h % 100 >= 35 {
                    continue; // This region doesn't have this industrial commodity
                }

                // Phase 93: Same heavy-tailed variance as ubiquitous.
                let (min_reserves, max_reserves) = tier.reserve_range();

                let mut reserves_rng =
                    StdRng::seed_from_u64(region_commodity_salt_hash(region_id, commodity, 1));
                let reserve_uniform = reserves_rng.gen::<f64>();
                let reserve_factor = reserve_uniform.powf(0.4);
                let total_reserves = min_reserves + (max_reserves - min_reserves) * reserve_factor;

                let mut quality_rng =
                    StdRng::seed_from_u64(region_commodity_salt_hash(region_id, commodity, 2));
                let quality_uniform = quality_rng.gen::<f64>();
                let quality = 0.1 + quality_uniform.powf(0.6) * 0.9;

                let mut depth_rng =
                    StdRng::seed_from_u64(region_commodity_salt_hash(region_id, commodity, 3));
                let depth_uniform = depth_rng.gen::<f64>();
                let depth = 20.0 + depth_uniform.powf(0.5) * 2480.0;

                let extraction_cost = 1.0 + (depth / 1000.0) + (1.0 - quality) * 0.5;

                let vein_id = format!("VEIN-{:04}", self.veins.len() + 1);
                let vein_name = generate_vein_name(commodity, self.veins.len() + 1);

                self.veins.push(GeologicalVein {
                    id: vein_id,
                    composite_id: None,
                    name: vein_name,
                    commodity,
                    rarity_tier: tier,
                    total_reserves,
                    current_reserves: total_reserves,
                    cells: vec![(*lat, *lon)],
                    overlapping_regions: vec![region_id.clone()],
                    extraction_cost,
                    quality,
                    depth,
                    discovered: false,
                });
            }
        }
    }

    /// Get all veins overlapping a given region.
    pub fn veins_for_region(&self, region_id: &str) -> Vec<&GeologicalVein> {
        self.veins
            .iter()
            .filter(|v| v.overlapping_regions.iter().any(|r| r == region_id))
            .collect()
    }

    /// Get all veins overlapping a region that contain a specific commodity.
    pub fn veins_for_region_and_commodity(
        &self,
        region_id: &str,
        commodity: Commodity,
    ) -> Vec<&GeologicalVein> {
        self.veins
            .iter()
            .filter(|v| {
                v.commodity == commodity
                    && v.overlapping_regions.iter().any(|r| r == region_id)
            })
            .collect()
    }

    /// Check if a region has a geological resource (any vein with that commodity).
    pub fn has_geological_resource(&self, region_id: &str, commodity: Commodity) -> bool {
        !self.veins_for_region_and_commodity(region_id, commodity).is_empty()
    }

    /// Get the total remaining reserves of a commodity in a region.
    pub fn region_reserves(&self, region_id: &str, commodity: Commodity) -> f64 {
        self.veins_for_region_and_commodity(region_id, commodity)
            .iter()
            .map(|v| v.current_reserves)
            .sum()
    }

    /// Phase 93: Get indices of all undiscovered veins of a specific commodity
    /// overlapping a given region. Used by the geological survey resolution
    /// phase to find hidden Rare/UltraRare deposits that a survey can reveal.
    /// Returns indices (not references) so the caller can mutate the planet
    /// while iterating.
    pub fn undiscovered_vein_indices_for_region_and_commodity(
        &self,
        region_id: &str,
        commodity: Commodity,
    ) -> Vec<usize> {
        self.veins
            .iter()
            .enumerate()
            .filter(|(_, v)| {
                !v.discovered
                    && v.commodity == commodity
                    && v.overlapping_regions.iter().any(|r| r == region_id)
            })
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Phase 93: Find a vein by its ID (or composite_id).
    pub fn vein_by_id(&self, vein_id: &str) -> Option<&GeologicalVein> {
        self.veins.iter().find(|v| {
            v.id == vein_id
                || v.composite_id.as_deref() == Some(vein_id)
        })
    }

    /// Phase 93: Find a mutable vein by its ID (or composite_id).
    pub fn vein_by_id_mut(&mut self, vein_id: &str) -> Option<&mut GeologicalVein> {
        self.veins.iter_mut().find(|v| {
            v.id == vein_id
                || v.composite_id.as_deref() == Some(vein_id)
        })
    }
}

/// Generate a planet with geological veins from the given regions.
///
/// # Arguments
/// * `regions` - Slice of (region_id, lat, lon) tuples for all world regions.
/// * `rng` - Random number generator.
pub fn generate_planet<R: Rng>(
    regions: &[(String, f64, f64)],
    rng: &mut R,
) -> Planet {
    let mut planet = Planet::default();

    // Build grid cells from regions.
    for (region_id, lat, lon) in regions {
        planet.grid_cells.push(PlanetCell {
            lat: *lat,
            lon: *lon,
            region_id: Some(region_id.clone()),
        });
    }

    planet.generate_veins(regions, rng);
    planet
}

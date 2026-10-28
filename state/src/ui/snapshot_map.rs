//! v4.24.0 WS5 — Interactive procedural world map snapshot.
//!
//! Builds a deterministic planar layout for every land region of every
//! country and ships it to the Tauri frontend as Voronoi-style polygon
//! cells so the Map tab can render real borders with plain SVG.
//!
//! # Determinism contract
//!
//! Region `coord_x`/`coord_y` are `(0, 0)` on generated worlds (the Phase 30
//! spring migration is only exercised in tests), so the layout below does
//! NOT rely on them being populated — when they ARE populated they are
//! used as relative offsets within the country's cell. Everything else is
//! derived from sorted country names / sorted region ids plus a stable
//! FNV-1a string hash. No `HashMap` iteration order, no RNG — the same
//! `GameState` always produces a byte-identical snapshot.
//!
//! # Layout
//!
//! * Countries are anchored on a sorted-name grid with a deterministic
//!   per-name jitter, spaced by the largest country radius plus a sea gap.
//! * Inside a country, the capital sits at the anchor and remaining
//!   regions are placed on concentric rings (6, 12, 18, …) in creation
//!   order so the circular `LandBorder` adjacency maps onto neighbouring
//!   cells. A small hash jitter breaks the hex-grid symmetry.
//! * Each region polygon is its Voronoi cell — the country boundary blob
//!   clipped by the perpendicular bisectors against every sibling site —
//!   so shared borders are exact straight lines.
//! * Regions whose cell touches the country boundary are flagged coastal
//!   when the engine itself records no coastline (maritime graph nodes
//!   are not generated on the live path).

use crate::military::fronts::RegionControl;
use crate::society::geography::{EdgeType, NodeType, Region};
use crate::state::{Country, GameState};

/// A single 2D point in world-map space (procedural units, not lat/lon).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MapPoint {
    pub x: f64,
    pub y: f64,
}

/// One region polygon on the world map.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MapRegionDto {
    /// Region id (`"{country}-Region{N}"`), globally unique.
    pub id: String,
    /// Owning country name.
    pub country: String,
    /// Display name (`display_name` with `id` fallback).
    pub name: String,
    /// Ordered polygon vertices (world-map units, y-down for SVG).
    pub polygon: Vec<MapPoint>,
    /// Label anchor — the layout site the polygon was grown around.
    pub centroid: MapPoint,
    /// `Region.is_capital` — rendered as the capital-district star.
    pub is_capital_district: bool,
    /// Region borders the sea frontier (engine traits/edges when present,
    /// else procedural: its cell touches the country boundary).
    pub is_coastal: bool,
    /// Phase 85B emancipated City Region (`city_metadata.is_some()`).
    pub is_city_region: bool,
    pub population: i64,
    /// Regional GDP (currency units).
    pub gdp: f64,
    /// Regional GDP per capita (`Region.gdp_pc`).
    pub gdp_per_capita: f64,
    /// Cadastre devastation index, 0.0 pristine – 1.0 total ruin.
    pub devastation: f64,
    /// Urban-class share of the region's demographic population (0–1).
    pub urbanization: f64,
    /// Phase 47 development level (0–1).
    pub development_level: f64,
    /// Military control: `owner` | `occupied` | `contested` | `rebel_control`.
    pub control: String,
    /// Occupying/contesting country when `control == "occupied"`.
    pub controller: String,
    /// A battle was recorded at this region within the last few turns.
    pub active_battle: bool,
    /// Region belongs to a currently-active military front.
    pub on_front: bool,
    /// Land-border neighbour region ids (engine edge graph).
    pub neighbors: Vec<String>,
}

/// One land-border adjacency edge between two regions (deduplicated).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MapEdgeDto {
    pub from_region: String,
    pub to_region: String,
    /// `land_border` | `coastline` | `sea_lane` | `river`.
    pub edge_type: String,
}

/// A country's slice of the world map.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct CountryMapDto {
    /// Country name (map key in `GameState.countries`).
    pub country: String,
    /// Deterministic index (sorted country order) for the UI palette.
    pub color_index: u32,
    /// Countries this nation is currently at war with.
    pub at_war_with: Vec<String>,
    /// Display name of the capital region, for the country label.
    pub capital_name: String,
    /// Country-label anchor (the capital's layout site).
    pub anchor: MapPoint,
    /// Outer boundary polygon the region cells tile (sea frontier).
    pub boundary: Vec<MapPoint>,
    pub regions: Vec<MapRegionDto>,
}

/// Complete world-map snapshot for the Map tab and mini-map embeds.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct WorldMapSnapshot {
    /// Current game turn (`calendar.global_turn`).
    pub turn: u32,
    /// `[min_x, min_y, max_x, max_y]` of every boundary vertex.
    pub bounds: Vec<f64>,
    pub countries: Vec<CountryMapDto>,
    /// Land-border adjacency edges across the whole world.
    pub edges: Vec<MapEdgeDto>,
}

// ============================================================================
// Deterministic hashing / layout constants
// ============================================================================

/// FNV-1a 64 — stable across runs and platforms (no `HashMap` seeding).
fn stable_hash(s: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Hash a string with a salt into `[0, 1)` — splitmix64 finalizer for
/// well-scattered jitter coordinates.
fn hash_unit(s: &str, salt: u64) -> f64 {
    let mut x = stable_hash(s) ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    ((x >> 11) as f64) / ((1u64 << 53) as f64)
}

/// Mean centre-to-centre distance between neighbouring region sites.
const REGION_SPACING: f64 = 10.0;
/// Open-sea gap between neighbouring country cells (world units).
const SEA_GAP: f64 = 16.0;
/// Vertices of the jittered country boundary blob.
const BOUNDARY_VERTICES: usize = 14;

/// Recover generation order (`Region1, Region2, …`) — the circular
/// `LandBorder` adjacency was built over that order, so laying regions out
/// in it keeps engine-graph neighbours geometric neighbours.
fn region_sort_key(id: &str) -> (u64, String) {
    let digits: String = id.chars().rev().take_while(|c| c.is_ascii_digit()).collect();
    let suffix: String = digits.chars().rev().collect();
    match suffix.parse::<u64>() {
        Ok(n) => (0, format!("{:020}{}", n, id)),
        Err(_) => (1, id.to_string()),
    }
}

/// Ring assignment for `k` non-capital regions: rings hold 6, 12, 18, …
/// Returns `(ring_index, position_in_ring, ring_capacity)` per input index.
fn ring_layout(k: usize) -> Vec<(usize, usize, usize)> {
    let mut layout = Vec::with_capacity(k);
    let mut remaining = k;
    let mut ring = 1usize;
    while remaining > 0 {
        let cap = 6 * ring;
        let take = cap.min(remaining);
        for j in 0..take {
            layout.push((ring, j, cap));
        }
        remaining -= take;
        ring += 1;
    }
    layout
}

/// Jittered N-gon boundary blob around `center` with mean radius `r`.
/// Deterministic per `country` — identical output every run.
fn country_boundary(country: &str, center: (f64, f64), r: f64) -> Vec<MapPoint> {
    let mut pts = Vec::with_capacity(BOUNDARY_VERTICES);
    for i in 0..BOUNDARY_VERTICES {
        let angle = (i as f64 / BOUNDARY_VERTICES as f64) * std::f64::consts::TAU
            + hash_unit(country, 7) * std::f64::consts::TAU / BOUNDARY_VERTICES as f64;
        // ±18% radius jitter → organic coastline silhouette.
        let jr = 1.0 + (hash_unit(country, 1000 + i as u64) - 0.5) * 0.36;
        pts.push(MapPoint {
            x: center.0 + r * jr * angle.cos(),
            y: center.1 + r * jr * angle.sin(),
        });
    }
    pts
}

// ============================================================================
// Voronoi-lite cell clipping (Sutherland–Hodgman half-plane intersection)
// ============================================================================

/// Signed side of point `p` relative to the perpendicular bisector of
/// `site`–`other`. `<= 0` keeps `p` on `site`'s side.
fn bisector_side(p: (f64, f64), site: (f64, f64), other: (f64, f64)) -> f64 {
    let mx = (site.0 + other.0) * 0.5;
    let my = (site.1 + other.1) * 0.5;
    // Normal pointing from site toward other.
    let nx = other.0 - site.0;
    let ny = other.1 - site.1;
    (p.0 - mx) * nx + (p.1 - my) * ny
}

/// Clip polygon `poly` (convex or not — Sutherland–Hodgman keeps convexity
/// of the inputs) to the half-plane of points closer to `site` than to
/// `other`.
fn clip_to_bisector(
    poly: &[(f64, f64)],
    site: (f64, f64),
    other: (f64, f64),
) -> Vec<(f64, f64)> {
    if poly.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(poly.len() + 1);
    let mut prev = *poly.last().unwrap();
    let mut prev_side = bisector_side(prev, site, other) <= 0.0;
    for &cur in poly {
        let cur_side = bisector_side(cur, site, other) <= 0.0;
        if cur_side != prev_side {
            // Edge crosses the bisector — emit the intersection point.
            let t = bisector_side(prev, site, other)
                / (bisector_side(prev, site, other) - bisector_side(cur, site, other));
            out.push((
                prev.0 + (cur.0 - prev.0) * t,
                prev.1 + (cur.1 - prev.1) * t,
            ));
        }
        if cur_side {
            out.push(cur);
        }
        prev = cur;
        prev_side = cur_side;
    }
    out
}

/// Voronoi cell of `site` inside `boundary`, clipped against every site in
/// `others`. Falls back to a small octagon if the cell degenerates.
fn voronoi_cell(
    site: (f64, f64),
    others: &[(f64, f64)],
    boundary: &[(f64, f64)],
) -> Vec<(f64, f64)> {
    let mut poly: Vec<(f64, f64)> = boundary.to_vec();
    // Clip against nearest sites first — distant bisectors cannot cut the
    // cell once nearer ones bounded it, and ordered iteration keeps the
    // float result byte-stable.
    let mut sorted: Vec<(f64, f64)> = others.to_vec();
    sorted.sort_by(|a, b| {
        let da = (a.0 - site.0).powi(2) + (a.1 - site.1).powi(2);
        let db = (b.0 - site.0).powi(2) + (b.1 - site.1).powi(2);
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    });
    for other in sorted {
        if (other.0 - site.0).abs() < 1e-12 && (other.1 - site.1).abs() < 1e-12 {
            continue; // coincident site — bisector is the whole plane
        }
        poly = clip_to_bisector(&poly, site, other);
        if poly.is_empty() {
            break;
        }
    }
    if poly.len() < 3 {
        // Degenerate cell (overlapping sites) — synthesize a small octagon
        // so the region still renders and stays clickable.
        let r = REGION_SPACING * 0.2;
        poly = (0..8)
            .map(|i| {
                let a = (i as f64 / 8.0) * std::f64::consts::TAU;
                (site.0 + r * a.cos(), site.1 + r * a.sin())
            })
            .collect();
    }
    poly
}

/// Squared distance from `p` to segment `a`–`b`.
fn seg_dist2(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let abx = b.0 - a.0;
    let aby = b.1 - a.1;
    let len2 = abx * abx + aby * aby;
    let t = if len2 > 0.0 {
        (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let dx = p.0 - (a.0 + abx * t);
    let dy = p.1 - (a.1 + aby * t);
    dx * dx + dy * dy
}

/// Does `cell` share a face with `boundary`? True when two or more cell
/// vertices sit on the boundary outline (the sea frontier of the blob).
fn cell_touches_boundary(
    cell: &[(f64, f64)],
    boundary: &[(f64, f64)],
    eps2: f64,
) -> bool {
    let mut hits = 0usize;
    for &v in cell {
        let on_rim = (0..boundary.len()).any(|i| {
            seg_dist2(v, boundary[i], boundary[(i + 1) % boundary.len()]) <= eps2
        });
        if on_rim {
            hits += 1;
        }
    }
    hits >= 2
}

// ============================================================================
// Stats aggregation
// ============================================================================

/// Mean parcel devastation for a region — same cadastre aggregation the
/// military dashboard uses (`0.0` when the region has no parcels).
fn region_devastation(country: &Country, region: &Region) -> f64 {
    if region.parcel_ids.is_empty() {
        return 0.0;
    }
    let cadastre = &country.cadastre;
    let mut total = 0.0;
    let mut count = 0usize;
    for pid in &region.parcel_ids {
        if let Some(p) = cadastre.get(*pid) {
            total += p.devastation_index;
            count += 1;
        }
    }
    if count > 0 {
        total / count as f64
    } else {
        0.0
    }
}

/// Urban-class share of the region's class-demographic population (0–1).
/// Regions without class demographics report `development_level` as the
/// proxy — Genesis always seeds `class_demographics`, so the fallback only
/// fires for legacy saves that predate regional demographics.
fn region_urbanization(region: &Region) -> f64 {
    let urban: i64 = region
        .class_demographics
        .urban_classes
        .values()
        .map(|c| c.population.max(0))
        .sum();
    let rural: i64 = region
        .class_demographics
        .rural_classes
        .values()
        .map(|c| c.population.max(0))
        .sum();
    let total = urban + rural;
    if total > 0 {
        (urban as f64 / total as f64).clamp(0.0, 1.0)
    } else {
        region.development_level.clamp(0.0, 1.0)
    }
}

/// Engine-truth coastal flag: traits or waterborne edges.
fn region_engine_coastal(region: &Region) -> bool {
    region.geographic_traits.has_coastline
        || region
            .edges
            .iter()
            .any(|e| matches!(e.edge_type, EdgeType::Coastline | EdgeType::SeaLane))
}

fn edge_type_label(t: EdgeType) -> &'static str {
    match t {
        EdgeType::LandBorder => "land_border",
        EdgeType::Coastline => "coastline",
        EdgeType::SeaLane => "sea_lane",
        EdgeType::River => "river",
    }
}

/// Map a `RegionControl` into the DTO `(control, controller)` pair.
fn control_label(control: Option<&RegionControl>) -> (String, String) {
    match control {
        Some(RegionControl::Occupied(c)) => ("occupied".to_string(), c.clone()),
        Some(RegionControl::Contested) => ("contested".to_string(), String::new()),
        Some(RegionControl::RebelControl) => ("rebel_control".to_string(), String::new()),
        _ => ("owner".to_string(), String::new()),
    }
}

/// Turns within which a recorded battle still counts as "active".
const ACTIVE_BATTLE_TURNS: u32 = 4;
/// Turns within which a front still counts as active for the map.
const ACTIVE_FRONT_TURNS: u32 = 6;

// ============================================================================
// Snapshot builder
// ============================================================================

/// Build the deterministic world-map snapshot for the current game state.
///
/// Called by the `get_world_map` Tauri command. Pure and read-only — the
/// layout is recomputed on every call and identical for identical input.
pub fn build_world_map(state: &GameState) -> WorldMapSnapshot {
    let turn = state.calendar.global_turn;

    // Canonical country order — `countries` is a `HashMap`, so sort first.
    let mut country_names: Vec<&String> = state.countries.keys().collect();
    country_names.sort();

    // ------------------------------------------------------------------
    // Pass 1: lay out region sites per country (anchor-relative), then
    // compute country radii → cell grid → absolute anchors.
    // ------------------------------------------------------------------
    struct CountryLayout {
        name: String,
        /// Sorted land regions in creation order (capital first).
        regions: Vec<Region>,
        /// Site per region (absolute coords after anchoring).
        sites: Vec<(f64, f64)>,
        /// Boundary blob vertices.
        boundary: Vec<(f64, f64)>,
        radius: f64,
        anchor: (f64, f64),
    }

    let mut layouts: Vec<CountryLayout> = Vec::with_capacity(country_names.len());
    for name in &country_names {
        let country = &state.countries[*name];
        // Land regions only, in creation order — maritime nodes are never
        // loaded into `country.regions`, but filter defensively anyway.
        let mut regions: Vec<Region> = country
            .regions
            .iter()
            .filter(|r| r.node_type == NodeType::LandRegion)
            .cloned()
            .collect();
        regions.sort_by(|a, b| region_sort_key(&a.id).cmp(&region_sort_key(&b.id)));

        let land_count = regions.len().max(1) as f64;
        // Blob radius grows with sqrt(region count) → uniform site density.
        let n_rings = ring_layout(regions.len().saturating_sub(1))
            .last()
            .map(|l| l.0)
            .unwrap_or(0);
        let radius = REGION_SPACING * (n_rings as f64 + 0.75).max(land_count.sqrt() * 1.05);
        layouts.push(CountryLayout {
            name: (*name).clone(),
            regions,
            sites: Vec::new(),
            boundary: Vec::new(),
            radius,
            anchor: (0.0, 0.0),
        });
    }

    // Cell pitch: largest blob + sea gap. Jittered anchor offsets break
    // the grid regularity while staying fully deterministic.
    let max_radius = layouts
        .iter()
        .map(|l| l.radius)
        .fold(0.0_f64, f64::max)
        .max(REGION_SPACING);
    let pitch = 2.0 * max_radius + SEA_GAP;
    let cols = (layouts.len() as f64).sqrt().ceil() as usize;
    for (i, layout) in layouts.iter_mut().enumerate() {
        let row = i / cols.max(1);
        let col = i % cols.max(1);
        let jx = (hash_unit(&layout.name, 11) - 0.5) * pitch * 0.3;
        let jy = (hash_unit(&layout.name, 23) - 0.5) * pitch * 0.3;
        layout.anchor = (col as f64 * pitch + jx, row as f64 * pitch + jy);
        layout.boundary = country_boundary(&layout.name, layout.anchor, layout.radius)
            .into_iter()
            .map(|p| (p.x, p.y))
            .collect();
    }

    // ------------------------------------------------------------------
    // Pass 2: region sites per country — capital at anchor, the rest on
    // concentric rings in creation order with a small hash jitter. When a
    // region carries real coords (Phase 30 migration has run), use them
    // as relative offsets around the anchor instead of the ring slot.
    // ------------------------------------------------------------------
    for layout in &mut layouts {
        let n = layout.regions.len();
        let ring_plan = ring_layout(n.saturating_sub(1));
        // Are stored coords usable? Only when at least two regions have
        // distinct non-zero positions (the generator leaves them all 0,0).
        let coords_usable = layout
            .regions
            .iter()
            .filter(|r| r.coord_x != 0.0 || r.coord_y != 0.0)
            .count()
            >= 2;
        // Scale stored coords (kilometre-ish engine units) into cell units.
        let coord_scale = if coords_usable {
            let mut max_d = 0.0f64;
            let (cx, cy) = layout.regions.iter().fold((0.0, 0.0), |(sx, sy), r| {
                (sx + r.coord_x, sy + r.coord_y)
            });
            let (mx, my) = (cx / n as f64, cy / n as f64);
            for r in &layout.regions {
                max_d = max_d.max(((r.coord_x - mx).powi(2) + (r.coord_y - my).powi(2)).sqrt());
            }
            if max_d > 0.0 {
                layout.radius * 0.7 / max_d
            } else {
                0.0
            }
        } else {
            0.0
        };
        let (mx, my) = layout.regions.iter().fold((0.0, 0.0), |(sx, sy), r| {
            (sx + r.coord_x, sy + r.coord_y)
        });
        let coord_mean = (mx / n as f64, my / n as f64);

        let mut sites = Vec::with_capacity(n);
        let mut non_capital = 0usize;
        for region in &layout.regions {
            if coords_usable {
                sites.push((
                    layout.anchor.0 + (region.coord_x - coord_mean.0) * coord_scale,
                    layout.anchor.1 + (region.coord_y - coord_mean.1) * coord_scale,
                ));
            } else if region.is_capital || non_capital >= ring_plan.len() {
                if region.is_capital {
                    sites.push(layout.anchor);
                } else {
                    // More regions than ring slots — stack on the outer ring.
                    let a = hash_unit(&region.id, 31) * std::f64::consts::TAU;
                    sites.push((
                        layout.anchor.0 + layout.radius * 0.8 * a.cos(),
                        layout.anchor.1 + layout.radius * 0.8 * a.sin(),
                    ));
                }
            } else {
                let (ring, pos, cap) = ring_plan[non_capital];
                non_capital += 1;
                // Stagger every other ring so sites don't radial-align.
                let phase = if ring % 2 == 0 { std::f64::consts::PI / cap as f64 } else { 0.0 };
                let angle = (pos as f64 / cap as f64) * std::f64::consts::TAU + phase;
                let rr = layout.radius * ring as f64 / (ring_plan.last().map(|l| l.0).unwrap_or(1) as f64 + 0.55);
                // ±9% radius + ±4° angle jitter from the id hash.
                let jr = 1.0 + (hash_unit(&region.id, 41) - 0.5) * 0.18;
                let ja = (hash_unit(&region.id, 43) - 0.5) * 0.07;
                sites.push((
                    layout.anchor.0 + rr * jr * (angle + ja).cos(),
                    layout.anchor.1 + rr * jr * (angle + ja).sin(),
                ));
            }
        }
        layout.sites = sites;
    }

    // ------------------------------------------------------------------
    // Pass 3: front/occupation/battle overlays (countries sorted →
    // deterministic merge order; fronts share `WAR-{a}-{b}` ids across
    // belligerents so first-writer-wins is stable).
    // ------------------------------------------------------------------
    let mut region_control: std::collections::HashMap<String, RegionControl> =
        std::collections::HashMap::new();
    let mut on_front: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut battle_recent: std::collections::HashSet<String> = std::collections::HashSet::new();
    for name in &country_names {
        let country = &state.countries[*name];
        for front in &country.military_fronts {
            if !front.is_active(turn, ACTIVE_FRONT_TURNS) {
                continue;
            }
            for rid in &front.regions {
                on_front.insert(rid.clone());
            }
            for (rid, ctrl) in &front.region_control {
                region_control.entry(rid.clone()).or_insert_with(|| ctrl.clone());
            }
        }
        for front in &country.military_fronts {
            for battle in &front.battles {
                if turn.saturating_sub(battle.turn) <= ACTIVE_BATTLE_TURNS {
                    battle_recent.insert(battle.location.clone());
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Pass 4: Voronoi cells + DTO assembly.
    // ------------------------------------------------------------------
    let mut countries_out = Vec::with_capacity(layouts.len());
    let mut edges_out: Vec<MapEdgeDto> = Vec::new();
    let mut edge_seen: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    let mut bounds = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];

    for (color_index, layout) in layouts.iter().enumerate() {
        let country = &state.countries[layout.name.as_str()];
        let boundary_pts: Vec<MapPoint> = layout
            .boundary
            .iter()
            .map(|p| MapPoint { x: p.0, y: p.1 })
            .collect();
        for p in &layout.boundary {
            bounds[0] = bounds[0].min(p.0);
            bounds[1] = bounds[1].min(p.1);
            bounds[2] = bounds[2].max(p.0);
            bounds[3] = bounds[3].max(p.1);
        }

        // Rim epsilon: fraction of mean boundary edge length.
        let eps2 = (layout.radius * 1e-3).powi(2);

        let mut regions_out = Vec::with_capacity(layout.regions.len());
        let mut capital_name = String::new();
        let mut capital_anchor = MapPoint {
            x: layout.anchor.0,
            y: layout.anchor.1,
        };
        for (idx, region) in layout.regions.iter().enumerate() {
            let site = layout.sites[idx];
            let others: Vec<(f64, f64)> = layout
                .sites
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != idx)
                .map(|(_, s)| *s)
                .collect();
            let cell = voronoi_cell(site, &others, &layout.boundary);
            let polygon: Vec<MapPoint> = cell
                .iter()
                .map(|p| MapPoint { x: p.0, y: p.1 })
                .collect();

            // Land-border neighbours from the engine edge graph.
            let mut neighbors: Vec<String> = region
                .edges
                .iter()
                .filter(|e| matches!(e.edge_type, EdgeType::LandBorder | EdgeType::River))
                .map(|e| e.target_node.clone())
                .collect();
            neighbors.sort();
            neighbors.dedup();
            for e in &region.edges {
                if !matches!(e.edge_type, EdgeType::LandBorder | EdgeType::River) {
                    continue;
                }
                // Dedup undirected pair (store lexicographic order once).
                let (a, b) = if region.id < e.target_node {
                    (region.id.clone(), e.target_node.clone())
                } else {
                    (e.target_node.clone(), region.id.clone())
                };
                if edge_seen.insert((a.clone(), b.clone())) {
                    edges_out.push(MapEdgeDto {
                        from_region: a,
                        to_region: b,
                        edge_type: edge_type_label(e.edge_type).to_string(),
                    });
                }
            }

            if region.is_capital {
                capital_name = if region.display_name.is_empty() {
                    region.id.clone()
                } else {
                    region.display_name.clone()
                };
                capital_anchor = MapPoint { x: site.0, y: site.1 };
            }

            let (control, controller) = control_label(region_control.get(&region.id));
            regions_out.push(MapRegionDto {
                id: region.id.clone(),
                country: layout.name.clone(),
                name: if region.display_name.is_empty() {
                    region.id.clone()
                } else {
                    region.display_name.clone()
                },
                polygon,
                centroid: MapPoint { x: site.0, y: site.1 },
                is_capital_district: region.is_capital,
                is_coastal: region_engine_coastal(region)
                    || cell_touches_boundary(&cell, &layout.boundary, eps2),
                is_city_region: region.city_metadata.is_some(),
                population: region.population,
                gdp: region.gdp,
                gdp_per_capita: region.gdp_pc,
                devastation: region_devastation(country, region),
                urbanization: region_urbanization(region),
                development_level: region.development_level,
                control,
                controller,
                active_battle: battle_recent.contains(&region.id),
                on_front: on_front.contains(&region.id),
                neighbors,
            });
        }

        countries_out.push(CountryMapDto {
            country: layout.name.clone(),
            color_index: color_index as u32,
            at_war_with: country.at_war_with.clone(),
            capital_name,
            anchor: capital_anchor,
            boundary: boundary_pts,
            regions: regions_out,
        });
    }

    if !bounds[0].is_finite() {
        bounds = [0.0, 0.0, 1.0, 1.0];
    }
    edges_out.sort_by(|a, b| {
        a.from_region
            .cmp(&b.from_region)
            .then_with(|| a.to_region.cmp(&b.to_region))
    });

    WorldMapSnapshot {
        turn,
        bounds: bounds.to_vec(),
        countries: countries_out,
        edges: edges_out,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_region(id: &str, country: &str, capital: bool, pop: i64) -> Region {
        Region {
            id: id.to_string(),
            display_name: format!("{} District", id),
            owner_country: country.to_string(),
            population: pop,
            gdp: 1_000_000.0,
            gdp_pc: 500.0,
            is_capital: capital,
            node_type: NodeType::LandRegion,
            development_level: 0.4,
            ..Default::default()
        }
    }

    fn test_country(name: &str, region_count: usize) -> Country {
        let mut country = Country {
            name: name.to_string(),
            ..Default::default()
        };
        let mut regions = Vec::new();
        for i in 0..region_count {
            let id = format!("{}-Region{}", name, i + 1);
            let mut r = test_region(&id, name, i == 0, 100_000 * (i as i64 + 1));
            if i > 0 {
                r.edges.push(crate::society::geography::Edge {
                    target_node: format!("{}-Region{}", name, i),
                    edge_type: EdgeType::LandBorder,
                    distance: 100.0,
                    is_navigable: false,
                    territorial_owner: None,
                });
            }
            if i == 0 && region_count > 1 {
                r.edges.push(crate::society::geography::Edge {
                    target_node: format!("{}-Region2", name),
                    edge_type: EdgeType::LandBorder,
                    distance: 100.0,
                    is_navigable: false,
                    territorial_owner: None,
                });
            }
            regions.push(r);
        }
        country.regions = regions;
        country
    }

    #[test]
    fn world_map_is_deterministic_and_covers_all_regions() {
        let mut state = GameState::new();
        state
            .countries
            .insert("Alpha".to_string(), test_country("Alpha", 9));
        state
            .countries
            .insert("Beta".to_string(), test_country("Beta", 4));
        state.calendar.global_turn = 12;

        let a = build_world_map(&state);
        let b = build_world_map(&state);
        // Byte-identical snapshot across calls (layout has no RNG/HashMap order).
        assert_eq!(
            serde_json::to_string(&a).unwrap(),
            serde_json::to_string(&b).unwrap()
        );

        assert_eq!(a.countries.len(), 2);
        assert_eq!(a.countries[0].country, "Alpha");
        assert_eq!(a.countries[0].color_index, 0);
        assert_eq!(a.countries[1].country, "Beta");
        assert_eq!(a.countries[0].regions.len(), 9);
        assert_eq!(a.countries[1].regions.len(), 4);

        // Every region has a renderable polygon inside the country bounds.
        for c in &a.countries {
            assert!(c.boundary.len() >= 3);
            for r in &c.regions {
                assert!(r.polygon.len() >= 3, "region {} polygon degenerate", r.id);
                for v in &r.polygon {
                    assert!(v.x.is_finite() && v.y.is_finite());
                }
            }
        }

        // Capital flags + labels.
        let alpha = &a.countries[0];
        assert_eq!(alpha.capital_name, "Alpha-Region1 District");
        assert!(alpha
            .regions
            .iter()
            .any(|r| r.is_capital_district && r.id == "Alpha-Region1"));

        // Outer-ring regions touch the sea frontier (procedural coastal).
        assert!(alpha.regions.iter().any(|r| r.is_coastal));

        // LandBorder edge dedup: Region2→Region1 recorded once.
        assert!(a
            .edges
            .iter()
            .any(|e| e.from_region == "Alpha-Region1" && e.to_region == "Alpha-Region2"));
        assert_eq!(
            a.edges
                .iter()
                .filter(|e| e.from_region == "Alpha-Region1" && e.to_region == "Alpha-Region2")
                .count(),
            1
        );

        // Bounds cover every boundary vertex.
        assert!(a.bounds[0] < a.bounds[2] && a.bounds[1] < a.bounds[3]);
        for c in &a.countries {
            for p in &c.boundary {
                assert!(p.x >= a.bounds[0] - 1e-6 && p.x <= a.bounds[2] + 1e-6);
            }
        }
    }

    #[test]
    fn ring_layout_counts_are_hexagonal() {
        assert_eq!(ring_layout(0).len(), 0);
        assert_eq!(ring_layout(6), vec![
            (1, 0, 6), (1, 1, 6), (1, 2, 6), (1, 3, 6), (1, 4, 6), (1, 5, 6)
        ]);
        assert_eq!(ring_layout(7).last(), Some(&(2, 0, 12)));
        assert_eq!(ring_layout(19).last(), Some(&(3, 0, 18)));
    }

    #[test]
    fn voronoi_partitions_boundary() {
        // Two sites inside a square — cells must partition it along x=0.
        let boundary = vec![
            (-10.0, -10.0),
            (10.0, -10.0),
            (10.0, 10.0),
            (-10.0, 10.0),
        ];
        let cell = voronoi_cell((-4.0, 0.0), &[(4.0, 0.0)], &boundary);
        assert!(cell.len() >= 3);
        for v in &cell {
            assert!(v.0 <= 1e-9, "cell vertex {:?} crosses bisector", v);
        }
    }
}

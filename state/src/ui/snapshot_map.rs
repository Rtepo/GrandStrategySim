//! v4.24.0 WS5 — Interactive procedural world map snapshot.
//! v4.25.0 WS1 — Countries now share continental landmasses: 1–2 continent
//! blobs tile into Voronoi country cells with real shared borders, and every
//! polygon edge is subdivided with position-keyed value noise so borders are
//! organic and identical on both sides.
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
//! * Countries are packed onto 1–2 continent blobs via
//!   `geography::continent_layout` — the same anchors world generation uses
//!   to wire international `LandBorder` edges, so rendered borders and the
//!   engine adjacency graph always agree. Inside a continent the first
//!   country anchors the hub and the rest sit on a jittered ring; each
//!   country's boundary is its Voronoi cell inside the continent blob.
//! * Inside a country, the capital sits at the anchor and remaining
//!   regions are placed on concentric rings (6, 12, 18, …) in creation
//!   order so the circular `LandBorder` adjacency maps onto neighbouring
//!   cells. A small hash jitter breaks the hex-grid symmetry.
//! * Each region polygon is its Voronoi cell — the country boundary cell
//!   clipped by the perpendicular bisectors against every sibling site —
//!   then subdivided with deterministic fractal noise (≥6 segments per
//!   edge, so ≥18 vertices per region) for natural borders.
//! * Regions whose cell touches the continent's sea frontier are flagged
//!   coastal when the engine itself records no coastline (maritime graph
//!   nodes are not generated on the live path).

use crate::military::fronts::RegionControl;
use crate::society::cadastre::ZoningDesignation;
use crate::society::geography::{
    continent_layout, map_hash_unit, map_region_sort_key, map_ring_layout, EdgeType, NodeType,
    Region, MAP_REGION_SPACING,
};
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
    /// Land-border neighbour region ids (engine edge graph + synthesized
    /// geometric cross-country borders).
    pub neighbors: Vec<String>,
    /// v4.25.0 WS1: Index into the country's `megaregions` list (-1 when the
    /// region belongs to no megaregion). Drives the Megaregions map mode.
    pub megaregion_index: i64,
    /// v4.25.0 WS1: Megaregion display name (empty when unassigned).
    pub megaregion_name: String,
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

/// A decorative map feature (inland lake or mountain range) — pure
/// presentation geometry derived from the same deterministic layout.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MapFeatureDto {
    /// `lake` | `mountain`.
    pub kind: String,
    /// `lake`: closed polygon vertices. `mountain`: ridge polyline points.
    pub points: Vec<MapPoint>,
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
    /// v4.25.0 WS1: Decorative features (inland lakes, mountain ranges).
    pub features: Vec<MapFeatureDto>,
    /// v4.25.0 WS1 release gate: number of deduplicated `land_border` edges
    /// whose endpoints belong to DIFFERENT countries — the shared
    /// international borders migration/freight routing can cross.
    pub shared_land_borders_count: u32,
    /// v4.25.0 WS1 release gate: smallest region-polygon vertex count in the
    /// snapshot (fractal subdivision guarantees ≥16).
    pub min_region_polygon_vertices: u32,
}

// ============================================================================
// Deterministic hashing / layout constants
// ============================================================================

/// Thin aliases over the shared geography hashing helpers — the worldgen
/// border wiring (`wire_international_land_borders`) and this snapshot must
/// agree bit-for-bit on every jitter value.
fn hash_unit(s: &str, salt: u64) -> f64 {
    map_hash_unit(s, salt)
}

/// Mean centre-to-centre distance between neighbouring region sites.
const REGION_SPACING: f64 = MAP_REGION_SPACING;

/// Recover generation order (`Region1, Region2, …`) — the circular
/// `LandBorder` adjacency was built over that order, so laying regions out
/// in it keeps engine-graph neighbours geometric neighbours.
fn region_sort_key(id: &str) -> (u64, String) {
    map_region_sort_key(id)
}

/// Ring assignment for `k` non-capital regions: rings hold 6, 12, 18, …
/// Returns `(ring_index, position_in_ring, ring_capacity)` per input index.
fn ring_layout(k: usize) -> Vec<(usize, usize, usize)> {
    map_ring_layout(k)
}

/// Vertices of the jittered continent boundary blob (before subdivision).
const CONTINENT_BOUNDARY_VERTICES: usize = 26;

/// Jittered N-gon continent silhouette around `center` with mean radius `r`.
/// A three-lobe sinusoidal wobble plus per-vertex hash jitter produces
/// natural coastlines with bays and peninsulas once subdivided.
fn continent_blob(continent: usize, center: (f64, f64), r: f64) -> Vec<(f64, f64)> {
    let lobe_phase = hash_unit(&format!("continent-lobe-{continent}"), 61) * std::f64::consts::TAU;
    let mut pts = Vec::with_capacity(CONTINENT_BOUNDARY_VERTICES);
    for i in 0..CONTINENT_BOUNDARY_VERTICES {
        let frac = i as f64 / CONTINENT_BOUNDARY_VERTICES as f64;
        let angle = frac * std::f64::consts::TAU
            + hash_unit(&format!("continent-{continent}"), 7) * std::f64::consts::TAU
                / CONTINENT_BOUNDARY_VERTICES as f64;
        // ±20% vertex jitter + 8% slow wobble → organic continent outline.
        let jr = 1.0
            + (hash_unit(&format!("continent-{continent}-{i}"), 83) - 0.5) * 0.40
            + 0.08 * (angle * 3.0 + lobe_phase).sin();
        pts.push((center.0 + r * jr * angle.cos(), center.1 + r * jr * angle.sin()));
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
// Fractal border subdivision (deterministic value noise)
// ============================================================================
//
// Every polygon edge is subdivided into ≥6 segments and each interior point
// is displaced perpendicular to the edge by deterministic 2-octave value
// noise. The displacement is a pure function of the point's POSITION (not of
// the edge or region), so a border shared by two cells/subdivided polygons
// is displaced identically on both sides — shared borders stay shared.

/// Integer-coordinate hash → `[0, 1)`; the value-noise lattice corner.
fn lattice_hash(ix: i64, iy: i64, salt: u64) -> f64 {
    let mut x = (ix as u64)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (iy as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ salt.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^= x >> 31;
    ((x >> 11) as f64) / ((1u64 << 53) as f64)
}

/// Smoothstep `3t² − 2t³`.
fn smoothstep(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// 2D value noise on a unit lattice — bilinear-smooth between hashed
/// corners. Deterministic, platform-stable (integer math only).
fn value_noise_2d(x: f64, y: f64, salt: u64) -> f64 {
    let xi = x.floor() as i64;
    let yi = y.floor() as i64;
    let xf = x - xi as f64;
    let yf = y - yi as f64;
    let a = lattice_hash(xi, yi, salt);
    let b = lattice_hash(xi + 1, yi, salt);
    let c = lattice_hash(xi, yi + 1, salt);
    let d = lattice_hash(xi + 1, yi + 1, salt);
    let u = smoothstep(xf);
    let v = smoothstep(yf);
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

/// Fractal displacement for an edge-interior point — position-keyed.
fn fractal_displace(
    p: (f64, f64),
    edge_dir: (f64, f64),
    edge_len: f64,
    salt: u64,
) -> (f64, f64) {
    let n = 0.62 * value_noise_2d(p.0 * 0.11, p.1 * 0.11, salt)
        + 0.38 * value_noise_2d(p.0 * 0.34, p.1 * 0.34, salt + 91);
    // Amplitude: ~16% of edge length, capped so small cells stay sane.
    let amp = (edge_len * 0.16).min(REGION_SPACING * 0.45);
    let inv = 1.0 / edge_len.max(1e-9);
    let (nx, ny) = (-edge_dir.1 * inv, edge_dir.0 * inv);
    (
        p.0 + nx * (n - 0.5) * 2.0 * amp,
        p.1 + ny * (n - 0.5) * 2.0 * amp,
    )
}

/// Salt shared by every subdivided edge in the snapshot — a single global
/// noise field is what makes shared borders line up across adjacent cells.
const SUBDIV_SALT: u64 = 424_242;

/// Subdivide every polygon edge into ≥6 (up to 24) segments with fractal
/// interior displacement. Guarantees ≥18 vertices for any triangle+ cell —
/// the `min_region_polygon_vertices` release gate asserts ≥16.
fn subdivide_polygon(poly: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let m = poly.len();
    if m < 3 {
        return poly.to_vec();
    }
    let mut out = Vec::with_capacity(m * 8);
    for i in 0..m {
        let a = poly[i];
        let b = poly[(i + 1) % m];
        out.push(a);
        let dir = (b.0 - a.0, b.1 - a.1);
        let len = (dir.0 * dir.0 + dir.1 * dir.1).sqrt();
        // ≥6 segments per edge; longer edges get proportionally more.
        let n = ((len / (REGION_SPACING * 0.55)).ceil() as usize).clamp(6, 24);
        for k in 1..n {
            let t = k as f64 / n as f64;
            let p = (a.0 + dir.0 * t, a.1 + dir.1 * t);
            out.push(fractal_displace(p, dir, len, SUBDIV_SALT));
        }
    }
    out
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
    // Pass 1: continent placement — countries packed onto 1–2 landmasses.
    // `geography::continent_layout` is the SAME function world generation
    // calls to wire international `LandBorder` edges, so the rendered
    // shared borders always mirror the engine adjacency graph.
    // ------------------------------------------------------------------
    struct CountryLayout {
        name: String,
        /// Sorted land regions in creation order (capital first).
        regions: Vec<Region>,
        /// Site per region (absolute coords after anchoring).
        sites: Vec<(f64, f64)>,
        /// Country cell — the Voronoi cell of the country's anchor inside
        /// its continent blob (this is the rendered country boundary).
        boundary: Vec<(f64, f64)>,
        /// Owning continent's sea frontier (coastal detection + bounds).
        continent_boundary: Vec<(f64, f64)>,
        /// Continent index for cross-country adjacency grouping.
        continent: usize,
        radius: f64,
        anchor: (f64, f64),
    }

    let mut layouts: Vec<CountryLayout> = Vec::with_capacity(country_names.len());
    let mut continent_entries: Vec<(String, usize)> = Vec::with_capacity(country_names.len());
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
        regions.sort_by_key(|r| region_sort_key(&r.id));

        continent_entries.push(((*name).clone(), regions.len()));
        layouts.push(CountryLayout {
            name: (*name).clone(),
            regions,
            sites: Vec::new(),
            boundary: Vec::new(),
            continent_boundary: Vec::new(),
            continent: 0,
            radius: 0.0,
            anchor: (0.0, 0.0),
        });
    }

    let placements = continent_layout(&continent_entries);
    let n_continents = placements
        .iter()
        .map(|p| p.continent)
        .max()
        .map(|c| c + 1)
        .unwrap_or(0);
    for ci in 0..n_continents {
        let members: Vec<usize> = placements
            .iter()
            .enumerate()
            .filter(|(_, p)| p.continent == ci)
            .map(|(i, _)| i)
            .collect();
        if members.is_empty() {
            continue;
        }
        let blob = continent_blob(
            ci,
            placements[members[0]].continent_center,
            placements[members[0]].continent_radius,
        );
        let sites: Vec<(f64, f64)> = members.iter().map(|&i| placements[i].anchor).collect();
        for &mi in &members {
            // Country boundary = the anchor's Voronoi cell inside the
            // continent blob. A single-country continent gets the whole
            // blob (no sibling bisectors to clip against).
            let cell = voronoi_cell(placements[mi].anchor, &sites, &blob);
            layouts[mi].boundary = cell;
            layouts[mi].continent_boundary = blob.clone();
            layouts[mi].continent = ci;
            layouts[mi].anchor = placements[mi].anchor;
            layouts[mi].radius = placements[mi].blob_radius;
        }
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
    // Pass 4a: all region cells + cross-country adjacency bookkeeping.
    // ------------------------------------------------------------------
    let mut region_cells: Vec<Vec<Vec<(f64, f64)>>> = Vec::with_capacity(layouts.len());
    let mut region_owner: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for (li, layout) in layouts.iter().enumerate() {
        let mut cells = Vec::with_capacity(layout.regions.len());
        for (idx, _) in layout.regions.iter().enumerate() {
            let site = layout.sites[idx];
            // Passing ALL sites (including the region's own) is fine —
            // `voronoi_cell` skips coincident sites.
            cells.push(voronoi_cell(site, &layout.sites, &layout.boundary));
        }
        for region in &layout.regions {
            region_owner.insert(region.id.clone(), li);
        }
        region_cells.push(cells);
    }

    // Country pairs already wired in the engine graph (worldgen adds
    // `LandBorder` edges between neighbouring countries' rim regions).
    let mut wired_pairs: std::collections::HashSet<(usize, usize)> =
        std::collections::HashSet::new();
    for (li, layout) in layouts.iter().enumerate() {
        for region in &layout.regions {
            for e in &region.edges {
                if !matches!(e.edge_type, EdgeType::LandBorder | EdgeType::River) {
                    continue;
                }
                if let Some(&oj) = region_owner.get(&e.target_node) {
                    if oj != li {
                        wired_pairs.insert((li.min(oj), li.max(oj)));
                    }
                }
            }
        }
    }

    // Geometric adjacency: two country cells share a border when ≥2 of one
    // cell's vertices lie on the other's outline (Voronoi clipping makes
    // shared vertices bit-identical, so a hairline epsilon suffices).
    // For adjacent pairs with no engine edge (saves predating the worldgen
    // wiring) we synthesize one border edge between the rim regions so the
    // map still shows real land borders; mountain ridges decorate a
    // deterministic subset of shared borders either way.
    let mut features_out: Vec<MapFeatureDto> = Vec::new();
    let mut synth_neighbors: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut synth_edges: Vec<(String, String)> = Vec::new();
    for i in 0..layouts.len() {
        for j in (i + 1)..layouts.len() {
            if layouts[i].continent != layouts[j].continent {
                continue;
            }
            let eps2 =
                (layouts[i].radius.max(layouts[j].radius) * 2e-3).powi(2).max(1e-9);
            let shared = shared_vertices(&layouts[i].boundary, &layouts[j].boundary, eps2);
            if shared.len() < 2 {
                continue;
            }
            let mid = shared.iter().fold((0.0, 0.0), |(sx, sy), p| {
                (sx + p.0 / shared.len() as f64, sy + p.1 / shared.len() as f64)
            });
            if !wired_pairs.contains(&(i, j)) {
                let ra = border_region_at(&region_cells[i], &layouts[i].regions, mid);
                let rb = border_region_at(&region_cells[j], &layouts[j].regions, mid);
                if let (Some(ra), Some(rb)) = (ra, rb) {
                    let (a, b) = if ra < rb { (ra, rb) } else { (rb, ra) };
                    synth_neighbors
                        .entry(a.clone())
                        .or_default()
                        .push(b.clone());
                    synth_neighbors
                        .entry(b.clone())
                        .or_default()
                        .push(a.clone());
                    synth_edges.push((a, b));
                }
            }
            if hash_unit(&format!("mtn:{}:{}", layouts[i].name, layouts[j].name), 73) < 0.4
                && shared.len() >= 2
            {
                features_out.push(MapFeatureDto {
                    kind: "mountain".to_string(),
                    points: mountain_ridge(&shared, i as u64 * 131 + j as u64)
                        .into_iter()
                        .map(|p| MapPoint { x: p.0, y: p.1 })
                        .collect(),
                });
            }
        }
    }

    // ------------------------------------------------------------------
    // Pass 4b: fractal subdivision + DTO assembly.
    // ------------------------------------------------------------------
    let mut countries_out = Vec::with_capacity(layouts.len());
    let mut edges_out: Vec<MapEdgeDto> = Vec::new();
    let mut edge_seen: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    let mut bounds = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    let mut min_poly_verts = usize::MAX;
    let mut shared_land_borders: u32 = 0;

    for (color_index, layout) in layouts.iter().enumerate() {
        let country = &state.countries[layout.name.as_str()];
        // Country boundary = its Voronoi cell, subdivided with the same
        // position-keyed noise as the region cells so coastlines and shared
        // borders stay seamless.
        let boundary_sub = subdivide_polygon(&layout.boundary);
        let boundary_pts: Vec<MapPoint> = boundary_sub
            .iter()
            .map(|p| MapPoint { x: p.0, y: p.1 })
            .collect();
        for p in &boundary_sub {
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
            let cell = &region_cells[color_index][idx];
            let sub = subdivide_polygon(cell);
            min_poly_verts = min_poly_verts.min(sub.len());
            let polygon: Vec<MapPoint> =
                sub.iter().map(|p| MapPoint { x: p.0, y: p.1 }).collect();

            // Land-border neighbours from the engine edge graph, merged with
            // any geometric cross-country border synthesized above.
            let mut neighbors: Vec<String> = region
                .edges
                .iter()
                .filter(|e| matches!(e.edge_type, EdgeType::LandBorder | EdgeType::River))
                .map(|e| e.target_node.clone())
                .collect();
            if let Some(extra) = synth_neighbors.get(&region.id) {
                neighbors.extend(extra.iter().cloned());
            }
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
                    if e.edge_type == EdgeType::LandBorder {
                        if let Some(&oj) = region_owner.get(&e.target_node) {
                            if oj != color_index {
                                shared_land_borders += 1;
                            }
                        }
                    }
                    edges_out.push(MapEdgeDto {
                        from_region: a,
                        to_region: b,
                        edge_type: edge_type_label(e.edge_type).to_string(),
                    });
                }
            }

            // Megaregion membership for the Megaregions map mode.
            let (megaregion_index, megaregion_name) = country
                .megaregions
                .iter()
                .enumerate()
                .find(|(_, m)| m.regions.contains(&region.id))
                .map(|(i, m)| (i as i64, m.name.clone()))
                .unwrap_or((-1, String::new()));

            if region.is_capital {
                capital_name = if region.display_name.is_empty() {
                    region.id.clone()
                } else {
                    region.display_name.clone()
                };
                capital_anchor = MapPoint { x: site.0, y: site.1 };
            }

            let is_coastal = region_engine_coastal(region)
                || cell_touches_boundary(cell, &layout.continent_boundary, eps2);

            // Inland lake on a deterministic subset of roomy interior cells.
            if !is_coastal && hash_unit(&format!("lake:{}", region.id), 71) < 0.16 {
                if let Some(lake) = lake_feature(cell, &region.id) {
                    features_out.push(lake);
                }
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
                is_coastal,
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
                megaregion_index,
                megaregion_name,
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

    // Synthesized cross-country borders (geometric adjacency the engine
    // graph predates) — emitted as land_border edges and counted in the
    // shared-border release gate.
    for (a, b) in synth_edges {
        if edge_seen.insert((a.clone(), b.clone())) {
            shared_land_borders += 1;
            edges_out.push(MapEdgeDto {
                from_region: a,
                to_region: b,
                edge_type: "land_border".to_string(),
            });
        }
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
        features: features_out,
        shared_land_borders_count: shared_land_borders,
        min_region_polygon_vertices: if min_poly_verts == usize::MAX {
            0
        } else {
            min_poly_verts as u32
        },
    }
}

// ============================================================================
// Cross-country adjacency helpers
// ============================================================================

/// Vertices of polygon `a` that lie on polygon `b`'s outline within `eps2`.
/// Voronoi clipping makes shared vertices bit-identical, so `eps2` only
/// guards against degenerate fallbacks.
fn shared_vertices(
    a: &[(f64, f64)],
    b: &[(f64, f64)],
    eps2: f64,
) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for &v in a {
        let on_b =
            (0..b.len()).any(|i| seg_dist2(v, b[i], b[(i + 1) % b.len()]) <= eps2);
        if on_b {
            out.push(v);
        }
    }
    out
}

/// Region whose cell has a vertex closest to `mid` — the rim region a
/// synthesized cross-border edge should attach to.
fn border_region_at(
    cells: &[Vec<(f64, f64)>],
    regions: &[Region],
    mid: (f64, f64),
) -> Option<String> {
    let mut best: Option<(f64, usize)> = None;
    for (idx, cell) in cells.iter().enumerate() {
        for v in cell {
            let d2 = (v.0 - mid.0).powi(2) + (v.1 - mid.1).powi(2);
            if best.map(|(b, _)| d2 < b).unwrap_or(true) {
                best = Some((d2, idx));
            }
        }
    }
    best.map(|(_, idx)| regions[idx].id.clone())
}

/// Jagged polyline along a shared country border → mountain ridge.
fn mountain_ridge(shared: &[(f64, f64)], salt: u64) -> Vec<(f64, f64)> {
    if shared.len() < 2 {
        return Vec::new();
    }
    // Extremes of the shared span = ridge endpoints.
    let (mut a, mut b, mut best) = (shared[0], shared[0], 0.0_f64);
    for &p in shared {
        for &q in shared {
            let d = (p.0 - q.0).powi(2) + (p.1 - q.1).powi(2);
            if d > best {
                best = d;
                a = p;
                b = q;
            }
        }
    }
    let dir = (b.0 - a.0, b.1 - a.1);
    let len = (dir.0 * dir.0 + dir.1 * dir.1).sqrt().max(1e-9);
    let (nx, ny) = (-dir.1 / len, dir.0 / len);
    let mut out = Vec::with_capacity(9);
    for k in 0..=8 {
        let t = k as f64 / 8.0;
        let base = (a.0 + dir.0 * t, a.1 + dir.1 * t);
        let h = (value_noise_2d(base.0 * 0.3, base.1 * 0.3, salt) - 0.5) * len * 0.22;
        out.push((base.0 + nx * h, base.1 + ny * h));
    }
    out
}

/// Small noise-jittered blob → inland lake inside a roomy interior cell.
fn lake_feature(cell: &[(f64, f64)], region_id: &str) -> Option<MapFeatureDto> {
    if cell.is_empty() {
        return None;
    }
    let (mut minx, mut miny, mut maxx, mut maxy) =
        (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    let (mut sx, mut sy) = (0.0, 0.0);
    for v in cell {
        minx = minx.min(v.0);
        miny = miny.min(v.1);
        maxx = maxx.max(v.0);
        maxy = maxy.max(v.1);
        sx += v.0 / cell.len() as f64;
        sy += v.1 / cell.len() as f64;
    }
    let (w, h) = (maxx - minx, maxy - miny);
    if w < REGION_SPACING * 2.0 || h < REGION_SPACING * 2.0 {
        return None;
    }
    let r = w.min(h) * 0.18;
    let mut pts = Vec::with_capacity(9);
    for i in 0..9 {
        let a = (i as f64 / 9.0) * std::f64::consts::TAU + hash_unit(region_id, 77);
        let jr = 1.0 + (hash_unit(&format!("{region_id}-lk{i}"), 79) - 0.5) * 0.45;
        pts.push(MapPoint {
            x: sx + r * jr * a.cos(),
            y: sy + r * jr * a.sin(),
        });
    }
    Some(MapFeatureDto {
        kind: "lake".to_string(),
        points: pts,
    })
}

// ============================================================================
// Regional cadastral parcel map (Regions-page drill-down)
// ============================================================================

/// One parcel cell on a region's cadastral map.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MapParcelDto {
    /// Slotmap key of the `ParcelChunk` (unique within the country).
    pub id: String,
    /// Ordered quad vertices (cadastral-map units, y-down for SVG).
    pub polygon: Vec<MapPoint>,
    /// `unplanned` | `agricultural` | `industrial` | `residential` |
    /// `commercial` | `mixed` | `protected_natural` | `strategic_military`.
    pub zoning: String,
    /// Render class — zoning overlaid with land-use signals:
    /// `devastated` (devastation ≥ 0.3), `resource_mines` (industrial zoning
    /// in an extraction region), or the zoning key itself.
    pub use_class: String,
    /// `state` | `private` | `corporate` | `municipal` | `cooperative` |
    /// `religious` | `foreign_fund`.
    pub owner_type: String,
    /// Raw owner entity id (`TREASURY`, `JST:<region>`, company/VIP id).
    pub owner_id: String,
    pub size_hectares: f64,
    /// Cadastre devastation index, 0.0 pristine – 1.0 total ruin.
    pub devastation: f64,
    /// Legal certainty (0 = unsurveyed, 1 = fully surveyed).
    pub legal_certainty: f64,
    /// Infrastructure access score (0 = no road/utilities, 1 = full).
    pub infrastructure_access: f64,
    pub is_frozen: bool,
    pub is_border_zone: bool,
    /// Phase 61.4 endowment tag (`forest_district`, `PrivateEstate`, …).
    pub land_use_tag: String,
}

/// A region's cadastral parcel layout for the Regions-page drill-down map.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct RegionParcelMapDto {
    pub country: String,
    pub region_id: String,
    /// Display name (`display_name` with `id` fallback).
    pub region_name: String,
    /// `[min_x, min_y, max_x, max_y]` of the parcel grid.
    pub bounds: Vec<f64>,
    pub parcels: Vec<MapParcelDto>,
}

/// `ZoningDesignation` → stable snake_case label (mirrors serde naming).
fn zoning_label(z: ZoningDesignation) -> &'static str {
    match z {
        ZoningDesignation::Unplanned => "unplanned",
        ZoningDesignation::Agricultural => "agricultural",
        ZoningDesignation::Industrial => "industrial",
        ZoningDesignation::Residential => "residential",
        ZoningDesignation::Commercial => "commercial",
        ZoningDesignation::Mixed => "mixed",
        ZoningDesignation::ProtectedNatural => "protected_natural",
        ZoningDesignation::StrategicMilitary => "strategic_military",
    }
}

/// `ParcelOwnerType` → stable snake_case label (mirrors serde naming).
fn owner_type_label(t: crate::society::cadastre::ParcelOwnerType) -> &'static str {
    use crate::society::cadastre::ParcelOwnerType as T;
    match t {
        T::State => "state",
        T::Private => "private",
        T::Corporate => "corporate",
        T::Municipal => "municipal",
        T::Cooperative => "cooperative",
        T::Religious => "religious",
        T::ForeignFund => "foreign_fund",
    }
}

/// Build the deterministic cadastral parcel map for one land region.
///
/// Parcels carry no stored geometry — the cadastre is topological — so the
/// grid below lays `region.parcel_ids` out on a jittered lattice (shared
/// vertices → seamless cells) in slotmap order. Deterministic per
/// `(region_id, lattice vertex)`, identical every call.
pub fn build_region_parcel_map(
    country: &Country,
    region_id: &str,
) -> Option<RegionParcelMapDto> {
    let region = country
        .regions
        .iter()
        .find(|r| r.id == region_id && r.node_type == NodeType::LandRegion)?;

    let mut pids = region.parcel_ids.clone();
    pids.sort();
    let n = pids.len();

    const GRID: f64 = 100.0;
    let cols = ((n as f64).sqrt().ceil() as usize).max(1);
    let rows = n.div_ceil(cols).max(1);
    let cell_w = GRID / cols as f64;
    let cell_h = GRID / rows as f64;

    // Jittered lattice shared between neighbouring cells — no gaps/overlaps.
    let mut lattice = vec![vec![(0.0_f64, 0.0_f64); cols + 1]; rows + 1];
    for (r, row) in lattice.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            let on_rim = r == 0 || c == 0 || r == rows || c == cols;
            let amp = if on_rim { 0.08 } else { 0.30 } * cell_w.min(cell_h);
            let jx =
                (hash_unit(&format!("pl:{region_id}:{r}:{c}"), 3) - 0.5) * 2.0 * amp;
            let jy =
                (hash_unit(&format!("pl:{region_id}:{r}:{c}"), 4) - 0.5) * 2.0 * amp;
            *cell = (c as f64 * cell_w + jx, r as f64 * cell_h + jy);
        }
    }

    // Industrial parcels in an extraction region render as resource mines.
    let has_extraction = region.extraction_limits.values().any(|v| *v > 0);

    let mut parcels = Vec::with_capacity(n);
    for (i, pid) in pids.iter().enumerate() {
        let Some(p) = country.cadastre.get(*pid) else {
            continue;
        };
        let (r, c) = (i / cols, i % cols);
        let polygon = vec![
            MapPoint {
                x: lattice[r][c].0,
                y: lattice[r][c].1,
            },
            MapPoint {
                x: lattice[r][c + 1].0,
                y: lattice[r][c + 1].1,
            },
            MapPoint {
                x: lattice[r + 1][c + 1].0,
                y: lattice[r + 1][c + 1].1,
            },
            MapPoint {
                x: lattice[r + 1][c].0,
                y: lattice[r + 1][c].1,
            },
        ];
        let use_class = if p.devastation_index >= 0.3 {
            "devastated"
        } else if p.zoning == ZoningDesignation::Industrial && has_extraction {
            "resource_mines"
        } else {
            zoning_label(p.zoning)
        };
        parcels.push(MapParcelDto {
            id: format!("{pid:?}"),
            polygon,
            zoning: zoning_label(p.zoning).to_string(),
            use_class: use_class.to_string(),
            owner_type: owner_type_label(p.owner_type).to_string(),
            owner_id: p.owner_id.clone(),
            size_hectares: p.size_hectares,
            devastation: p.devastation_index,
            legal_certainty: p.legal_certainty,
            infrastructure_access: p.infrastructure_access,
            is_frozen: p.is_frozen,
            is_border_zone: p.is_border_zone,
            land_use_tag: p.land_use_tag.clone(),
        });
    }

    Some(RegionParcelMapDto {
        country: country.name.clone(),
        region_id: region_id.to_string(),
        region_name: if region.display_name.is_empty() {
            region.id.clone()
        } else {
            region.display_name.clone()
        },
        bounds: vec![0.0, 0.0, GRID, GRID],
        parcels,
    })
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

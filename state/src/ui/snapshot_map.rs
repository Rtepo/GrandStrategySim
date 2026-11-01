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
    /// v4.27.0: True offshore island/archipelago region — its polygon is a
    /// real island blob OUTSIDE the country boundary, connected to the
    /// mainland only by `sea_lane` edges (never land adjacency).
    pub is_island: bool,
    /// v4.27.0: Climate/terrain classification driving the Topography &
    /// Biome map (`Mountains` | `Highlands` | `Steppe` | `Plains` |
    /// `Forest` | `Wetlands` | `Coast`).
    pub biome: String,
    /// v4.27.0: Dominant local-council faction for the Party map mode
    /// (empty when the region has no seated council).
    pub dominant_party: String,
    /// v4.27.0: Seat share (0–1) of the dominant faction.
    pub party_share: f64,
    /// v4.27.0: Net internal+international migration balance (persons/turn)
    /// — negative = net emigration, positive = net immigration.
    pub net_migration: i64,
    /// v4.27.0: Urban-class population — drives city-dot scaling in the
    /// Political & Infrastructure mode.
    pub urban_population: i64,
}

/// v4.27.0: A major city marker — dot scaled by urban population, rendered
/// by the Political & Infrastructure map mode.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MapCityDto {
    pub region_id: String,
    pub name: String,
    pub point: MapPoint,
    pub population: i64,
    pub is_capital: bool,
}

/// v4.27.0: A primary transport artery between regional hubs. Points run
/// centroid-to-centroid, so the polyline always crosses polygon interiors
/// (never the outer country border).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MapRouteDto {
    /// `road` | `rail`.
    pub kind: String,
    pub points: Vec<MapPoint>,
}

/// v4.27.0: A directed migrant flow between two regions — rendered as a
/// scaled directional arrow in the Internal Migration map mode.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MapFlowDto {
    pub from_region: String,
    pub to_region: String,
    /// Persons per turn moving from → to.
    pub amount: i64,
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
    /// v4.27.0: Major cities (dots scaled by urban population).
    pub cities: Vec<MapCityDto>,
    /// v4.27.0: Primary road/rail transport arteries between regional hubs.
    pub routes: Vec<MapRouteDto>,
    /// v4.27.0: Directed inter-region migrant flows for the Internal
    /// Migration map mode.
    pub flows: Vec<MapFlowDto>,
}

/// A decorative map feature (inland lake, inland sea, river, or mountain
/// range) — pure presentation geometry derived from the same
/// deterministic layout.
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export, export_to = "../../src/types/api.ts")]
pub struct MapFeatureDto {
    /// `lake` | `inland_sea` | `river` | `mountain`.
    pub kind: String,
    /// `lake`: closed polygon vertices. `mountain`: ridge polyline points.
    pub points: Vec<MapPoint>,
    /// Owning region id for cell-local features (`lake`, `inland_sea`,
    /// `river`); empty for `mountain` (spans a country border — see
    /// `country_tag`).
    pub region_id: String,
    /// Owning country for cell features; `A|B` (sorted, pipe-joined) for
    /// border mountains so `onlyCountry` views can include them.
    pub country_tag: String,
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
    /// snapshot (fractal subdivision guarantees ≥40).
    pub min_region_polygon_vertices: u32,
    /// v4.27.0 release gate: regions whose Voronoi cell degenerated into a
    /// zero-area fallback blob — the "pseudo-island dots" defect. Must
    /// stay 0 (sites are clamped inside the country boundary; deliberate
    /// islands get real blob polygons instead).
    pub degenerate_region_count: u32,
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
// v4.27.0: polygon predicates — island siting, site clamping, degenerate
// detection. All pure functions of position + deterministic hash salts.
// ============================================================================

/// Ray-cast point-in-polygon test (even-odd rule).
fn point_in_polygon(p: (f64, f64), poly: &[(f64, f64)]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if ((yi > p.1) != (yj > p.1))
            && (p.0 < (xj - xi) * (p.1 - yi) / (yj - yi) + xi)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Shoelace signed area (absolute value).
fn polygon_area(poly: &[(f64, f64)]) -> f64 {
    if poly.len() < 3 {
        return 0.0;
    }
    let mut s = 0.0;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        s += a.0 * b.1 - b.0 * a.1;
    }
    s.abs() * 0.5
}

/// Polygon centroid (vertex mean — good enough for layout work).
fn polygon_centroid(poly: &[(f64, f64)]) -> (f64, f64) {
    if poly.is_empty() {
        return (0.0, 0.0);
    }
    let (mut sx, mut sy) = (0.0, 0.0);
    for p in poly {
        sx += p.0;
        sy += p.1;
    }
    (sx / poly.len() as f64, sy / poly.len() as f64)
}

/// Pull a site that fell outside `boundary` back inside: binary-search the
/// anchor→site ray for the boundary crossing, then settle at 85% of the
/// way out (keeps the cell near the rim instead of collapsing to centre).
/// Guarantees `point_in_polygon(result, boundary)` for convex-ish cells.
fn clamp_inside_boundary(
    site: (f64, f64),
    boundary: &[(f64, f64)],
    anchor: (f64, f64),
) -> (f64, f64) {
    if point_in_polygon(site, boundary) {
        return site;
    }
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for _ in 0..40 {
        let mid = (lo + hi) * 0.5;
        let p = (anchor.0 + (site.0 - anchor.0) * mid, anchor.1 + (site.1 - anchor.1) * mid);
        if point_in_polygon(p, boundary) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let t = lo * 0.85;
    (anchor.0 + (site.0 - anchor.0) * t, anchor.1 + (site.1 - anchor.1) * t)
}

/// Push a rim site OUTSIDE the country boundary: march the anchor→site ray
/// past the boundary crossing, then offset by `off` world units seaward.
fn push_offshore(
    site: (f64, f64),
    boundary: &[(f64, f64)],
    anchor: (f64, f64),
    off: f64,
) -> (f64, f64) {
    // Find the ray parameter where the site exits the polygon.
    let (mut lo, mut hi) = (0.0_f64, 4.0_f64);
    if !point_in_polygon(
        (anchor.0 + (site.0 - anchor.0) * hi, anchor.1 + (site.1 - anchor.1) * hi),
        boundary,
    ) {
        for _ in 0..48 {
            let mid = (lo + hi) * 0.5;
            let p = (anchor.0 + (site.0 - anchor.0) * mid, anchor.1 + (site.1 - anchor.1) * mid);
            if point_in_polygon(p, boundary) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
    }
    // `hi` ≈ first outside point — step a bit further seaward.
    let t = hi + off;
    (anchor.0 + (site.0 - anchor.0) * t, anchor.1 + (site.1 - anchor.1) * t)
}

/// Jittered N-gon island blob around `site` — a REAL polygon with proper
/// area (~0.5–0.9 REGION_SPACING radius), subdivided like every other cell
/// so coastlines stay fractal.
fn island_blob(site: (f64, f64), region_id: &str) -> Vec<(f64, f64)> {
    let base_r = REGION_SPACING * (0.55 + hash_unit(&format!("isle-r:{region_id}"), 101) * 0.45);
    let n = 10 + (hash_unit(&format!("isle-n:{region_id}"), 103) * 5.0) as usize;
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let a = (i as f64 / n as f64) * std::f64::consts::TAU;
        let jr = 1.0 + (hash_unit(&format!("isle:{region_id}:{i}"), 107) - 0.5) * 0.5;
        pts.push((
            site.0 + base_r * jr * a.cos(),
            site.1 + base_r * jr * a.sin(),
        ));
    }
    pts
}

/// Climate/terrain biome for the Topography & Biome map. Priority order:
/// true mountains > coast > arid steppe > riverine wetlands > forest/
/// plains by climate — deterministic hash breaks the fertile/forest tie.
fn biome_of(region: &Region, is_coastal: bool) -> &'static str {
    let t = &region.geographic_traits;
    if matches!(region.climate, crate::society::geography::Climate::Mountainous)
        || region.elevation_difference_m > 800.0
    {
        return "Mountains";
    }
    if t.has_mountain_pass || region.elevation_difference_m > 300.0 {
        return "Highlands";
    }
    if is_coastal {
        return "Coast";
    }
    if matches!(region.climate, crate::society::geography::Climate::Desert) {
        return "Steppe";
    }
    if t.has_navigable_river
        && hash_unit(&format!("wet:{}", region.id), 109) < 0.4
    {
        return "Wetlands";
    }
    match region.climate {
        crate::society::geography::Climate::Fertile => {
            if hash_unit(&format!("for:{}", region.id), 111) < 0.45 {
                "Forest"
            } else {
                "Plains"
            }
        }
        _ => {
            if hash_unit(&format!("for:{}", region.id), 113) < 0.3 {
                "Forest"
            } else {
                "Plains"
            }
        }
    }
}

/// Dominant local-council faction `(name, seat share 0–1)` for the Party
/// map mode — real governance data, empty name when no council is seated.
fn dominant_faction(region: &Region) -> (String, f64) {
    let Some(gov) = &region.governance else {
        return (String::new(), 0.0);
    };
    let fd = &gov.council.faction_distribution;
    let total = (fd.populares_count + fd.moderates_count + fd.optimates_count) as f64;
    if total <= 0.0 {
        return (String::new(), 0.0);
    }
    let (name, seats) = [
        ("Populares", fd.populares_count),
        ("Moderates", fd.moderates_count),
        ("Optimates", fd.optimates_count),
    ]
    .into_iter()
    .max_by_key(|(_, s)| *s)
    .unwrap_or(("", 0));
    (name.to_string(), seats as f64 / total)
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
/// v4.27.0: three octaves (was two) for higher-detail coastlines.
fn fractal_displace(
    p: (f64, f64),
    edge_dir: (f64, f64),
    edge_len: f64,
    salt: u64,
) -> (f64, f64) {
    let n = 0.52 * value_noise_2d(p.0 * 0.11, p.1 * 0.11, salt)
        + 0.33 * value_noise_2d(p.0 * 0.34, p.1 * 0.34, salt + 91)
        + 0.15 * value_noise_2d(p.0 * 0.9, p.1 * 0.9, salt + 173);
    // Amplitude: ~16% of edge length, capped so small cells stay sane.
    let amp = (edge_len * 0.16).min(REGION_SPACING * 0.45);
    let inv = 1.0 / edge_len.max(1e-9);
    let (mut nx, mut ny) = (-edge_dir.1 * inv, edge_dir.0 * inv);
    // Canonical half-edge orientation: two cells traverse their shared edge
    // in opposite directions (consistent winding), which flips the normal
    // sign and cracks the border open. Force a canonical normal direction
    // so both polygons displace the shared border identically — zero gaps.
    if nx < 0.0 || (nx == 0.0 && ny < 0.0) {
        nx = -nx;
        ny = -ny;
    }
    (
        p.0 + nx * (n - 0.5) * 2.0 * amp,
        p.1 + ny * (n - 0.5) * 2.0 * amp,
    )
}

/// Salt shared by every subdivided edge in the snapshot — a single global
/// noise field is what makes shared borders line up across adjacent cells.
const SUBDIV_SALT: u64 = 424_242;

/// Subdivide every polygon edge with fractal interior displacement.
/// v4.27.0: each edge is cut into ≥max(8, ⌈46/m⌉) segments (longer edges
/// get proportionally more, up to 36) so every region lands at ≥40–60
/// vertices — the `min_region_polygon_vertices` release gate asserts ≥40.
fn subdivide_polygon(poly: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let m = poly.len();
    if m < 3 {
        return poly.to_vec();
    }
    // Per-edge segment floor that guarantees ≥46 total vertices.
    let floor = ((46usize + m - 1) / m).max(8);
    let mut out = Vec::with_capacity(m * 12);
    for i in 0..m {
        let a = poly[i];
        let b = poly[(i + 1) % m];
        out.push(a);
        let dir = (b.0 - a.0, b.1 - a.1);
        let len = (dir.0 * dir.0 + dir.1 * dir.1).sqrt();
        // ≥floor segments per edge; longer edges get proportionally more.
        let n = ((len / (REGION_SPACING * 0.5)).ceil() as usize).clamp(floor, 36);
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
        /// v4.27.0: indices into `regions`/`sites` designated as offshore
        /// islands — excluded from Voronoi clipping, drawn as island blobs.
        islands: std::collections::HashSet<usize>,
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
            islands: std::collections::HashSet::new(),
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

        // --------------------------------------------------------------
        // v4.27.0: island designation + pseudo-island elimination.
        //
        // (a) Offshore sites previously degenerated to zero-area fallback
        //     octagons ("pseudo-island dots") that the engine edge graph
        //     then wired to every land region. Two fixes:
        // (b) Deliberate islands: a deterministic subset of rim regions is
        //     pushed OUTSIDE the boundary and rendered as a real island
        //     blob connected by sea lanes — true archipelagos.
        // (c) Accidentally-offshore land sites are clamped back inside so
        //     every land region keeps a proper polygon.
        // --------------------------------------------------------------
        let n_regions = layout.regions.len();
        if n_regions >= 6 {
            // Candidate rim regions: non-capital sites farthest from the
            // anchor (outer ring). Island quota is hash-stable per country.
            let quota = 1 + (hash_unit(&format!("isle-n:{}", layout.name), 97)
                < 0.45) as usize;
            let mut rim: Vec<usize> = (0..n_regions)
                .filter(|&i| !layout.regions[i].is_capital)
                .collect();
            rim.sort_by(|&a, &b| {
                let da = (layout.sites[a].0 - layout.anchor.0).powi(2)
                    + (layout.sites[a].1 - layout.anchor.1).powi(2);
                let db = (layout.sites[b].0 - layout.anchor.0).powi(2)
                    + (layout.sites[b].1 - layout.anchor.1).powi(2);
                // Deterministic order: farthest first, id-hash tiebreak.
                db.partial_cmp(&da)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| layout.regions[a].id.cmp(&layout.regions[b].id))
            });
            let mut taken = 0usize;
            for &i in &rim {
                if taken >= quota {
                    break;
                }
                let id = &layout.regions[i].id;
                // ~30% of rim candidates become islands.
                if hash_unit(&format!("isle:{id}"), 97) >= 0.30 {
                    continue;
                }
                let off = 0.35
                    + hash_unit(&format!("isle-off:{id}"), 99) * 0.30;
                layout.sites[i] = push_offshore(
                    layout.sites[i],
                    &layout.boundary,
                    layout.anchor,
                    off,
                );
                layout.islands.insert(i);
                taken += 1;
            }
        }
        // (c) clamp any remaining out-of-boundary sites into the interior.
        for i in 0..layout.sites.len() {
            if layout.islands.contains(&i) {
                continue;
            }
            layout.sites[i] =
                clamp_inside_boundary(layout.sites[i], &layout.boundary, layout.anchor);
        }
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
    // One Lloyd-relaxation pass: ring-only seed layouts (capital + a single
    // ring, or legacy circular coordinates) collapse into radial wedge
    // cells that all meet at the country centre — "pizza slices". Moving
    // each site to its cell centroid breaks the shared centre vertex so
    // true interior (landlocked) and coastal cells emerge.
    // v4.27.0: island sites are excluded from Voronoi clipping — an
    // offshore site would bisect and steal coastal area from land cells.
    // Each land cell clips against land sites only; islands get blobs.
    for lloyd in 0..2 {
        region_cells.clear();
        for layout in layouts.iter() {
            let land_sites: Vec<(f64, f64)> = layout
                .sites
                .iter()
                .enumerate()
                .filter(|(i, _)| !layout.islands.contains(i))
                .map(|(_, s)| *s)
                .collect();
            let mut cells = Vec::with_capacity(layout.regions.len());
            for (idx, _) in layout.regions.iter().enumerate() {
                if layout.islands.contains(&idx) {
                    cells.push(Vec::new()); // island — blob polygon later
                    continue;
                }
                // Passing ALL land sites (including the region's own) is
                // fine — `voronoi_cell` skips coincident sites.
                cells.push(voronoi_cell(
                    layout.sites[idx],
                    &land_sites,
                    &layout.boundary,
                ));
            }
            region_cells.push(cells);
        }
        if lloyd == 0 {
            for (li, layout) in layouts.iter_mut().enumerate() {
                for (idx, _) in layout.regions.iter().enumerate() {
                    if layout.islands.contains(&idx) {
                        continue;
                    }
                    let cell = &region_cells[li][idx];
                    if cell.len() >= 3 {
                        let (mut sx, mut sy) = (0.0, 0.0);
                        for p in cell {
                            sx += p.0;
                            sy += p.1;
                        }
                        let n = cell.len() as f64;
                        layout.sites[idx] = (sx / n, sy / n);
                    }
                }
            }
        }
    }
    // ------------------------------------------------------------------
    // v4.27.0: TRUE geometric adjacency — `neighbors` and `edges` are
    // recomputed strictly from shared Voronoi cell edges (≥2 coincident
    // vertices), replacing the engine edge graph (circular + capital-star
    // wiring) that listed neighbours the map never shows — e.g. offshore
    // dot regions "bordering" every land region. The text list now 100%
    // matches rendered neighbours.
    // ------------------------------------------------------------------
    let mut features_out: Vec<MapFeatureDto> = Vec::new();
    // Undirected adjacency keyed by unordered region pair.
    let mut geo_pairs: std::collections::BTreeSet<(String, String)> =
        std::collections::BTreeSet::new();
    // Cross-country subset (drives shared_land_borders counting).
    let mut cross_pairs: std::collections::BTreeSet<(String, String)> =
        std::collections::BTreeSet::new();
    let mut geo_neighbors: std::collections::HashMap<
        String,
        std::collections::BTreeSet<String>,
    > = std::collections::HashMap::new();
    // Epsilon for shared-vertex matching — bisector intersections coincide
    // up to float noise; distinct vertices sit ≥0.1 spacing apart.
    let eps_nb2 = (REGION_SPACING * 0.02).powi(2);
    let mut add_adjacency =
        |a: &str, b: &str, cross: bool, geo_pairs: &mut std::collections::BTreeSet<(String, String)>, cross_pairs: &mut std::collections::BTreeSet<(String, String)>, geo_neighbors: &mut std::collections::HashMap<String, std::collections::BTreeSet<String>>| {
            let (x, y) = if a < b {
                (a.to_string(), b.to_string())
            } else {
                (b.to_string(), a.to_string())
            };
            if geo_pairs.insert((x.clone(), y.clone())) {
                geo_neighbors
                    .entry(a.to_string())
                    .or_default()
                    .insert(b.to_string());
                geo_neighbors
                    .entry(b.to_string())
                    .or_default()
                    .insert(a.to_string());
                if cross {
                    cross_pairs.insert((x, y));
                }
            }
        };

    // Same-country region adjacency — O(n²) over non-island cells.
    for (li, layout) in layouts.iter().enumerate() {
        for i in 0..layout.regions.len() {
            if layout.islands.contains(&i) {
                continue;
            }
            for j in (i + 1)..layout.regions.len() {
                if layout.islands.contains(&j) {
                    continue;
                }
                let (ci, cj) = (&region_cells[li][i], &region_cells[li][j]);
                if ci.len() >= 3
                    && cj.len() >= 3
                    && shared_vertices(ci, cj, eps_nb2).len() >= 2
                {
                    add_adjacency(
                        &layout.regions[i].id,
                        &layout.regions[j].id,
                        false,
                        &mut geo_pairs,
                        &mut cross_pairs,
                        &mut geo_neighbors,
                    );
                }
            }
        }
    }

    // Cross-country adjacency — only for country pairs that share a
    // continental border (cells share the duplicated boundary vertices).
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
            for (ri, ra) in layouts[i].regions.iter().enumerate() {
                if layouts[i].islands.contains(&ri) {
                    continue;
                }
                let ca = &region_cells[i][ri];
                if ca.len() < 3 {
                    continue;
                }
                for (rj, rb) in layouts[j].regions.iter().enumerate() {
                    if layouts[j].islands.contains(&rj) {
                        continue;
                    }
                    let cb = &region_cells[j][rj];
                    if cb.len() < 3 {
                        continue;
                    }
                    if shared_vertices(ca, cb, eps_nb2).len() >= 2 {
                        add_adjacency(
                            &ra.id,
                            &rb.id,
                            true,
                            &mut geo_pairs,
                            &mut cross_pairs,
                            &mut geo_neighbors,
                        );
                    }
                }
            }
            if hash_unit(&format!("mtn:{}:{}", layouts[i].name, layouts[j].name), 73) < 0.4
            {
                features_out.push(MapFeatureDto {
                    kind: "mountain".to_string(),
                    points: mountain_ridge(&shared, i as u64 * 131 + j as u64)
                        .into_iter()
                        .map(|p| MapPoint { x: p.0, y: p.1 })
                        .collect(),
                    region_id: String::new(),
                    country_tag: format!("{}|{}", layouts[i].name, layouts[j].name),
                });
            }
        }
    }

    // Island maritime links — sea lanes to the 2 nearest mainland cells of
    // the same country (maritime adjacency, never land borders).
    let mut sea_lane_pairs: Vec<(String, String)> = Vec::new();
    for layout in layouts.iter() {
        let land: Vec<usize> = (0..layout.regions.len())
            .filter(|i| !layout.islands.contains(i))
            .collect();
        for &ii in &layout.islands {
            let isite = layout.sites[ii];
            let mut by_dist: Vec<usize> = land.clone();
            by_dist.sort_by(|&a, &b| {
                let da = (layout.sites[a].0 - isite.0).powi(2)
                    + (layout.sites[a].1 - isite.1).powi(2);
                let db = (layout.sites[b].0 - isite.0).powi(2)
                    + (layout.sites[b].1 - isite.1).powi(2);
                da.partial_cmp(&db)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| layout.regions[a].id.cmp(&layout.regions[b].id))
            });
            for &li in by_dist.iter().take(2) {
                let (a, b) = (
                    layout.regions[ii].id.clone(),
                    layout.regions[li].id.clone(),
                );
                let key = if a < b { (a.clone(), b.clone()) } else { (b.clone(), a.clone()) };
                if !sea_lane_pairs.contains(&key) {
                    sea_lane_pairs.push(key);
                    // Maritime adjacency also belongs in the textual
                    // neighbours list — "connected via sea zones/straits".
                    geo_neighbors
                        .entry(a.clone())
                        .or_default()
                        .insert(b.clone());
                    geo_neighbors
                        .entry(b.clone())
                        .or_default()
                        .insert(a.clone());
                }
            }
        }
    }

    // Engine `River` edges for edge-type labelling (both directions).
    let mut river_pairs: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();
    for layout in layouts.iter() {
        for region in &layout.regions {
            for e in &region.edges {
                if e.edge_type == EdgeType::River {
                    let (a, b) = if region.id < e.target_node {
                        (region.id.clone(), e.target_node.clone())
                    } else {
                        (e.target_node.clone(), region.id.clone())
                    };
                    river_pairs.insert((a, b));
                }
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
    let mut degenerate_regions: u32 = 0;

    // v4.27.0 inland-sea placement: per continent, the largest interior
    // (non-coastal) cell — picked deterministically by area.
    let mut inland_sea_pick: std::collections::HashSet<(usize, usize)> =
        std::collections::HashSet::new();
    for (li, layout) in layouts.iter().enumerate() {
        let mut best: Option<(usize, f64)> = None;
        for (idx, cell) in region_cells[li].iter().enumerate() {
            if layout.islands.contains(&idx) || cell.len() < 3 {
                continue;
            }
            let coastal = region_engine_coastal(&layout.regions[idx])
                || cell_touches_boundary(
                    cell,
                    &layout.continent_boundary,
                    (layout.radius * 1e-3).powi(2),
                );
            if coastal {
                continue;
            }
            let area = polygon_area(cell);
            if best.map(|(_, a)| area > a).unwrap_or(true) {
                best = Some((idx, area));
            }
        }
        if let Some((idx, _)) = best {
            inland_sea_pick.insert((li, idx));
        }
    }

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
            let is_island = layout.islands.contains(&idx);
            let cell = &region_cells[color_index][idx];
            // Islands draw their own blob polygon; land regions use the
            // Voronoi cell. Zero-area degenerate cells are counted for the
            // pseudo-island release gate (should stay 0 post-clamping).
            let raw = if is_island {
                island_blob(site, &region.id)
            } else {
                if cell.len() < 3
                    || polygon_area(cell) < (REGION_SPACING * 0.45).powi(2)
                {
                    degenerate_regions += 1;
                }
                cell.clone()
            };
            let sub = subdivide_polygon(&raw);
            min_poly_verts = min_poly_verts.min(sub.len());
            let polygon: Vec<MapPoint> =
                sub.iter().map(|p| MapPoint { x: p.0, y: p.1 }).collect();

            // v4.27.0: neighbours strictly from shared Voronoi cell edges —
            // deduplicated, symmetric, 100% matching the rendered map.
            let neighbors: Vec<String> = geo_neighbors
                .get(&region.id)
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default();

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

            let is_coastal = is_island
                || region_engine_coastal(region)
                || cell_touches_boundary(cell, &layout.continent_boundary, eps2);

            // Inland lake on a deterministic subset of roomy interior
            // cells (v4.27.0: raised coverage 16%→24%).
            if !is_island
                && !is_coastal
                && hash_unit(&format!("lake:{}", region.id), 71) < 0.24
            {
                if let Some(lake) =
                    lake_feature(cell, &region.id, &layout.name, 0.18, "lake")
                {
                    features_out.push(lake);
                }
            }
            // Navigable-river polyline through cells that carry one.
            if !is_island && region.geographic_traits.has_navigable_river {
                if let Some(river) = river_feature(cell, &region.id, &layout.name) {
                    features_out.push(river);
                }
            }
            // Inland sea: one grand basin per continent on its largest
            // interior cell (deterministic pick by area).
            if !is_island
                && !is_coastal
                && inland_sea_pick.get(&(color_index, idx)).is_some()
            {
                if let Some(sea) =
                    lake_feature(cell, &region.id, &layout.name, 0.33, "inland_sea")
                {
                    features_out.push(sea);
                }
            }

            let (control, controller) = control_label(region_control.get(&region.id));
            let (dominant_party, party_share) = dominant_faction(region);
            let urban_population: i64 = region
                .class_demographics
                .urban_classes
                .values()
                .map(|c| c.population.max(0))
                .sum();
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
                is_island,
                biome: biome_of(region, is_coastal).to_string(),
                dominant_party,
                party_share,
                net_migration: region_net_migration(region),
                urban_population,
            });
        }

        // v4.27.0: cities + transport arteries for the Political &
        // Infrastructure mode. Cities = top regions by urban population
        // (capital always included), anchored at their centroid.
        let mut by_urban: Vec<usize> = (0..regions_out.len()).collect();
        by_urban.sort_by(|&a, &b| {
            regions_out[b]
                .urban_population
                .cmp(&regions_out[a].urban_population)
        });
        let mut cities = Vec::new();
        for &i in by_urban.iter().take(12) {
            let r = &regions_out[i];
            if r.urban_population <= 0 && !r.is_capital_district {
                continue;
            }
            cities.push(MapCityDto {
                region_id: r.id.clone(),
                name: r.name.clone(),
                point: r.centroid.clone(),
                population: r.urban_population,
                is_capital: r.is_capital_district,
            });
        }
        // Arteries: centroid→centroid polylines between geometric
        // neighbours — through polygon interiors by construction. Roads
        // link every adjacent pair where at least one endpoint is a major
        // hub; rails connect the capital to the top hubs.
        let city_set: std::collections::HashSet<&str> =
            cities.iter().map(|c| c.region_id.as_str()).collect();
        let centroid_of = |rid: &str| -> Option<MapPoint> {
            regions_out
                .iter()
                .find(|r| r.id == rid)
                .map(|r| r.centroid.clone())
        };
        let mut routes: Vec<MapRouteDto> = Vec::new();
        let mut route_seen: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        for (a, b) in &geo_pairs {
            let (ra, rb) = (centroid_of(a), centroid_of(b));
            let (Some(pa), Some(pb)) = (ra, rb) else {
                continue;
            };
            let is_hub = |rid: &str| city_set.contains(rid);
            let mut emit = |kind: &str, routes: &mut Vec<MapRouteDto>, route_seen: &mut std::collections::HashSet<(String, String)>| {
                if !route_seen.insert((a.clone(), b.clone())) {
                    return;
                }
                let mid = MapPoint {
                    x: (pa.x + pb.x) * 0.5,
                    y: (pa.y + pb.y) * 0.5,
                };
                routes.push(MapRouteDto {
                    kind: kind.to_string(),
                    points: vec![pa, mid, pb],
                });
            };
            if is_hub(a) || is_hub(b) {
                emit("road", &mut routes, &mut route_seen);
            }
        }
        // Rail: capital ↔ three largest urban hubs.
        if let Some(cap) = cities.iter().find(|c| c.is_capital) {
            for c in cities.iter().filter(|c| !c.is_capital).take(3) {
                let mid = MapPoint {
                    x: (cap.point.x + c.point.x) * 0.5,
                    y: (cap.point.y + c.point.y) * 0.5,
                };
                routes.push(MapRouteDto {
                    kind: "rail".to_string(),
                    points: vec![cap.point.clone(), mid, c.point.clone()],
                });
            }
        }

        countries_out.push(CountryMapDto {
            country: layout.name.clone(),
            color_index: color_index as u32,
            at_war_with: country.at_war_with.clone(),
            capital_name,
            anchor: capital_anchor,
            boundary: boundary_pts,
            regions: regions_out,
            cities,
            routes,
            // v4.27.0 (TASK-006): real internal region→region flows from
            // the migration ledger — rendered as directional arrows.
            flows: country
                .last_internal_flows
                .iter()
                .map(|f| MapFlowDto {
                    from_region: f.from_region.clone(),
                    to_region: f.to_region.clone(),
                    amount: f.count,
                })
                .collect(),
        });
    }

    // Emit deduplicated adjacency edges — geometric land borders (labelled
    // `river` where an engine river runs along the border) + island sea
    // lanes. Text neighbours and rendered edges share this one source of
    // truth.
    for (a, b) in &geo_pairs {
        if !edge_seen.insert((a.clone(), b.clone())) {
            continue;
        }
        let kind = if river_pairs.contains(&(a.clone(), b.clone())) {
            "river"
        } else {
            "land_border"
        };
        edges_out.push(MapEdgeDto {
            from_region: a.clone(),
            to_region: b.clone(),
            edge_type: kind.to_string(),
        });
    }
    for (a, b) in &sea_lane_pairs {
        if edge_seen.insert((a.clone(), b.clone())) {
            edges_out.push(MapEdgeDto {
                from_region: a.clone(),
                to_region: b.clone(),
                edge_type: "sea_lane".to_string(),
            });
        }
    }
    shared_land_borders += cross_pairs.len() as u32;

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
        degenerate_region_count: degenerate_regions,
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

/// Small noise-jittered blob → inland lake or inland sea inside a roomy
/// interior cell. `scale` is the radius fraction of the cell's smallest
/// dimension; `kind` is `lake` or `inland_sea` (deeper water render).
fn lake_feature(
    cell: &[(f64, f64)],
    region_id: &str,
    country_tag: &str,
    scale: f64,
    kind: &str,
) -> Option<MapFeatureDto> {
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
    let r = w.min(h) * scale;
    let n = if kind == "inland_sea" { 14 } else { 9 };
    let mut pts = Vec::with_capacity(n);
    for i in 0..n {
        let a = (i as f64 / n as f64) * std::f64::consts::TAU + hash_unit(region_id, 77);
        let jr = 1.0 + (hash_unit(&format!("{region_id}-lk{i}"), 79) - 0.5) * 0.45;
        pts.push(MapPoint {
            x: sx + r * jr * a.cos(),
            y: sy + r * jr * a.sin(),
        });
    }
    Some(MapFeatureDto {
        kind: kind.to_string(),
        points: pts,
        region_id: region_id.to_string(),
        country_tag: country_tag.to_string(),
    })
}

/// v4.27.0: river polyline through a cell — enters and exits through two
/// rim vertices with a noise-displaced midline. Rendered as a water course.
fn river_feature(
    cell: &[(f64, f64)],
    region_id: &str,
    country_tag: &str,
) -> Option<MapFeatureDto> {
    if cell.len() < 3 {
        return None;
    }
    // Two rim vertices picked deterministically far apart.
    let n = cell.len();
    let i0 = (hash_unit(&format!("rv0:{region_id}"), 83) * n as f64) as usize % n;
    let i1 = (i0 + n / 2) % n;
    let (a, b) = (cell[i0], cell[i1]);
    let mut pts = Vec::with_capacity(5);
    for k in 0..5 {
        let t = k as f64 / 4.0;
        let bx = a.0 + (b.0 - a.0) * t;
        let by = a.1 + (b.1 - a.1) * t;
        let wob = (value_noise_2d(bx * 0.4, by * 0.4, 89) - 0.5)
            * REGION_SPACING
            * 0.35
            * (t * (1.0 - t) * 4.0); // taper at the ends
        let dir = (b.0 - a.0, b.1 - a.1);
        let len = (dir.0 * dir.0 + dir.1 * dir.1).sqrt().max(1e-9);
        pts.push(MapPoint {
            x: bx + (-dir.1 / len) * wob,
            y: by + (dir.0 / len) * wob,
        });
    }
    Some(MapFeatureDto {
        kind: "river".to_string(),
        points: pts,
        region_id: region_id.to_string(),
        country_tag: country_tag.to_string(),
    })
}

/// v4.27.0: per-region net migration (persons/turn) — wired to the real
/// regional flow ledger added by TASK-006
/// (`Region.migration.last_turn_net`); 0 for saves that predate it.
fn region_net_migration(region: &Region) -> i64 {
    region.migration.last_turn_net
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
    /// v4.27.0: real parcel topography (`river` | `lake` | `sea` | `none`)
    /// — water parcels render as rivers/lakes on the cadastral map.
    pub water_access: String,
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

/// `WaterAccessType` → stable snake_case label for the parcel map.
fn water_access_label(t: crate::society::cadastre::WaterAccessType) -> &'static str {
    use crate::society::cadastre::WaterAccessType as W;
    match t {
        W::None => "none",
        W::River => "river",
        W::Lake => "lake",
        W::Sea => "sea",
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
        } else if p.topography.water_access != crate::society::cadastre::WaterAccessType::None {
            // v4.27.0: real topographic water parcels — rivers/lakes/sea
            // frontage render as water on the cadastral map.
            "water"
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
            water_access: water_access_label(p.topography.water_access).to_string(),
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

    // ==================================================================
    // v4.27.0 TASK-005 — geometry invariants
    // ==================================================================

    fn shoelace(pts: &[MapPoint]) -> f64 {
        let mut s = 0.0;
        for i in 0..pts.len() {
            let (a, b) = (&pts[i], &pts[(i + 1) % pts.len()]);
            s += a.x * b.y - b.x * a.y;
        }
        s.abs() / 2.0
    }

    fn multi_country_state() -> GameState {
        let mut state = GameState::new();
        for (i, n) in [9usize, 9, 7, 12, 6, 8].iter().enumerate() {
            let name = format!("Ctry{i}");
            state
                .countries
                .insert(name.clone(), test_country(&name, *n));
        }
        state
    }

    #[test]
    fn v4270_no_zero_area_pseudo_islands() {
        let snap = build_world_map(&multi_country_state());
        assert_eq!(
            snap.degenerate_region_count, 0,
            "degenerate fallback blobs still emitted"
        );
        for c in &snap.countries {
            for r in &c.regions {
                assert!(
                    shoelace(&r.polygon) > 1.0,
                    "region {} polygon area ~0",
                    r.id
                );
            }
        }
    }

    #[test]
    fn v4270_adjacency_symmetric_deduplicated_and_matches_edges() {
        let snap = build_world_map(&multi_country_state());
        let mut listed: std::collections::HashMap<&str, std::collections::BTreeSet<&str>> =
            std::collections::HashMap::new();
        for c in &snap.countries {
            for r in &c.regions {
                // No self-loop, no duplicates inside a single list.
                let set: std::collections::BTreeSet<&str> =
                    r.neighbors.iter().map(|s| s.as_str()).collect();
                assert_eq!(
                    set.len(),
                    r.neighbors.len(),
                    "duplicate neighbours in {}: {:?}",
                    r.id,
                    r.neighbors
                );
                assert!(!set.contains(r.id.as_str()), "self-adjacency {}", r.id);
                listed.insert(r.id.as_str(), set);
            }
        }
        // Symmetry: a ∈ nb(b) ⇔ b ∈ nb(a).
        for (a, nbs) in &listed {
            for b in nbs {
                assert!(
                    listed
                        .get(b)
                        .map(|s| s.contains(*a))
                        .unwrap_or(false),
                    "asymmetric adjacency {} -> {}",
                    a,
                    b
                );
            }
        }
        // The rendered edge set and the text lists share one source: every
        // undirected edge appears in both endpoints' neighbour lists.
        for e in &snap.edges {
            assert!(
                listed[e.from_region.as_str()].contains(e.to_region.as_str()),
                "edge {}—{} missing from neighbour list",
                e.from_region,
                e.to_region
            );
        }
        // And every text neighbour is backed by a real edge.
        let edge_set: std::collections::BTreeSet<(&str, &str)> = snap
            .edges
            .iter()
            .map(|e| (e.from_region.as_str(), e.to_region.as_str()))
            .collect();
        for (a, nbs) in &listed {
            for b in nbs {
                let key = if a < b { (*a, *b) } else { (*b, *a) };
                assert!(
                    edge_set.contains(&key),
                    "neighbour {}—{} has no rendered edge",
                    a,
                    b
                );
            }
        }
    }

    #[test]
    fn v4270_fractal_subdivision_guarantees_40_vertices() {
        let snap = build_world_map(&multi_country_state());
        assert!(
            snap.min_region_polygon_vertices >= 40,
            "min polygon verts {} < 40",
            snap.min_region_polygon_vertices
        );
    }

    #[test]
    fn v4270_islands_are_maritime_never_land_adjacent() {
        let snap = build_world_map(&multi_country_state());
        let islands: Vec<&MapRegionDto> = snap
            .countries
            .iter()
            .flat_map(|c| c.regions.iter())
            .filter(|r| r.is_island)
            .collect();
        for isl in &islands {
            // Real offshore polygon — a blob with nonzero area, not a dot.
            assert!(
                shoelace(&isl.polygon) > 1.0,
                "island {} polygon is degenerate",
                isl.id
            );
            // Every rendered edge touching an island is a sea lane —
            // never a land border.
            for e in snap
                .edges
                .iter()
                .filter(|e| e.from_region == isl.id || e.to_region == isl.id)
            {
                assert_eq!(
                    e.edge_type, "sea_lane",
                    "island {} has non-maritime edge {:?}",
                    isl.id, e.edge_type
                );
            }
            // Its neighbours exist (maritime links to the mainland).
            assert!(
                !isl.neighbors.is_empty(),
                "island {} has no maritime neighbours",
                isl.id
            );
        }
    }
}

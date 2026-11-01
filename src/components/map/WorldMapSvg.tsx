/**
 * v4.24.0 WS5 — Shared SVG renderer for the procedural world map.
 *
 * Used full-size by `MapPage` and compact by the `MiniMap` embeds in the
 * Regions / Military pages. Pure presentation: all geometry comes from
 * the deterministic `WorldMapSnapshot` DTO (Voronoi cells + country
 * boundary blobs built in `state/src/ui/snapshot_map.rs`).
 */
import { useMemo, useState } from "react";
import type {
  CountryMapDto,
  MapEdgeDto,
  MapFeatureDto,
  MapRegionDto,
  WorldMapSnapshot,
} from "../../types/api";

// ============================================================================
// Color plumbing
// ============================================================================

/** Deterministic political palette keyed by `CountryMapDto.color_index`. */
const COUNTRY_PALETTE = [
  "#5b8ec4", // steel blue
  "#c46a5b", // terracotta
  "#6fae7c", // sage green
  "#b98bc9", // mauve
  "#d4a94e", // ochre
  "#5bb8b0", // teal
  "#c98bb0", // rose
  "#8d9bc4", // periwinkle
  "#a2c46a", // olive
  "#c4875b", // clay
  "#7fa3c4", // slate blue
  "#bc6ac4", // orchid
  "#64a894", // sea green
  "#c4b25b", // mustard
  "#9a7fc4", // lavender
  "#c47a7f", // dusty rose
];

export function countryColor(index: number): string {
  return COUNTRY_PALETTE[index % COUNTRY_PALETTE.length];
}

export type MapMode =
  | "political"
  | "governance"
  | "military"
  | "gdp"
  | "population"
  | "megaregions"
  | "infrastructure"
  | "migration"
  | "party"
  | "biome";

/** v4.27.0: Topography & Biome palette — fixed hues per biome class so the
 * Climate map reads identically across countries. */
export const BIOME_COLORS: Record<string, string> = {
  Mountains: "#8b8fa3",
  Highlands: "#b3a284",
  Steppe: "#c9b458",
  Plains: "#8fb35f",
  Forest: "#2f7d4a",
  Wetlands: "#4e9e8f",
  Coast: "#5f9ec4",
};

/** v4.27.0: categorical faction palette for the Party map mode — keyed by
 * a deterministic name hash so the same faction keeps its color. */
const PARTY_PALETTE = [
  "#c94f4f", "#4f7dc9", "#4fc98a", "#b04fc9", "#c9a44f", "#4fb8c9",
  "#c94f8f", "#8ac94f", "#7a5fd0", "#d0845f", "#5fd0a8", "#d0d05f",
];

export function partyColor(name: string): string {
  if (!name) return "#3d4653";
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) >>> 0;
  return PARTY_PALETTE[h % PARTY_PALETTE.length];
}

/** Diverging ramp for net migration: red (emigration) ↔ grey ↔ green. */
function migrationColor(net: number, maxAbs: number): string {
  if (maxAbs <= 0) return "#6b7280";
  const t = Math.max(-1, Math.min(1, net / maxAbs));
  const lerp = (a: number, b: number, f: number) => Math.round(a + (b - a) * f);
  if (t >= 0) {
    // grey → green
    const f = Math.min(1, t);
    return `rgb(${lerp(107, 34, f)},${lerp(114, 150, f)},${lerp(128, 84, f)})`;
  }
  const f = Math.min(1, -t);
  return `rgb(${lerp(107, 196, f)},${lerp(114, 78, f)},${lerp(128, 78, f)})`;
}

/** Categorical palette for the Megaregions mode — high-contrast, distinct
 * from the country palette so administrative blocs read at a glance. */
const MEGAREGION_PALETTE = [
  "#e6194b", "#3cb44b", "#4363d8", "#f58231", "#911eb4", "#46f0f0",
  "#f032e6", "#bcf60c", "#008080", "#9a6324", "#800000", "#000075",
];

export function megaregionColor(index: number): string {
  return MEGAREGION_PALETTE[Math.abs(index) % MEGAREGION_PALETTE.length];
}

/** Sequential heat ramp (light → saturated) for [0,1] inputs. */
function heatColor(t: number, stops: [number, number, number][]): string {
  const c = Math.max(0, Math.min(1, t));
  const n = stops.length - 1;
  const i = Math.min(n - 1, Math.floor(c * n));
  const f = c * n - i;
  const a = stops[i];
  const b = stops[i + 1];
  const mix = (x: number, y: number) => Math.round(x + (y - x) * f);
  return `rgb(${mix(a[0], b[0])},${mix(a[1], b[1])},${mix(a[2], b[2])})`;
}

const GDP_RAMP: [number, number, number][] = [
  [222, 226, 235], [154, 180, 214], [84, 130, 184], [36, 82, 138], [16, 48, 92],
];
const DEV_RAMP: [number, number, number][] = [
  [232, 226, 214], [206, 190, 140], [150, 168, 92], [84, 138, 74], [26, 92, 48],
];
const URBAN_RAMP: [number, number, number][] = [
  [235, 228, 240], [198, 172, 214], [152, 118, 190], [104, 70, 158], [62, 34, 112],
];
const DEVAST_RAMP: [number, number, number][] = [
  [120, 160, 110], [226, 210, 120], [238, 160, 70], [214, 88, 52], [148, 24, 24],
];

/** log1p normalization so GDP/pop distributions spread across the ramp. */
function logNorm(v: number, max: number): number {
  if (max <= 0) return 0;
  return Math.log1p(Math.max(0, v)) / Math.log1p(max);
}

export function regionFill(
  r: MapRegionDto,
  c: CountryMapDto,
  mode: MapMode,
  maxGdpPc: number,
  maxPop: number,
  maxNetMigration: number = 1,
): string {
  switch (mode) {
    case "political":
    case "infrastructure":
      return countryColor(c.color_index);
    case "governance":
      return heatColor(r.development_level, DEV_RAMP);
    case "military":
      // Devastation heat; battles/occupation get dedicated markers/hatch.
      return heatColor(r.devastation, DEVAST_RAMP);
    case "gdp":
      return heatColor(logNorm(r.gdp_per_capita, maxGdpPc), GDP_RAMP);
    case "population":
      // Blend population magnitude and urbanization share.
      return heatColor(
        0.5 * logNorm(Number(r.population), maxPop) + 0.5 * r.urbanization,
        URBAN_RAMP,
      );
    case "megaregions":
      // Regions outside any megaregion keep a dimmed country tint.
      return Number(r.megaregion_index) < 0
        ? countryColor(c.color_index)
        : megaregionColor(Number(r.megaregion_index));
    case "migration":
      return migrationColor(Number(r.net_migration ?? 0), maxNetMigration);
    case "party":
      // Seat share modulates opacity via lightness — dominant faction hue.
      return r.dominant_party ? partyColor(r.dominant_party) : "#3d4653";
    case "biome":
      return BIOME_COLORS[r.biome] ?? "#8fb35f";
  }
}

// ============================================================================
// Component
// ============================================================================

interface WorldMapSvgProps {
  snapshot: WorldMapSnapshot;
  mode: MapMode;
  /** Currently selected/highlighted region id. */
  selectedRegionId?: string | null;
  /** Restrict rendering to one country (mini-map embeds). */
  onlyCountry?: string | null;
  /** Dim other countries instead of hiding them (embed context). */
  highlightCountry?: string | null;
  onRegionClick?: (region: MapRegionDto, country: CountryMapDto) => void;
  /** Compact embed: no labels/markers beyond capitals, thinner strokes. */
  compact?: boolean;
  /** Show region name labels (full map only — still gated by density). */
  showLabels?: boolean;
  className?: string;
}

function polyPoints(r: MapRegionDto): string {
  return r.polygon.map((p) => `${p.x.toFixed(2)},${p.y.toFixed(2)}`).join(" ");
}

function boundaryPath(c: CountryMapDto): string {
  return (
    c.boundary.map((p, i) => `${i === 0 ? "M" : "L"}${p.x.toFixed(2)},${p.y.toFixed(2)}`).join(" ") + " Z"
  );
}

/** Five-point star path centered on (cx, cy). */
function starPath(cx: number, cy: number, rOuter: number): string {
  const rInner = rOuter * 0.42;
  let d = "";
  for (let i = 0; i < 10; i++) {
    const r = i % 2 === 0 ? rOuter : rInner;
    const a = (i * Math.PI) / 5 - Math.PI / 2;
    d += `${i === 0 ? "M" : "L"}${(cx + r * Math.cos(a)).toFixed(2)},${(cy + r * Math.sin(a)).toFixed(2)}`;
  }
  return d + " Z";
}

export function WorldMapSvg({
  snapshot,
  mode,
  selectedRegionId,
  onlyCountry,
  highlightCountry,
  onRegionClick,
  compact = false,
  showLabels = false,
  className,
}: WorldMapSvgProps) {
  const [hoveredId, setHoveredId] = useState<string | null>(null);

  const countries = useMemo(
    () =>
      onlyCountry
        ? snapshot.countries.filter((c) => c.country === onlyCountry)
        : snapshot.countries,
    [snapshot, onlyCountry],
  );

  const bounds = useMemo(() => {
    if (!onlyCountry) return snapshot.bounds;
    const c = snapshot.countries.find((x) => x.country === onlyCountry);
    if (!c) return snapshot.bounds;
    let [minX, minY, maxX, maxY] = [Infinity, Infinity, -Infinity, -Infinity];
    for (const p of c.boundary) {
      minX = Math.min(minX, p.x);
      minY = Math.min(minY, p.y);
      maxX = Math.max(maxX, p.x);
      maxY = Math.max(maxY, p.y);
    }
    return [minX, minY, maxX, maxY];
  }, [snapshot, onlyCountry]);

  const [maxGdpPc, maxPop] = useMemo(() => {
    let g = 0;
    let p = 0;
    for (const c of countries) {
      for (const r of c.regions) {
        g = Math.max(g, r.gdp_per_capita);
        p = Math.max(p, Number(r.population));
      }
    }
    return [g, p];
  }, [countries]);

  // v4.27.0: max |net migration| across visible regions — normalizes the
  // diverging Internal Migration ramp.
  const maxNetMigration = useMemo(() => {
    let m = 1;
    for (const c of countries)
      for (const r of c.regions)
        m = Math.max(m, Math.abs(Number(r.net_migration ?? 0)));
    return m;
  }, [countries]);

  // Region id → centroid for edge polylines and migration arrows.
  const centroidById = useMemo(() => {
    const m = new Map<string, { x: number; y: number }>();
    for (const c of snapshot.countries)
      for (const r of c.regions) m.set(r.id, { x: r.centroid.x, y: r.centroid.y });
    return m;
  }, [snapshot]);

  // Ids of regions inside the visible country set — used to scope
  // sea-lane edges and flow arrows to the rendered geometry.
  const visibleRegionIds = useMemo(() => {
    const s = new Set<string>();
    for (const c of countries) for (const r of c.regions) s.add(r.id);
    return s;
  }, [countries]);

  // v4.27.0: lakes / inland seas / rivers / mountain ridges — scoped to
  // the visible countries when `onlyCountry` crops the view.
  const visibleFeatures = useMemo(
    () =>
      snapshot.features.filter((f) => {
        if (!onlyCountry) return true;
        return f.country_tag.split("|").includes(onlyCountry);
      }),
    [snapshot, onlyCountry],
  );

  // v4.27.0: maritime borders — dashed straits/sea-lane links between
  // islands and the mainland (never land adjacency).
  const seaLaneEdges = useMemo(
    () =>
      snapshot.edges.filter(
        (e) =>
          e.edge_type === "sea_lane" &&
          visibleRegionIds.has(e.from_region) &&
          visibleRegionIds.has(e.to_region),
      ),
    [snapshot, visibleRegionIds],
  );

  const spanX = bounds[2] - bounds[0] || 1;
  const spanY = bounds[3] - bounds[1] || 1;
  const pad = Math.max(spanX, spanY) * 0.03;
  const viewBox = `${bounds[0] - pad} ${bounds[1] - pad} ${spanX + 2 * pad} ${spanY + 2 * pad}`;

  // Density heuristic: show text labels only when cells are large enough
  // relative to the viewport (~900px reference width).
  const regionCount = countries.reduce((s, c) => s + c.regions.length, 0);
  const labelsOn = showLabels && !compact && regionCount <= 140;

  const strokeW = compact ? 0.4 : 0.7;
  const labelSize = spanX / 60;
  const starR = Math.max(spanX, spanY) / (compact ? 220 : 160);

  return (
    <svg
      viewBox={viewBox}
      className={className}
      role="img"
      aria-label="Procedural world map"
      preserveAspectRatio="xMidYMid meet"
    >
      <defs>
        {/* Occupied-region diagonal hatch overlay (military mode). */}
        <pattern
          id="occ-hatch"
          width="4"
          height="4"
          patternUnits="userSpaceOnUse"
          patternTransform="rotate(45)"
        >
          <rect width="4" height="4" fill="transparent" />
          <line x1="0" y1="0" x2="0" y2="4" stroke="#1c1917" strokeWidth="1.4" opacity="0.75" />
        </pattern>
      </defs>
      {/* Sea backdrop */}
      <rect
        x={bounds[0] - pad}
        y={bounds[1] - pad}
        width={spanX + 2 * pad}
        height={spanY + 2 * pad}
        fill="#0f2739"
      />
      {/* v4.27.0: maritime borders — dashed straits linking islands to the
          mainland. Drawn beneath landmasses. */}
      {seaLaneEdges.map((e) => (
        <SeaLane key={`sl-${e.from_region}-${e.to_region}`} edge={e} centroids={centroidById} strokeW={strokeW} />
      ))}
      {countries.map((c) => {
        const dimmed = !!highlightCountry && c.country !== highlightCountry;
        const atWar = c.at_war_with.length > 0;
        return (
          <g key={c.country} opacity={dimmed ? 0.22 : 1}>
            {/* Country silhouette — darkens the sea gap inside the blob. */}
            <path
              d={boundaryPath(c)}
              fill={mode === "political" ? countryColor(c.color_index) : "#233244"}
              opacity={mode === "political" ? 0.35 : 0.6}
              stroke={atWar && mode === "military" ? "#ef4444" : "#0a1622"}
              strokeWidth={atWar && mode === "military" ? strokeW * 2.2 : strokeW}
            />
            {c.regions.map((r) => {
              const selected = r.id === selectedRegionId;
              const hovered = r.id === hoveredId;
              const fill = regionFill(r, c, mode, maxGdpPc, maxPop, maxNetMigration);
              return (
                <g key={r.id}>
                  {/* v4.27.0: true offshore islands get a sandy littoral rim
                      so they read as land surrounded by sea (linked by
                      sea-lane edges, never land adjacency). */}
                  {r.is_island && (
                    <polygon
                      points={polyPoints(r)}
                      fill="none"
                      stroke="#e7d8a8"
                      strokeWidth={strokeW * 2.4}
                      opacity={0.9}
                      pointerEvents="none"
                    />
                  )}
                  <polygon
                    points={polyPoints(r)}
                    fill={fill}
                    fillOpacity={
                      mode === "political" || mode === "infrastructure" ? 0.82 : 0.95
                    }
                    stroke={selected ? "#facc15" : hovered ? "#e2e8f0" : "#0a1622"}
                    strokeWidth={selected ? strokeW * 2.4 : hovered ? strokeW * 1.6 : strokeW}
                    style={{ cursor: onRegionClick ? "pointer" : "default" }}
                    onClick={onRegionClick ? () => onRegionClick(r, c) : undefined}
                    onMouseEnter={() => setHoveredId(r.id)}
                    onMouseLeave={() => setHoveredId((h) => (h === r.id ? null : h))}
                  >
                    <title>{`${r.name} — ${c.country}`}</title>
                  </polygon>
                  {/* Occupied / contested overlays (military mode). */}
                  {mode === "military" && r.control === "occupied" && (
                    <polygon points={polyPoints(r)} fill="url(#occ-hatch)" pointerEvents="none" />
                  )}
                  {mode === "military" && r.control === "contested" && (
                    <polygon
                      points={polyPoints(r)}
                      fill="none"
                      stroke="#f97316"
                      strokeWidth={strokeW * 2}
                      strokeDasharray="3 2"
                      pointerEvents="none"
                    />
                  )}
                  {mode === "military" && r.control === "rebel_control" && (
                    <polygon
                      points={polyPoints(r)}
                      fill="none"
                      stroke="#a855f7"
                      strokeWidth={strokeW * 2}
                      strokeDasharray="3 2"
                      pointerEvents="none"
                    />
                  )}
                  {/* Active-battle flash marker. */}
                  {mode === "military" && r.active_battle && (
                    <circle
                      cx={r.centroid.x}
                      cy={r.centroid.y}
                      r={starR * 1.15}
                      fill="#ef4444"
                      stroke="#fecaca"
                      strokeWidth={strokeW * 0.8}
                    >
                      <animate
                        attributeName="opacity"
                        values="1;0.25;1"
                        dur="1.4s"
                        repeatCount="indefinite"
                      />
                    </circle>
                  )}
                  {/* Capital-district star. */}
                  {r.is_capital_district && (
                    <path
                      d={starPath(r.centroid.x, r.centroid.y, starR)}
                      fill="#fde047"
                      stroke="#1c1917"
                      strokeWidth={strokeW * 0.7}
                      pointerEvents="none"
                    />
                  )}
                  {/* Region name — capital names always, others when the
                      cells are sparse enough to stay readable. */}
                  {!compact && (labelsOn || r.is_capital_district || hovered) && (
                    <text
                      x={r.centroid.x}
                      y={r.centroid.y + (r.is_capital_district ? starR * 2.2 : labelSize * 0.4)}
                      textAnchor="middle"
                      fontSize={r.is_capital_district ? labelSize * 1.05 : labelSize * 0.9}
                      fill="#f1f5f9"
                      stroke="#0a1622"
                      strokeWidth={labelSize * 0.14}
                      paintOrder="stroke"
                      pointerEvents="none"
                      fontFamily="inherit"
                    >
                      {r.name}
                    </text>
                  )}
                </g>
              );
            })}
            {/* v4.27.0: cell-local water features — lakes, inland seas,
                rivers clipped to this country's regions. */}
            {visibleFeatures
              .filter((f) => f.country_tag === c.country)
              .map((f, fi) => (
                <FeatureMark key={`feat-${c.country}-${fi}`} feature={f} strokeW={strokeW} />
              ))}
            {/* v4.27.0: Political & Infrastructure mode — road/rail
                arteries through polygon interiors + city dots scaled by
                urban population. */}
            {mode === "infrastructure" && (
              <g pointerEvents="none">
                {c.routes.map((rt, ri) => (
                  <polyline
                    key={`rt-${c.country}-${ri}`}
                    points={rt.points.map((p) => `${p.x.toFixed(2)},${p.y.toFixed(2)}`).join(" ")}
                    fill="none"
                    stroke={rt.kind === "rail" ? "#e8e8e8" : "#3b2f1e"}
                    strokeWidth={rt.kind === "rail" ? strokeW * 1.1 : strokeW * 0.9}
                    strokeDasharray={rt.kind === "rail" ? `${strokeW * 4} ${strokeW * 2}` : `${strokeW * 3} ${strokeW * 2}`}
                    opacity={0.85}
                  />
                ))}
                {c.cities.map((city) => {
                  const maxCity = Math.max(1, ...c.cities.map((x) => Number(x.population)));
                  const r = starR * (0.8 + 2.2 * Math.sqrt(Number(city.population) / maxCity));
                  return (
                    <g key={`city-${city.region_id}`}>
                      <circle
                        cx={city.point.x}
                        cy={city.point.y}
                        r={r}
                        fill={city.is_capital ? "#fde047" : "#f8fafc"}
                        stroke="#1c1917"
                        strokeWidth={strokeW * 0.8}
                      />
                      {!compact && (
                        <text
                          x={city.point.x}
                          y={city.point.y - r * 1.4}
                          textAnchor="middle"
                          fontSize={labelSize * 0.8}
                          fill="#f1f5f9"
                          stroke="#0a1622"
                          strokeWidth={labelSize * 0.12}
                          paintOrder="stroke"
                        >
                          {city.name}
                        </text>
                      )}
                    </g>
                  );
                })}
              </g>
            )}
            {/* v4.27.0: Internal Migration mode — directional arrows
                between regions, scaled and colored by flow intensity. */}
            {mode === "migration" &&
              c.flows.map((f, fi) => (
                <FlowArrow
                  key={`flow-${c.country}-${fi}`}
                  from={centroidById.get(f.from_region)}
                  to={centroidById.get(f.to_region)}
                  amount={Number(f.amount)}
                  strokeW={strokeW}
                />
              ))}
            {/* Country name at the capital anchor (full map only). */}
            {!compact && (
              <text
                x={c.anchor.x}
                y={c.anchor.y - starR * 4}
                textAnchor="middle"
                fontSize={labelSize * 1.5}
                fontWeight={700}
                fill="#ffffff"
                stroke="#0a1622"
                strokeWidth={labelSize * 0.18}
                paintOrder="stroke"
                pointerEvents="none"
                fontFamily="inherit"
                opacity={0.95}
              >
                {c.country}
              </text>
            )}
          </g>
        );
      })}
      {/* v4.27.0: border-spanning mountain ridges — drawn once over both
          neighbouring countries. */}
      {visibleFeatures
        .filter((f) => f.kind === "mountain" && f.country_tag.includes("|"))
        .map((f, fi) => (
          <FeatureMark key={`mtn-${fi}`} feature={f} strokeW={strokeW} />
        ))}
    </svg>
  );
}

// ============================================================================
// v4.27.0 overlay primitives
// ============================================================================

/** Dashed maritime border between two region centroids (island strait). */
function SeaLane({
  edge,
  centroids,
  strokeW,
}: {
  edge: MapEdgeDto;
  centroids: Map<string, { x: number; y: number }>;
  strokeW: number;
}) {
  const a = centroids.get(edge.from_region);
  const b = centroids.get(edge.to_region);
  if (!a || !b) return null;
  const midX = (a.x + b.x) / 2;
  const midY = (a.y + b.y) / 2;
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len = Math.hypot(dx, dy) || 1;
  // Bow the lane perpendicular to the direct chord so it reads as water.
  const bow = len * 0.12;
  const cx = midX - (dy / len) * bow;
  const cy = midY + (dx / len) * bow;
  return (
    <path
      d={`M${a.x.toFixed(2)},${a.y.toFixed(2)} Q${cx.toFixed(2)},${cy.toFixed(2)} ${b.x.toFixed(2)},${b.y.toFixed(2)}`}
      fill="none"
      stroke="#7dd3fc"
      strokeWidth={strokeW}
      strokeDasharray={`${strokeW * 3} ${strokeW * 2.4}`}
      opacity={0.65}
      pointerEvents="none"
    />
  );
}

/** Lakes/seas as filled water polygons; rivers as strokes; ridges brown. */
function FeatureMark({ feature, strokeW }: { feature: MapFeatureDto; strokeW: number }) {
  const pts = feature.points;
  if (pts.length < 2) return null;
  if (feature.kind === "lake" || feature.kind === "inland_sea") {
    return (
      <polygon
        points={pts.map((p) => `${p.x.toFixed(2)},${p.y.toFixed(2)}`).join(" ")}
        fill={feature.kind === "inland_sea" ? "#0b3a5c" : "#1d6fa8"}
        stroke="#0a1622"
        strokeWidth={strokeW * 0.6}
        pointerEvents="none"
      />
    );
  }
  if (feature.kind === "river") {
    return (
      <polyline
        points={pts.map((p) => `${p.x.toFixed(2)},${p.y.toFixed(2)}`).join(" ")}
        fill="none"
        stroke="#4ea3d8"
        strokeWidth={strokeW * 1.4}
        strokeLinecap="round"
        opacity={0.9}
        pointerEvents="none"
      />
    );
  }
  // mountain ridge
  return (
    <polyline
      points={pts.map((p) => `${p.x.toFixed(2)},${p.y.toFixed(2)}`).join(" ")}
      fill="none"
      stroke="#7c6a4f"
      strokeWidth={strokeW * 1.6}
      strokeLinecap="round"
      opacity={0.9}
      pointerEvents="none"
    />
  );
}

/** Directional SVG arrow scaled and colored by migrant-flow intensity. */
function FlowArrow({
  from,
  to,
  amount,
  strokeW,
}: {
  from?: { x: number; y: number };
  to?: { x: number; y: number };
  amount: number;
  strokeW: number;
}) {
  if (!from || !to || amount <= 0) return null;
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const len = Math.hypot(dx, dy);
  if (len < 1e-6) return null;
  // Shorten the shaft so the head doesn't pierce the target centroid.
  const ux = dx / len;
  const uy = dy / len;
  const endX = to.x - ux * len * 0.12;
  const endY = to.y - uy * len * 0.12;
  const w = strokeW * (1 + Math.min(3, Math.log10(amount + 1)));
  const headLen = w * 3.2;
  const hx1 = endX - ux * headLen - uy * headLen * 0.5;
  const hy1 = endY - uy * headLen + ux * headLen * 0.5;
  const hx2 = endX - ux * headLen + uy * headLen * 0.5;
  const hy2 = endY - uy * headLen - ux * headLen * 0.5;
  const color = amount >= 1000 ? "#22c55e" : amount >= 200 ? "#84cc16" : "#a3a380";
  return (
    <g pointerEvents="none">
      <line x1={from.x} y1={from.y} x2={endX} y2={endY} stroke={color} strokeWidth={w} opacity={0.85} />
      <polygon
        points={`${endX},${endY} ${hx1},${hy1} ${hx2},${hy2}`}
        fill={color}
        opacity={0.95}
      />
    </g>
  );
}

/**
 * v4.24.0 WS5 — Shared SVG renderer for the procedural world map.
 *
 * Used full-size by `MapPage` and compact by the `MiniMap` embeds in the
 * Regions / Military pages. Pure presentation: all geometry comes from
 * the deterministic `WorldMapSnapshot` DTO (Voronoi cells + country
 * boundary blobs built in `state/src/ui/snapshot_map.rs`).
 */
import { useMemo, useState } from "react";
import type { CountryMapDto, MapRegionDto, WorldMapSnapshot } from "../../types/api";

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

export type MapMode = "political" | "governance" | "military" | "gdp" | "population";

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
): string {
  switch (mode) {
    case "political":
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
              const fill = regionFill(r, c, mode, maxGdpPc, maxPop);
              return (
                <g key={r.id}>
                  <polygon
                    points={polyPoints(r)}
                    fill={fill}
                    fillOpacity={mode === "political" ? 0.82 : 0.95}
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
    </svg>
  );
}

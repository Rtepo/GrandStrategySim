/**
 * v4.24.0 WS5 — Compact world-map embed for the Regions / Military pages.
 *
 * Non-interactive thumbnail that navigates to the full Map tab on click.
 * `onlyCountry` scopes the viewBox to one country (Regions embed);
 * omit it for the whole world with `highlightCountry` dimming (Military).
 *
 * v4.30.0 (TASK-021): `interactive` mode drops the navigate-button wrap
 * and enables the full pan/zoom viewport + region & water click handlers
 * so the Regions country map can be zoomed into small regions and every
 * lake polygon stays clickable.
 */
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { getWorldMap } from "../../hooks/useTauriCommand";
import { WorldMapSvg, type MapMode } from "./WorldMapSvg";
import type { CountryMapDto, FrontlineDto, MapRegionDto, WaterBodyDto } from "../../types/api";

interface MiniMapProps {
  /** Scope the render to one country (Regions page embed). */
  onlyCountry?: string | null;
  /** Dim all but this country (context highlighting). */
  highlightCountry?: string | null;
  /** Choropleth mode — defaults to political for Regions, military for Military. */
  mode?: MapMode;
  /** Explicit height class, e.g. "h-48". */
  heightClass?: string;
  title?: string;
  /** v4.30.0: pan/zoom + click handlers instead of navigate-on-click. */
  interactive?: boolean;
  onRegionClick?: (region: MapRegionDto, country: CountryMapDto) => void;
  onWaterClick?: (water: WaterBodyDto) => void;
  /** v4.30.0 (TASK-024): battle-marker deep-link handler. */
  onBattleClick?: (regionId: string) => void;
  /** v4.32.0 (TASK-035): frontline-arc inspector handler — forwarded to
   * WorldMapSvg so embedded maps get the same clickable frontlines as
   * the full-screen Map page. */
  onFrontlineClick?: (frontline: FrontlineDto) => void;
  /** Show region/city labels while zoomed (interactive only). */
  showLabels?: boolean;
}

export function MiniMap({
  onlyCountry,
  highlightCountry,
  mode = "political",
  heightClass = "h-56",
  title = "World map — click to open",
  interactive = false,
  onRegionClick,
  onWaterClick,
  onBattleClick,
  onFrontlineClick,
  showLabels = false,
}: MiniMapProps) {
  const navigate = useNavigate();
  const { data: snapshot } = useQuery({
    queryKey: ["world-map"],
    queryFn: getWorldMap,
    refetchInterval: 5000,
  });

  if (!snapshot || snapshot.countries.length === 0) {
    return (
      <div className={`${heightClass} rounded-md bg-[#0f2739] flex items-center justify-center`}>
        <span className="text-xs text-slate-400">Map unavailable</span>
      </div>
    );
  }

  if (interactive) {
    return (
      <div
        className={`relative block w-full ${heightClass} rounded-md overflow-hidden border border-border`}
        title="Drag to pan, scroll to zoom, double-click to reset"
      >
        <WorldMapSvg
          snapshot={snapshot}
          mode={mode}
          onlyCountry={onlyCountry}
          highlightCountry={highlightCountry}
          onRegionClick={onRegionClick}
          onWaterClick={onWaterClick}
          onBattleClick={onBattleClick}
          onFrontlineClick={onFrontlineClick}
          showLabels={showLabels}
          className="w-full h-full block"
        />
        <button
          type="button"
          onClick={() => navigate("/map")}
          title={title}
          className="absolute bottom-1 right-2 text-[10px] text-slate-300/80 bg-black/40 rounded px-1.5 py-0.5 hover:bg-black/60"
        >
          Open Map →
        </button>
      </div>
    );
  }

  return (
    <button
      type="button"
      onClick={() => navigate("/map")}
      title={title}
      className={`relative block w-full ${heightClass} rounded-md overflow-hidden border border-border hover:ring-2 hover:ring-primary transition-shadow cursor-pointer`}
    >
      <WorldMapSvg
        snapshot={snapshot}
        mode={mode}
        onlyCountry={onlyCountry}
        highlightCountry={highlightCountry}
        compact
        className="w-full h-full block"
      />
      <span className="absolute bottom-1 right-2 text-[10px] text-slate-300/80 bg-black/40 rounded px-1.5 py-0.5 pointer-events-none">
        Open Map →
      </span>
    </button>
  );
}

/** v4.30.0 (TASK-021): embedded Before/After border mini-map for
    concluded wars — REAL region polygons of both belligerents, ceded
    polygons highlighted crimson (before) / gold (after). Replaces the
    v4.29.0 placeholder rectangles.
    v4.32.0 (TASK-035): moved from MilitaryPage into the shared map
    components — neighbouring states render in muted grey for context,
    owner matching is case-insensitive, and the transferred regions are
    cross-hatched in the victor's color for the AFTER panel. */
export function CessionMiniMap({
  ceded,
  from,
  to,
  regionName,
  regionShapes,
}: {
  ceded: string[];
  from: string;
  to: string;
  regionName: (id: string) => string;
  /** region id → {points, current owner} from the world-map DTO. */
  regionShapes: Map<string, { points: { x: number; y: number }[]; owner: string }>;
}) {
  const cededSet = new Set(ceded);
  // v4.32.0 (TASK-035): draw ALL countries' polygons — neighbouring
  // states render in muted grey so the continental context reads, while
  // the two belligerents get their faction colors. Owner matching is
  // case-insensitive so war-log names map to map-DTO owners.
  const fromL = from.toLowerCase();
  const toL = to.toLowerCase();
  const ids = [...regionShapes.keys()];
  const shapes = ids
    .map((id) => ({ id, ...regionShapes.get(id)! }))
    .filter((s) => s.points.length >= 3);
  if (shapes.length === 0) {
    return (
      <div className="text-xs text-muted-foreground">
        Territory Transferred: {ceded.map(regionName).join(", ")}
      </div>
    );
  }
  let [minX, minY, maxX, maxY] = [Infinity, Infinity, -Infinity, -Infinity];
  for (const s of shapes)
    for (const p of s.points) {
      minX = Math.min(minX, p.x);
      minY = Math.min(minY, p.y);
      maxX = Math.max(maxX, p.x);
      maxY = Math.max(maxY, p.y);
    }
  const spanX = maxX - minX || 1;
  const spanY = maxY - minY || 1;

  const Panel = ({
    label,
    cededOwner,
    cededFill,
  }: {
    label: string;
    cededOwner: string;
    cededFill: string;
  }) => (
    <div className="flex-1">
      <div className="text-muted-foreground uppercase text-[10px] mb-0.5">{label}</div>
      <svg
        viewBox={`${minX - 4} ${minY - 4} ${spanX + 8} ${spanY + 8}`}
        className="rounded border border-border/60 bg-[#0f1d2b] w-full"
        style={{ aspectRatio: `${spanX + 8} / ${spanY + 8}`, maxHeight: 130 }}
      >
        {/* v4.31.0 (TASK-029): clean anti-aliased render — 1px non-scaling
            strokes (kills the jagged MS-Paint look) + a subtle hatched
            glow overlay on the transferred border regions. */}
        <defs>
          <pattern
            id={`hatch-${label}`}
            width="6"
            height="6"
            patternUnits="userSpaceOnUse"
            patternTransform="rotate(45)"
          >
            <rect width="6" height="6" fill="transparent" />
            <line x1="0" y1="0" x2="0" y2="6" stroke={cededFill} strokeWidth="1" opacity="0.55" />
          </pattern>
        </defs>
        {shapes.map((s) => {
          const isCeded = cededSet.has(s.id);
          const owner = isCeded ? cededOwner : s.owner;
          const ownerL = s.owner.toLowerCase();
          const fill = isCeded
            ? `url(#hatch-${label})`
            : ownerL === fromL
              ? "#8f1d1d"
              : ownerL === toL
                ? "#3f7fd4"
                : "#334155";
          return (
            <g key={`${label}-${s.id}`}>
              <polygon
                points={s.points.map((p) => `${p.x.toFixed(2)},${p.y.toFixed(2)}`).join(" ")}
                fill={isCeded ? `${cededFill}55` : fill}
                stroke={isCeded ? "#fde047" : "#0a1622"}
                strokeWidth={1}
                vectorEffect="non-scaling-stroke"
                shapeRendering="geometricPrecision"
              >
                <title>{`${regionName(s.id)} — ${owner}${isCeded ? " (ceded)" : ""}`}</title>
              </polygon>
              {isCeded && (
                <polygon
                  points={s.points.map((p) => `${p.x.toFixed(2)},${p.y.toFixed(2)}`).join(" ")}
                  fill={`url(#hatch-${label})`}
                  stroke="none"
                  pointerEvents="none"
                />
              )}
            </g>
          );
        })}
      </svg>
    </div>
  );
  return (
    <div className="space-y-1">
      <div className="flex gap-3 items-start">
        <Panel label={`BEFORE — ${from}`} cededOwner={from} cededFill="#dc2626" />
        <div className="self-center text-muted-foreground">→</div>
        <Panel label={`AFTER — ${to}`} cededOwner={to} cededFill="#facc15" />
      </div>
      <div>
        <span className="text-[10px] font-semibold text-amber-400 uppercase tracking-wide">
          Territory Transferred:
        </span>{" "}
        {ceded.map((r) => regionName(r)).join(", ")}
      </div>
    </div>
  );
}

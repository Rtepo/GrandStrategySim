/**
 * v4.28.0 (TASK-012) — Shared cadastral parcel grid.
 *
 * v4.31.0 (TASK-028): parcels are now true 2D Voronoi cells (jittered
 * seed lattice + Lloyd relaxation) clipped to the region boundary —
 * compact aspect ratios replace the old barcode strips; ~60% interior
 * parcels ringed by ~40% border-zone parcels.
 *
 * Extracted from `RegionsPage` so both the Regions drill-down and the
 * Land → Climate tab render the same 36–60 parcel lattice. Three modes:
 *  - `zoning`  — zoning class (industrial orange, residential violet, …)
 *  - `party`   — precinct-level dominant faction (varied wards)
 *  - `terrain` — topography (mountains/forest/wetlands/water/…)
 */
import { useState } from "react";
import { Badge, Button } from "../ui";
import { fmt } from "../../lib/format";
import { partyColor } from "../../lib/partyVisuals";
import { councilFactionColor } from "../../lib/palette";
import type { MapParcelDto, RegionParcelMapDto } from "../../types/api";

export type ParcelMode = "zoning" | "party" | "terrain";
/** v4.29.0 (TASK-018): Party Support sub-toggle — national parliamentary
    parties vs regional council factions. */
export type PartyView = "national" | "regional";

export const PARCEL_USE_COLORS: Record<string, string> = {
  agricultural: "#4d9e3f",
  // v4.28.0: industrial = orange/amber per MPZP convention; residential =
  // violet so it can never be confused with water's cyan/blue.
  industrial: "#f97316",
  residential: "#8b5cf6",
  commercial: "#d4a02e",
  mixed: "#9a6ec4",
  protected_natural: "#1e7d64",
  strategic_military: "#5a5f6e",
  resource_mines: "#6e4a8f",
  devastated: "#8f1d1d",
  unplanned: "#3d4757",
  // v4.27.0: real topographic water parcels (river / lake / sea frontage).
  water: "#0284c7",
};

export const PARCEL_USE_LABELS: Record<string, string> = {
  agricultural: "Agricultural",
  industrial: "Industrial",
  residential: "Residential",
  commercial: "Commercial",
  mixed: "Mixed Use",
  protected_natural: "Protected Natural",
  strategic_military: "Strategic Military",
  resource_mines: "Resource Mines",
  devastated: "Damaged",
  unplanned: "Unplanned",
  water: "Water (River / Lake)",
};

const PARCEL_TERRAIN_COLORS: Record<string, string> = {
  mountains: "#78716c",
  highlands: "#a8a29e",
  forest: "#166534",
  plains: "#84cc16",
  steppe: "#d6b656",
  farmland: "#eab308",
  wetlands: "#2dd4bf",
  coast: "#38bdf8",
  river: "#0284c7",
  lake: "#0369a1",
};

const PARCEL_TERRAIN_LABELS: Record<string, string> = {
  mountains: "Mountains",
  highlands: "Highlands",
  forest: "Forest",
  plains: "Plains",
  steppe: "Steppe",
  farmland: "Farmland",
  wetlands: "Wetlands",
  coast: "Coast",
  river: "River",
  lake: "Lake",
};

function parcelFill(p: MapParcelDto, mode: ParcelMode, partyView: PartyView = "regional"): string {
  switch (mode) {
    case "party":
      // v4.30.0 (TASK-022): regional wards use the canonical council
      // palette — no more Moderates≡Optimates purple collision.
      return (partyView === "national" ? p.national_party : p.dominant_party)
        ? (partyView === "national"
            ? partyColor(null, p.national_party)
            : councilFactionColor(p.dominant_party))
        : "#3d4757";
    case "terrain":
      return PARCEL_TERRAIN_COLORS[p.terrain] ?? "#3d4757";
    default:
      return PARCEL_USE_COLORS[p.use_class] ?? PARCEL_USE_COLORS.unplanned;
  }
}

function parcelLabel(p: MapParcelDto, mode: ParcelMode, partyView: PartyView = "regional"): string {
  switch (mode) {
    case "party": {
      const name = partyView === "national" ? p.national_party : p.dominant_party;
      const share = partyView === "national" ? p.national_share : p.party_share;
      return name ? `${name} — ${(share * 100).toFixed(0)}%` : "No council";
    }
    case "terrain":
      return PARCEL_TERRAIN_LABELS[p.terrain] ?? p.terrain;
    default:
      return PARCEL_USE_LABELS[p.use_class] ?? p.zoning;
  }
}

const MODE_LABELS: { value: ParcelMode; label: string }[] = [
  { value: "zoning", label: "Zoning" },
  { value: "party", label: "Party Support" },
  { value: "terrain", label: "Topography" },
];

export function RegionParcelGrid({
  parcelMap,
  selectedParcel,
  onSelectParcel,
  onBackToCountry,
  initialMode = "zoning",
  showModeSwitcher = true,
}: {
  parcelMap: RegionParcelMapDto;
  selectedParcel: MapParcelDto | null;
  onSelectParcel: (p: MapParcelDto | null) => void;
  onBackToCountry?: () => void;
  initialMode?: ParcelMode;
  showModeSwitcher?: boolean;
}) {
  const [mode, setMode] = useState<ParcelMode>(initialMode);
  const [partyView, setPartyView] = useState<PartyView>("regional");
  const [minX, minY, maxX, maxY] = parcelMap.bounds;
  const w = Math.max(1e-6, maxX - minX);
  const h = Math.max(1e-6, maxY - minY);
  const toX = (x: number) => ((x - minX) / w) * 1000;
  const toY = (y: number) => ((y - minY) / h) * 560;

  // Legend entries = the classes actually present under the active mode.
  const present = new Map<string, string>();
  for (const p of parcelMap.parcels) {
    const key =
      mode === "party"
        ? (partyView === "national" ? p.national_party : p.dominant_party) || "No council"
        : mode === "terrain"
          ? p.terrain
          : p.use_class;
    if (!present.has(key)) {
      present.set(key, parcelLabel(p, mode, partyView));
    }
  }
  const legendColor = (key: string) =>
    mode === "party"
      ? key === "No council"
        ? "#3d4757"
        : partyView === "national"
          ? partyColor(null, key)
          : councilFactionColor(key)
      : mode === "terrain"
        ? (PARCEL_TERRAIN_COLORS[key] ?? "#3d4757")
        : (PARCEL_USE_COLORS[key] ?? PARCEL_USE_COLORS.unplanned);

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <div className="text-sm font-semibold text-foreground">
          Cadastral Parcels — {parcelMap.region_name}
          <span className="ml-2 text-xs text-muted-foreground">
            {parcelMap.parcels.length} parcels
          </span>
        </div>
        <div className="flex items-center gap-1">
          {showModeSwitcher &&
            MODE_LABELS.map((m) => (
              <Button
                key={m.value}
                variant={mode === m.value ? "default" : "outline"}
                size="sm"
                onClick={() => setMode(m.value)}
              >
                {m.label}
              </Button>
            ))}
          {onBackToCountry && (
            <Button variant="outline" size="sm" onClick={onBackToCountry}>
              ← Back to Country Map
            </Button>
          )}
        </div>
      </div>
      {/* v4.29.0 (TASK-018): Party Support sub-toggle — switch the ward
          coloring between national parliamentary parties and regional
          council factions. */}
      {mode === "party" && (
        <div className="flex items-center gap-1 text-xs">
          <span className="text-muted-foreground mr-1">Wards:</span>
          {(
            [
              { v: "national" as PartyView, label: "National Parties" },
              { v: "regional" as PartyView, label: "Regional Council Factions" },
            ]
          ).map((o) => (
            <Button
              key={o.v}
              variant={partyView === o.v ? "default" : "outline"}
              size="sm"
              onClick={() => setPartyView(o.v)}
            >
              {o.label}
            </Button>
          ))}
        </div>
      )}
      <svg
        viewBox={`0 0 1000 560`}
        className="w-full h-64 rounded bg-[#0f1d2b] border border-border"
      >
        {/* v4.30.0 (TASK-022): the organic region contour the parcels are
            clipped inside — the map no longer renders as a rectangle. */}
        {parcelMap.region_boundary?.length > 0 && (
          <polygon
            points={parcelMap.region_boundary.map((v) => `${toX(v.x)},${toY(v.y)}`).join(" ")}
            fill="#16283a"
            stroke="#3b4f66"
            strokeWidth={2}
          />
        )}
        {parcelMap.parcels.map((p) => {
          const pts = p.polygon.map((v) => `${toX(v.x)},${toY(v.y)}`).join(" ");
          const selected = selectedParcel?.id === p.id;
          return (
            <polygon
              key={p.id}
              points={pts}
              fill={parcelFill(p, mode, partyView)}
              fillOpacity={selected ? 1 : 0.8}
              stroke={selected ? "#ffffff" : "#0a1622"}
              strokeWidth={selected ? 2 : 0.5}
              className="cursor-pointer hover:opacity-90"
              onClick={() => onSelectParcel(selected ? null : p)}
            >
              <title>{`${p.display_name ?? p.id} — ${parcelLabel(p, mode, partyView)} — ${fmt(p.size_hectares)} ha — ${p.owner_type}`}</title>
            </polygon>
          );
        })}
      </svg>
      <div className="flex flex-wrap gap-x-4 gap-y-1">
        {[...present.entries()].map(([cls, label]) => (
          <span key={cls} className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <span
              className="inline-block w-3 h-3 rounded-sm border border-border"
              style={{ backgroundColor: legendColor(cls) }}
            />
            {label}
          </span>
        ))}
      </div>
      {selectedParcel && (
        <div className="grid grid-cols-4 gap-2 text-xs border border-border rounded p-2">
          <div><span className="text-muted-foreground">Parcel:</span> {selectedParcel.display_name ?? selectedParcel.id}</div>
          <div><span className="text-muted-foreground">Zoning:</span> {PARCEL_USE_LABELS[selectedParcel.use_class] ?? selectedParcel.zoning}</div>
          <div><span className="text-muted-foreground">Terrain:</span> {PARCEL_TERRAIN_LABELS[selectedParcel.terrain] ?? selectedParcel.terrain}</div>
          {selectedParcel.dominant_party && (
            <div><span className="text-muted-foreground">Precinct:</span> {selectedParcel.dominant_party} ({(selectedParcel.party_share * 100).toFixed(0)}%)</div>
          )}
          <div><span className="text-muted-foreground">Owner:</span> {selectedParcel.owner_type}</div>
          <div><span className="text-muted-foreground">Area:</span> {fmt(selectedParcel.size_hectares)} ha</div>
          <div><span className="text-muted-foreground">Devastation:</span> {(selectedParcel.devastation * 100).toFixed(0)}%</div>
          <div><span className="text-muted-foreground">Survey:</span> {(selectedParcel.legal_certainty * 100).toFixed(0)}%</div>
          <div><span className="text-muted-foreground">Infra access:</span> {(selectedParcel.infrastructure_access * 100).toFixed(0)}%</div>
          {selectedParcel.water_access && selectedParcel.water_access !== "none" && (
            <div><span className="text-muted-foreground">Water:</span> {selectedParcel.water_access}</div>
          )}
          {selectedParcel.is_frozen && <Badge variant="destructive">Frozen</Badge>}
          {selectedParcel.is_border_zone ? (
            <Badge variant="outline">Border Zone</Badge>
          ) : (
            <Badge variant="outline">Interior</Badge>
          )}
        </div>
      )}
    </div>
  );
}

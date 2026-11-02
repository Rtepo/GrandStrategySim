/**
 * v4.28.0 (TASK-012) — Shared cadastral parcel grid.
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
import type { MapParcelDto, RegionParcelMapDto } from "../../types/api";

export type ParcelMode = "zoning" | "party" | "terrain";

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
  wetlands: "Wetlands",
  coast: "Coast",
  river: "River",
  lake: "Lake",
};

function parcelFill(p: MapParcelDto, mode: ParcelMode): string {
  switch (mode) {
    case "party":
      return p.dominant_party ? partyColor(null, p.dominant_party) : "#3d4757";
    case "terrain":
      return PARCEL_TERRAIN_COLORS[p.terrain] ?? "#3d4757";
    default:
      return PARCEL_USE_COLORS[p.use_class] ?? PARCEL_USE_COLORS.unplanned;
  }
}

function parcelLabel(p: MapParcelDto, mode: ParcelMode): string {
  switch (mode) {
    case "party":
      return p.dominant_party
        ? `${p.dominant_party} — ${(p.party_share * 100).toFixed(0)}%`
        : "No council";
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
        ? p.dominant_party || "No council"
        : mode === "terrain"
          ? p.terrain
          : p.use_class;
    if (!present.has(key)) {
      present.set(key, parcelLabel(p, mode));
    }
  }
  const legendColor = (key: string) =>
    mode === "party"
      ? key === "No council"
        ? "#3d4757"
        : partyColor(null, key)
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
      <svg
        viewBox={`0 0 1000 560`}
        className="w-full h-64 rounded bg-[#0f1d2b] border border-border"
      >
        {parcelMap.parcels.map((p) => {
          const pts = p.polygon.map((v) => `${toX(v.x)},${toY(v.y)}`).join(" ");
          const selected = selectedParcel?.id === p.id;
          return (
            <polygon
              key={p.id}
              points={pts}
              fill={parcelFill(p, mode)}
              fillOpacity={selected ? 1 : 0.8}
              stroke={selected ? "#ffffff" : "#0a1622"}
              strokeWidth={selected ? 2 : 0.5}
              className="cursor-pointer hover:opacity-90"
              onClick={() => onSelectParcel(selected ? null : p)}
            >
              <title>{`${parcelLabel(p, mode)} — ${fmt(p.size_hectares)} ha — ${p.owner_type}`}</title>
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
          <div><span className="text-muted-foreground">Parcel:</span> {selectedParcel.id}</div>
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
          {selectedParcel.is_border_zone && <Badge variant="outline">Border Zone</Badge>}
        </div>
      )}
    </div>
  );
}

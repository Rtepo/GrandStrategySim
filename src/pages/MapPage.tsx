/**
 * v4.24.0 WS5 — Interactive procedural world map.
 *
 * Renders the deterministic `WorldMapSnapshot` (Voronoi region cells +
 * country boundary blobs) as an SVG with five choropleth modes, a
 * click-through region detail card, and zoom-on-demand labels.
 */
import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import {
  Card,
  CardHeader,
  CardTitle,
  CardContent,
  Badge,
  Button,
} from "../components/ui";
import { useGameStore } from "../store/gameStore";
import { getWorldMap } from "../hooks/useTauriCommand";
import { fmt, num } from "../lib/format";
import {
  WorldMapSvg,
  countryColor,
  type MapMode,
} from "../components/map/WorldMapSvg";
import type { CountryMapDto, MapRegionDto } from "../types/api";
import { ZoomIn, ZoomOut, Maximize2 } from "lucide-react";

const MODES: { value: MapMode; label: string }[] = [
  { value: "political", label: "Political" },
  { value: "governance", label: "Governance" },
  { value: "military", label: "Military Fronts" },
  { value: "gdp", label: "GDP per Capita" },
  { value: "population", label: "Population / Urbanization" },
];

const ZOOM_STEPS = [1, 1.5, 2.2, 3.2, 4.5];

export function MapPage() {
  const navigate = useNavigate();
  const { setSelectedCountry } = useGameStore();
  const [mode, setMode] = useState<MapMode>("political");
  const [zoomIdx, setZoomIdx] = useState(0);
  const [selected, setSelected] = useState<{
    region: MapRegionDto;
    country: CountryMapDto;
  } | null>(null);

  const { data: snapshot, isLoading } = useQuery({
    queryKey: ["world-map"],
    queryFn: getWorldMap,
    refetchInterval: 5000,
  });

  const zoom = ZOOM_STEPS[zoomIdx];
  const zoomIn = () => setZoomIdx((i) => Math.min(ZOOM_STEPS.length - 1, i + 1));
  const zoomOut = () => setZoomIdx((i) => Math.max(0, i - 1));

  const openRegionInRegionsPage = (region: MapRegionDto, country: CountryMapDto) => {
    setSelectedCountry(country.country);
    navigate("/regions", { state: { regionId: region.id } });
  };

  const legend = useMemo(() => {
    switch (mode) {
      case "political":
        return snapshot?.countries.map((c) => ({
          color: countryColor(c.color_index),
          label: c.country,
        })) ?? [];
      case "governance":
        return [
          { color: "#e8e2d6", label: "Underdeveloped" },
          { color: "#96a85c", label: "Developing" },
          { color: "#1a5c30", label: "Developed" },
        ];
      case "military":
        return [
          { color: "#78a06e", label: "Pristine" },
          { color: "#ee9f46", label: "Damaged" },
          { color: "#941818", label: "Devastated" },
          { color: "#ef4444", label: "Active battle" },
        ];
      case "gdp":
        return [
          { color: "#dee2eb", label: "Low GDP/cap" },
          { color: "#5482b8", label: "Mid" },
          { color: "#10305c", label: "High" },
        ];
      case "population":
        return [
          { color: "#ebe4f0", label: "Sparse / rural" },
          { color: "#9876be", label: "Mixed" },
          { color: "#3e2270", label: "Dense / urban" },
        ];
    }
  }, [mode, snapshot]);

  if (isLoading) {
    return <div className="p-6 text-muted-foreground">Loading world map...</div>;
  }
  if (!snapshot || snapshot.countries.length === 0) {
    return <div className="p-6 text-muted-foreground">No map data available.</div>;
  }

  return (
    <div className="p-6 space-y-4">
      <div className="flex items-center justify-between flex-wrap gap-2">
        <h2 className="text-xl font-bold text-foreground">
          World Map <span className="text-sm font-normal text-muted-foreground">— turn {snapshot.turn}</span>
        </h2>
        <div className="flex items-center gap-1">
          {MODES.map((m) => (
            <button
              key={m.value}
              onClick={() => setMode(m.value)}
              className={`px-3 py-1.5 rounded text-xs font-medium transition-colors ${
                mode === m.value
                  ? "bg-primary text-primary-foreground"
                  : "bg-muted text-muted-foreground hover:bg-accent hover:text-accent-foreground"
              }`}
            >
              {m.label}
            </button>
          ))}
          <div className="flex items-center gap-0.5 ml-2 border-l border-border pl-2">
            <Button variant="outline" size="sm" onClick={zoomOut} disabled={zoomIdx === 0} title="Zoom out">
              <ZoomOut size={14} />
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setZoomIdx(0)}
              disabled={zoomIdx === 0}
              title="Reset zoom"
            >
              <Maximize2 size={14} />
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={zoomIn}
              disabled={zoomIdx === ZOOM_STEPS.length - 1}
              title="Zoom in"
            >
              <ZoomIn size={14} />
            </Button>
          </div>
        </div>
      </div>

      <div className="grid grid-cols-4 gap-4">
        <Card className="col-span-3">
          <CardContent className="p-0">
            <div className="overflow-auto max-h-[78vh] rounded-md bg-[#0f2739]">
              <div style={{ width: `${zoom * 100}%`, minWidth: "100%" }}>
                <WorldMapSvg
                  snapshot={snapshot}
                  mode={mode}
                  selectedRegionId={selected?.region.id ?? null}
                  onRegionClick={(region, country) => setSelected({ region, country })}
                  showLabels={zoom > 1.4}
                  className="w-full block"
                />
              </div>
            </div>
            <div className="flex flex-wrap gap-x-4 gap-y-1 px-3 py-2 border-t border-border">
              {legend.map((l) => (
                <span key={l.label} className="flex items-center gap-1.5 text-xs text-muted-foreground">
                  <span
                    className="inline-block w-3 h-3 rounded-sm border border-border"
                    style={{ backgroundColor: l.color }}
                  />
                  {l.label}
                </span>
              ))}
            </div>
          </CardContent>
        </Card>

        <div className="col-span-1">
          {selected ? (
            <RegionCard
              region={selected.region}
              country={selected.country}
              onOpenRegions={() => openRegionInRegionsPage(selected.region, selected.country)}
              onOpenMilitary={() => navigate("/military")}
              onClose={() => setSelected(null)}
            />
          ) : (
            <Card>
              <CardContent className="p-6 text-center text-muted-foreground text-sm">
                Click a region to inspect it. Zoom in to reveal all region names.
              </CardContent>
            </Card>
          )}
        </div>
      </div>
    </div>
  );
}

// ============================================================================
// Region detail card
// ============================================================================

function Field({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between gap-2">
      <span className="text-muted-foreground">{label}</span>
      <span className="text-foreground font-medium text-right">{value}</span>
    </div>
  );
}

function RegionCard({
  region,
  country,
  onOpenRegions,
  onOpenMilitary,
  onClose,
}: {
  region: MapRegionDto;
  country: CountryMapDto;
  onOpenRegions: () => void;
  onOpenMilitary: () => void;
  onClose: () => void;
}) {
  const warBadge =
    region.control === "occupied" ? (
      <Badge variant="destructive">Occupied{region.controller ? ` by ${region.controller}` : ""}</Badge>
    ) : region.control === "contested" ? (
      <Badge variant="destructive">Contested</Badge>
    ) : region.control === "rebel_control" ? (
      <Badge variant="destructive">Rebel Control</Badge>
    ) : null;

  return (
    <Card>
      <CardHeader>
        <div className="flex items-start justify-between gap-2">
          <div>
            <CardTitle className="text-base">{region.name}</CardTitle>
            <p className="text-xs text-muted-foreground mt-0.5">
              {country.country}
              {region.is_capital_district ? " · Capital District" : ""}
              {region.is_city_region ? " · City Region" : ""}
            </p>
          </div>
          <button
            onClick={onClose}
            className="text-muted-foreground hover:text-foreground text-lg leading-none"
            aria-label="Close"
          >
            ×
          </button>
        </div>
      </CardHeader>
      <CardContent className="space-y-3 text-sm">
        <div className="flex flex-wrap gap-1">
          {warBadge}
          {region.active_battle && <Badge variant="destructive">Active Battle</Badge>}
          {region.on_front && !warBadge && <Badge variant="outline">Front Line</Badge>}
          {region.is_coastal && <Badge variant="outline">Coastal</Badge>}
          {country.at_war_with.length > 0 && (
            <Badge variant="destructive">At War: {country.at_war_with.join(", ")}</Badge>
          )}
        </div>
        <div className="space-y-1.5">
          <Field label="Population" value={num(region.population)} />
          <Field label="Regional GDP" value={fmt(region.gdp)} />
          <Field label="GDP per Capita" value={fmt(region.gdp_per_capita)} />
          <Field label="Urbanization" value={`${(region.urbanization * 100).toFixed(1)}%`} />
          <Field label="Development Level" value={`${(region.development_level * 100).toFixed(1)}%`} />
          <Field
            label="Devastation"
            value={`${(region.devastation * 100).toFixed(1)}%`}
          />
        </div>
        {region.neighbors.length > 0 && (
          <div>
            <div className="text-xs text-muted-foreground uppercase mb-1">Adjacent Regions</div>
            <div className="text-xs text-foreground/80 leading-relaxed">
              {region.neighbors.join(", ")}
            </div>
          </div>
        )}
        <div className="flex gap-2 pt-1">
          <Button variant="outline" size="sm" className="flex-1" onClick={onOpenRegions}>
            Open in Regions
          </Button>
          <Button variant="outline" size="sm" className="flex-1" onClick={onOpenMilitary}>
            Military View
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

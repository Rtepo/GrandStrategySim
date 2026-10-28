/**
 * v4.24.0 WS5 — Compact world-map embed for the Regions / Military pages.
 *
 * Non-interactive thumbnail that navigates to the full Map tab on click.
 * `onlyCountry` scopes the viewBox to one country (Regions embed);
 * omit it for the whole world with `highlightCountry` dimming (Military).
 */
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { getWorldMap } from "../../hooks/useTauriCommand";
import { WorldMapSvg, type MapMode } from "./WorldMapSvg";

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
}

export function MiniMap({
  onlyCountry,
  highlightCountry,
  mode = "political",
  heightClass = "h-56",
  title = "World map — click to open",
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

# TASK-005 — Map geometry: true adjacency, islands, inland lakes, map modes, parcels, climate

## Defect
v4.26 left zero-area "pseudo-island" dot regions floating off Gallia/Lechia
that falsely border all land regions; `adjacent_regions` contains duplicates
("Central Marches, Central Marches") and disagrees with the rendered map;
the 4-color Megaregions toggle sits broken on the World Map; the Regions
view has no cities/routes/migration/party map modes; the parcel grid is too
coarse and lacks water/industrial/commercial zoning; Land & Cadastre →
Climate has no topography/biome map.

## Required behaviour
1. **Zero pseudo-islands**: degenerate sliver regions removed; real offshore
   island/archipelago polygons generated with non-trivial area, reachable
   only via sea zones/straits (maritime adjacency — never land adjacency).
   `adjacent_regions` recomputed strictly from shared Voronoi polygon edges,
   deduplicated + symmetric, and the text list must 100% match rendered
   neighbours.
2. **Fractal coastlines + inland water**: polygon edge subdivision
   (>= 40–60 vertices/region, multi-octave noise); inland lakes and inland
   seas carved as internal water polygons.
3. **Regions map-mode switcher** (top of RegionsPage country map) with 4
   modes — the broken Megaregions toggle is REMOVED from the World Map:
   - Political & Infrastructure: cities (dots scaled by urban pop) and
     primary road/rail arteries routed through polygon interiors.
   - Megaregions: distinct palette colour per megaregion.
   - Internal Migration: net-migration choropleth + directional SVG arrows
     scaled/coloured by flow intensity.
   - Party / Faction Support: regions (and parcels at region zoom) coloured
     by dominant party/faction share.
4. **Parcels + Climate**: 36–60 cadastral parcels per region incl. rivers/
   lakes and Industrial/Commercial zones matching actual buildings;
   interactive Topography & Biome map (Mountains, Highlands, Steppe, Plains,
   Forest, Wetlands, Coast) inside Land & Cadastre → Climate.

## Acceptance
- `verify_task.sh TASK-005` green.
- `zero_area_pseudo_islands_count == 0`; `adjacency_is_symmetric_and_deduplicated`.
- No maritime connection expressed as land adjacency.

## Files
- `state/src/society/geography.rs`, `state/src/ui/snapshot_map.rs`
- `src/pages/RegionsPage.tsx`, `src/pages/MapPage.tsx`, `src/pages/LandPage.tsx`,
  `src/components/map/WorldMapSvg.tsx`

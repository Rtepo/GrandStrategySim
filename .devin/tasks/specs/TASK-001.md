# TASK-001 — Voronoi topology, canonical half-edge borders, real names, Megaregions + legend

## Defect
World/Regions maps render radial "pizza-slice" wedges meeting at one interior
point. Per-region noise displaces each polygon's copy of a shared edge
independently → black fissures between neighbours. Raw ids
(`Anatolia-Region4`) leak into `ADJACENT REGIONS` labels.

## Required behaviour
1. **Multi-ring / Lloyd-relaxed Voronoi layout**: country layouts produce
   true interior (landlocked) AND coastal regions — cell partition from
   region centroids, 1–2 Lloyd relaxation passes; no central convergence
   point, no wedge topology.
2. **Canonical half-edge noise**: shared boundary A↔B is ONE polyline.
   Displace it once (keyed by the unordered region pair) and emit the same
   vertices in both polygons → zero gaps/fissures by construction. A Voronoi
   clip already guarantees shared vertices; noise must perturb the shared
   vertex, never per-polygon copies.
3. **Name resolution**: `ADJACENT REGIONS` and labels resolve ids → display
   names before render; `*-RegionN` never reaches the SVG.
4. **Megaregions map-mode button** on MapPage + **high-contrast multi-stop
   color scale with a legend bar**.

## Acceptance
- `verify_task.sh TASK-001` green (diff + UI grep gate incl. Megaregions
  button + legend).
- Shared-edge invariant: adjacent cells share vertex-identical boundaries
  (unit test or DTO-level equality check).
- `v4250_release_gate` borders/polygon-vertex assertions stay green.

## Files
- `state/src/society/geography.rs` — layout/Voronoi generation
- `state/src/ui/snapshot_map.rs` — polygon emission, name resolution
- `src/components/map/WorldMapSvg.tsx`, `src/pages/MapPage.tsx`

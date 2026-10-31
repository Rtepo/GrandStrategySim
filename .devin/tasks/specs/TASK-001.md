# TASK-001 — Voronoi polygon map, canonical edge noise, real region names

## Defect
World/Regions maps render radial "pizza-slice" wedges meeting at a single
interior point. Per-region noise is applied independently to each polygon's
copy of a shared border, so neighbouring regions diverge → black fissures
between every internal region. Raw region ids (`Anatolia-Region4`) leak into
labels. Backend already emits parcel data; UI parity is missing.

## Required behaviour
1. Voronoi / Lloyd-relaxed region layout: region seeds → Voronoi cells clipped
   to country extent → 1–2 Lloyd relaxation passes for organic shapes.
   Continental and coastal regions must both receive cells (sea-adjacent
   borders stay wet, not pinched).
2. Canonical half-edge noise: build a shared-edge map keyed by sorted region
   pair `(A,B)`; generate ONE noise displacement per edge vertex and apply the
   SAME displacement to both polygons. Zero cracks by construction.
3. Name resolution: labels resolve region ids to display names
   (`display_name`/localized) before render; raw `*-RegionN` strings must never
   reach the SVG.

## Acceptance
- `verify_task.sh TASK-001` green (diff + UI grep gate).
- No two adjacent polygons may compute their shared edge independently —
  prove with a unit test asserting vertex-identical shared edges, or a
  deterministic-equality check on the emitted DTO.
- Gate test `shared_land_borders_count` and `min_region_polygon_vertices`
  in `v4250_release_gate_test.rs` must stay green.

## Files
- `state/src/society/geography.rs` — layout/Voronoi generation
- `state/src/ui/snapshot_map.rs` — polygon emission, name resolution
- `src/components/map/WorldMapSvg.tsx` — polygon render + labels

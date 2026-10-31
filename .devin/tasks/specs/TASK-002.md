# TASK-002 — Regional cadastral parcel sub-map + Megaregions toggle

## Defect
Clicking a region shows data tables only; the cadastral parcel geometry the
backend already emits (`snapshot_map.rs` parcel polygons per region) is never
rendered. No Megaregions map mode exists.

## Required behaviour
1. In `RegionsPage.tsx`, selecting a region renders a parcel sub-map (SVG):
   per-parcel polygons from the region-map DTO, coloured by zoning
   (residential/industrial/agricultural/forest/devastated), with click/hover
   selection showing parcel details (owner, zone, devastation index).
2. Map-mode toggle on the regions map: Regions ↔ Megaregions — Megaregion
   mode colours groups by `megaregion_id` membership with a legend.
3. Reuse existing `WorldMapSvg` conventions (jittered lattice, MapPoint DTO);
   do not invent a second geometry pipeline — consume the parcel polygons the
   backend already ships.

## Acceptance
- `verify_task.sh TASK-002` green.
- Parcel polygons are individually selectable; empty-parcel regions degrade
  gracefully (message, not blank canvas).

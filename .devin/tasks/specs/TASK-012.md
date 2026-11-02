# TASK-012 — Contiguous Megaregions, Parcel-Level Industrial/Party/Climate Maps & Focused Migration Arrows

## Context
Megaregions include disconnected regions; cadastral parcels miss Industrial
zoning and Residential ≈ Water colors; party support paints whole regions flat;
biome map paints whole regions flat; migration arrows render all-at-once as an
unreadable web.

## Requirements
1. **Contiguous megaregions**: build via connected-component BFS/flood-fill on
   the region adjacency graph — every megaregion 100% contiguous.
2. **Cadastral parcel map fixes**: Industrial parcels (#f97316) generated in
   proportion to regional industrial buildings/MPZP; Residential color distinct
   from Water (purple/violet or warm terracotta vs cyan/blue).
3. **Parcel-level party map**: each parcel colored by local precinct
   polling/turnout (varied political neighborhoods).
4. **Parcel-level biome map**: Land → Climate shows the 60-parcel topography
   (Mountains/Highlands/Forest/Plains/Steppe/Wetlands/River-Lake/Coast).
5. **Focused migration arrows**: only render flow arrows for the
   selected/hovered region — curved, thickness-scaled inflow/outflow with
   migrant counts on hover.

## Rust assertions
- Megaregions are connected components (BFS over adjacency).
- Parcel DTOs include industrial parcels where buildings exist.
- Parcel DTOs carry party_share + biome fields.

## UI checks
- Parcel map modes: zoning / party / topography.
- Migration arrows only for selected region.

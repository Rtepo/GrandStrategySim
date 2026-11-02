# TASK-017 — Map Scale/LOD Labels, Clickable Lakes, Cultural Cities, Annexation Sync

1. **Zoom-adaptive labels**: expand SVG coordinate space; at low zoom
   show only clean centered country names; region + city names reveal
   progressively on zoom; centroid placement with collision avoidance.
2. **Clickable lakes & real topology**: inland lakes/inner seas clickable
   (name, km², mean depth, bordering regions); smoother ocean bounding
   ring; no capital stars / city dots inside lake polygons (centroid on
   land_polygon − lake_polygons); richer geography (supercontinent with
   peninsulas/straits/inland seas + secondary continent + real
   multi-island archipelagos); cultural city-name generator.
3. **Annexation desync**: peace-treaty cession must fully transfer
   `country_id` (taxation, population, companies, parliament seats,
   megaregion assignment, map color) — no orphaned Unassigned bucket.

# TASK-021 — Dynamic map ownership, pan/zoom, 100% clickable lakes, real cession mini-maps (v4.30.0)

- World Map + RegionsView filter/group region polygons by CURRENT `owner_country` every turn — conquered polygons move to the victor silhouette, lost regions never render as dead grey shapes; click details + country label centroid follow ownership.
- Auto-fit world map viewBox to continent bounding box (fills canvas, no scrollbars); zoom-adaptive label collision/scale; wrap the Regions country map in the same pan/zoom container.
- 100% of inland lakes/seas clickable (name, area, depth, bordering regions); smooth angular water polygons.
- Historical Wars: replace colored squares with real SVG region polygons (before/after side-by-side), transferred regions highlighted gold/crimson.
Gate: regions_map_polygons_count == owned_regions_count; clickable_lakes_count == total_lakes_count.

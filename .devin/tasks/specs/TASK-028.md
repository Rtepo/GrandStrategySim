# TASK-028 — Regions map grey neighbors + true 2D parcels (v4.31.0)

- Non-owned same-continent regions render as muted grey (#1e293b) land polygons preserving coastline; roads/cities clipped to owned regions (or muted on grey neighbors).
- Parcel grid: 2D Poisson-disc/jittered grid seeds inside region contour + Lloyd relaxation; aspect_ratio < 2.5; ~60% interior parcels / ~40% border-zone.
Gate: parcel_max_aspect_ratio < 3.5 && parcel_interior_non_border_share > 0.35.

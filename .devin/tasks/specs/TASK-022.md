# TASK-022 — Realistic parcel layout, true border zones, global palette (v4.30.0)

- Border Zone badge only on parcels touching the region's outer boundary (not interior ParcelId(28v1)-style false positives); clean names ("Parcel #28 — Highland Ward"); clip parcel lattice/Voronoi to the actual region polygon.
- Single shared PALETTE module across Parliament/Council/Parcel/Military: Optimates #2563eb, Moderates #f59e0b, Populares #10b981 — Moderates/Optimates no longer share purple.
- Unit-type badges [INF]/[ART]/[CAV]/[ENG]/[NAV] use identical colors in Battles, Tactical Phases, OOB.

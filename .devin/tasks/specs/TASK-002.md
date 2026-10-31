# TASK-002 — Cadastral parcel grid map + sector/courts/gold invariants

## Defect
Clicking a region shows tables only; parcel polygons emitted by
`snapshot_map.rs` are never rendered. No Megaregions map mode.

## Required behaviour
1. Selecting a region in `RegionsPage.tsx` swaps the top canvas to an
   **interactive Regional Cadastral Parcel Grid Map**: per-parcel polygons
   colored by zoning — Agricultural, Industrial, Residential, Commercial,
   Protected Natural, Resource Mines, Damaged — with click/hover parcel
   detail (owner, zone, devastation).
2. **"← Back to Country Map"** button returns to the region map.
3. Megaregions map-mode toggle (Regions ↔ Megaregions coloring + legend).
4. **Invariants to hold** (backend, already green — do not regress):
   LocalServices wage share < 0.45; Construction > 25k FTE;
   WasteManagement > 5k FTE; ArmamentsIndustry/PublicServices/
   ExportServices/Religion non-trivial workforces; arbitration exposure
   < 20% GDP; positive CB gold reserves.

## Acceptance
- `verify_task.sh TASK-002` green.
- Empty-parcel regions degrade gracefully (message, not blank canvas).
- `v4250_release_gate` sector/arbitration/gold assertions stay green.

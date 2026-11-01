# TASK-006 — International & internal migration tracking

## Defect
`Net Migration: 0.00` everywhere — pops never migrate between regions or
across countries, so rural→urban drift, wage-seeking flows and war/poverty
flight are invisible. Society view has no origins/destinations breakdown;
Regions view shows no per-region in/out/net flow.

## Required behaviour
1. **Live migration engine**: pops migrate each turn between regions of the
   same country (rural→urban, low-wage→high-wage pull) and between countries
   (push: war, poverty, persecution; pull: wage, stability, open borders).
   Net migration must be non-zero and both-signed across regions.
2. **SocietyPage**: Immigration Origins & Emigration Destinations breakdown
   table — exact source/destination countries + migrant counts.
3. **RegionsPage**: per-region In-Migration, Out-Migration, Net Migration
   and Top Source/Destination regions.

## Acceptance
- `verify_task.sh TASK-006` green.
- Snapshot exposes non-zero regional + international flow rows.

## Files
- `state/src/economy/labor/migration.rs`, `state/src/ui/snapshot_regions.rs`
  (or `snapshot_society.rs` for the country breakdown)
- `src/pages/SocietyPage.tsx`, `src/pages/RegionsPage.tsx`

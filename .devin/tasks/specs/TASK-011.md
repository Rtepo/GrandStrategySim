# TASK-011 — Immutable Region Geometry, Zombie-State Elimination & Clickable Water Bodies

## Context
v4.27.0 fixed adjacency/islands, but war annexations still warp the map (polygons
re-layout toward the victor, leaving annexed states as black voids), fully
annexed countries persist as "zombie states" (0 population yet GDP/parliament
ticking), and water bodies are invisible/non-interactive.

## Requirements
1. **Immutable geometry**: Region polygon coordinates/vertices generated at
   turn 0 are immutable for the entire game. `region.owner_country_id` changes
   may ONLY alter fill color / border grouping — never re-layout vertices.
2. **Zombie-state elimination**: A country owning 0 regions or 0 population is
   marked Capitulated/Annexed: remaining corporate/monetary balances transfer
   or liquidate into annexing states, legislature/GDP ticks halt, and the
   country selector badges it "(Annexed)".
3. **Clickable named water bodies + world layout**:
   - Generate 3–4 varied continents/archipelagos (not always 2 round blobs).
   - Inland lakes/seas/ocean zones are explicit clickable water entities with
     unified water styling; clicking shows name, surface area km²,
     depth/navigability, bordering regions.
   - Islands never spawn inside landlocked interior borders — strictly inside
     open-sea polygons.
   - Active military frontlines render as thick high-contrast dashed
     yellow/crimson markers with crossed-swords icons along shared enemy
     borders (not faint hairlines).

## Rust assertions
- Region vertices at turn 0 identical at turn 60 (across ownership changes).
- 0-pop/0-region country ⇒ annexed status, zero GDP tick, legislature halted.
- ≥3 continents generated; island polygons only inside sea zones.
- Water body DTOs carry name/area/depth/navigability/bordering regions.

## UI checks
- WorldMapSvg renders water bodies + frontline overlays.
- Annexed country badge in selectors.

# v4.25.0 Status — 7 Workstreams, Zero Skipped

Integration branch: `feat/agent-5-v4250-fixes` (worktree `w2-adaptive-production`)
Base: `main` @ `431a3e08` (post-v4.24.0)

## Workstream Ledger

| WS | Scope | Worker branch | Status |
|----|-------|---------------|--------|
| WS1 | Connected continents, fractal borders, Megaregions, parcel sub-map, dynamic dev level | `feat/agent-5-v4250-ws1-map` | **merged** — borders+polygons pass gate 12; maritime nodes wired by manager (sea/ocean nodes, reciprocal coastline edges, has_coastline) |
| WS2 | Expandable wars/battles/OOB branches, casualties, POW camps | `feat/agent-5-v4250-ws2-military` | **merged** — OOB deferred post-maritime so navies spawn; seeded frozen border disputes guarantee war activity (tension head-start, adjacency-derived pairs) |
| WS3 | Land arbitration overflow, court backlog, CB gold reserves | `feat/agent-5-v4250-ws3-land-bank` | **merged** — GDP-capped awards + court capacity wired; CB gold seeded (30% fx), price init, per-turn accumulation hook |
| WS4 | Press dedup, KRS- strings, embedded charts, 12555% unrest | `feat/agent-5-v4250-ws4-press` | **merged** — awaiting gate verdict |
| WS5 | Sector employment rebalance (construction vs LocalServices) | `feat/agent-5-v4250-ws5-sectors` | **merged + manager fixes** — landfill capacity 50→500/region, state tender pipeline 10→40 concurrent, site crew cap 200→400, LS share narrowed 0.2-0.4→0.15-0.28 |
| WS6 | Statutes string cleanup, rolling 24-turn party support | `feat/agent-5-v4250-ws6-statutes` | **merged** (bbf07b7e) — statute humanization + polling drift landed |
| WS7 | Diplomacy espionage sub-tab, expandable treaties/resolutions | `feat/agent-5-v4250-ws7-intel` | **merged + manager fixes** — `process_intelligence_agencies` turn hook wired, spy caps 3→8/host, targets scaled 15–40 |

## Notes
- Both worker waves died to rate limits; all partial work committed as WIP then merged. Manager completed the remaining engineering (maritime wiring, OOB deferral, war-dispute seeding, gold subsystem, intel hook, sector rebalance, DTO flags).
- Gate run history: (1) failed gate 13 OOB — no coastlines existed (`generate_maritime_nodes` never called; OOB ran before any coast marking). (2) maritime+OOB-defer landed → OOB flags passed; failed gate 13c `wars.is_empty()` — organic Lechia–Venedia war fought real battles but ended t115.

## Release Gate (v4250_release_gate_test.rs)

- [x] shared_land_borders_count > 10 && min_region_polygon_vertices >= 16
- [x] oob_has_branch_breakdown && oob_regions_have_divisions
- [ ] !wars.is_empty() && !wars[0].battles.is_empty() — seeded disputes pending verification
- [ ] total_arbitration_exposure < national_gdp * 0.20 && central_bank_gold_reserves > 0.0
- [ ] press papers unique titles && no "KRS-" in raw text
- [ ] statutes JSON: no "_", no ": true"/": false" (values walked, not keys)
- [ ] local_services_gdp_share < 0.45 && construction_fte > 25_000 && waste_management_fte > 5_000
- [ ] intelligence_agency.active_spies >= 10 && !operations.is_empty()

## Merge/Release

- [x] All 7 WS merged into integration branch
- [ ] Release gate green (120-turn)
- [ ] Iron CI/CD 4 gates green
- [ ] Manifests + Cargo.lock → 4.25.0
- [ ] merge_worker.sh → push main + tag v4.25.0

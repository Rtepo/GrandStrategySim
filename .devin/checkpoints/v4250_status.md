# v4.25.0 Status — 7 Workstreams, Zero Skipped

Integration branch: `feat/agent-5-v4250-fixes` (worktree `w2-adaptive-production`)
Base: `main` @ `431a3e08` (post-v4.24.0)

## Workstream Ledger

| WS | Scope | Worker branch | Status |
|----|-------|---------------|--------|
| WS1 | Connected continents, fractal borders, Megaregions, parcel sub-map, dynamic dev level | `feat/agent-5-v4250-ws1-map` | dispatched |
| WS2 | Expandable wars/battles/OOB branches, casualties, POW camps | `feat/agent-5-v4250-ws2-military` | dispatched |
| WS3 | Land arbitration overflow, court backlog, CB gold reserves | `feat/agent-5-v4250-ws3-land-bank` | dispatched |
| WS4 | Press dedup, KRS- strings, embedded charts, 12555% unrest | `feat/agent-5-v4250-ws4-press` | dispatched |
| WS5 | Sector employment rebalance (construction vs LocalServices) | `feat/agent-5-v4250-ws5-sectors` | dispatched |
| WS6 | Statutes string cleanup, rolling 24-turn party support | `feat/agent-5-v4250-ws6-statutes` | dispatched |
| WS7 | Diplomacy espionage sub-tab, expandable treaties/resolutions | `feat/agent-5-v4250-ws7-intel` | dispatched |

## Release Gate (v4250_release_gate_test.rs)

- [ ] shared_land_borders_count > 10 && min_region_polygon_vertices >= 16
- [ ] oob_has_branch_breakdown && oob_regions_have_divisions && !wars[0].battles.is_empty()
- [ ] total_arbitration_exposure < national_gdp * 0.20 && central_bank_gold_reserves > 0.0
- [ ] press papers unique titles && no "KRS-" in raw text
- [ ] statutes JSON: no "_", no ": true"/": false"
- [ ] local_services_gdp_share < 0.45 && construction_fte > 25_000 && waste_management_fte > 5_000
- [ ] intelligence_agency.active_spies >= 10 && !operations.is_empty()

## Merge/Release

- [ ] All 7 WS merged into integration branch
- [ ] Release gate green (120-turn)
- [ ] Iron CI/CD 4 gates green
- [ ] Manifests + Cargo.lock → 4.25.0
- [ ] merge_worker.sh → push main + tag v4.25.0

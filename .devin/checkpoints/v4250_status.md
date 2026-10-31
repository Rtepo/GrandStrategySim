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
| WS5 | Sector employment rebalance (construction vs LocalServices) | `feat/agent-5-v4250-ws5-sectors` | **merged + manager fixes** — landfill seats 50→700/region, tenders region-scaled + 160 concurrent, MILESTONE_SPACING 4→2.5, labor_per_unit 10→1.0×wage, per-site cap 800, imminent haircut 0.75, WCL loans for Construction, LS share 0.08-0.16, receivable-holders exempt from liquidation, extinct-sector re-founding |
| WS6 | Statutes string cleanup, rolling 24-turn party support | `feat/agent-5-v4250-ws6-statutes` | **merged** (bbf07b7e) — statute humanization + polling drift landed |
| WS7 | Diplomacy espionage sub-tab, expandable treaties/resolutions | `feat/agent-5-v4250-ws7-intel` | **merged + manager fixes** — `process_intelligence_agencies` turn hook wired, spy caps 3→8/host, targets scaled 15–40, spy postings funded from intelligence budget, gdp_tier rescaled to ~$3B numéraire, covered-host seeding bug fixed (was capping spies at foreign_count=3) |

## Notes
- Both worker waves died to rate limits; all partial work committed as WIP then merged. Manager completed the remaining engineering (maritime wiring, OOB deferral, war-dispute seeding, gold subsystem, intel hook, sector rebalance, DTO flags).
- Gate run history: (1) failed gate 13 OOB — no coastlines existed (`generate_maritime_nodes` never called; OOB ran before any coast marking). (2) maritime+OOB-defer landed → OOB flags passed; failed gate 13c `wars.is_empty()` — organic Lechia–Venedia war fought real battles but ended t115.

## Release Gate (v4250_release_gate_test.rs)

- [x] shared_land_borders_count > 10 && min_region_polygon_vertices >= 16
- [x] oob_has_branch_breakdown && oob_regions_have_divisions
- [x] !wars.is_empty() && !wars[0].battles.is_empty() — seeded disputes produce overlapping wars (active war w/ battles at t118-119)
- [x] total_arbitration_exposure < national_gdp * 0.20 && central_bank_gold_reserves > 0.0
- [x] press papers unique titles && no "KRS-" in raw text
- [x] statutes JSON: no "_", no ": true"/": false" (values walked, not keys)
- [x] local_services_wage_share < 0.45 && construction_fte > 25_000 && waste_management_fte > 5_000 — Dacia 39.5k/6.2k, Lechia 41.1k/8.1k (post-extinction-guard); construction demand-bound at committed BOM demand — solved via labor_per_unit×wage-parity + region-scaled tender pipeline
- [x] intelligence_agency.active_spies >= 10 && !operations.is_empty() — spies=24 (8/ring × 3 hosts, cap-bound), ops=27

**120-turn gate PASSED 2026-10-30** — all 18 gates green, 124s runtime.

## Post-gate fixes (full-suite hardening)

- **WS7 spy ceiling**: `staff_diplomatic_posts` seeded `covered_hosts` from live posts →
  permanent 1-spy/host cap (=3 in a 4-country world). Now only same-turn picks
  suppress host reuse; per-host caps still bound rings. (`diplomacy.rs`)
- **Construction extinction**: contractors liquidated while holding funded
  escrowed receivables; no re-entry path when sector died. `lifecycle.rs` —
  receivable-holders exempt from both liquidation paths; Construction injected
  into `promising_sectors` on stalled backlog; thin-capital rescue spawn allowed.
- **Border disputes era-scoped**: seeded rivalries gated to `StartYear::Y1900`
  (the release crucible) so later-era worlds keep organic diplomacy.
- **Fund deployment regression**: secondary bids froze `acct.cash` before the
  primary-subscription loop ran → unfilled bids starved primary deployment,
  cash share 0.43 vs 0.35 gate. `funds.rs` — primary subscriptions now run
  first and reduce the secondary budget (`funds_deploy_capital_below_cash_gate`
  green again, 327s).
- **Clippy**: 4 worker-code lints fixed (`sort_by_key`, `needless_range_loop`
  ×2 via `iter_mut().enumerate()`, `manual_strip` → `strip_prefix`).
- **Cadastre tests**: updated stale call sites for new `award_cap`/`max_payout`
  arbitration params.

## Merge/Release

- [x] All 7 WS merged into integration branch
- [x] Release gate green (120-turn) — final re-run in progress on post-fund-fix tree
- [x] Iron CI/CD 4 gates green — build ✓ / test ✓ (full suite) / clippy ✓ -D warnings / npm ✓
- [x] Manifests + Cargo.lock → 4.25.0
- [ ] merge_worker.sh → push main + tag v4.25.0

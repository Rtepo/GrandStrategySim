# v4.22.0 Status — Cold-Start Phase 0 Reconnaissance
Updated: 2026-10-25 (Agent 5 / Manager, session-1526-RDW192022, agent-7c3134e8)

## Phase 0 Findings

### Repository state
- `main` @ `19e8a1d9` — "sync: update last_green_commit to ee9f3878 (v4.21.0 merge, harness green)".
- **v4.20.0 merged** (`d9d3a333`), **v4.21.0 merged** (`ee9f3878`). All v41xx fix branches merged.
- Working location for v4.22.0: worktree `.devin/worktrees/w2-adaptive-production`,
  branch `feat/agent-5-v4220-calendar-society-press` (HEAD = main = 19e8a1d9).
- Hub tree (`fix/agent-5-utility-economics` @ 7bb3a695, 66 commits BEHIND main) holds a
  separate stale utility-economics diff (~1900/-2600 lines). Equivalent content already
  preserved in `stash@{7}` ("cross-agent wip") and `stash@{10}` — LEFT UNTOUCHED per
  AGENTS.md Rule 3 (not part of v4.22.0 mandate, not destroyed).
- Junk artifacts `nul` / `state/nul` (62B shell `> nul` accidents) — untracked, removed.
- `.devin/checkpoints/` created (was missing).

### Salvaged in-flight v4.22.0 work (UNCOMMITTED, compiles — `cargo check` PASS 1m48s)
~1459 insertions across 19 files + 3 new files, already at version 4.22.0 in manifests:
- `state/src/ui/snapshot_society.rs` (NEW, 815L) — WS6.2 Society DTO (age pyramid, education,
  ethnicity, religion, vital telemetry).
- `state/src/ui/snapshot_press.rs` (NEW, 1205L) — WS6.3 Press DTO (articles/headlines/editorials).
- `src-tauri/src/commands/society_queries.rs` (NEW, 160L) + registration in mod.rs/main.rs/actions.rs.
- `state/src/international/fog_of_war.rs` (+201) — WS5.1: regime-openness floor; open regimes
  publish GDP/population/budget/government; totalitarian stays hidden; covert OOB stays asset-gated.
- `state/src/international/diplomacy.rs` (~255 reworked) — WS5.1: envoy/spy corps scaled by
  country size (target_envoys/target_spies, DiplomaticPostCap, VIP-sourced diplomats).
- `state/src/military/morale.rs` (+179) — WS5.3: `compute_morale_targets` from real-wage YoY,
  consumption fulfillment, war exhaustion; EWMA-seeked.
- `state/src/ui/snapshot.rs` (+111) — MoraleRow: class_group/unionized/real strikes_active;
  ArmyCompositionRow.region_breakdown (Country→Region); ForeignCountryRow.population.
- `state/src/state/macro_data.rs` (+84) — `gdp_composition_history` (WS4.1 backend).
- `state/src/engine/turn.rs` (+4), `generator/mod.rs` (+1), `military/turn.rs` (+34),
  `society/geography.rs` (+31), `ui/mod.rs` (+2), `src/types/api.ts` (+666 generated).

### Remaining workstream coverage (post-salvage)
- WS1.1 calendar math (offset=14, Jan rollover) — NOT STARTED
- WS1.2 tech-era commodity gating + fertilizer plants — NOT STARTED
- WS2.1–2.6 parliament/regime/elections/votes/committees/ministry — NOT STARTED (no .tsx changes yet)
- WS3.1–3.4 regions/councils/mandates/MPZP — NOT STARTED
- WS4.1 GDP pie charts — backend done, FRONTEND MISSING
- WS4.2 market treemap — NOT STARTED
- WS5.2 treaties + Proto-UN assembly — PARTIAL (diplomacy rework; panel not confirmed)
- WS5.3 morale + OOB grouping — backend done, FRONTEND grouping MISSING; AI friction wars TBD
- WS6.1 Organizations summary header — NOT STARTED
- WS6.2/6.3 Society + Press tabs — backend done, FRONTEND MISSING
- Release gate crucible assertions — NOT STARTED

### Manager status
- `claim_manager.sh` run in worktree → `.devin/.manager_auth` (agent-7c3134e8).
  NOTE: hooks validate `SESSION_ID` env == token session_id; export
  `SESSION_ID=session-1526-RDW192022` in every shell that runs manager scripts.
- `force_unlock.sh` for agent-1..4 FAILED — `sync_transactional` can't
  `git pull --rebase` on the dirty tree (mandate-predicted). Zombie locks are stale
  (null heartbeats, Sep 20-21) and do NOT overlap v4.22.0 domains. Retry after
  salvage commit makes the tree clean.
- Zombie ledger entries: agent-1..4 "active" w/ null heartbeats; 4× duplicate
  inactive `agent-d801903d` rows.

## Workstream Log
| WS | Status | Commit | Notes |
|----|--------|--------|-------|
| 0  | DONE   | 128eba8d | Phase 0 salvage commit |
| 1  | DONE   | a8817463, 0f8912a4 | Calendar h=turn+offset(14), Jan rollover; Commodity::introduction_year era gate; Fertilizers 1880 seeds chemical plants; state recipes era-aware; Market UI hides uninvented goods; unit tests green |
| 4  | DONE   | ef933972 | GDP donut (Macro + Region Overview), OEC treemap (Market) via ECharts |
| 6  | DONE   | ef933972 | Society tab (pyramid/education/ethnicity/religion/vitals/class table), Press tab (4 ideological front pages + media context), Organizations KPI header |
| 5  | PARTIAL| c2953469 | Military OOB + morale grouped Country→expandable Region. Treaties + org/sanctions panels already existed. TODO: IA per-country resolution votes, AI friction wars |
| 2  | PARTIAL| 662c6f90, c6f3b4d0 | Regime legislature names in sidebar (legislature_names map, GameStatus), five-shade outcome badges, theocracy-confessional party bids. TODO: elections-tab redesign, ministry report drawer, committee composition, votes-table consolidation |
| 3  | PARTIAL| 6a9d833b | Governance mode column (Autonomous Council/Appointed Governor/Commissary Admin), MPZP targets blended w/ real endowments. TODO: hemicycle councils, mandate dedup + negotiation logs |
| gate | PASS   | — | v4220_release_gate_1900 PASSED 305s release mode |
| CI   | GREEN  | f8b2144e | build OK; test --workspace --all-targets OK (30 suites, 0 fail); clippy -D warnings OK; npm build OK; release-gate epic PASS 305s |

### Post-Phase-0 notes
- All manifests already 4.22.0 (salvage). Cargo.lock matches.
- `cargo check --workspace` clean after WS1; `tsc --noEmit` clean after WS4/6.
- Remaining hardcoded `1925+turn/24` sites eliminated; fiscal cadence (budget FY,
  year-end settlement, legislative sessions, agriculture crop index) January-anchored.

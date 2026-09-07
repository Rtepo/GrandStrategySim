# Standard Operating Procedure — SillyElaborateState

## Pre-Commit Checklist (MUST pass before request_integration.sh)

1. `cargo check --workspace` — exit 0
2. `cargo test --workspace --all-targets -- --skip headless_50_tick_smoke` — exit 0
3. `cargo clippy --workspace --all-targets -- -D warnings` — exit 0
4. `npm run build` — exit 0
5. `git status` — clean (all changes committed)

## Rule 13: Comprehensive Technological Matrices

Every new sector/building MUST define:
- >=3 Production method slots
- >=3 Automation method slots
- >=3 Organization method slots
- All input commodities MUST have at least one producing sector (no orphans)
- All output commodities MUST be consumed by at least one sector or demographic

## Commodity Graph Integrity

Before adding a new commodity:
1. Verify it is produced by at least one production method
2. Verify it is consumed by at least one production method or demographic
3. Update `phase76_commodity_graph_test` if the commodity count assertion exists
4. Update `energy_wave1_test` / `energy_wave2_test` commodity count assertions

## Supply Chain Integrity

Every sector MUST have:
- Organization progression methods (tested by `every_sector_has_organization_progression`)
- Non-empty production outputs (tested by `supply_chain_integrity_test`)
- No orphan inputs (tested by `no_orphan_inputs_phase76`)

## Git Hygiene

- Work ONLY in your assigned worktree
- NEVER commit to main directly
- NEVER use: git stash pop, git reset --hard, git push --force, git clean -fd
- Run `request_integration.sh` from your worktree (NOT the hub)
- If `request_integration.sh` fails the pre-integration guard, fix the error — do NOT bypass it

## Event Bus

- INTEGRATION_REQUESTED: Worker -> Manager (via request_integration.sh)
- PROMOTED_TO_MAIN: Manager -> All (after CI/CD passes)
- AUDIT_REQUESTED: Manager -> Agent 4 (after sprint complete)
- AUDIT_FAIL: Agent 4 -> Manager (structured JSON with failed_checks)
- AUDIT_FAIL_ADDENDUM: Agent 4 -> Manager (new_failed_checks schema)
- AUDIT_PASS: Agent 4 -> Manager (all checks pass)
- REMEDIATION_REQUESTED: Agent 4 -> Worker (v3: direct routing, bypassing manager)
- CLARIFICATION_REQUESTED: Manager -> Worker (CI/CD failure details)
- SYSTEM_ALERT: Manager/Agent 4 -> User (critical issues requiring human attention)

## v3: Shift-Left Pre-Integration (updated)

`request_integration.sh` v3 now performs BEFORE emitting the event:
1. Clean tree verification (`git status --porcelain` must be empty)
2. `git fetch origin main` + `git rebase origin/main` (or merge fallback)
3. `cargo check --workspace`
4. `cargo nextest run --test-threads=4` (or `cargo test` fallback) + `cargo test --doc`
5. `cargo clippy --workspace --all-targets -- -D warnings`
6. `npm run build`

All log captures use `tail -n 50` to prevent LLM context window bloat.

## v3: OOM Prevention

- All `cargo nextest` commands use `--test-threads=4` to cap concurrent test
  threads and prevent RAM spikes from the simulation engine.
- All log captures passed back to workers use `tail -n 50` to prevent
  LLM context window bloat and secondary OOM crashes.
- `sccache` is configured with `SCCACHE_CACHE_SIZE="2G"` to prevent
  unbounded disk growth.

## Crash Recovery Protocol (OOM)

If an agent terminal crashes due to Out-Of-Memory (OOM) or any other reason:

1. **Restart the agent's terminal.** No special flags or commands needed.
2. **Run one of the following to resume seamlessly:**
   - `bash .devin/scripts/console.sh $inbox <agent>` — read pending events
   - `git status` — check working tree state and current branch
3. **No manual manager routing is required.** The v3 architecture handles
   this automatically:
   - `auto_wake.sh` will re-detect any unprocessed events targeting the agent.
   - `route_failures.sh` (Agent 4) routes AUDIT_FAIL directly to workers.
   - The daemon's dedup guard prevents duplicate REMEDIATION_REQUESTED events.
4. **If the working tree is dirty after a crash:**
   - Inspect with `git status` and `git diff`.
   - Commit or discard changes as appropriate.
   - Re-run `request_integration.sh` (which will verify clean tree before rebase).
5. **If the daemon itself crashed:**
   - Restart with `bash .devin/scripts/launch_daemon.sh`.
   - The daemon resumes polling and processes any queued events.
   - No events are lost — they persist as JSON files in `.devin/events/`.

## v3: Auto-Wake Daemons

To start an auto-wake daemon for an agent:

```bash
# Agent 4 (Auditor) — auto-wakes on PROMOTED_TO_MAIN / AUDIT_REQUESTED
bash .devin/scripts/auto_wake.sh agent-4 "bash .devin/scripts/run_audit.sh"

# Worker agents — auto-wake on REMEDIATION_REQUESTED / CLARIFICATION_REQUESTED
bash .devin/scripts/auto_wake.sh agent-1 "bash .devin/scripts/console.sh \$inbox agent-1"
bash .devin/scripts/auto_wake.sh agent-2 "bash .devin/scripts/console.sh \$inbox agent-2"
bash .devin/scripts/auto_wake.sh agent-3 "bash .devin/scripts/console.sh \$inbox agent-3"
```

The daemon uses a seen-file (`.devin/.auto_wake_seen_<agent>.txt`) to prevent
re-triggering. Race condition guards protect against concurrent file archival
by the integration daemon.

## v3.1: Centralized OOM Recovery ($recover)

Use `$recover <agent>` to recover a crashed agent:
```bash
bash .devin/scripts/console.sh $recover agent-2
```
This inspects the worktree, cleans stale locks (index.lock, REBASE_HEAD,
MERGE_HEAD), and emits a RECOVERY_WAKE event for auto-wake reboot.
No manual manager routing required.

## v3.1: Daemon Lifecycle Automation

- `$release` automatically stops the daemon (end of sprint cycle).
- `$kickoff` automatically launches the daemon if not already running.

## v3.1: Aggressive Garbage Collection

The daemon runs `cleanup_stale_diagnostics()` every 50 cycles (~12.5 min),
deleting stale diagnostic files from the project root:
`build_out*.txt`, `build_err*.txt`, `clippy*.txt`, `*_test.txt`, `*_FAILED.txt`, etc.
Uses a single `find` command with concatenated `-name` patterns (`maxdepth 1` only).

## v3.1: Active RAM Throttling

- **Daemon:** Before `cargo nextest` execution, checks system RAM. If usage
  exceeds 85%, suspends the test run and waits 30s (max 10 retries = 5 min).
- **auto_wake.sh:** Before executing a wake command, checks RAM. If usage
  exceeds 85%, defers the wake (prints alert only).
- **bc-free:** Uses bash parameter expansion `${used_pct%.*}` for integer
  conversion instead of the `bc` command.

## v4: Data-Driven Testing Framework

### v4: CI Strict Mode (cargo-insta)

`export CI=true` is set before every `cargo nextest` invocation in both
`integration_daemon.sh` and `request_integration.sh`. This ensures
cargo-insta fails hard on snapshot mismatches instead of silently writing
`.snap.new` files or hanging on interactive prompts.

### v4: Snapshot Review Workflow

1. Run tests locally (without `CI=true`) to generate `.snap.new` files.
2. Run `bash .devin/scripts/console.sh $approve_snapshots` to review.
3. Accept/reject each snapshot interactively.
4. Commit approved `.snap` files: `git add state/tests/snapshots/ && git commit`.

### v4: Epic Test Segregation

Epic/diagnostic tests are in `state/tests/epics/` and gated by the
`epic-tests` Cargo feature via `[[test]]` blocks with `required-features`.
Fast CI does NOT compile epic tests (Cargo skips them entirely).
Epic CI runs with `--features epic-tests,diagnostic` (pre-merge or `$audit_standard`).

### v4: Diagnostic JSON Dumps

The diagnostic harness emits JSON dumps to `state/tests/diagnostic_output/`
for external Python auditor verification:
- `sector_ledger.json`, `market_clearing.json`, `banking_state.json`, `manifest.json`
- The `AUDIT_REQUESTED` event includes `diagnostic_output_path` so Agent 4
  can locate the files without reading Rust source code.

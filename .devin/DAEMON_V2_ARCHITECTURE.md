# Daemon v2 — Technical Audit & Fault-Tolerant Orchestration Blueprint

Audit of the v4.25.0 release pipeline that produced "18/18 green" while
silently dropping ~75% of the user's UI requirements. Root causes are
architectural, not editorial.

## Phase 1 — Failure-Mode Audit

### 1. Task Drop Rate (75% loss)
- **Ephemeral instruction channel.** Requirements lived only in chat context.
  `.devin/tasks/` holds legacy per-file JSON (`queued/`, `active/`,
  `in_progress/`) with `objective`/`scope`/`constraints` fields but **no
  status machine, no verification contract, no completion gate** — nothing
  enumerated the user's asks atomically, so nothing could measure what was
  dropped.
- **Manager recon → Worker execution dilution.** Multi-part prompts were
  paraphrased into workstreams; each hand-off compressed intent. There was no
  persisted spec file a worker could re-read, so "expandable wars, battles,
  OOB branches, casualties, POW camps" became "military DTO data."
- **Backend-only test gates (Full-Stack Goodharting).** The v4.25 release
  gate asserts on Rust-side DTO structs (`wars[0].battles`, `active_spies`,
  sector FTE). It passed while `MilitaryPage.tsx` rendered the same flat
  tables as v4.24 — the DTO fields existed and were *never rendered*. No CI
  stage ever opened a `.tsx` file.

### 2. OOM Crash Vectors
- **Transcript/log bloat.** `auto_wake.sh` already guards one path
  (payload `tail -50`) but nothing rotates daemon logs or the CLI session
  transcripts (`%APPDATA%/devin/cli`) — verbose tool output accumulates in
  JSONL session history and context windows until the session starves.
- **Linker RAM spikes.** `cargo test --workspace --all-targets` links 27+
  test binaries, each embedding full dependency debuginfo under MSVC.
  `.cargo/config.toml` already swaps `link.exe` → `rust-lld` (speed) but no
  profile cap exists — multi-GB link jobs on an already-loaded box → exit 137.
- **No transcript rotation** anywhere in `.devin/scripts/`.

### 3. Rate-Limit / Quota Halts
- `auto_wake.sh` spawns `devin -p --resume` for event-driven wakes; v4.3.2
  *removed* `--resume` (ACP conflict on IDE-active sessions). Nothing scans
  worker output for 429/rate_limit/quota — a quota halt exits the process and
  the task dies in place. There is no cooldown-then-relaunch loop anywhere.

### 4. Two-Hour Debug Test Bottleneck
- Root `Cargo.toml` carried **zero profile overrides** — `cargo test` ran
  opt-level 0 with full debuginfo across the entire suite, including the
  120-turn worldgen crucibles. Incremental linking was on but useless across
  full-workspace runs.
- **Redundant suite re-runs**: the merge harness re-ran the entire suite even
  for tree-identical merges (now partially mitigated by merge_worker
  GREEN-SKIP); there was no per-task fast gate — every verification ran the
  whole book.

## Phase 2 — Daemon Orchestration v2 (implemented)

| Pillar | Artifact | Mechanism |
|--------|----------|-----------|
| 1. Atomic ledger + full-stack gate | `.devin/tasks/backlog.json`, `task_ctl.sh`, `verify_task.sh`, `specs/TASK-*.md`, `BOOT_PROMPT.md` | Tasks are JSON atoms: `status` (PENDING→IN_PROGRESS→VERIFYING→DONE, BLOCKED), `required_rust_files`, `required_ui_files`, `ui_ast_grep_checks` (feature-specific regexes that must appear in the `.tsx`). `task_ctl.sh done` **is** `verify_task.sh` — backend-only work cannot reach DONE. |
| 2. OS watchdog | `daemon_supervisor.sh` | External process loop: rebuilds `context.md` (boot prompt + spec + live diff) before every launch; scans run-log tail for `429|rate_limit|quota|overloaded|RESOURCE_EXHAUSTED` → cooldown → relaunch; exit 137/OOM marker → WIP snapshot (patch + commit attempt) → artifact cleanup → log rotation → respawn; `> SUP_MAX_RESTARTS` → task BLOCKED. |
| 3. Test bottleneck | `Cargo.toml` `[profile.test]` + `[profile.dev.package."*"]` | `opt-level = 1`, `debug = "line-tables-only"`, incremental test links. Dependency debuginfo (the bulk of every test PDB) shrinks ~70%+; sim crucibles run at opt-level 1. Release profile untouched — gate reproducibility preserved. Tiered CI: `verify_task.sh` is the <60s per-task gate (diff + grep + `tsc --noEmit`); the full suite runs once at merge. |
| 4. Backlog seeding | `backlog.json` TASK-001..004 + specs | Dropped v4.25.0 UI work re-captured atomically with honest, feature-specific UI checks (expandable wars → `expandedWar`; POW camps → `POW|pow_camp`; parcels → `parcel`; intel tab → `Espionage|CovertOperation`). |

## Invariants
- **Zero-loss**: instructions live on disk (`BOOT_PROMPT.md` + spec + ledger);
  memory is never the source of truth. `context.md` is rebuilt per launch.
- **No blind DONE**: a task's `.tsx` files must carry diffs *and* grep hits.
- **Breathe, don't die**: rate-limit → timed cooldown; OOM → WIP + respawn;
  both bounded by `SUP_MAX_RESTARTS` before escalation to BLOCKED.
- **Conservative cleanup**: only `target/debug/incremental` is purged — never
  `git clean`, never `reset --hard` (AGENTS.md Rule 2 applies to daemons too).

## Rollout
1. `bash .devin/scripts/task_ctl.sh list` — see seeded TASK-001..004.
2. Manager assigns: `task_ctl.sh assign TASK-001 agent-1`.
3. Worker daemon launched under supervisor:
   `daemon_supervisor.sh TASK-001 agent-1 <worktree> --cmd "devin -p \"$(cat {CTX})\""`
4. Worker finishes → supervisor auto-runs `verify_task.sh` → DONE or back
   to IN_PROGRESS with a recorded FAIL in `verify_history`.

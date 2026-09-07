# Architecture v3.1: Crash Recovery, Lifecycle Automation, and Aggressive GC

Architecture v3.1 enhances v3 with centralized OOM crash recovery (`$recover`),
daemon lifecycle automation (stop on release, launch on kickoff), aggressive
garbage collection of stale diagnostic files, active RAM throttling at 85%
(bc-free integer comparison), and standardized `tail -n 50` context pruning.

## Features

1. **Centralized OOM Recovery** — `$recover <agent_id>` inspects worktree,
   cleans stale locks, emits `RECOVERY_WAKE` event for auto-wake reboot.
2. **Daemon Lifecycle Automation** — Stop on `$release`, auto-launch on
   `$kickoff`.
3. **Aggressive Garbage Collection** — Single `find` with concatenated
   `-name` patterns, every 50 cycles, `maxdepth 1` only.
4. **Active RAM Throttling** — 85% threshold, bc-free via `${used_pct%.*}`
   integer conversion, suspends nextest/wake under memory pressure.
5. **Context Pruning** — `tail -n 50` enforced on all event-bound log captures.

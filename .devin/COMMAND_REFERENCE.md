# Command Reference - SillyElaborateState Infrastructure v3.0

## Quick Start

All commands are run from the hub directory:

```bash
bash .devin/scripts/console.sh <command> [args]
```

## Commands

### $kickoff - Assign Blueprint to Worker

**When to use:** After Agent 4 (Architect) drafts a roadmap and gives you the agent + blueprint ID.

```bash
bash .devin/scripts/console.sh $kickoff agent-3 004-FIX
```

**What it does automatically:**
1. Reads `roadmap.json` to find the blueprint
2. Derives the branch name (e.g., `fix/agent-3-sports-recreation-impl`)
3. Updates `blueprint_agent_map.json` with the mapping
4. Appends the branch to `sprint_manifest.txt`
5. Copies the task brief to `.devin/tasks/active/`
6. Emits `TASK_ASSIGNED` event to the worker with SOP context

**You provide:** agent name + blueprint ID (from Agent 4)
**System handles:** everything else

---

### $audit_standard - Trigger Full Audit

**When to use:** When all sprint branches are merged and you want Agent 4 to run the comprehensive 23-rule audit.

```bash
bash .devin/scripts/console.sh $audit_standard
```

**What it does automatically:**
1. Builds the 23 Global Rules checklist
2. Emits `AUDIT_REQUESTED` event to Agent 4
3. Agent 4 outputs `AUDIT_FAIL` (structured JSON) or `AUDIT_PASS`

**You provide:** nothing
**System handles:** checklist generation, event emission

---

### $forward_fail - Route Audit Failures to Workers

**When to use:** After Agent 4 emits `AUDIT_FAIL` and you want to automatically route the failures to the responsible workers.

```bash
bash .devin/scripts/console.sh $forward_fail latest
```

**What it does automatically:**
1. Finds the most recent `AUDIT_FAIL` event in the queue
2. Parses the `blueprint_results` array
3. Looks up each failing blueprint in `blueprint_agent_map.json`
4. Emits `REMEDIATION_REQUESTED` to each responsible worker agent
5. Emits `SYSTEM_ALERT` to user with summary

**You provide:** `latest` (or a specific event ID)
**System handles:** event discovery, parsing, agent routing

---

### $unblock - Clear CI/CD Block

**When to use:** When a worker agent hits the 3-strike CI/CD failure limit and is blocked.

```bash
bash .devin/scripts/console.sh $unblock agent-3
```

**What it does automatically:**
1. Looks up the agent's branch from `blueprint_agent_map.json`
2. Falls back to `.cicd_failure_state.json` if no mapping found
3. Verifies the branch is actually blocked
4. Resets `consecutive_failures` to 0 and `blocked` to false
5. Emits `CLARIFICATION_REQUESTED` to wake the agent

**You provide:** agent name only
**System handles:** branch detection, unblock, agent notification

---

### $pulse - System Telemetry Dashboard

**When to use:** Quick health check of the daemon, sprint progress, and agent strike status.

```bash
bash .devin/scripts/console.sh $pulse
```

**What it does automatically:**
1. Reads `.devin/.integration_daemon.pid` and verifies the process is alive
2. Reads `sprint_manifest.txt` and shows each branch with PENDING/PROMOTED status
3. Parses `.cicd_failure_state.json` and shows strike count + block status per agent

**You provide:** nothing
**System handles:** all telemetry gathering

---

### $logs - Quick Failure Log Access

**When to use:** When an agent fails CI/CD and you want to see the exact error without hunting for log files.

```bash
bash .devin/scripts/console.sh $logs agent-3
```

**What it does automatically:**
1. Looks up the agent's branch from `blueprint_agent_map.json`
2. Sanitizes the branch name for log file matching
3. Finds the most recent log file (priority: FAILED > test > cicd_output)
4. Executes `tail -n 30` to show the last 30 lines of the failure

**You provide:** agent name only
**System handles:** branch lookup, log file discovery, tail output

---

### $override - Administrative Fast-Track Merge

**When to use:** When changes are trivial (docs, typos, config) and waiting 10 minutes for CI/CD is wasteful.

```bash
bash .devin/scripts/console.sh $override agent-3
```

**What it does automatically:**
1. Looks up the agent's branch from `blueprint_agent_map.json`
2. Verifies branch exists and working tree is clean
3. Checks out `main` and merges the branch
4. Aborts on merge conflicts (no force merge)
5. Resets failure state for the branch
6. Emits `PROMOTED_TO_MAIN` event with `method: override_bypass`

**WARNING:** This bypasses CI/CD. Use ONLY for trivial changes (docs, typos, config).
**You provide:** agent name only
**System handles:** branch lookup, merge, event emission, failure state reset

---

### $smoke_main - Run Smoke Test on Main

**When to use:** Before generating new roadmaps or after major merges to verify baseline macroeconomic stability.

```bash
bash .devin/scripts/console.sh $smoke_main
```

**What it does automatically:**
1. Switches to `main` branch (if not already)
2. Runs `cargo test --workspace --test headless_smoke_test -- headless_50_tick_smoke --nocapture`
3. Timeout: 600 seconds
4. Reports PASS / FAIL / TIMEOUT

**You provide:** nothing
**System handles:** branch checkout, test execution, result reporting

---

### $daemon - Restart Integration Daemon

**When to use:** When the daemon is hanging, stuck in a dirty-tree loop, or needs a fresh start after infrastructure updates.

```bash
bash .devin/scripts/console.sh $daemon
```

**What it does automatically:**
1. Runs `stop_daemon.sh` to gracefully stop the current daemon (SIGKILL if needed)
2. Clears any stale PID file
3. Runs `launch_daemon.sh` to spin up a fresh background process
4. Outputs the new daemon PID

**You provide:** nothing
**System handles:** stop, cleanup, launch, PID reporting

---

### $release - Version Bump, Tag, and Push

**When to use:** When preparing a new GitHub release.

```bash
bash .devin/scripts/console.sh $release 1.2.0
```

**What it does automatically:**
1. Verifies clean working tree and `main` branch
2. Bumps version in `Cargo.toml` (workspace root), `state/Cargo.toml`, `src-tauri/Cargo.toml`, `package.json`, `src-tauri/tauri.conf.json` (whichever exist)
3. Commits with `chore: bump version to v<version>`
4. Creates annotated git tag `v<version>`
5. Pushes commit and tag to `origin`

**You provide:** version string (e.g., `1.2.0` or `v1.2.0`)
**System handles:** version bump across all config files, commit, tag, push

---

### $inbox - Worker Self-Service Event Reader (Newest Wins)

**When to use:** When a worker agent needs to read their assigned tasks or remediation requests without waiting for a long prompt from the manager.

```bash
bash .devin/scripts/console.sh $inbox agent-3
```

**What it does automatically:**
1. Scans `.devin/events/` for the NEWEST JSON file where `target` matches the agent
2. Parses the event and prints a highly readable briefing:
   - Event Type, Event ID, From, To, Timestamp
   - Blueprint ID, Branch, Task Name, Verdict
   - Failed Checks (if any)
   - Reason, Action Required, Instructions
   - SOP context (if attached)
   - Constraints, Deliverables (if present)
3. Archives ALL pending events for the agent (flush inbox completely)
4. Emits a system directive auto-triggering the worker to start coding immediately
5. If inbox is empty, reports "No pending events"

**You provide:** agent name only
**System handles:** event discovery (newest), parsing, readable formatting, full inbox flush, auto-trigger directive

---

### $menu - Show Help Screen

**When to use:** When you forget the available commands or their syntax.

```bash
bash .devin/scripts/console.sh $menu
```

**What it does:** Displays all available commands with brief descriptions and examples.

---

## Workflow Diagram

```
Agent 4 drafts roadmap
  -> User runs: $kickoff agent-3 004-FIX
    -> System assigns branch, emits TASK_ASSIGNED
      -> Agent 3 implements, runs request_integration.sh
        -> Daemon runs CI/CD (6-stage pipeline)
          -> PASS: PROMOTED_TO_MAIN emitted
          -> FAIL: 3-strike block -> User runs: $unblock agent-3
            -> Agent 3 fixes, resubmits -> cycle repeats
          -> Trivial changes: $override agent-3 (bypass CI/CD)

All branches merged:
  -> User runs: $audit_standard
    -> Agent 4 audits against 23 Global Rules
      -> AUDIT_PASS: Sprint complete!
      -> AUDIT_FAIL: User runs: $forward_fail latest
        -> System routes REMEDIATION_REQUESTED to workers
          -> Workers fix, resubmit -> cycle repeats

Health checks:
  -> $pulse: daemon status + sprint progress + agent strikes
  -> $logs agent-3: quick failure log access
  -> $smoke_main: baseline stability verification
  -> $daemon: restart hanging daemon

Release:
  -> $release 1.2.0: bump, tag, push
```

## Event Flow

```
$kickoff      -> TASK_ASSIGNED (manager -> worker)
CI/CD pass    -> PROMOTED_TO_MAIN (daemon -> all)
$override     -> PROMOTED_TO_MAIN (manager -> all, method: override_bypass)
/audit        -> AUDIT_REQUESTED (manager -> agent-4)
audit fail    -> AUDIT_FAIL (agent-4 -> manager)
                v3: route_failures.sh -> REMEDIATION_REQUESTED (agent-4 -> workers, direct)
$forward_fail -> REMEDIATION_REQUESTED (manager -> workers, fallback with dedup)
$unblock      -> CLARIFICATION_REQUESTED (manager -> worker)
```

---

## v3: Auto-Wake Daemons

### auto_wake.sh — Active Listening

**When to use:** To enable autonomous agent wake-up when events arrive.

```bash
# Agent 4 (Auditor) — auto-wakes on PROMOTED_TO_MAIN / AUDIT_REQUESTED
bash .devin/scripts/auto_wake.sh agent-4 "bash .devin/scripts/run_audit.sh"

# Worker agents — auto-wake on REMEDIATION_REQUESTED / CLARIFICATION_REQUESTED
bash .devin/scripts/auto_wake.sh agent-1 "bash .devin/scripts/console.sh \$inbox agent-1"
bash .devin/scripts/auto_wake.sh agent-2 "bash .devin/scripts/console.sh \$inbox agent-2"
bash .devin/scripts/auto_wake.sh agent-3 "bash .devin/scripts/console.sh \$inbox agent-3"
```

**What it does:**
1. Polls `.devin/events/` every 10 seconds for events targeting the agent
2. Race condition guard: re-checks file existence before reading (prevents ENOENT)
3. Prints a high-visibility alert with event details (payload truncated to 50 lines)
4. Optionally executes the wake command
5. Tracks seen events in `.devin/.auto_wake_seen_<agent>.txt`

---

## v3: Direct Auditor Routing

### route_failures.sh — Agent 4 Direct AUDIT_FAIL Routing

**When to use:** Automatically called by `run_audit.sh` after AUDIT_FAIL emission.

```bash
bash .devin/scripts/route_failures.sh <audit_fail_event_file>
```

**What it does:**
1. Parses `blueprint_agent_map.json`
2. For each failing blueprint in `blueprint_results`, emits `REMEDIATION_REQUESTED` directly to the mapped worker
3. Also handles `AUDIT_FAIL_ADDENDUM` events with `new_failed_checks` schema
4. Emits `SYSTEM_ALERT` to user with routing summary
5. No manager intervention required

---

## v3: Shift-Left Pre-Integration

### request_integration.sh v3

**v3 enhancements:**
1. Clean tree verification before any git operations
2. `git fetch origin main` + `git rebase origin/main` (or merge fallback)
3. Full local CI: cargo check + nextest (--test-threads=4) + doctests + clippy + npm
4. sccache enabled with 2G cache limit
5. All log captures use `tail -n 50` to prevent OOM

---

## v3: OOM Crash Recovery

If an agent terminal crashes (OOM or other):

1. Restart the terminal
2. Run `bash .devin/scripts/console.sh $inbox <agent>` or `git status`
3. No manual manager routing needed — auto_wake + route_failures handle it
4. Events persist as JSON files — nothing is lost

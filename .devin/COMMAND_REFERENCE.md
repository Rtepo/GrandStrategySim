# Command Reference — SillyElaborateState Infrastructure v2.3

## Quick Start

All commands are run from the hub directory:

```bash
bash .devin/scripts/console.sh <command> [args]
```

## Commands

### /kickoff — Assign Blueprint to Worker

**When to use:** After Agent 4 (Architect) drafts a roadmap and gives you the agent + blueprint ID.

```bash
bash .devin/scripts/console.sh /kickoff agent-3 004-FIX
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

### /audit_standard — Trigger Full Audit

**When to use:** When all sprint branches are merged and you want Agent 4 to run the comprehensive 23-rule audit.

```bash
bash .devin/scripts/console.sh /audit_standard
```

**What it does automatically:**
1. Builds the 23 Global Rules checklist
2. Emits `AUDIT_REQUESTED` event to Agent 4
3. Agent 4 outputs `AUDIT_FAIL` (structured JSON) or `AUDIT_PASS`

**You provide:** nothing
**System handles:** checklist generation, event emission

---

### /forward_fail — Route Audit Failures to Workers

**When to use:** After Agent 4 emits `AUDIT_FAIL` and you want to automatically route the failures to the responsible workers.

```bash
bash .devin/scripts/console.sh /forward_fail latest
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

### /unblock — Clear CI/CD Block

**When to use:** When a worker agent hits the 3-strike CI/CD failure limit and is blocked.

```bash
bash .devin/scripts/console.sh /unblock agent-3
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

## Workflow Diagram

```
Agent 4 drafts roadmap
  -> User runs: /kickoff agent-3 004-FIX
    -> System assigns branch, emits TASK_ASSIGNED
      -> Agent 3 implements, runs request_integration.sh
        -> Daemon runs CI/CD (6-stage pipeline)
          -> PASS: PROMOTED_TO_MAIN emitted
          -> FAIL: 3-strike block -> User runs: /unblock agent-3
            -> Agent 3 fixes, resubmits

All branches merged:
  -> User runs: /audit_standard
    -> Agent 4 audits against 23 Global Rules
      -> AUDIT_PASS: Sprint complete!
      -> AUDIT_FAIL: User runs: /forward_fail latest
        -> System routes REMEDIATION_REQUESTED to workers
          -> Workers fix, resubmit -> cycle repeats
```

## Event Flow

```
/kickoff      -> TASK_ASSIGNED (manager -> worker)
CI/CD pass    -> PROMOTED_TO_MAIN (daemon -> all)
/audit        -> AUDIT_REQUESTED (manager -> agent-4)
audit fail    -> AUDIT_FAIL (agent-4 -> manager)
/forward_fail -> REMEDIATION_REQUESTED (manager -> workers)
/unblock      -> CLARIFICATION_REQUESTED (manager -> worker)
```

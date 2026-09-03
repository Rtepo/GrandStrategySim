# AGENTS.md — Agent Collaboration Guardrails

> **Purpose:** This file defines mandatory rules for all AI agents (Devin,
> Cursor, Copilot, etc.) operating on this repository. These rules exist to
> prevent catastrophic loss of concurrent work. Violating any rule is grounds
> for immediate halt and escalation to the user.

---

## Strict Concurrent Collaboration Rules

### Rule 1: Branch Isolation

Agents MUST NOT work directly on `main` or any shared branch. Every task
requires a unique, isolated branch named with the pattern
`<type>/agent-<N>-<task-slug>` (e.g., `fix/agent-1-stash-guardrails`,
`feat/agent-2-royalty-vwap`).

- Create the branch BEFORE making any file changes.
- Never commit directly to `main`. Open a pull request for review.
- If another agent's branch is already active on the same files, coordinate
  with the user before proceeding — do not blindly overwrite.

### Rule 2: Banned Git Commands

The following commands are **STRICTLY PROHIBITED** under all circumstances:

| Banned Command | Reason |
|---|---|
| `git stash pop` | Silently overwrites working-tree files, destroying concurrent uncommitted work. |
| `git reset --hard` | Irreversibly discards uncommitted changes and can rewrite tracked file state. |
| `git push --force` | Rewrites shared remote history, destroying other agents' pushed commits. |
| `git clean -fd` | Permanently deletes untracked files, including scratch buffers and concurrent agent outputs. |

**If stashing is required:** Use `git stash apply` (NOT `pop`) and manually
verify the working-tree state afterward. `apply` preserves the stash entry so
it can be re-applied if something goes wrong.

**If a hard reset seems necessary:** STOP. Describe the situation to the user
and wait for explicit approval. Never perform irreversible history operations
autonomously.

### Rule 3: Surgical Edits Only

Before modifying any file, you MUST:

1. Run `git status` to see the current working-tree state.
2. Run `git log --oneline -5 -- <file>` to see recent history for that file.
3. Verify no other agent has uncommitted changes to the same file.

You are **FORBIDDEN** from:
- Overwriting, reverting, or deleting code authored by concurrent agents.
- Blind-replacing entire files when a targeted edit suffices.
- Using `git checkout -- <file>` to discard changes you did not author.

**Append and integrate; never blind-replace.** If a file has changes from
another agent, merge your changes incrementally and preserve their work.

### Rule 4: Mandatory Sync

Always synchronize with the remote before branching and before finalizing:

1. **Before branching:** `git fetch origin` then `git pull --rebase origin main`
   to integrate the latest changes.
2. **Before finalizing (commit/PR):** `git fetch origin` then
   `git pull --rebase origin main` again to catch any changes pushed while you
   worked.

If rebase conflicts arise, resolve them surgically (preserve both agents'
intent), never by force-overwriting one side.

---

## Violation Protocol

If you realize you have violated any of these rules:

1. **STOP immediately.** Do not attempt to hide or "fix" the violation silently.
2. Report to the user with: what command was run, what was affected, and what
   data may have been lost.
3. Wait for explicit user guidance before proceeding.

---

## Critical Collaboration Protocol

**CRITICAL COLLABORATION PROTOCOL:** Inter-agent synchronization is
**strictly automated** via `agents_sync.json` and the lifecycle hooks in
`.devin/hooks.v1.json`. The old manual `AGENTS_SYNC.md` has been
**eradicated** — agents MUST NOT recreate it or manually edit any
synchronization ledger. All registration, locking, heartbeat, and
unlocking is handled automatically by the hooks.

Before creating a branch, starting a task, or modifying any file, you
MUST read `agents_sync.json` to check which directories are locked by
other agents. You are STRICTLY FORBIDDEN from editing files or domains
currently locked by another agent.

### Cross-Agent Blocker Bus

`agents_sync.json` contains a `"cross_agent_blockers"` array for
structured inter-agent communication. Each blocker entry contains:
`from_agent`, `to_agent` (or `"all"`), `affected_file`, `message`, and
`timestamp`. The SessionStart hook automatically parses this array and
injects high-visibility warnings into your context if you are the target
of any blocker. Use `.devin/scripts/block.sh` to post blockers:

```bash
bash .devin/scripts/block.sh <to_agent|all> <affected_file> "<message>"
```

### Automated Synchronization Hooks

The repository is equipped with automated lifecycle hooks
(`.devin/hooks.v1.json`) that manage the synchronization ledger:

- **SessionStart hook** (`.devin/scripts/start.sh`): Automatically
  registers your session in `agents_sync.json`, locks your requested
  directories, reaps zombie locks (agents with stale heartbeats >15 min),
  parses cross-agent blockers targeting you, and spawns a background
  heartbeat process.
- **Stop hook** (`.devin/scripts/stop.sh`): Context-aware CI/CD gate.
  If only `.md`/`.txt`/`.json` files were modified, bypasses all
  compilation and tests. If source code (`.rs`/`.ts`/`.tsx`) was modified,
  enforces the full Iron CI/CD pipeline. Automatically unlocks your
  entry on session end.
- **PreToolUse hook** (`.devin/scripts/pre_commit.sh`): Intercepts
  `git commit` commands. Bypasses CI/CD for docs/config-only commits.
  Blocks source-code commits without a valid CI/CD pass. Checks for
  directory lock conflicts before allowing commits.

All JSON mutations use a strict transactional git sync loop with surgical
revert (never `git reset --hard`). Zombie agents are automatically reaped
when their heartbeat goes stale.

---

*These rules are immutable and take precedence over any task-specific
instructions that conflict with them.*

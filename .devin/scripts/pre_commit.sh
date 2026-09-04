#!/bin/bash
# pre_commit.sh — PreToolUse hook for git commit gating.
#
# Fires on: PreToolUse event, matcher: "exec"
# Timeout: 60 seconds (C4)
#
# Behavior:
#   1. Read stdin JSON, extract tool_input.command
#   2. If command does NOT contain "git commit", exit 0 (pass-through)
#   3. If command contains "git commit":
#      a. Get staged files via git diff --cached --name-only
#      b. BYPASS RULE (ABSOLUTE PRIORITY): If ALL staged files are
#         .md/.txt/.json/.sh/.ps1/.yaml/.yml → exit 0
#         This check runs BEFORE any CI/CD state evaluation.
#      c. EXECUTION: If ANY staged file is source code → require valid CI/CD pass
#         via commit-hash comparison (HEAD vs last_green_commit)
#      d. Lock conflict check: verify no staged file is locked by another agent
#      e. Heartbeat update (C2)

set -uo pipefail

# Source shared library
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/sync_lib.sh"

# ─── Read stdin (hook event data) ──────────────────────────────────────────
STDIN_JSON=""
if [ ! -t 0 ]; then
    STDIN_JSON=$(cat)
fi

# Extract command from stdin JSON
COMMAND=$(echo "$STDIN_JSON" | node -e '
    let input = "";
    process.stdin.on("data", d => input += d);
    process.stdin.on("end", () => {
        try {
            const data = JSON.parse(input);
            console.log(data.tool_input?.command || "");
        } catch(e) {
            console.log("");
        }
    });
' 2>/dev/null || echo "")

# Extract session_id
SESSION_ID=$(echo "$STDIN_JSON" | node -e '
    let input = "";
    process.stdin.on("data", d => input += d);
    process.stdin.on("end", () => {
        try {
            const data = JSON.parse(input);
            console.log(data.session_id || "");
        } catch(e) {
            console.log("");
        }
    });
' 2>/dev/null || echo "")

if [ -z "$SESSION_ID" ]; then
    SESSION_ID="session-$$-$(hostname 2>/dev/null || echo 'unknown')"
fi
export SESSION_ID

# ─── Pass-through if not a git commit ──────────────────────────────────────
if ! echo "$COMMAND" | grep -q "git commit"; then
    exit 0
fi

# ─── Get staged files ──────────────────────────────────────────────────────
STAGED_FILES=$(git diff --cached --name-only 2>/dev/null || true)

if [ -z "$STAGED_FILES" ]; then
    # Nothing staged — allow
    exit 0
fi

# ─── Bypass Rule: Check if ALL staged files are docs/config ────────────────
SOURCE_EXTENSIONS="\.rs \.ts \.tsx \.js \.jsx \.mjs \.cjs \.css \.scss \.sass \.less \.vue \.svelte \.astro \.toml \.py \.c \.cpp \.cc \.h \.hpp \.go \.java \.kt \.swift \.rb"

HAS_SOURCE_CHANGE=0
for file in $STAGED_FILES; do
    for ext in $SOURCE_EXTENSIONS; do
        if echo "$file" | grep -qE "${ext}$"; then
            HAS_SOURCE_CHANGE=1
            break 2
        fi
    done
done

# If no source-code files are staged, allow commit without CI/CD
if [ "$HAS_SOURCE_CHANGE" -eq 0 ]; then
    # Still update heartbeat (C2) — non-blocking
    sync_transactional mutator_heartbeat "sync: heartbeat for $SESSION_ID (pre-commit)" 3 2>/dev/null || true
    exit 0
fi

# ─── Execution Rule: Source code staged — enforce CI/CD gate (commit-hash) ─
# ABSOLUTE BYPASS PRIORITY: The bypass rule above (HAS_SOURCE_CHANGE == 0)
# already exited 0 for non-source staged files. We only reach here if source
# files are staged. Now we use commit-hash comparison instead of mtime.
#
# Rationale: mtime is fundamentally incompatible with Git workflows.
# `git pull` updates file mtimes even when no local code change occurred,
# causing false-positive CI/CD blocks for agents who merely synchronized.
#
# New logic: Compare git rev-parse HEAD against last_green_commit.
#   - If they match → the commit at HEAD was already tested globally → allow.
#   - If they differ → the current HEAD has not been tested → block.
#
# last_green_commit is read from:
#   1. agents_sync.json last_green_commit (global, synced via git pull)
#   2. .devin/.cicd_state (local, written by run_iron_cicd skill)
# The global source takes priority since it reflects manager-verified state.

PROJECT_DIR="${DEVIN_PROJECT_DIR:-$(pwd)}"
CICD_STATE_FILE="$PROJECT_DIR/.devin/.cicd_state"
LEDGER_PATH="$PROJECT_DIR/$LEDGER_FILE"

# Get current HEAD commit hash
CURRENT_HEAD=$(git rev-parse HEAD 2>/dev/null || echo "")

if [ -z "$CURRENT_HEAD" ]; then
    # Not a git repo or HEAD unavailable — allow (can't enforce)
    exit 0
fi

# Read last_green_commit from agents_sync.json (global, priority 1)
GREEN_COMMIT=""
if [ -f "$LEDGER_PATH" ]; then
    GREEN_COMMIT=$(node -e '
        try {
            const data = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
            console.log(data.last_green_commit || "");
        } catch(e) { console.log(""); }
    ' "$LEDGER_PATH" 2>/dev/null || echo "")
fi

# Fall back to .cicd_state (local, priority 2)
if [ -z "$GREEN_COMMIT" ] && [ -f "$CICD_STATE_FILE" ]; then
    CICD_CONTENT=$(cat "$CICD_STATE_FILE" 2>/dev/null || echo "")
    if echo "$CICD_CONTENT" | grep -q "^PASSED"; then
        # Format: "PASSED <ISO timestamp> <commit hash>"
        GREEN_COMMIT=$(echo "$CICD_CONTENT" | awk '{print $3}' 2>/dev/null || echo "")
    fi
fi

# If no green commit recorded anywhere — block
if [ -z "$GREEN_COMMIT" ]; then
    node -e '
        console.log(JSON.stringify({
            decision: "block",
            reason: "IRON CI/CD: Source code is staged but no CI/CD pass has been recorded. Invoke the /run_iron_cicd skill first."
        }));
    '
    exit 2
fi

# Commit-hash comparison: HEAD vs last_green_commit
if [ "$CURRENT_HEAD" != "$GREEN_COMMIT" ]; then
    node -e '
        console.log(JSON.stringify({
            decision: "block",
            reason: "IRON CI/CD: HEAD (" + process.argv[1].slice(0,12) + ") does not match last_green_commit (" + process.argv[2].slice(0,12) + "). The current commit has not been tested. Invoke the /run_iron_cicd skill to run the full pipeline before committing."
        }));
    ' "$CURRENT_HEAD" "$GREEN_COMMIT"
    exit 2
fi

# ─── Lock conflict check ───────────────────────────────────────────────────
# Read agents_sync.json and check if any staged file is in another
# active agent's locked_dirs.
if [ -f "$LEDGER_FILE" ]; then
    CONFLICT=$(cat "$LEDGER_FILE" | node -e '
        let input = "";
        process.stdin.on("data", d => input += d);
        process.stdin.on("end", () => {
            try {
                const data = JSON.parse(input);
                const staged = (process.argv[1] || "").split("\n").filter(f => f);
                const sessionId = process.env.SESSION_ID || "";
                for (const agent of data.agents) {
                    if (agent.status !== "active") continue;
                    if (agent.session_id === sessionId) continue;
                    for (const file of staged) {
                        for (const locked of (agent.locked_dirs || [])) {
                            if (file.startsWith(locked) || locked.startsWith(file)) {
                                console.log(agent.agent_id + "|" + file + "|" + locked);
                                return;
                            }
                        }
                    }
                }
                console.log("");
            } catch(e) {
                console.log("");
            }
        });
    ' "$STAGED_FILES" 2>/dev/null || echo "")

    if [ -n "$CONFLICT" ]; then
        CONFLICT_AGENT=$(echo "$CONFLICT" | cut -d'|' -f1)
        CONFLICT_FILE=$(echo "$CONFLICT" | cut -d'|' -f2)
        CONFLICT_LOCK=$(echo "$CONFLICT" | cut -d'|' -f3)
        node -e '
            const agent = process.argv[1];
            const file = process.argv[2];
            const lock = process.argv[3];
            console.log(JSON.stringify({
                decision: "block",
                reason: "File " + file + " is in a directory locked by agent " + agent + " (locked: " + lock + "). Coordinate with that agent before committing."
            }));
        ' "$CONFLICT_AGENT" "$CONFLICT_FILE" "$CONFLICT_LOCK"
        exit 2
    fi
fi

# ─── Heartbeat update (C2) ─────────────────────────────────────────────────
sync_transactional mutator_heartbeat "sync: heartbeat for $SESSION_ID (pre-commit)" 3 2>/dev/null || true

exit 0

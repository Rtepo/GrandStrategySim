#!/bin/bash
# start.sh — SessionStart hook for multi-agent synchronization.
#
# Fires on: SessionStart event
# Timeout: 60 seconds (C4)
#
# Behavior:
#   1. trap 'cleanup' EXIT — ensures lock release + heartbeat kill on crash
#   2. Read stdin JSON, extract session_id
#   3. Detect requested directories from DEVIN_LOCKED_DIRS or branch name
#   4. Register via transactional loop (C1): reap zombies, check conflicts, register
#   5. If conflict: print error + ledger to stdout (non-blocking warning)
#   6. Spawn background heartbeat (C2 + C3)
#   7. Print additionalContext with current ledger state

set -euo pipefail

# Source shared library
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/sync_lib.sh"

# ─── Read stdin (hook event data) ──────────────────────────────────────────
STDIN_JSON=""
if [ ! -t 0 ]; then
    STDIN_JSON=$(cat)
fi

# Extract session_id from stdin JSON (fallback to PID+hostname hash)
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
    # Fallback: PID + hostname hash
    SESSION_ID="session-$$-$(hostname 2>/dev/null || echo 'unknown')"
fi

export SESSION_ID

# Generate a stable agent_id from session_id
AGENT_ID=$(echo "$SESSION_ID" | node -e '
    let input = "";
    process.stdin.on("data", d => input += d);
    process.stdin.on("end", () => {
        const crypto = require("crypto");
        const hash = crypto.createHash("md5").update(input.trim()).digest("hex").substring(0, 8);
        console.log("agent-" + hash);
    });
' 2>/dev/null || echo "agent-unknown")
export AGENT_ID

# Detect current branch
AGENT_BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "main")
export AGENT_BRANCH

# Detect requested directories
# Priority: DEVIN_LOCKED_DIRS env var > branch name inference
if [ -n "${DEVIN_LOCKED_DIRS:-}" ]; then
    LOCKED_DIRS="$DEVIN_LOCKED_DIRS"
else
    # Infer from branch name (e.g., feat/construction-xxx -> construction/)
    LOCKED_DIRS=$(echo "$AGENT_BRANCH" | node -e '
        let input = "";
        process.stdin.on("data", d => input += d);
        process.stdin.on("end", () => {
            const branch = input.trim();
            // Extract domain from branch patterns like feat/construction-xxx
            const match = branch.match(/(?:feat|fix|refactor)\/([a-z_]+)-/);
            if (match) {
                console.log(match[1] + "/");
            } else {
                console.log(""); // No inference
            }
        });
    ' 2>/dev/null || echo "")
fi
export LOCKED_DIRS

AGENT_TASK="${DEVIN_AGENT_TASK:-session-startup}"
export AGENT_TASK

# ─── Cleanup trap (C2) ─────────────────────────────────────────────────────
HEARTBEAT_PID=""

cleanup() {
    # Kill background heartbeat process
    if [ -n "$HEARTBEAT_PID" ]; then
        kill "$HEARTBEAT_PID" 2>/dev/null || true
    fi
    # Unlock via transactional loop (C1) — surgical revert on failure
    sync_transactional mutator_unlock_agent "sync: unlock agent $SESSION_ID on session end" 5 || true
    # Clear this agent's posted blockers (C5)
    sync_transactional mutator_clear_my_blockers "sync: clear blockers from $AGENT_ID on session end" 3 || true
}
trap 'cleanup' EXIT

# ─── Register via transactional loop (C1) ──────────────────────────────────
# The mutator reaps zombies (C2), checks for directory conflicts, and
# registers the agent — all against freshly pulled state, inside the loop.
if [ -n "$LOCKED_DIRS" ]; then
    sync_transactional mutator_check_and_register "sync: register agent $SESSION_ID (dirs: $LOCKED_DIRS)" 5
    register_rc=$?
else
    # No dirs to lock — just register without conflict check
    sync_transactional mutator_register_agent "sync: register agent $SESSION_ID (no locks)" 5
    register_rc=$?
fi

if [ "$register_rc" -ne 0 ]; then
    # Conflict detected — print warning but do NOT block the session
    echo "WARNING: Directory conflict detected. Another agent may be working in the same area." >&2
    echo "Review agents_sync.json before modifying shared files." >&2
fi

# ─── Spawn background heartbeat (C2 + C3) ──────────────────────────────────
# The heartbeat script updates last_heartbeat every 3 minutes via the
# transactional sync loop, with index.lock evasion (C3).
HEARTBEAT_SCRIPT="$SCRIPT_DIR/heartbeat.sh"
if [ -f "$HEARTBEAT_SCRIPT" ]; then
    nohup bash "$HEARTBEAT_SCRIPT" "$SESSION_ID" >/dev/null 2>&1 &
    HEARTBEAT_PID=$!
fi

# ─── Parse cross-agent blockers targeting this agent (C5) ──────────────────
# Read the ledger and extract any blockers where to_agent matches
# AGENT_ID or "all". Inject them as high-visibility warnings.
BLOCKER_WARNINGS=$(get_blockers_for_agent 2>/dev/null || echo "")

# ─── Print additionalContext with current ledger state + blockers ──────────
LEDGER_SUMMARY=$(ledger_summary 2>/dev/null || echo "Ledger unavailable")

ADDITIONAL_CONTEXT="Multi-Agent Sync: Registered as $AGENT_ID (session: $SESSION_ID, branch: $AGENT_BRANCH). Locked dirs: [${LOCKED_DIRS:-none}]. Active agents:
$LEDGER_SUMMARY

CRITICAL: Before editing any file, check agents_sync.json to verify no other agent has locked the target directory. Update your row when starting/finishing tasks."

# Append blocker warnings if any exist
if [ -n "$BLOCKER_WARNINGS" ]; then
    ADDITIONAL_CONTEXT="$ADDITIONAL_CONTEXT

$BLOCKER_WARNINGS

You MUST coordinate with the blocking agent(s) before modifying the affected files. Use 'bash .devin/scripts/block.sh <to_agent|all> <file> \"<message>\"' to post your own blockers."
else
    ADDITIONAL_CONTEXT="$ADDITIONAL_CONTEXT

No cross-agent blockers targeting you. Use 'bash .devin/scripts/block.sh <to_agent|all> <file> \"<message>\"' to post a blocker to another agent."
fi

# Output JSON for Devin to inject into context
node -e '
    const ctx = process.argv[1];
    console.log(JSON.stringify({
        hookSpecificOutput: {
            hookEventName: "SessionStart",
            additionalContext: ctx
        }
    }));
' "$ADDITIONAL_CONTEXT"

exit 0

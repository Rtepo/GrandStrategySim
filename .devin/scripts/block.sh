#!/bin/bash
# block.sh — CLI helper for posting cross-agent blockers (C5: Blocker Bus).
#
# Usage: bash .devin/scripts/block.sh <to_agent|all> <affected_file> "<message>"
#
# Appends a blocker entry to agents_sync.json via the transactional sync
# loop (C1). The blocker will be visible to the target agent on their
# next SessionStart hook invocation.
#
# Arguments:
#   $1 - to_agent: The agent ID to block (e.g., "agent-1") or "all"
#   $2 - affected_file: The file path that is blocked
#   $3 - message: Human-readable explanation of the blocker
#
# Example:
#   bash .devin/scripts/block.sh agent-1 engine/turn.rs "Need Treasury settlement verified before I can finalize escrow."
#   bash .devin/scripts/block.sh all economy/trade/ "Coordinating a major refactor — please pause edits here."

set -uo pipefail

# ─── Validate arguments ────────────────────────────────────────────────────
if [ $# -lt 3 ]; then
    echo "Usage: bash .devin/scripts/block.sh <to_agent|all> <affected_file> \"<message>\"" >&2
    echo "" >&2
    echo "Arguments:" >&2
    echo "  to_agent      - Agent ID to block (e.g., 'agent-1') or 'all'" >&2
    echo "  affected_file - File path that is blocked" >&2
    echo "  message       - Human-readable explanation" >&2
    echo "" >&2
    echo "Examples:" >&2
    echo "  bash .devin/scripts/block.sh agent-1 engine/turn.rs \"Need Treasury verified first.\"" >&2
    echo "  bash .devin/scripts/block.sh all economy/trade/ \"Major refactor in progress.\"" >&2
    exit 1
fi

BLOCKER_TO="$1"
BLOCKER_FILE="$2"
BLOCKER_MSG="$3"

export BLOCKER_TO
export BLOCKER_FILE
export BLOCKER_MSG

# ─── Determine this agent's ID ─────────────────────────────────────────────
# Try to read from the ledger (find the active agent with current session)
# Fall back to environment or "unknown"
if [ -z "${AGENT_ID:-}" ]; then
    if [ -z "${SESSION_ID:-}" ]; then
        # Try to extract from git branch name as a heuristic
        BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "")
        AGENT_ID=$(echo "$BRANCH" | node -e '
            let input = "";
            process.stdin.on("data", d => input += d);
            process.stdin.on("end", () => {
                const branch = input.trim();
                const match = branch.match(/agent-(\d+)/);
                if (match) {
                    console.log("agent-" + match[1]);
                } else {
                    console.log("agent-unknown");
                }
            });
        ' 2>/dev/null || echo "agent-unknown")
    else
        AGENT_ID=$(echo "$SESSION_ID" | node -e '
            let input = "";
            process.stdin.on("data", d => input += d);
            process.stdin.on("end", () => {
                const crypto = require("crypto");
                const hash = crypto.createHash("md5").update(input.trim()).digest("hex").substring(0, 8);
                console.log("agent-" + hash);
            });
        ' 2>/dev/null || echo "agent-unknown")
    fi
fi
export AGENT_ID

# ─── Source shared library ─────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/sync_lib.sh"

# ─── Post the blocker via transactional sync loop (C1) ─────────────────────
sync_transactional mutator_add_blocker "sync: blocker from $AGENT_ID to $BLOCKER_TO on $BLOCKER_FILE" 5
RC=$?

if [ $RC -eq 0 ]; then
    echo "✓ Blocker posted successfully:"
    echo "  FROM: $AGENT_ID"
    echo "  TO:   $BLOCKER_TO"
    echo "  FILE: $BLOCKER_FILE"
    echo "  MSG:  $BLOCKER_MSG"
else
    echo "✗ Failed to post blocker after 5 attempts (network/git conflict)." >&2
    exit 1
fi

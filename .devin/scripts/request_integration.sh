#!/bin/bash
# request_integration.sh — Worker: signal integration readiness via event bus.
#
# Usage: bash .devin/scripts/request_integration.sh "<description>"
#
# Behavior:
#   1. Detect the hub directory (HUB_DIR)
#   2. Emit an INTEGRATION_REQUESTED event to $HUB_DIR/.devin/events/
#   3. No Git operations required — pure event emission
#
# The manager polls events or checks .devin/events/ at session start.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/sync_lib.sh"

# ─── SECURITY: Worktree path check ─────────────────────────────────────────
# Workers MUST run this script from inside their assigned worktree, NOT from
# the main HUB_DIR. If a worker is in the hub, they are coding in the wrong
# place and their uncommitted work will be destroyed when the daemon switches
# branches for CI/CD. This check prevents that catastrophic data loss.
CURRENT_PWD="$(pwd)"
if [[ "$CURRENT_PWD" != *".devin/worktrees/"* ]] && [[ "$CURRENT_PWD" != *"-agent-"* ]]; then
    echo "" >&2
    echo "╔══════════════════════════════════════════════════════════════════╗" >&2
    echo "║  SECURITY BREACH: You are in the main HUB_DIR!                  ║" >&2
    echo "║                                                                  ║" >&2
    echo "║  You MUST NOT run request_integration.sh from the hub.           ║" >&2
    echo "║  Move to your assigned worktree first:                           ║" >&2
    echo "║    cd ../SillyElaborateState-agent-<N>                           ║" >&2
    echo "║                                                                  ║" >&2
    echo "║  If you have uncommitted code here, it will be LOST when the     ║" >&2
    echo "║  daemon switches branches for CI/CD.                             ║" >&2
    echo "╚══════════════════════════════════════════════════════════════════╝" >&2
    echo "" >&2
    exit 1
fi

# ─── Validate arguments ────────────────────────────────────────────────────
if [ $# -lt 1 ]; then
    echo "Usage: bash .devin/scripts/request_integration.sh \"<description>\"" >&2
    echo "Example: bash .devin/scripts/request_integration.sh \"Phase 94 M0 leak fixes complete\"" >&2
    exit 1
fi

DESCRIPTION="$1"

# ─── Determine agent identity ──────────────────────────────────────────────
AGENT_ID_VAL="${AGENT_ID:-unknown}"
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")

# ─── Build payload ─────────────────────────────────────────────────────────
PAYLOAD=$(node -e '
    const desc = process.argv[1];
    const branch = process.argv[2];
    console.log(JSON.stringify({
        description: desc,
        branch: branch,
        ready_for: "stage_merge"
    }));
' "$DESCRIPTION" "$CURRENT_BRANCH")

# ─── Emit event to hub ─────────────────────────────────────────────────────
bash "$SCRIPT_DIR/emit_event.sh" "INTEGRATION_REQUESTED" "$AGENT_ID_VAL" "agent-5" "$PAYLOAD"

echo ""
echo "Request recorded. The manager will merge your branch during the next"
echo "integration cycle. No further action is needed from you."

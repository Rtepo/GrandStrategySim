#!/bin/bash
# unblock_branch.sh — Manager utility to unlock a CI/CD-blocked branch.
#
# Usage: bash .devin/scripts/unblock_branch.sh <branch>
#
# Resets consecutive_failures to 0 and blocked to false for the given branch
# in .devin/.cicd_failure_state.json.
#
# Only the manager (Agent 5) should run this.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/sync_lib.sh"
validate_manager_auth
if [ "$AGENT_ROLE" != "manager" ]; then
    echo "ERROR: Only the manager (Agent 5) can unblock branches." >&2
    exit 1
fi

BRANCH="${1:-}"
if [ -z "$BRANCH" ]; then
    echo "Usage: bash .devin/scripts/unblock_branch.sh <branch>" >&2
    exit 1
fi

STATE_FILE="$HUB_DIR/.devin/.cicd_failure_state.json"

BRANCH_IN="$BRANCH" STATE_IN="$STATE_FILE" STATE_OUT="$STATE_FILE" node <<'NODE_EOF'
const fs = require("fs");
const branch = process.env.BRANCH_IN;
const stateFile = process.env.STATE_IN;

let state = { branches: {} };
try {
    let raw = fs.readFileSync(stateFile, "utf8");
    if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
    state = JSON.parse(raw);
    if (!state.branches) state.branches = {};
} catch(e) {
    // File doesn't exist or is corrupt — start fresh
    state = { branches: {} };
}

if (state.branches[branch]) {
    state.branches[branch].consecutive_failures = 0;
    state.branches[branch].blocked = false;
    state.branches[branch].unblocked_ts = new Date().toISOString();
} else {
    state.branches[branch] = {
        consecutive_failures: 0,
        blocked: false,
        unblocked_ts: new Date().toISOString()
    };
}

// Atomic write: write to temp then rename
const tmp = stateFile + ".tmp";
fs.writeFileSync(tmp, JSON.stringify(state, null, 2));
fs.renameSync(tmp, stateFile);
console.log("Branch " + branch + " has been unblocked.");
NODE_EOF

exit 0

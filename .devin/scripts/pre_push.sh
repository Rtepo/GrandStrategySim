#!/bin/bash
# pre_push.sh — Branch protection hook (RBAC).
#
# Fires on: PreToolUse event, matcher: "exec"
# Timeout: 60 seconds
#
# Blocks workers from `git push origin main` and `git merge` into main.
# Only the manager (validated via .devin/.manager_auth) may alter main.
# Workers may push to feat/* and fix/* branches.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/sync_lib.sh"

# Read stdin
STDIN_JSON=""
if [ ! -t 0 ]; then
    STDIN_JSON=$(cat)
fi

# Extract command and session_id
COMMAND=$(echo "$STDIN_JSON" | node -e '
    let input = "";
    process.stdin.on("data", d => input += d);
    process.stdin.on("end", () => {
        try { const d = JSON.parse(input); console.log(d.tool_input?.command || ""); }
        catch(e) { console.log(""); }
    });
' 2>/dev/null || echo "")

SESSION_ID=$(echo "$STDIN_JSON" | node -e '
    let input = "";
    process.stdin.on("data", d => input += d);
    process.stdin.on("end", () => {
        try { const d = JSON.parse(input); console.log(d.session_id || ""); }
        catch(e) { console.log(""); }
    });
' 2>/dev/null || echo "")

if [ -z "$SESSION_ID" ]; then
    SESSION_ID="session-$$-$(hostname 2>/dev/null || echo 'unknown')"
fi
export SESSION_ID

# Pass-through if not a git push or git merge
if ! echo "$COMMAND" | grep -qE "git (push|merge)"; then
    exit 0
fi

# Determine role
validate_manager_auth

if [ "$AGENT_ROLE" = "manager" ]; then
    # Manager can push/merge anywhere
    exit 0
fi

# Worker — check target branch
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "")

# Parse target branch from command
TARGET_BRANCH=""

# git push origin <branch> or git push origin <branch>:<branch>
if echo "$COMMAND" | grep -q "git push"; then
    # Extract refspec from push command
    TARGET_BRANCH=$(echo "$COMMAND" | node -e '
        let input = "";
        process.stdin.on("data", d => input += d);
        process.stdin.on("end", () => {
            const cmd = input.trim();
            // git push origin main -> "main"
            // git push origin main:main -> "main"
            // git push origin HEAD:main -> "main"
            const parts = cmd.split(/\s+/);
            const pushIdx = parts.indexOf("push");
            if (pushIdx >= 0 && parts[pushIdx + 2]) {
                let refspec = parts[pushIdx + 2];
                // Handle refspec with colon
                if (refspec.includes(":")) {
                    refspec = refspec.split(":")[1];
                }
                // Remove flags
                if (refspec.startsWith("-")) refspec = "";
                console.log(refspec);
            } else {
                // No refspec — use upstream
                console.log("");
            }
        });
    ' 2>/dev/null || echo "")

    # If no refspec, read upstream
    if [ -z "$TARGET_BRANCH" ]; then
        TARGET_BRANCH=$(git rev-parse --abbrev-ref @{upstream} 2>/dev/null | sed 's|origin/||' || echo "")
    fi
fi

# git merge <branch> — check if merging INTO main (current branch is main)
if echo "$COMMAND" | grep -q "git merge"; then
    if [ "$CURRENT_BRANCH" = "main" ]; then
        TARGET_BRANCH="main"
    fi
fi

# If target is "main" (or starts with main), block for workers
if echo "$TARGET_BRANCH" | grep -qE "^main$|^main[^/]"; then
    node -e '
        console.log(JSON.stringify({
            decision: "block",
            reason: "BRANCH PROTECTION: Only the manager (agent-5) may push to or merge into main. Workers may only push to feat/* or fix/* branches. To merge your work, ask the manager to run: bash .devin/scripts/merge_worker.sh <your-branch>"
        }));
    '
    exit 2
fi

# If we can't determine the target, fail-safe: check current branch
if [ -z "$TARGET_BRANCH" ] && [ "$CURRENT_BRANCH" = "main" ]; then
    node -e '
        console.log(JSON.stringify({
            decision: "block",
            reason: "BRANCH PROTECTION: You are on main and attempting a git operation. Only the manager may alter main."
        }));
    '
    exit 2
fi

# Allow — target is feat/*, fix/*, or indeterminate (not main)
exit 0

#!/bin/bash
# stop.sh — Stop hook with context-aware CI/CD bypass + auto-unlock (C2).
#
# Fires on: Stop event
# Timeout: 60 seconds (C4)
#
# Behavior:
#   1. trap 'cleanup' EXIT — ensures auto-unlock even if CI/CD crashes
#   2. Get all modified files (staged + unstaged + untracked)
#   3. BYPASS RULE (ABSOLUTE PRIORITY): If ALL modified files are
#      .md/.txt/.json/.sh/.ps1/.yaml/.yml → skip all CI/CD → proceed to cleanup
#      This check runs BEFORE any CI/CD state evaluation.
#   4. EXECUTION RULE: If ANY modified file is .rs/.ts/.tsx/.js/.jsx/.css/.scss/.vue/.svelte
#      → enforce Iron CI/CD gate via commit-hash comparison:
#        Compare git rev-parse HEAD against last_green_commit (from .cicd_state
#        or agents_sync.json). If they match, the commit was already tested
#        globally → allow. Otherwise → block.
#   5. Auto-unlock: Call sync_transactional mutator_unlock_agent (C1)
#   6. Kill background heartbeat if running

set -uo pipefail

# Source shared library
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/sync_lib.sh"

# ─── Read stdin (hook event data) ──────────────────────────────────────────
STDIN_JSON=""
if [ ! -t 0 ]; then
    STDIN_JSON=$(cat)
fi

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

# ─── Cleanup trap (C2) ─────────────────────────────────────────────────────
cleanup() {
    # Unlock via transactional loop (C1) — surgical revert on failure
    sync_transactional mutator_unlock_agent "sync: unlock agent $SESSION_ID on stop" 5 2>/dev/null || true
}
trap 'cleanup' EXIT

# ─── Get all modified files ────────────────────────────────────────────────
# Tracked changes (staged + unstaged) vs HEAD
TRACKED_FILES=$(git diff --name-only HEAD 2>/dev/null || true)
# Untracked files
UNTRACKED_FILES=$(git ls-files --others --exclude-standard 2>/dev/null || true)

ALL_FILES=""
if [ -n "$TRACKED_FILES" ]; then
    ALL_FILES="$TRACKED_FILES"
fi
if [ -n "$UNTRACKED_FILES" ]; then
    if [ -n "$ALL_FILES" ]; then
        ALL_FILES="$ALL_FILES
$UNTRACKED_FILES"
    else
        ALL_FILES="$UNTRACKED_FILES"
    fi
fi

# ─── Context-Aware CI/CD Bypass ────────────────────────────────────────────
# Extensions that require CI/CD enforcement (source code + build manifests)
SOURCE_EXTENSIONS="\.rs \.ts \.tsx \.js \.jsx \.mjs \.cjs \.css \.scss \.sass \.less \.vue \.svelte \.astro \.toml \.py \.c \.cpp \.cc \.h \.hpp \.go \.java \.kt \.swift \.rb"

# Check if ANY modified file has a source-code extension
HAS_SOURCE_CHANGE=0
if [ -n "$ALL_FILES" ]; then
    for file in $ALL_FILES; do
        for ext in $SOURCE_EXTENSIONS; do
            if echo "$file" | grep -qE "${ext}$"; then
                HAS_SOURCE_CHANGE=1
                break 2
            fi
        done
    done
fi

# If no source-code files were modified, bypass CI/CD entirely
if [ "$HAS_SOURCE_CHANGE" -eq 0 ]; then
    # Write bypass marker for auditability
    echo "BYPASS $(date -u +%Y-%m-%dT%H:%M:%SZ) - no source-code changes. Modified: $(echo "$ALL_FILES" | tr '\n' ',' )" >> .devin/.cicd_bypass 2>/dev/null || true
    exit 0
fi

# ─── Source code WAS modified — enforce Iron CI/CD gate (commit-hash) ───────
# ABSOLUTE BYPASS PRIORITY: The bypass rule above (HAS_SOURCE_CHANGE == 0)
# already exited 0 for non-source changes. We only reach here if source files
# were modified. Now we use commit-hash comparison instead of mtime.
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
#   1. .devin/.cicd_state (local, written by run_iron_cicd skill)
#   2. agents_sync.json last_green_commit (global, synced via git pull)
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
            reason: "IRON CI/CD: Source code was modified but no CI/CD pass has been recorded. Invoke the /run_iron_cicd skill to run: cargo build --workspace, cargo test --workspace --all-targets, cargo clippy --workspace --all-targets -- -D warnings, npm run build. Only report back to the user after ALL four pass with zero errors and zero warnings."
        }));
    '
    exit 2
fi

# Commit-hash comparison: HEAD vs last_green_commit
if [ "$CURRENT_HEAD" = "$GREEN_COMMIT" ]; then
    # The commit at HEAD was already tested globally → allow stop
    exit 0
fi

# HEAD differs from last tested commit — block
node -e '
    console.log(JSON.stringify({
        decision: "block",
        reason: "IRON CI/CD: HEAD (" + process.argv[1].slice(0,12) + ") does not match last_green_commit (" + process.argv[2].slice(0,12) + "). The current commit has not been tested. Invoke the /run_iron_cicd skill to run the full pipeline."
    }));
' "$CURRENT_HEAD" "$GREEN_COMMIT"
exit 2

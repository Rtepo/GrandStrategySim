#!/bin/bash
# stop.sh — Stop hook with context-aware CI/CD bypass + auto-unlock (C2).
#
# Fires on: Stop event
# Timeout: 60 seconds (C4)
#
# Behavior:
#   1. trap 'cleanup' EXIT — ensures auto-unlock even if CI/CD crashes
#   2. Get all modified files (staged + unstaged + untracked)
#   3. BYPASS RULE: If ALL modified files are .md/.txt/.json/.sh/.ps1/.yaml/.yml
#      → skip all CI/CD → proceed to cleanup
#   4. EXECUTION RULE: If ANY modified file is .rs/.ts/.tsx/.js/.jsx/.css/.scss/.vue/.svelte
#      → enforce full Iron CI/CD gate (check .cicd_state timestamp)
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

# ─── Source code WAS modified — enforce Iron CI/CD gate ────────────────────
PROJECT_DIR="${DEVIN_PROJECT_DIR:-$(pwd)}"
CICD_STATE_FILE="$PROJECT_DIR/.devin/.cicd_state"

if [ ! -f "$CICD_STATE_FILE" ]; then
    # No CI/CD state file — block
    node -e '
        console.log(JSON.stringify({
            decision: "block",
            reason: "IRON CI/CD: Source code was modified but the Iron CI/CD pipeline has not been run. Invoke the /run_iron_cicd skill to run: cargo build --workspace, cargo test --workspace --all-targets, cargo clippy --workspace --all-targets -- -D warnings, npm run build. Only report back to the user after ALL four pass with zero errors and zero warnings."
        }));
    '
    exit 2
fi

# Check if CI/CD pass is valid (state file must be newer than newest source file)
CICD_CONTENT=$(cat "$CICD_STATE_FILE" 2>/dev/null || echo "")
if echo "$CICD_CONTENT" | grep -q "^PASSED"; then
    # Compare .cicd_state mtime against newest source file mtime
    CICD_MTIME=$(stat -c %Y "$CICD_STATE_FILE" 2>/dev/null || stat -f %m "$CICD_STATE_FILE" 2>/dev/null || echo 0)

    # Find newest source file mtime
    NEWEST_SOURCE_MTIME=0
    if [ -n "$ALL_FILES" ]; then
        for file in $ALL_FILES; do
            for ext in $SOURCE_EXTENSIONS; do
                if echo "$file" | grep -qE "${ext}$"; then
                    if [ -f "$file" ]; then
                        FMTIME=$(stat -c %Y "$file" 2>/dev/null || stat -f %m "$file" 2>/dev/null || echo 0)
                        if [ "$FMTIME" -gt "$NEWEST_SOURCE_MTIME" ]; then
                            NEWEST_SOURCE_MTIME=$FMTIME
                        fi
                    fi
                    break
                fi
            done
        done
    fi

    if [ "$CICD_MTIME" -ge "$NEWEST_SOURCE_MTIME" ]; then
        # CI/CD passed after the newest source change — allow stop
        exit 0
    fi

    # Source code was modified after last CI/CD pass — block
    node -e '
        console.log(JSON.stringify({
            decision: "block",
            reason: "IRON CI/CD: Source code was modified after the last CI/CD pass. Invoke the /run_iron_cicd skill to re-run the pipeline."
        }));
    '
    exit 2
fi

# State file exists but doesn't contain PASSED — block
node -e '
    console.log(JSON.stringify({
        decision: "block",
        reason: "IRON CI/CD: The CI/CD state file is invalid. Invoke the /run_iron_cicd skill to run the full pipeline."
    }));
'
exit 2

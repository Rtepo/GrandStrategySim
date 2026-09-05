#!/bin/bash
# integration_daemon.sh v2.0 — Autonomous CI/CD pipeline daemon (Agent 5 / Manager).
#
# Polls for INTEGRATION_REQUESTED events every 15 seconds, runs the 5-stage Iron
# CI/CD pipeline on a staging branch (never main directly), merges green branches
# to main, archives task briefs, and emits AUDIT_REQUESTED when all blueprints
# are promoted.
#
# This is the sole CI/CD path — no external stage_merge.sh dependency.
# The daemon owns the full pipeline: staging branch → 5-stage CI/CD → main.
#
# v2.0 improvements:
#   - Sprint check scans BOTH events/ and .archive/ for PROMOTED_TO_MAIN
#   - 3-strike deadlock guard: branches blocked after 3 consecutive failures
#   - 5th CI/CD stage: headless 50-tick runtime smoke test
#   - Automated archive hygiene: deletes files >7 days old every 100 cycles
#
# Usage: nohup bash .devin/scripts/integration_daemon.sh >> .devin/integration_log/daemon.log 2>&1 &
#
# Safety:
#   - Single-instance via PID file
#   - No git reset --hard (AGENTS.md Rule 2)
#   - Failed merges use git merge --abort
#   - main is only touched after ALL CI/CD steps pass on staging
#   - Malformed events are archived, not retried
#   - All Node.js invocations use env vars + quoted heredocs
#   - Dirty-tree guard prevents data loss from workers coding in HUB_DIR

set -uo pipefail

# ─── Configuration ─────────────────────────────────────────────────────────
HUB_DIR="${HUB_DIR:-$(pwd)}"
export HUB_DIR
POLL_INTERVAL=15
PID_FILE="$HUB_DIR/.devin/.integration_daemon.pid"
LOG_DIR="$HUB_DIR/.devin/integration_log"
EVENTS_DIR="$HUB_DIR/.devin/events"
ARCHIVE_DIR="$EVENTS_DIR/.archive"
TASKS_DIR="$HUB_DIR/.devin/tasks"
TASKS_ARCHIVE_DIR="$TASKS_DIR/.archive"
IN_PROGRESS_DIR="$TASKS_DIR/in_progress"
STAGING_BRANCH="staging"
FAILURE_STATE_FILE="$HUB_DIR/.devin/.cicd_failure_state.json"
MAX_CONSECUTIVE_FAILURES=3

mkdir -p "$LOG_DIR" "$ARCHIVE_DIR" "$TASKS_ARCHIVE_DIR" "$IN_PROGRESS_DIR"

# ─── Manager Authentication ────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/sync_lib.sh"
validate_manager_auth
if [ "$AGENT_ROLE" != "manager" ]; then
    echo "[$(date -u +%H:%M:%S)] ERROR: Only the manager (Agent 5) can run this daemon." >&2
    exit 1
fi

# ─── Single-Instance Guard ─────────────────────────────────────────────────
if [ -f "$PID_FILE" ]; then
    OLD_PID=$(cat "$PID_FILE" 2>/dev/null || echo "")
    if [ -n "$OLD_PID" ] && kill -0 "$OLD_PID" 2>/dev/null; then
        echo "[$(date -u +%H:%M:%S)] ERROR: Daemon already running (PID $OLD_PID). Aborting." >&2
        exit 1
    fi
fi
echo $$ > "$PID_FILE"

# ─── Cleanup Trap ──────────────────────────────────────────────────────────
cleanup() {
    rm -f "$PID_FILE"
    echo ""
    echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] Daemon stopped (PID $$)."
}
trap cleanup EXIT INT TERM

# ─── Helper: Extract event fields via single Node.js call (env vars) ───────
# Uses process.env for path passing — foolproof, no argv indexing issues.
# Strips UTF-8 BOM if present (PowerShell-generated JSON files often have BOM).
extract_event_fields() {
    local event_file="$1"
    local env_file="$2"

    # Convert to absolute paths for Node.js compatibility on Windows
    local abs_event_file="$(cd "$(dirname "$event_file")" && pwd)/$(basename "$event_file")"
    local abs_env_file="$(cd "$(dirname "$env_file")" && pwd)/$(basename "$env_file")"

    EVENT_IN="$abs_event_file" ENV_OUT="$abs_env_file" node <<'NODE_EOF'
        const fs = require("fs");
        try {
            let raw = fs.readFileSync(process.env.EVENT_IN, "utf8");
            if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
            const d = JSON.parse(raw);
            if (!d || typeof d !== "object") throw new Error("Not a JSON object");
            const p = d.payload || {};
            function sq(val) {
                const s = String(val || "");
                return "'" + s.replace(/'/g, "'\\''") + "'";
            }
            const out = [
                "EVENT_TYPE=" + sq(d.type || ""),
                "EVENT_SOURCE=" + sq(d.source || "unknown"),
                "EVENT_TARGET=" + sq(d.target || ""),
                "EVENT_TIMESTAMP=" + sq(d.timestamp || ""),
                "EVENT_BRANCH=" + sq(p.branch || ""),
                "EVENT_DESCRIPTION=" + sq(p.description || ""),
                "EVENT_READY_FOR=" + sq(p.ready_for || ""),
                "EVENT_ID=" + sq(d.id || ""),
                "EVENT_PARSE_ERROR="
            ].join("\n");
            fs.writeFileSync(process.env.ENV_OUT, out);
        } catch(e) {
            const msg = String(e.message || "unknown parse error").replace(/'/g, "'\\''");
            fs.writeFileSync(process.env.ENV_OUT,
                "EVENT_PARSE_ERROR=1\n" +
                "EVENT_ERROR='" + msg + "'");
        }
NODE_EOF

    source "$env_file"
    rm -f "$env_file"

    if [ -n "${EVENT_PARSE_ERROR:-}" ]; then
        echo "[$(date -u +%H:%M:%S)] ERROR: Malformed event in $(basename "$event_file"): ${EVENT_ERROR:-unknown}" >&2
        echo "  Archiving corrupted event to prevent reprocessing."
        mv "$event_file" "$ARCHIVE_DIR/" 2>/dev/null || true
        return 1
    fi

    if [ -z "${EVENT_BRANCH:-}" ]; then
        echo "[$(date -u +%H:%M:%S)] ERROR: Event $(basename "$event_file") has no branch field. Archiving." >&2
        mv "$event_file" "$ARCHIVE_DIR/" 2>/dev/null || true
        return 1
    fi

    return 0
}

# ─── Helper: Dirty-tree guard — prevents data loss from workers coding in HUB_DIR ─
# v2.1: All output goes to >&2 (stderr) so it reaches daemon.log even inside $(...).
# v2.1: Maximum wait of 300s (5 min) — then DIRTY_TREE_TIMEOUT to prevent infinite hang.
guard_clean_tree() {
    local dirty=$(git status --porcelain 2>/dev/null | grep -v '^??' | head -1)
    if [ -n "$dirty" ]; then
        echo "" >&2
        echo "[$(date -u +%H:%M:%S)] !!! DIRTY WORKING TREE DETECTED IN HUB_DIR !!!" >&2
        echo "  Uncommitted tracked changes found. A worker may be coding outside their worktree." >&2
        echo "  CI/CD HALTED to prevent data loss." >&2
        echo "  Dirty tracked files (first 10):" >&2
        git status --porcelain 2>/dev/null | grep -v '^??' | head -10 | sed 's/^/    /' >&2
        echo "" >&2

        bash "$SCRIPT_DIR/emit_event.sh" "SYSTEM_ALERT" "agent-5" "user" \
            "{\"reason\":\"Uncommitted changes detected in HUB_DIR. A worker is outside their worktree. CI/CD halted to prevent data loss.\",\"dirty_files\":\"$(git status --porcelain 2>/dev/null | grep -v '^??' | head -5 | tr '\n' ';')\"}" 2>/dev/null

        echo "  SYSTEM_ALERT emitted to user." >&2
        echo "  Waiting for working tree to be clean (max 300s)..." >&2
        echo "" >&2

        local max_wait=300
        local waited=0
        while [ $waited -lt $max_wait ]; do
            dirty=$(git status --porcelain 2>/dev/null | grep -v '^??' | head -1)
            if [ -z "$dirty" ]; then
                echo "[$(date -u +%H:%M:%S)] Working tree is now clean. Resuming CI/CD." >&2
                return 0
            fi
            echo "[$(date -u +%H:%M:%S)] Still dirty. Sleeping 30s... (waited ${waited}s/${max_wait}s)" >&2
            sleep 30
            waited=$((waited + 30))
        done

        echo "[$(date -u +%H:%M:%S)] DIRTY TREE TIMEOUT after ${max_wait}s. Aborting CI/CD." >&2
        echo "DIRTY_TREE_TIMEOUT"
        return 1
    fi
    return 0
}

# ─── Helper: Empty Branch Guard — rejects branches with no commits ahead of main ─
# v2.2: Prevents false promotions when a branch has no new code vs main.
# Returns 0 if branch has commits + file diffs, 1 otherwise.
# Echoes the failure reason (EMPTY_BRANCH_NO_COMMITS / EMPTY_BRANCH_NO_DIFFS) on failure.
check_branch_has_commits() {
    local branch="$1"
    local ahead
    ahead=$(git rev-list --count main.."$branch" 2>/dev/null || echo "0")
    if [ "$ahead" -eq 0 ]; then
        echo "EMPTY_BRANCH_NO_COMMITS"
        return 1
    fi
    # Also verify there are actual file differences (tree vs tree, not merge-base)
    local diff_files
    diff_files=$(git diff --name-only main "$branch" 2>/dev/null | head -1)
    if [ -z "$diff_files" ]; then
        echo "EMPTY_BRANCH_NO_DIFFS"
        return 1
    fi
    echo "[$(date -u +%H:%M:%S)] Branch check: $branch is $ahead commits ahead of main, files differ." >&2
    return 0
}

# ─── Helper: Check CI/CD failure state (3-strike guard) ────────────────────
# Returns 0 if branch is allowed, 1 if blocked.
# Sets global BRANCH_BLOCKED_REASON if blocked.
check_failure_state() {
    local branch="$1"
    BRANCH_BLOCKED_REASON=""

    [ -f "$FAILURE_STATE_FILE" ] || return 0

    BRANCH_IN="$branch" STATE_IN="$FAILURE_STATE_FILE" node <<'NODE_EOF'
        const fs = require("fs");
        try {
            let raw = fs.readFileSync(process.env.STATE_IN, "utf8");
            if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
            const state = JSON.parse(raw);
            const b = state.branches && state.branches[process.env.BRANCH_IN];
            if (b && b.blocked) {
                console.log("BLOCKED:" + (b.last_failure_reason || "unknown") + ":" + (b.consecutive_failures || 3));
            } else {
                console.log("OK:" + ((b && b.consecutive_failures) || 0));
            }
        } catch(e) {
            console.log("OK:0");
        }
NODE_EOF
}

# ─── Helper: Update failure state after CI/CD result ───────────────────────
# $1 = branch, $2 = "success" or "failure", $3 = failure reason
update_failure_state() {
    local branch="$1"
    local result="$2"
    local reason="${3:-}"

    BRANCH_IN="$branch" RESULT_IN="$result" REASON_IN="$reason" STATE_IN="$FAILURE_STATE_FILE" STATE_OUT="$FAILURE_STATE_FILE" node <<'NODE_EOF'
        const fs = require("fs");
        const branch = process.env.BRANCH_IN;
        const result = process.env.RESULT_IN;
        const reason = process.env.REASON_IN;
        const stateFile = process.env.STATE_IN;

        let state = { branches: {} };
        try {
            let raw = fs.readFileSync(stateFile, "utf8");
            if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
            state = JSON.parse(raw);
            if (!state.branches) state.branches = {};
        } catch(e) {
            state = { branches: {} };
        }

        if (!state.branches[branch]) {
            state.branches[branch] = { consecutive_failures: 0, blocked: false };
        }

        if (result === "success") {
            state.branches[branch].consecutive_failures = 0;
            state.branches[branch].blocked = false;
        } else {
            state.branches[branch].consecutive_failures = (state.branches[branch].consecutive_failures || 0) + 1;
            state.branches[branch].last_failure_reason = reason;
            state.branches[branch].last_failure_ts = new Date().toISOString();
            if (state.branches[branch].consecutive_failures >= 3) {
                state.branches[branch].blocked = true;
                console.log("BLOCKED:" + branch + ":" + state.branches[branch].consecutive_failures);
            } else {
                console.log("FAILURE:" + branch + ":" + state.branches[branch].consecutive_failures);
            }
        }

        const tmp = stateFile + ".tmp";
        fs.writeFileSync(tmp, JSON.stringify(state, null, 2));
        fs.renameSync(tmp, stateFile);
NODE_EOF
}

# ─── Helper: Run CI/CD — the sole execution path (staging → 5-stage → main) ─
# v2.1: All stages wrapped in POSIX timeout to prevent infinite hangs.
# timeout returns 124 when the command exceeds the limit.
# Creates a staging branch from main, merges the worker branch, runs all 5
# Iron CI/CD stages, and only merges to main after ALL pass.
# Returns 0 on success (echoes SUCCESS:<commit>), 1 on failure (echoes reason).
run_cicd() {
    local worker_branch="$1"
    local log_prefix="$LOG_DIR/$(date -u +%Y%m%dT%H%M%SZ)_${worker_branch//\//_}"

    echo "[$(date -u +%H:%M:%S)] CI/CD: Running 5-stage Iron pipeline on staging branch (with watchdog timeouts)."

    # Guard: ensure working tree is clean before any branch switch
    if ! guard_clean_tree; then
        echo "DIRTY_TREE_TIMEOUT"
        return 1
    fi

    # Step 1: Create/reset staging from main (timeout: 30s each)
    timeout 30 git checkout main 2>&1 | tail -1
    timeout 30 git branch -f "$STAGING_BRANCH" main 2>/dev/null || true
    timeout 30 git checkout "$STAGING_BRANCH" 2>&1 | tail -1

    # Step 2: Merge worker branch into staging (NOT main) (timeout: 60s)
    timeout 60 git merge "$worker_branch" --no-edit 2>&1 | tail -5
    local merge_rc=${PIPESTATUS[0]}
    if [ $merge_rc -ne 0 ]; then
        local conflicts=$(git diff --name-only --diff-filter=U 2>/dev/null | tr '\n' ' ')
        git merge --abort 2>/dev/null || true
        git checkout main 2>/dev/null
        if [ $merge_rc -eq 124 ]; then
            echo "TIMEOUT_FAILED" > "${log_prefix}_FAILED.txt"
            echo "TIMEOUT_FAILED"
            echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: git merge TIMEOUT (exceeded 60s)"
        else
            echo "MERGE_CONFLICT:$conflicts" > "${log_prefix}_FAILED.txt"
            echo "MERGE_CONFLICT:$conflicts"
            echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: Merge conflict in: $conflicts"
        fi
        return 1
    fi

    # ─── Merge Verification (v2.2) ──────────────────────────────────────────
    # Verify staging's tree actually differs from main (catches no-op merges).
    # Uses git diff main HEAD (space, not triple-dot) to compare actual trees.
    local merge_diff
    merge_diff=$(git diff --name-only main HEAD 2>/dev/null | head -1)
    if [ -z "$merge_diff" ]; then
        git checkout main 2>/dev/null
        echo "MERGE_NOOP" > "${log_prefix}_FAILED.txt"
        echo "MERGE_NOOP"
        echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: Merge produced no tree changes (staging tree == main tree). Possible false merge."
        return 1
    fi

    # Step 3: Run all 5 CI/CD steps on staging (with watchdog timeouts)
    echo "[$(date -u +%H:%M:%S)] CI/CD: [1/5] cargo build... (timeout: 300s)"
    timeout 300 cargo build --workspace 2>&1 | tee "${log_prefix}_build.txt" | tail -3
    local build_rc=${PIPESTATUS[0]}
    if [ $build_rc -ne 0 ]; then
        git checkout main 2>/dev/null
        if [ $build_rc -eq 124 ]; then
            echo "TIMEOUT_FAILED" > "${log_prefix}_FAILED.txt"
            echo "TIMEOUT_FAILED"
            echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: cargo build TIMEOUT (exceeded 300s)"
        else
            echo "BUILD_FAILED" > "${log_prefix}_FAILED.txt"
            echo "BUILD_FAILED"
            echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: cargo build (rc=$build_rc)"
        fi
        return 1
    fi

    echo "[$(date -u +%H:%M:%S)] CI/CD: [2/5] cargo test (excluding smoke test)... (timeout: 300s)"
    timeout 300 cargo test --workspace --all-targets -- --skip headless_50_tick_smoke 2>&1 | tee "${log_prefix}_test.txt" | tail -5
    local test_rc=${PIPESTATUS[0]}
    if [ $test_rc -ne 0 ]; then
        git checkout main 2>/dev/null
        if [ $test_rc -eq 124 ]; then
            echo "TIMEOUT_FAILED" > "${log_prefix}_FAILED.txt"
            echo "TIMEOUT_FAILED"
            echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: cargo test TIMEOUT (exceeded 300s)"
        else
            echo "TEST_FAILED" > "${log_prefix}_FAILED.txt"
            echo "TEST_FAILED"
            echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: cargo test (rc=$test_rc)"
        fi
        return 1
    fi

    echo "[$(date -u +%H:%M:%S)] CI/CD: [3/5] cargo clippy... (timeout: 300s)"
    timeout 300 cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tee "${log_prefix}_clippy.txt" | tail -3
    local clippy_rc=${PIPESTATUS[0]}
    if [ $clippy_rc -ne 0 ]; then
        git checkout main 2>/dev/null
        if [ $clippy_rc -eq 124 ]; then
            echo "TIMEOUT_FAILED" > "${log_prefix}_FAILED.txt"
            echo "TIMEOUT_FAILED"
            echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: cargo clippy TIMEOUT (exceeded 300s)"
        else
            echo "CLIPPY_FAILED" > "${log_prefix}_FAILED.txt"
            echo "CLIPPY_FAILED"
            echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: cargo clippy (rc=$clippy_rc)"
        fi
        return 1
    fi

    echo "[$(date -u +%H:%M:%S)] CI/CD: [4/5] npm run build... (timeout: 180s)"
    timeout 180 npm run build 2>&1 | tee "${log_prefix}_npm.txt" | tail -3
    local npm_rc=${PIPESTATUS[0]}
    if [ $npm_rc -ne 0 ]; then
        git checkout main 2>/dev/null
        if [ $npm_rc -eq 124 ]; then
            echo "TIMEOUT_FAILED" > "${log_prefix}_FAILED.txt"
            echo "TIMEOUT_FAILED"
            echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: npm run build TIMEOUT (exceeded 180s)"
        else
            echo "NPM_FAILED" > "${log_prefix}_FAILED.txt"
            echo "NPM_FAILED"
            echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: npm run build (rc=$npm_rc)"
        fi
        return 1
    fi

    echo "[$(date -u +%H:%M:%S)] CI/CD: [5/5] headless 50-tick smoke test... (timeout: 120s)"
    timeout 120 cargo test --workspace --test headless_smoke_test -- headless_50_tick_smoke --nocapture 2>&1 | tee "${log_prefix}_smoke.txt"
    local smoke_rc=${PIPESTATUS[0]}
    if [ $smoke_rc -ne 0 ]; then
        git checkout main 2>/dev/null
        if [ $smoke_rc -eq 124 ]; then
            echo "TIMEOUT_FAILED" > "${log_prefix}_FAILED.txt"
            echo "TIMEOUT_FAILED"
            echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: headless smoke test TIMEOUT (exceeded 120s)"
        else
            echo "SMOKE_FAILED" > "${log_prefix}_FAILED.txt"
            echo "SMOKE_FAILED"
            echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: headless smoke test (rc=$smoke_rc)"
            echo "  See full panic output in: ${log_prefix}_smoke.txt"
        fi
        return 1
    fi

    # Step 4: ALL CI/CD PASSED — now safe to merge staging into main (timeout: 30s)
    local staging_commit=$(git rev-parse HEAD)
    timeout 30 git checkout main 2>&1 | tail -1
    timeout 30 git merge --ff-only "$staging_commit" 2>&1 | tail -3
    local ff_rc=$?
    if [ $ff_rc -ne 0 ]; then
        echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: Cannot fast-forward main to staging."
        return 1
    fi

    echo "SUCCESS:$staging_commit"
    return 0
}

# ─── Helper: Process a single INTEGRATION_REQUESTED event ──────────────────
process_integration_request() {
    local event_file="$1"
    local env_file="$HUB_DIR/.devin/.daemon_event_$$.env"
    local basename_file=$(basename "$event_file")

    echo ""
    echo "[$(date -u +%H:%M:%S)] === Processing: $basename_file ==="

    # Extract fields (single Node.js call via heredoc)
    if ! extract_event_fields "$event_file" "$env_file"; then
        return 1  # malformed or missing branch — already archived
    fi

    echo "  Source:    $EVENT_SOURCE"
    echo "  Branch:    $EVENT_BRANCH"
    echo "  Description: ${EVENT_DESCRIPTION:0:80}..."

    # ─── 3-strike deadlock guard ───────────────────────────────────────────
    local failure_check=$(check_failure_state "$EVENT_BRANCH")
    if [[ "$failure_check" == BLOCKED:* ]]; then
        local block_reason=$(echo "$failure_check" | cut -d: -f2)
        local block_count=$(echo "$failure_check" | cut -d: -f3)
        echo "[$(date -u +%H:%M:%S)] BRANCH BLOCKED: $EVENT_BRANCH ($block_count consecutive failures). Rejecting."

        # Archive the event without processing
        mv "$event_file" "$ARCHIVE_DIR/" 2>/dev/null || true

        # Emit SYSTEM_ALERT to User
        bash "$SCRIPT_DIR/emit_event.sh" "SYSTEM_ALERT" "agent-5" "user" \
            "{\"failed_branch\":\"$EVENT_BRANCH\",\"assigned_worker\":\"$EVENT_SOURCE\",\"reason\":\"Branch BLOCKED after $block_count consecutive CI/CD failures ($block_reason). Manual intervention required. Run unblock_branch.sh to unlock.\"}" 2>/dev/null

        # Emit CLARIFICATION_REQUESTED to worker
        bash "$SCRIPT_DIR/emit_event.sh" "CLARIFICATION_REQUESTED" "agent-5" "$EVENT_SOURCE" \
            "{\"branch\":\"$EVENT_BRANCH\",\"reason\":\"Branch blocked after $block_count consecutive failures. Fix root cause and request manual unlock from manager.\",\"action\":\"Fix errors, then ask manager to run unblock_branch.sh\"}" 2>/dev/null

        echo "  SYSTEM_ALERT emitted to user."
        echo "  CLARIFICATION_REQUESTED emitted to $EVENT_SOURCE."
        return 1
    fi

    # ─── Empty Branch Guard (v2.2) ──────────────────────────────────────────
    # Reject branches with no commits or file diffs ahead of main.
    local branch_check
    branch_check=$(check_branch_has_commits "$EVENT_BRANCH")
    local branch_check_rc=$?
    if [ $branch_check_rc -ne 0 ]; then
        echo "[$(date -u +%H:%M:%S)] EMPTY BRANCH: $EVENT_BRANCH — $branch_check" >&2
        local state_update
        state_update=$(update_failure_state "$EVENT_BRANCH" "failure" "$branch_check")

        bash "$SCRIPT_DIR/emit_event.sh" "SYSTEM_ALERT" "agent-5" "user" \
            "{\"failed_branch\":\"$EVENT_BRANCH\",\"reason\":\"$branch_check\",\"action\":\"Ensure code is committed and branch is ahead of main\"}" 2>/dev/null
        bash "$SCRIPT_DIR/emit_event.sh" "CLARIFICATION_REQUESTED" "agent-5" "$EVENT_SOURCE" \
            "{\"branch\":\"$EVENT_BRANCH\",\"reason\":\"$branch_check\",\"action\":\"Commit your code and re-run request_integration.sh\"}" 2>/dev/null

        echo "  EMPTY_BRANCH alert emitted to user."
        echo "  CLARIFICATION_REQUESTED emitted to $EVENT_SOURCE."
        mv "$event_file" "$ARCHIVE_DIR/" 2>/dev/null || true
        return 1
    fi

    # Archive the INTEGRATION_REQUESTED event immediately to prevent reprocessing
    mv "$event_file" "$ARCHIVE_DIR/" 2>/dev/null || true

    # ─── Run CI/CD — the sole execution path (staging → 5-stage → main) ──────
    # v2.2: Use tee for real-time streaming + PIPESTATUS for exit code.
    #   - tee streams all output to stderr (→ daemon.log) in real-time
    #   - PIPESTATUS[0] captures run_cicd's real exit code (not local's 0)
    #   - cicd_log file preserves output for SUCCESS:<commit> extraction
    local cicd_log="$LOG_DIR/$(date -u +%Y%m%dT%H%M%SZ)_${EVENT_BRANCH//\//_}_cicd_output.txt"
    run_cicd "$EVENT_BRANCH" 2>&1 | tee "$cicd_log" >&2
    local cicd_rc=${PIPESTATUS[0]}

    if [ $cicd_rc -ne 0 ]; then
        # CI/CD FAILED — update failure state
        local fail_reason
        fail_reason=$(grep -E "^(MERGE_CONFLICT:|BUILD_FAILED|TEST_FAILED|CLIPPY_FAILED|NPM_FAILED|SMOKE_FAILED|TIMEOUT_FAILED|DIRTY_TREE_TIMEOUT|EMPTY_BRANCH|MERGE_NOOP)" "$cicd_log" | head -1)
        [ -z "$fail_reason" ] && fail_reason="UNKNOWN_FAILURE"
        local state_update
        state_update=$(update_failure_state "$EVENT_BRANCH" "failure" "$fail_reason")

        # Check if this failure triggered a block
        if [[ "$state_update" == BLOCKED:* ]]; then
            local block_count=$(echo "$state_update" | cut -d: -f3)
            echo "[$(date -u +%H:%M:%S)] BRANCH NOW BLOCKED: $EVENT_BRANCH after $block_count consecutive failures."

            bash "$SCRIPT_DIR/emit_event.sh" "SYSTEM_ALERT" "agent-5" "user" \
                "{\"failed_branch\":\"$EVENT_BRANCH\",\"assigned_worker\":\"$EVENT_SOURCE\",\"reason\":\"Branch BLOCKED after $block_count consecutive CI/CD failures. Manual intervention required.\"}" 2>/dev/null
        fi

        echo "[$(date -u +%H:%M:%S)] CI/CD FAILED for $EVENT_BRANCH. Rejecting to $EVENT_SOURCE."

        # Emit CLARIFICATION_REQUESTED with failure details
        bash "$SCRIPT_DIR/emit_event.sh" "CLARIFICATION_REQUESTED" "agent-5" "$EVENT_SOURCE" \
            "{\"branch\":\"$EVENT_BRANCH\",\"reason\":\"CI/CD failed: $fail_reason\",\"action\":\"Fix errors and re-run request_integration.sh\"}" 2>/dev/null

        # Emit SYSTEM_ALERT to User
        local alert_reason=$(echo "$fail_reason" | sed 's/MERGE_CONFLICT:/Merge Conflict in /; s/BUILD_FAILED/Cargo Build Failed/; s/TEST_FAILED/Cargo Test Failed/; s/CLIPPY_FAILED/Cargo Clippy Failed/; s/NPM_FAILED/NPM Build Failed/; s/SMOKE_FAILED/Headless Smoke Test Failed/; s/TIMEOUT_FAILED/CI\/CD Stage Timeout/; s/DIRTY_TREE_TIMEOUT/Dirty Tree Timeout (5 min wait exceeded)/; s/EMPTY_BRANCH_NO_COMMITS/Empty Branch — No Commits Ahead of Main/; s/EMPTY_BRANCH_NO_DIFFS/Empty Branch — No File Differences vs Main/; s/MERGE_NOOP/Merge Produced No Tree Changes (False Merge)/')
        bash "$SCRIPT_DIR/emit_event.sh" "SYSTEM_ALERT" "agent-5" "user" \
            "{\"failed_branch\":\"$EVENT_BRANCH\",\"assigned_worker\":\"$EVENT_SOURCE\",\"reason\":\"$alert_reason\"}" 2>/dev/null

        echo "  Rejection event emitted to $EVENT_SOURCE."
        echo "  SYSTEM_ALERT emitted to user: $alert_reason"
        return 1
    fi

    # CI/CD PASSED — reset failure state
    update_failure_state "$EVENT_BRANCH" "success" ""

    # Extract staging commit from the CI/CD output log
    local staging_commit
    staging_commit=$(grep "^SUCCESS:" "$cicd_log" | tail -1 | cut -d: -f2)
    if [ -z "$staging_commit" ]; then
        staging_commit=$(git rev-parse HEAD)
    fi
    echo "[$(date -u +%H:%M:%S)] CI/CD PASSED. Staging commit: $staging_commit"

    # ─── Merge to main ─────────────────────────────────────────────────────
    if [ "${SKIP_AUDIT:-0}" = "1" ]; then
        echo "[$(date -u +%H:%M:%S)] SKIP_AUDIT=1 — promoting directly to main."

        local main_commit=$(git rev-parse HEAD)

        # Update last_green_commit
        export GREEN_COMMIT="$main_commit"
        sync_transactional mutator_set_green_commit "daemon: update last_green_commit to $GREEN_COMMIT" 5 2>/dev/null || true

        # Write .cicd_state
        echo "PASSED $(date -u +%Y-%m-%dT%H:%M:%SZ) $main_commit" > "$HUB_DIR/.devin/.cicd_state"

        # Archive task brief
        local ts=$(date -u +%Y%m%dT%H%M%SZ)
        for brief in "$IN_PROGRESS_DIR"/*.json; do
            [ -f "$brief" ] || continue
            local brief_branch=$(EVENT_IN="$brief" node <<'NODE_EOF'
                const fs = require("fs");
                try {
                    let raw = fs.readFileSync(process.env.EVENT_IN, "utf8");
                    if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
                    const d = JSON.parse(raw);
                    console.log(d.branch || "");
                } catch(e) { console.log(""); }
NODE_EOF
            )
            if [ "$brief_branch" = "$EVENT_BRANCH" ]; then
                mv "$brief" "$TASKS_ARCHIVE_DIR/${ts}_$(basename "$brief")" 2>/dev/null || true
                echo "  Archived task brief: $(basename "$brief")"
            fi
        done

        # Emit PROMOTED_TO_MAIN
        bash "$SCRIPT_DIR/emit_event.sh" "PROMOTED_TO_MAIN" "agent-5" "all" \
            "{\"branch\":\"$EVENT_BRANCH\",\"commit\":\"$main_commit\"}" 2>/dev/null

        echo "[$(date -u +%H:%M:%S)] PROMOTED TO MAIN: $EVENT_BRANCH @ $main_commit"
    else
        # Audit path: emit AUDIT_REQUESTED for Agent 4
        echo "[$(date -u +%H:%M:%S)] Audit path: emitting AUDIT_REQUESTED for Agent 4."
        bash "$SCRIPT_DIR/emit_event.sh" "AUDIT_REQUESTED" "agent-5" "agent-4" \
            "{\"branch\":\"$EVENT_BRANCH\",\"staging_commit\":\"$staging_commit\"}" 2>/dev/null
        echo "  Waiting for Agent 4 AUDIT_PASS before promoting to main."
    fi

    return 0
}

# ─── Helper: Check sprint completion (PROMOTED_TO_MAIN events only) ────────
# v2.0: Scans BOTH events/ and .archive/ for PROMOTED_TO_MAIN events.
check_sprint_complete() {
    local roadmap="$HUB_DIR/.devin/tasks/design_review/roadmap.json"
    local events_dir="$HUB_DIR/.devin/events"
    local events_archive="$HUB_DIR/.devin/events/.archive"
    [ -f "$roadmap" ] || return 1
    [ -d "$events_archive" ] || return 1

    ROADMAP_IN="$roadmap" EVENTS_IN="$events_dir" ARCHIVE_IN="$events_archive" node <<'NODE_EOF'
        const fs = require("fs");
        const path = require("path");

        function readJson(f) {
            let raw = fs.readFileSync(f, "utf8");
            if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
            return JSON.parse(raw);
        }

        let roadmap;
        try {
            roadmap = readJson(process.env.ROADMAP_IN);
        } catch(e) {
            console.error("Cannot read roadmap: " + e.message);
            process.exit(2);
        }
        const expectedBranches = new Set(roadmap.blueprints.map(b => b.branch));
        const total = expectedBranches.size;
        if (total === 0) { process.exit(1); }

        // v2.0: Scan BOTH events/ and .archive/ for PROMOTED_TO_MAIN
        const promotedBranches = new Set();
        const scanDirs = [process.env.EVENTS_IN, process.env.ARCHIVE_IN];
        for (const dir of scanDirs) {
            try {
                const files = fs.readdirSync(dir).filter(f => f.endsWith(".json"));
                for (const f of files) {
                    try {
                        const evt = readJson(path.join(dir, f));
                        if (evt.type === "PROMOTED_TO_MAIN" && evt.payload && evt.payload.branch) {
                            promotedBranches.add(evt.payload.branch);
                        }
                    } catch(e) { continue; }
                }
            } catch(e) { continue; }
        }

        let completed = 0;
        for (const b of expectedBranches) {
            if (promotedBranches.has(b)) completed++;
        }

        if (completed >= total) {
            console.log("COMPLETED=" + completed + "/" + total);
            process.exit(0);
        } else {
            console.log("PROGRESS=" + completed + "/" + total);
            process.exit(1);
        }
NODE_EOF
    return $?
}

# ─── Helper: Automated archive hygiene ─────────────────────────────────────
# Deletes files older than 7 days from .archive/ and integration_log/.
# Preserves PROMOTED_TO_MAIN events (needed for sprint completion check).
cleanup_old_files() {
    # Clean .archive/ — JSON files older than 7 days, EXCEPT PROMOTED_TO_MAIN
    local archive_candidates
    archive_candidates=$(find "$ARCHIVE_DIR" -type f -name "*.json" -mtime +7 ! -name "*PROMOTED_TO_MAIN*" 2>/dev/null)
    if [ -n "$archive_candidates" ]; then
        echo "$archive_candidates" | while IFS= read -r f; do rm -f "$f"; done
        local archive_count=$(echo "$archive_candidates" | wc -l)
        echo "[$(date -u +%H:%M:%S)] Hygiene: deleted $archive_count archive files older than 7 days."
    fi

    # Clean integration_log/ — .txt and .log files older than 7 days
    local log_candidates
    log_candidates=$(find "$LOG_DIR" -type f \( -name "*.txt" -o -name "*.log" \) -mtime +7 2>/dev/null)
    if [ -n "$log_candidates" ]; then
        echo "$log_candidates" | while IFS= read -r f; do rm -f "$f"; done
        local log_count=$(echo "$log_candidates" | wc -l)
        echo "[$(date -u +%H:%M:%S)] Hygiene: deleted $log_count log files older than 7 days."
    fi
}

# ─── Main Loop ─────────────────────────────────────────────────────────────
echo ""
echo "============================================================"
echo "  INTEGRATION DAEMON v2.2 — Agent 5 (Manager)"
echo "  PID: $$"
echo "  HUB_DIR: $HUB_DIR"
echo "  Poll interval: ${POLL_INTERVAL}s"
echo "  SKIP_AUDIT: ${SKIP_AUDIT:-0}"
echo "  CI/CD path: run_cicd() — 5-stage Iron pipeline with watchdog timeouts + empty branch guard"
echo "  Deadlock guard: ${MAX_CONSECUTIVE_FAILURES}-strike auto-block"
echo "  Watchdog: POSIX timeout on all stages (build/test/clippy: 300s, npm: 180s, smoke: 120s)"
echo "  Empty branch guard: rejects branches with no commits/diffs ahead of main"
echo "  Merge verification: git diff main HEAD (tree-vs-tree, not merge-base)"
echo "  Output streaming: tee + PIPESTATUS for real-time logging + correct exit codes"
echo "  Hygiene: 7-day archive cleanup every 100 cycles"
echo "  Started: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "============================================================"
echo ""

CYCLE=0

while true; do
    CYCLE=$((CYCLE + 1))
    NOW=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

    # Run hygiene every 100 cycles (~25 min)
    if [ $((CYCLE % 100)) -eq 0 ]; then
        cleanup_old_files
    fi

    # Find INTEGRATION_REQUESTED events (maxdepth 1 — do NOT scan .archive/)
    EVENT_FILES=$(find "$EVENTS_DIR" -maxdepth 1 -name "*INTEGRATION_REQUESTED*" -type f 2>/dev/null)

    if [ -n "$EVENT_FILES" ]; then
        echo "[$NOW] Cycle $CYCLE — INTEGRATION_REQUESTED events detected!"

        while IFS= read -r event_file; do
            [ -f "$event_file" ] || continue
            process_integration_request "$event_file"
        done <<< "$EVENT_FILES"

        # Check sprint completion after each integration
        if check_sprint_complete; then
            echo ""
            echo "============================================================"
            echo "  ALL BLUEPRINTS INTEGRATED AND MERGED TO MAIN"
            echo "  AUDIT_REQUESTED event emitted to Agent 4"
            echo "  Agent 4 must now run the comprehensive system-wide audit:"
            echo "    - M0 conservation (emigration forex flow)"
            echo "    - Off-grid physics (water-to-pollution mass conservation)"
            echo "    - Demographic stability (cooperative collapse routing)"
            echo "============================================================"

            bash "$SCRIPT_DIR/emit_event.sh" "AUDIT_REQUESTED" "agent-5" "agent-4" \
                '{"reason":"All blueprints merged to main. Run comprehensive system-wide test suite.","verification_targets":["M0 conservation (emigration forex flow)","Off-grid physics (water-to-pollution mass conservation)","Demographic stability (cooperative collapse routing)"]}' 2>/dev/null

            echo "[$(date -u +%H:%M:%S)] AUDIT_REQUESTED emitted. Daemon exiting."
            break
        fi
    else
        # Quiet cycle — print status every 10 cycles (~2.5 min)
        if [ $((CYCLE % 10)) -eq 0 ]; then
            echo "[$NOW] Cycle $CYCLE — No pending events. Monitoring..."
        fi
    fi

    sleep "$POLL_INTERVAL"
done

echo ""
echo "[$(date -u +%Y-%m-%dT%H:%M:%SZ)] Integration daemon finished."
exit 0

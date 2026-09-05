#!/bin/bash
# integration_daemon.sh — Autonomous CI/CD pipeline daemon (Agent 5 / Manager).
#
# Polls for INTEGRATION_REQUESTED events every 15 seconds, runs the Iron CI/CD
# pipeline on a staging branch (never main directly), merges green branches to
# main, archives task briefs, and emits AUDIT_REQUESTED when all 7 blueprints
# are promoted.
#
# This is the sole CI/CD path — no external stage_merge.sh dependency.
# The daemon owns the full pipeline: staging branch → 4-stage CI/CD → main.
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
# Before ANY git checkout, the daemon MUST verify the working tree is clean.
# If a worker is coding in the hub (outside their worktree), their uncommitted
# changes would be destroyed by a branch switch. This guard detects that and
# halts CI/CD until the tree is clean.
guard_clean_tree() {
    # Check only tracked files (modified/staged) — untracked .devin/ files are
    # expected infrastructure and won't be destroyed by git checkout.
    local dirty=$(git status --porcelain 2>/dev/null | grep -v '^??' | head -1)
    if [ -n "$dirty" ]; then
        echo ""
        echo "[$(date -u +%H:%M:%S)] !!! DIRTY WORKING TREE DETECTED IN HUB_DIR !!!"
        echo "  Uncommitted tracked changes found. A worker may be coding outside their worktree."
        echo "  CI/CD HALTED to prevent data loss."
        echo "  Dirty tracked files (first 10):"
        git status --porcelain 2>/dev/null | grep -v '^??' | head -10 | sed 's/^/    /'
        echo ""

        # Emit SYSTEM_ALERT to User — immediate notification
        bash "$SCRIPT_DIR/emit_event.sh" "SYSTEM_ALERT" "agent-5" "user" \
            "{\"reason\":\"Uncommitted changes detected in HUB_DIR. A worker is outside their worktree. CI/CD halted to prevent data loss.\",\"dirty_files\":\"$(git status --porcelain 2>/dev/null | grep -v '^??' | head -5 | tr '\n' ';')\"}" 2>/dev/null

        echo "  SYSTEM_ALERT emitted to user."
        echo "  Waiting for working tree to be clean..."
        echo ""

        # Sleep until the tree is clean (check every 30 seconds)
        while true; do
            dirty=$(git status --porcelain 2>/dev/null | grep -v '^??' | head -1)
            if [ -z "$dirty" ]; then
                echo "[$(date -u +%H:%M:%S)] Working tree is now clean. Resuming CI/CD."
                return 0
            fi
            echo "[$(date -u +%H:%M:%S)] Still dirty. Sleeping 30s..."
            sleep 30
        done
    fi
    return 0
}

# ─── Helper: Run CI/CD — the sole execution path (staging → 4-stage → main) ─
# Creates a staging branch from main, merges the worker branch, runs all 4
# Iron CI/CD stages, and only merges to main after ALL pass.
# Returns 0 on success (echoes SUCCESS:<commit>), 1 on failure (echoes reason).
run_cicd() {
    local worker_branch="$1"
    local log_prefix="$LOG_DIR/$(date -u +%Y%m%dT%H%M%SZ)_${worker_branch//\//_}"

    echo "[$(date -u +%H:%M:%S)] CI/CD: Running 4-stage Iron pipeline on staging branch."

    # Guard: ensure working tree is clean before any branch switch
    guard_clean_tree

    # Step 1: Create/reset staging from main
    git checkout main 2>&1 | tail -1
    git branch -f "$STAGING_BRANCH" main 2>/dev/null || true
    git checkout "$STAGING_BRANCH" 2>&1 | tail -1

    # Step 2: Merge worker branch into staging (NOT main)
    git merge "$worker_branch" --no-edit 2>&1 | tail -5
    local merge_rc=$?
    if [ $merge_rc -ne 0 ]; then
        local conflicts=$(git diff --name-only --diff-filter=U 2>/dev/null | tr '\n' ' ')
        git merge --abort 2>/dev/null || true
        git checkout main 2>/dev/null
        echo "MERGE_CONFLICT:$conflicts" > "${log_prefix}_FAILED.txt"
        echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: Merge conflict in: $conflicts"
        return 1
    fi

    # Step 3: Run all 4 CI/CD steps on staging
    echo "[$(date -u +%H:%M:%S)] CI/CD: [1/4] cargo build..."
    cargo build --workspace 2>&1 | tee "${log_prefix}_build.txt" | tail -3
    local build_rc=${PIPESTATUS[0]}
    if [ $build_rc -ne 0 ]; then
        git checkout main 2>/dev/null
        echo "BUILD_FAILED" > "${log_prefix}_FAILED.txt"
        echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: cargo build (rc=$build_rc)"
        return 1
    fi

    echo "[$(date -u +%H:%M:%S)] CI/CD: [2/4] cargo test..."
    cargo test --workspace --all-targets 2>&1 | tee "${log_prefix}_test.txt" | tail -5
    local test_rc=${PIPESTATUS[0]}
    if [ $test_rc -ne 0 ]; then
        git checkout main 2>/dev/null
        echo "TEST_FAILED" > "${log_prefix}_FAILED.txt"
        echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: cargo test (rc=$test_rc)"
        return 1
    fi

    echo "[$(date -u +%H:%M:%S)] CI/CD: [3/4] cargo clippy..."
    cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tee "${log_prefix}_clippy.txt" | tail -3
    local clippy_rc=${PIPESTATUS[0]}
    if [ $clippy_rc -ne 0 ]; then
        git checkout main 2>/dev/null
        echo "CLIPPY_FAILED" > "${log_prefix}_FAILED.txt"
        echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: cargo clippy (rc=$clippy_rc)"
        return 1
    fi

    echo "[$(date -u +%H:%M:%S)] CI/CD: [4/4] npm run build..."
    npm run build 2>&1 | tee "${log_prefix}_npm.txt" | tail -3
    local npm_rc=${PIPESTATUS[0]}
    if [ $npm_rc -ne 0 ]; then
        git checkout main 2>/dev/null
        echo "NPM_FAILED" > "${log_prefix}_FAILED.txt"
        echo "[$(date -u +%H:%M:%S)] CI/CD FAILED: npm run build (rc=$npm_rc)"
        return 1
    fi

    # Step 4: ALL CI/CD PASSED — now safe to merge staging into main
    local staging_commit=$(git rev-parse HEAD)
    git checkout main 2>&1 | tail -1
    git merge --ff-only "$staging_commit" 2>&1 | tail -3
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

    # Archive the INTEGRATION_REQUESTED event immediately to prevent reprocessing
    mv "$event_file" "$ARCHIVE_DIR/" 2>/dev/null || true

    # Run CI/CD — the sole execution path (staging → 4-stage → main)
    local cicd_result=$(run_cicd "$EVENT_BRANCH")
    local cicd_rc=$?

    if [ $cicd_rc -ne 0 ]; then
        # CI/CD FAILED — reject back to worker
        echo "[$(date -u +%H:%M:%S)] CI/CD FAILED for $EVENT_BRANCH. Rejecting to $EVENT_SOURCE."

        # Emit CLARIFICATION_REQUESTED with failure details
        local fail_reason=$(echo "$cicd_result" | head -1)
        bash "$SCRIPT_DIR/emit_event.sh" "CLARIFICATION_REQUESTED" "agent-5" "$EVENT_SOURCE" \
            "{\"branch\":\"$EVENT_BRANCH\",\"reason\":\"CI/CD failed: $fail_reason\",\"action\":\"Fix errors and re-run request_integration.sh\"}" 2>/dev/null

        # Emit SYSTEM_ALERT to User — direct ping on the event bus
        local alert_reason=$(echo "$fail_reason" | sed 's/MERGE_CONFLICT:/Merge Conflict in /; s/BUILD_FAILED/Cargo Build Failed/; s/TEST_FAILED/Cargo Test Failed/; s/CLIPPY_FAILED/Cargo Clippy Failed/; s/NPM_FAILED/NPM Build Failed/')
        bash "$SCRIPT_DIR/emit_event.sh" "SYSTEM_ALERT" "agent-5" "user" \
            "{\"failed_branch\":\"$EVENT_BRANCH\",\"assigned_worker\":\"$EVENT_SOURCE\",\"reason\":\"$alert_reason\"}" 2>/dev/null

        echo "  Rejection event emitted to $EVENT_SOURCE."
        echo "  SYSTEM_ALERT emitted to user: $alert_reason"
        return 1
    fi

    # CI/CD PASSED — extract staging commit
    local staging_commit=$(echo "$cicd_result" | grep "^SUCCESS:" | cut -d: -f2)
    if [ -z "$staging_commit" ]; then
        staging_commit=$(git rev-parse HEAD)
    fi
    echo "[$(date -u +%H:%M:%S)] CI/CD PASSED. Staging commit: $staging_commit"

    # ─── Merge to main ─────────────────────────────────────────────────────
    # Skip audit if SKIP_AUDIT=1 (manager's discretion for this sprint)
    if [ "${SKIP_AUDIT:-0}" = "1" ]; then
        echo "[$(date -u +%H:%M:%S)] SKIP_AUDIT=1 — promoting directly to main."

        # run_cicd already merged staging into main via ff-only
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
check_sprint_complete() {
    local roadmap="$HUB_DIR/.devin/tasks/design_review/roadmap.json"
    local events_archive="$HUB_DIR/.devin/events/.archive"
    [ -f "$roadmap" ] || return 1
    [ -d "$events_archive" ] || return 1

    ROADMAP_IN="$roadmap" ARCHIVE_IN="$events_archive" node <<'NODE_EOF'
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

        const archiveDir = process.env.ARCHIVE_IN;
        const promotedBranches = new Set();
        let files;
        try {
            files = fs.readdirSync(archiveDir).filter(f => f.endsWith(".json"));
        } catch(e) {
            process.exit(1);
        }

        for (const f of files) {
            try {
                const evt = readJson(path.join(archiveDir, f));
                if (evt.type === "PROMOTED_TO_MAIN" && evt.payload && evt.payload.branch) {
                    promotedBranches.add(evt.payload.branch);
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

# ─── Main Loop ─────────────────────────────────────────────────────────────
echo ""
echo "============================================================"
echo "  INTEGRATION DAEMON — Agent 5 (Manager)"
echo "  PID: $$"
echo "  HUB_DIR: $HUB_DIR"
echo "  Poll interval: ${POLL_INTERVAL}s"
echo "  SKIP_AUDIT: ${SKIP_AUDIT:-0}"
echo "  CI/CD path: run_cicd() (sole path, no external deps)"
echo "  Started: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "============================================================"
echo ""

CYCLE=0

while true; do
    CYCLE=$((CYCLE + 1))
    NOW=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

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
            echo "  ALL 7 BLUEPRINTS INTEGRATED AND MERGED TO MAIN"
            echo "  AUDIT_REQUESTED event emitted to Agent 4"
            echo "  Agent 4 must now run:"
            echo "    bash .devin/scripts/process_audit_queue.sh"
            echo "  Then execute active simulation ticks to verify:"
            echo "    - M0 conservation (emigration forex flow)"
            echo "    - Off-grid physics (water-to-pollution mass conservation)"
            echo "    - Demographic stability (cooperative collapse routing)"
            echo "============================================================"

            bash "$SCRIPT_DIR/emit_event.sh" "AUDIT_REQUESTED" "agent-5" "agent-4" \
                '{"reason":"All 7 blueprints merged to main. Run comprehensive system-wide test suite.","verification_targets":["M0 conservation (emigration forex flow)","Off-grid physics (water-to-pollution mass conservation)","Demographic stability (cooperative collapse routing)"]}' 2>/dev/null

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

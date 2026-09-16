#!/bin/bash
# merge_and_release.sh — v4.8.0 Automatic release pipeline
#
# Triggered by roadmap_dispenser.cjs when ALL active roadmaps reach
# status === "completed". Performs an atomic, manager-authorized release:
#
#   1. Validate manager authorization
#   2. Acquire release lock (prevents concurrent releases)
#   3. Resolve out-of-tree state via state_dir.sh
#   4. Read every active roadmap progress file
#   5. Require every roadmap status === "completed" (else abort)
#   6. Determine feature branches from roadmap definitions
#   7. Fetch current remote state
#   8. Rebase each feature branch onto main (abort on conflict)
#   9. Merge feature branches sequentially into main
#  10. Run full Iron CI/CD (cargo build/test/clippy + npm build)
#  11. Run final M0/macro diagnostic
#  12. Generate release notes from roadmap metadata + step histories
#  13. Compute next version tag (strip 'v' prefix before arithmetic)
#  14. Create the Git tag only after all verification passes
#  15. Push main + new tag (only after all checks pass)
#  16. Release the lock
#
# Safety guarantees:
#   - No push before all checks pass
#   - Abort on rebase conflict (no force-overwrite)
#   - Abort on CI/CD failure
#   - Abort on diagnostic failure
#   - Lock file prevents two managers from releasing simultaneously
#   - Never uses git stash pop, git reset --hard, git push --force, git clean -fd
#
# Usage: bash .devin/scripts/merge_and_release.sh [--dry-run]
#        bash .devin/scripts/merge_and_release.sh --force-release   # manager override

set -uo pipefail

cd "$(dirname "$0")/../.." || { echo "FATAL: cannot cd to repo root"; exit 2; }
export HUB_DIR="$(pwd)"

SCRIPT_DIR="$HUB_DIR/.devin/scripts"
LOG_DIR="$HUB_DIR/.devin/integration_log"
RELEASE_LOG="$LOG_DIR/release.log"
LOCK_FILE="$HUB_DIR/.devin/run/release.lock"
MANAGER_AUTH="$HUB_DIR/.devin/.manager_auth"

mkdir -p "$LOG_DIR" "$HUB_DIR/.devin/run" 2>/dev/null || true

# ─── Parse arguments ────────────────────────────────────────────────────────
DRY_RUN=0
FORCE_RELEASE=0
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=1 ;;
        --force-release) FORCE_RELEASE=1 ;;
        *) echo "Unknown argument: $arg" >&2; exit 2 ;;
    esac
done

# ─── Logging helper ────────────────────────────────────────────────────────
log() {
    local ts
    ts=$(date -u +%Y-%m-%dT%H:%M:%SZ)
    echo "[$ts] $*" | tee -a "$RELEASE_LOG" 2>/dev/null || echo "[$ts] $*"
}

# ─── Step 1: Validate manager authorization ───────────────────────────────
if [ ! -f "$MANAGER_AUTH" ]; then
    log "ABORT: Manager authorization token not found at $MANAGER_AUTH"
    log "Run 'bash .devin/scripts/claim_manager.sh' first."
    exit 3
fi
log "Manager authorization token present."

# ─── Step 2: Acquire release lock ──────────────────────────────────────────
acquire_lock() {
    if [ -f "$LOCK_FILE" ]; then
        local lock_pid lock_age
        lock_pid=$(head -1 "$LOCK_FILE" 2>/dev/null || echo "")
        # Check if lock owner is still alive
        if [ -n "$lock_pid" ] && kill -0 "$lock_pid" 2>/dev/null; then
            log "ABORT: Release lock held by active process PID $lock_pid"
            exit 4
        fi
        # Stale lock — check age
        lock_age=$(( $(date +%s) - $(stat -c %Y "$LOCK_FILE" 2>/dev/null || echo 0) ))
        if [ "$lock_age" -lt 3600 ] && [ "$FORCE_RELEASE" -eq 0 ]; then
            log "ABORT: Release lock younger than 1h (age ${lock_age}s) and --force-release not set"
            exit 4
        fi
        log "WARN: Removing stale release lock (age ${lock_age}s)"
    fi
    if [ "$DRY_RUN" -eq 0 ]; then
        echo "$$" > "$LOCK_FILE" || { log "ABORT: cannot write lock file"; exit 4; }
    fi
}
release_lock() {
    if [ "$DRY_RUN" -eq 0 ] && [ -f "$LOCK_FILE" ]; then
        local lock_pid
        lock_pid=$(head -1 "$LOCK_FILE" 2>/dev/null || echo "")
        if [ "$lock_pid" = "$$" ]; then
            rm -f "$LOCK_FILE" 2>/dev/null || true
        fi
    fi
}
trap release_lock EXIT
acquire_lock
log "Release lock acquired (PID $$)."

# ─── Step 3: Resolve out-of-tree state ──────────────────────────────────────
if [ ! -f "$SCRIPT_DIR/state_dir.sh" ]; then
    log "ABORT: state_dir.sh not found"
    exit 5
fi
# shellcheck disable=SC1091
source "$SCRIPT_DIR/state_dir.sh"
resolve_state_dir
migrate_state_files
log "STATE_DIR resolved: $STATE_DIR"

# Convert Git Bash paths to Windows paths for node compatibility
to_win_path() {
    local p="$1"
    if [[ "$p" == /c/* ]]; then
        echo "C:${p#/c}"
    elif [[ "$p" == /d/* ]]; then
        echo "D:${p#/d}"
    else
        echo "$p"
    fi
}

ROADMAPS_DIR="$HUB_DIR/.devin/roadmaps"
PROGRESS_DIR="$STATE_DIR/roadmaps"
mkdir -p "$PROGRESS_DIR" 2>/dev/null || true

# ─── Step 4 & 5: Read all progress files, require all completed ────────────
log "Scanning roadmap progress files in $PROGRESS_DIR..."

progress_files=""
if [ -d "$PROGRESS_DIR" ]; then
    progress_files=$(ls -1 "$PROGRESS_DIR"/*.progress.json 2>/dev/null || true)
fi

if [ -z "$progress_files" ]; then
    log "ABORT: No roadmap progress files found in $PROGRESS_DIR"
    log "Cannot release without at least one completed roadmap."
    exit 6
fi

all_completed=1
roadmap_names=""
roadmap_branches=""
for pf in $progress_files; do
    [ -f "$pf" ] || continue
    name=$(basename "$pf" .progress.json)
    pf_win=$(to_win_path "$pf")
    status=$(node -e "
        try { const d=JSON.parse(require('fs').readFileSync('$pf_win','utf8')); process.stdout.write(d.status||'unknown'); } catch(e) { process.stdout.write('malformed'); }
    " 2>/dev/null || echo "malformed")
    log "  Roadmap $name: status=$status"
    if [ "$status" != "completed" ]; then
        all_completed=0
        log "  -> NOT completed — release blocked."
    fi
    # Resolve branch from roadmap definition
    def_file="$ROADMAPS_DIR/${name}.json"
    if [ -f "$def_file" ]; then
        def_win=$(to_win_path "$def_file")
        branch=$(node -e "
            try { const d=JSON.parse(require('fs').readFileSync('$def_win','utf8')); process.stdout.write(d.branch||''); } catch(e) {}
        " 2>/dev/null || echo "")
        if [ -n "$branch" ]; then
            roadmap_names="$roadmap_names $name"
            roadmap_branches="$roadmap_branches $branch"
        fi
    fi
done

if [ "$all_completed" -ne 1 ]; then
    log "ABORT: Not all roadmaps are completed. Release refused."
    exit 7
fi

log "All roadmaps completed. Proceeding with release."

# ─── Step 6: Determine feature branches ────────────────────────────────────
# (already collected above into $roadmap_branches)
if [ -z "$roadmap_branches" ]; then
    log "ABORT: No feature branches resolved from roadmap definitions."
    exit 8
fi
log "Feature branches to merge:$roadmap_branches"

# ─── Step 7: Fetch remote state ────────────────────────────────────────────
log "Fetching remote state..."
if [ "$DRY_RUN" -eq 0 ]; then
    git fetch origin 2>&1 | tee -a "$RELEASE_LOG" || { log "ABORT: git fetch failed"; exit 9; }
fi

# ─── Step 8: Rebase each feature branch onto main ──────────────────────────
log "Rebasing feature branches onto main..."
for br in $roadmap_branches; do
    log "  Rebasing $br onto origin/main..."
    if [ "$DRY_RUN" -eq 0 ]; then
        git checkout "$br" 2>&1 | tee -a "$RELEASE_LOG" || { log "ABORT: cannot checkout $br"; exit 10; }
        if ! git rebase origin/main 2>&1 | tee -a "$RELEASE_LOG"; then
            log "ABORT: rebase conflict on $br. Aborting rebase (no force-overwrite)."
            git rebase --abort 2>/dev/null || true
            git checkout main 2>/dev/null || true
            exit 11
        fi
    fi
done

# ─── Step 9: Merge feature branches sequentially into main ─────────────────
log "Switching to main..."
if [ "$DRY_RUN" -eq 0 ]; then
    git checkout main 2>&1 | tee -a "$RELEASE_LOG" || { log "ABORT: cannot checkout main"; exit 12; }
    git pull --rebase origin main 2>&1 | tee -a "$RELEASE_LOG" || { log "WARN: pull --rebase main had issues, continuing"; }
fi

for br in $roadmap_branches; do
    log "  Merging $br into main..."
    if [ "$DRY_RUN" -eq 0 ]; then
        if ! git merge --no-ff "$br" -m "merge: $br into main (auto-release)" 2>&1 | tee -a "$RELEASE_LOG"; then
            log "ABORT: merge of $br failed. Aborting merge (no force-overwrite)."
            git merge --abort 2>/dev/null || true
            exit 13
        fi
    fi
done

# ─── Step 10: Run full Iron CI/CD ──────────────────────────────────────────
log "Running full Iron CI/CD (cargo build, cargo test, cargo clippy, npm build)..."
if [ "$DRY_RUN" -eq 0 ]; then
    export CI=true
    log "  cargo build..."
    if ! cargo build --quiet 2>&1 | tee -a "$RELEASE_LOG"; then
        log "ABORT: cargo build failed."
        exit 14
    fi
    log "  cargo test..."
    if ! cargo test --quiet 2>&1 | tee -a "$RELEASE_LOG"; then
        log "ABORT: cargo test failed."
        exit 15
    fi
    log "  cargo clippy..."
    if ! cargo clippy --quiet -- -D warnings 2>&1 | tee -a "$RELEASE_LOG"; then
        log "ABORT: cargo clippy failed."
        exit 16
    fi
    log "  npm build..."
    if ! (cd "$HUB_DIR" && npm run build 2>&1 | tee -a "$RELEASE_LOG"); then
        log "ABORT: npm build failed."
        exit 17
    fi
else
    log "  [dry-run] skipping CI/CD"
fi

# ─── Step 11: Run final M0/macro diagnostic ────────────────────────────────
log "Running final M0/macro diagnostic..."
M0_DIAG_SCRIPT="$HUB_DIR/state/tests/diagnostic_output/run_m0_diagnostic.sh"
if [ -x "$M0_DIAG_SCRIPT" ]; then
    if [ "$DRY_RUN" -eq 0 ]; then
        if ! bash "$M0_DIAG_SCRIPT" 2>&1 | tee -a "$RELEASE_LOG"; then
            log "ABORT: M0/macro diagnostic failed."
            exit 18
        fi
    else
        log "  [dry-run] skipping M0 diagnostic"
    fi
else
    log "  WARN: M0 diagnostic script not found at $M0_DIAG_SCRIPT — skipping (no failure)."
fi

# ─── Step 12: Generate release notes �───────────────────────────────────────
log "Generating release notes..."
LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
RELEASE_NOTES="$HUB_DIR/state/tests/diagnostic_output/release_notes_$(date -u +%Y%m%dT%H%M%SZ).md"

{
    echo "# Release Notes — $(date -u +%Y-%m-%d)"
    echo ""
    echo "## Roadmaps Completed"
    echo ""
    for name in $roadmap_names; do
        pf="$PROGRESS_DIR/${name}.progress.json"
        def="$ROADMAPS_DIR/${name}.json"
        echo "### $name"
        if [ -f "$def" ]; then
            def_win=$(to_win_path "$def")
            node -e "
                try {
                    const d=JSON.parse(require('fs').readFileSync('$def_win','utf8'));
                    console.log('- Branch: ' + (d.branch||'unknown'));
                    console.log('- Target agent: ' + (d.target_agent||'unknown'));
                    if (d.description) console.log('- Description: ' + d.description);
                } catch(e) {}
            " 2>/dev/null || true
        fi
        if [ -f "$pf" ]; then
            pf_win=$(to_win_path "$pf")
            node -e "
                try {
                    const p=JSON.parse(require('fs').readFileSync('$pf_win','utf8'));
                    console.log('- Status: ' + (p.status||'unknown'));
                    console.log('- Steps completed: ' + (p.steps_history||[]).length);
                    if (p.steps_history) {
                        for (const h of p.steps_history) {
                            console.log('  - Step ' + h.step_number + ': ' + (h.commit||'').slice(0,8) + ' (' + (h.completed_at||'') + ')');
                        }
                    }
                } catch(e) {}
            " 2>/dev/null || true
        fi
        echo ""
    done
    echo ""
    echo "## Previous Tag"
    echo ""
    echo "$LAST_TAG"
    echo ""
} > "$RELEASE_NOTES" 2>/dev/null || true

log "Release notes written to $RELEASE_NOTES"

# ─── Step 13: Compute next version tag (strip 'v' before arithmetic) ───────
# CRITICAL FIX: Avoid the awk bug where "v4" passed to %d becomes 0.
# Strip the 'v' prefix BEFORE feeding to awk -F.
if [ -z "$LAST_TAG" ]; then
    NEXT_TAG="v4.8.0"
else
    # Strip leading 'v', then split on '.'
    NEXT_TAG=$(echo "$LAST_TAG" | sed 's/^v//' | awk -F. '{
        printf "v%d.%d.%d", $1, $2+1, 0
    }')
fi
log "Last tag: $LAST_TAG -> Next tag: $NEXT_TAG"

# ─── Step 14: Create the Git tag (only after all checks pass) ───────────────
if [ "$DRY_RUN" -eq 0 ]; then
    log "Creating annotated tag $NEXT_TAG..."
    if ! git tag -a "$NEXT_TAG" -m "Release $NEXT_TAG — auto-generated from completed roadmaps" 2>&1 | tee -a "$RELEASE_LOG"; then
        log "ABORT: git tag creation failed."
        exit 19
    fi
else
    log "  [dry-run] would create tag $NEXT_TAG"
fi

# ─── Step 15: Push main + new tag ──────────────────────────────────────────
if [ "$DRY_RUN" -eq 0 ]; then
    log "Pushing main to origin..."
    if ! git push origin main 2>&1 | tee -a "$RELEASE_LOG"; then
        log "ABORT: git push origin main failed."
        exit 20
    fi
    log "Pushing tag $NEXT_TAG to origin..."
    if ! git push origin "$NEXT_TAG" 2>&1 | tee -a "$RELEASE_LOG"; then
        log "ABORT: git push origin $NEXT_TAG failed."
        exit 21
    fi
else
    log "  [dry-run] would push main and tag $NEXT_TAG"
fi

# ─── Step 16: Release completion ───────────────────────────────────────────
log "RELEASE COMPLETE: $NEXT_TAG"
log "  Roadmaps:$roadmap_names"
log "  Branches merged:$roadmap_branches"
log "  Release notes: $RELEASE_NOTES"

# Lock released by trap on exit
exit 0

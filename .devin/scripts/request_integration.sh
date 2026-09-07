#!/bin/bash
# request_integration.sh v3 — Worker: signal integration readiness via event bus.
#
# Usage: bash .devin/scripts/request_integration.sh "<description>"
#
# v3 Behavior:
#   1. Security check (must be in worktree, not HUB_DIR)
#   2. v3: Clean tree verification — abort if dirty (before rebase)
#   3. v3: Shift-left main sync — fetch + rebase onto origin/main
#   4. v3: sccache enabled (2G limit)
#   5. Pre-integration guard: cargo check + test + clippy + npm
#   6. Emit INTEGRATION_REQUESTED event to hub
#
# The manager polls events or checks .devin/events/ at session start.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/sync_lib.sh"

# ─── Detect hub directory for sccache path ─────────────────────────────────
detect_hub_dir() {
    local dir="${DEVIN_PROJECT_DIR:-$(pwd)}"
    while [ "$dir" != "/" ] && [ -n "$dir" ]; do
        case "$dir" in
            */worktrees/*) ;;
            *)
                if [ -d "$dir/.devin/events" ]; then
                    echo "$dir"
                    return 0
                fi
                ;;
        esac
        dir="$(cd "$dir/.." 2>/dev/null && pwd)"
    done
    local parent
    parent="$(cd "${DEVIN_PROJECT_DIR:-$(pwd)}/.." 2>/dev/null && pwd)"
    if [ -d "$parent/SillyElaborateState/.devin" ]; then
        echo "$parent/SillyElaborateState"
        return 0
    fi
    echo "${DEVIN_PROJECT_DIR:-$(pwd)}"
}
HUB_DIR="$(detect_hub_dir)"

# ─── v3: sccache compilation cache ─────────────────────────────────────────
if command -v sccache &>/dev/null; then
    export RUSTC_WRAPPER="sccache"
    export SCCACHE_DIR="${SCCACHE_DIR:-$HUB_DIR/.devin/.sccache}"
    export SCCACHE_CACHE_SIZE="2G"
    mkdir -p "$SCCACHE_DIR" 2>/dev/null || true
    echo "[sccache] Enabled (cache: $SCCACHE_DIR, limit: 2G)"
fi

# ─── SECURITY: Worktree path check ─────────────────────────────────────────
# Workers MUST run this script from inside their assigned worktree, NOT from
# the main HUB_DIR. If a worker is in the hub, they are coding in the wrong
# place and their uncommitted work will be destroyed when the daemon switches
# branches for CI/CD. This check prevents that catastrophic data loss.
CURRENT_PWD="$(pwd)"
if [[ "$CURRENT_PWD" != *".devin/worktrees/"* ]] && [[ "$CURRENT_PWD" != *"-agent-"* ]]; then
    echo "" >&2
    echo "==============================================================" >&2
    echo "  SECURITY BREACH: You are in the main HUB_DIR!" >&2
    echo "  You MUST NOT run request_integration.sh from the hub." >&2
    echo "  Move to your assigned worktree first:" >&2
    echo "    cd ../SillyElaborateState-agent-<N>" >&2
    echo "  If you have uncommitted code here, it will be LOST when the" >&2
    echo "  daemon switches branches for CI/CD." >&2
    echo "==============================================================" >&2
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

# ─── v3 Patch: Shift-Left Tree Safety ──────────────────────────────────────
# A rebase on a dirty working tree will fail or, worse, silently drop
# uncommitted changes. We MUST verify the tree is completely clean
# before any git fetch/rebase operation.
echo ""
echo "=== Shift-Left: Pre-Sync Tree Safety Check ==="
tree_state=$(git status --porcelain 2>/dev/null)
if [ -n "$tree_state" ]; then
    echo ""
    echo "=============================================================="
    echo "  SHIFT-LEFT ABORTED: Working tree is dirty"
    echo ""
    echo "  You have uncommitted changes. A rebase would fail or"
    echo "  silently drop your work. Commit your changes first,"
    echo "  then re-run request_integration.sh."
    echo ""
    echo "  Dirty files:"
    git status --short 2>/dev/null | head -10 | sed 's/^/    /'
    echo ""
    echo "  The INTEGRATION_REQUESTED event has NOT been emitted."
    echo "=============================================================="
    exit 1
fi
echo "  Working tree is clean. Safe to sync with origin/main."

# ─── v3: Shift-Left Main Sync ──────────────────────────────────────────────
# Pull and rebase onto origin/main BEFORE running local tests.
# This ensures the worker's branch is compatible with the latest main,
# preventing merge conflicts at the daemon's staging step.
echo ""
echo "=== Shift-Left: Syncing with origin/main ==="

git fetch origin main 2>&1 | tail -n 50

LOCAL_MAIN=$(git rev-parse origin/main 2>/dev/null || echo "")
MERGE_BASE=$(git merge-base HEAD origin/main 2>/dev/null || echo "")

if [ "$LOCAL_MAIN" != "$MERGE_BASE" ]; then
    echo "  origin/main has advanced. Rebasing local branch..."
    if ! git rebase origin/main 2>&1 | tail -n 50; then
        echo "  Rebase failed. Attempting merge instead..."
        git rebase --abort 2>/dev/null || true
        if ! git merge origin/main --no-edit 2>&1 | tail -n 50; then
            echo ""
            echo "=============================================================="
            echo "  SHIFT-LEFT SYNC FAILED: Merge conflict with origin/main"
            echo ""
            echo "  Resolve conflicts manually, commit, then re-run"
            echo "  request_integration.sh. The INTEGRATION_REQUESTED event"
            echo "  has NOT been emitted."
            echo "=============================================================="
            git merge --abort 2>/dev/null || true
            exit 1
        fi
    fi
    echo "  Branch synced with origin/main."
else
    echo "  Branch is up-to-date with origin/main."
fi
echo ""

# ─── Pre-Integration Guard (v3: full local CI mirror) ─────────────────────
# Force local cargo check + test + clippy + npm before emitting the event.
# v3: If it passes here, it passes on the daemon.
if [ -f Cargo.toml ]; then
    echo "=== Pre-Integration Guard v3: Full Local CI ==="

    echo "Running cargo check..."
    if ! cargo check --workspace 2>&1 | tail -n 50; then
        echo ""
        echo "=============================================================="
        echo "  PRE-INTEGRATION GUARD: cargo check FAILED"
        echo "  Your code does not compile. Fix errors before requesting"
        echo "  integration. The event has NOT been emitted."
        echo "=============================================================="
        exit 1
    fi

    echo "Running cargo test (excluding smoke)..."
    # v4: Export CI=true for insta strict mode. No --features epic-tests
    #     → epic test binaries are skipped by Cargo (fast CI).
    export CI=true
    if command -v cargo-nextest &>/dev/null; then
        echo "  (using cargo-nextest with --test-threads=4 for OOM safety, fast mode)"
        if ! cargo nextest run --workspace --all-targets \
            --skip headless_50_tick_smoke \
            --profile ci --test-threads=4 2>&1 | tail -n 50; then
            echo ""
            echo "=============================================================="
            echo "  PRE-INTEGRATION GUARD: cargo nextest FAILED"
            echo "  Local tests fail. Fix failing tests before requesting"
            echo "  integration. The event has NOT been emitted."
            echo "=============================================================="
            exit 1
        fi
        # v3: nextest ignores doctests — run separately
        echo "Running cargo test --doc (doctests)..."
        if ! cargo test --workspace --doc 2>&1 | tail -n 50; then
            echo ""
            echo "=============================================================="
            echo "  PRE-INTEGRATION GUARD: cargo test --doc FAILED"
            echo "  Doctests fail. The event has NOT been emitted."
            echo "=============================================================="
            exit 1
        fi
    else
        if ! cargo test --workspace --all-targets -- --skip headless_50_tick_smoke 2>&1 | tail -n 50; then
            echo ""
            echo "=============================================================="
            echo "  PRE-INTEGRATION GUARD: cargo test FAILED"
            echo "  Local tests fail. Fix failing tests before requesting"
            echo "  integration. The event has NOT been emitted."
            echo "=============================================================="
            exit 1
        fi
    fi

    echo "Running cargo clippy..."
    if ! cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -n 50; then
        echo ""
        echo "=============================================================="
        echo "  PRE-INTEGRATION GUARD: cargo clippy FAILED"
        echo "  Clippy warnings detected. Fix before requesting"
        echo "  integration. The event has NOT been emitted."
        echo "=============================================================="
        exit 1
    fi

    echo "Running npm run build..."
    if ! npm run build 2>&1 | tail -n 50; then
        echo ""
        echo "=============================================================="
        echo "  PRE-INTEGRATION GUARD: npm run build FAILED"
        echo "  Frontend build fails. Fix before requesting integration."
        echo "  The event has NOT been emitted."
        echo "=============================================================="
        exit 1
    fi

    echo ""
    echo "Pre-Integration Guard v3: PASS (check + test + doctest + clippy + npm green)"
    echo ""
fi

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

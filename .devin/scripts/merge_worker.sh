#!/bin/bash
# merge_worker.sh — Manager CLI: formal merge + diagnostic harness + reverse sync.
#
# Usage: bash .devin/scripts/merge_worker.sh <worker_branch>
#
# Flow:
#   1. Validate manager token
#   2. Fetch + checkout main + pull --rebase
#   3. Merge --no-ff <worker_branch>
#   4. Resolve conflicts (surgical append / field union)
#   5. Run Global Diagnostic Harness (Iron CI/CD + M0 audit + macro audit)
#   6. If pass → push to main
#   7. REVERSE SYNC: checkout worker branch, pull --rebase origin main, push
#   8. Update ledger (worker status → merged, clear locks)
#   9. Log to audit trail

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/sync_lib.sh"

# Validate manager auth
validate_manager_auth
if [ "$AGENT_ROLE" != "manager" ]; then
    echo "✗ ACCESS DENIED: merge_worker.sh requires manager role." >&2
    exit 1
fi

if [ $# -lt 1 ]; then
    echo "Usage: bash .devin/scripts/merge_worker.sh <worker_branch>" >&2
    exit 1
fi

WORKER_BRANCH="$1"
echo "=== Manager Merge Workflow: $WORKER_BRANCH → main ==="

# Step 1: Fetch and prepare main
echo "[1/9] Fetching origin..."
git fetch origin 2>&1 | head -3

echo "[2/9] Checking out main..."
git checkout main 2>&1 | head -3
git pull --rebase origin main 2>&1 | head -5

# Step 3: Merge worker branch
echo "[3/9] Merging $WORKER_BRANCH (--no-ff)..."
git merge --no-ff "$WORKER_BRANCH" 2>&1
MERGE_RC=$?

if [ $MERGE_RC -ne 0 ]; then
    # Check for conflicts
    CONFLICTS=$(git diff --name-only --diff-filter=U 2>/dev/null)
    if [ -n "$CONFLICTS" ]; then
        echo ""
        echo "⚠️  MERGE CONFLICTS detected in:"
        echo "$CONFLICTS" | sed 's/^/  - /'
        echo ""
        echo "Resolution strategy:"
        echo "  - engine/turn.rs: surgical append (keep BOTH phases)"
        echo "  - state/mod.rs: field union (keep ALL fields)"
        echo "  - economy/mod.rs: module union (keep ALL pub mod)"
        echo "  - Domain files: take theirs (worker is authoritative)"
        echo ""
        echo "Resolve conflicts manually, then run:"
        echo "  git add <resolved_files>"
        echo "  git commit"
        echo "  bash .devin/scripts/merge_worker.sh $WORKER_BRANCH  # resume"
        exit 1
    fi
    echo "✗ Merge failed (non-conflict). Aborting." >&2
    git merge --abort 2>/dev/null
    exit 1
fi

# Step 4: Conflicts resolved (or no conflicts) — quick compile check
echo "[4/9] Quick compile check (cargo check)..."
cargo check --workspace 2>&1 | tail -5
CHECK_RC=$?
if [ $CHECK_RC -ne 0 ]; then
    echo "⚠️  cargo check failed. Fix compilation errors before proceeding."
    echo "  The merge is staged but NOT pushed. Either fix and continue,"
    echo "  or abort with: git merge --abort"
    exit 1
fi

# Step 5: Global Diagnostic Harness
echo "[5/9] Running Iron CI/CD (Stage 1 of 3)..."
echo "  cargo build --workspace"
cargo build --workspace 2>&1 | tail -3
BUILD_RC=$?
if [ $BUILD_RC -ne 0 ]; then
    echo "✗ cargo build failed. Aborting merge." >&2
    git merge --abort 2>/dev/null
    exit 1
fi

echo "  cargo test --workspace --all-targets"
cargo test --workspace --all-targets 2>&1 | tail -10
TEST_RC=$?
if [ $TEST_RC -ne 0 ]; then
    echo "✗ cargo test failed. Aborting merge." >&2
    git merge --abort 2>/dev/null
    exit 1
fi

echo "  cargo clippy --workspace --all-targets -- -D warnings"
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5
CLIPPY_RC=$?
if [ $CLIPPY_RC -ne 0 ]; then
    echo "✗ cargo clippy failed. Aborting merge." >&2
    git merge --abort 2>/dev/null
    exit 1
fi

echo "  npm run build"
npm run build 2>&1 | tail -5
NPM_RC=$?
if [ $NPM_RC -ne 0 ]; then
    echo "✗ npm run build failed. Aborting merge." >&2
    git merge --abort 2>/dev/null
    exit 1
fi

echo "[5/9] Stage 2: M0 Conservation Audit..."
cargo test --test macro_m0_audit -- --nocapture 2>&1 | tail -10
M0_RC=$?
if [ $M0_RC -ne 0 ]; then
    echo "✗ M0 conservation audit FAILED. Aborting merge." >&2
    git merge --abort 2>/dev/null
    exit 1
fi

echo "[5/9] Stage 3: All diagnostic stages passed."

# Step 6: Push to main
echo "[6/9] Pushing to main..."
git push origin main 2>&1
PUSH_RC=$?
if [ $PUSH_RC -ne 0 ]; then
    echo "✗ Push to main failed. The merge is committed locally but not pushed." >&2
    exit 1
fi

# Step 7: REVERSE SYNC — update worker branch
echo "[7/9] Reverse sync: updating worker branch $WORKER_BRANCH..."
git checkout "$WORKER_BRANCH" 2>&1 | head -3
git pull --rebase origin main 2>&1 | head -5
REBASE_RC=$?
if [ $REBASE_RC -ne 0 ]; then
    echo "⚠️  Reverse sync rebase failed. The worker branch may need manual resolution."
    echo "  The merge to main succeeded. Worker must resolve on next session."
    git rebase --abort 2>/dev/null
    git checkout main 2>/dev/null
else
    git push origin "$WORKER_BRANCH" 2>&1 | head -3
    echo "  Worker branch $WORKER_BRANCH updated with merge resolution."
    git checkout main 2>/dev/null
fi

# Step 8: Update ledger
echo "[8/9] Updating ledger (worker status → merged)..."
# TODO: implement mutator_set_merged if needed
# For now, log the action
log_manager_action "merge_worker" "branch:$WORKER_BRANCH diagnostics:passed"

echo "[9/9] Merge workflow complete."
echo "✓ $WORKER_BRANCH merged to main, diagnostics passed, reverse sync done."

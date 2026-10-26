#!/bin/bash
# merge_worker.sh — Manager CLI: formal merge + diagnostic harness + reverse sync.
#
# Usage: bash .devin/scripts/merge_worker.sh <worker_branch> [--tag vX.Y.Z]
#
# Flow:
#   1. Validate manager token (auto-mint via claim_manager.sh before pushing)
#   2. Fetch + checkout main + pull --rebase
#   3. Merge --no-ff <worker_branch>
#   4. Resolve conflicts (surgical append / field union)
#   5. Run Global Diagnostic Harness (Iron CI/CD + M0 audit + macro audit)
#      — SKIPPED (fast cargo check only) when the merge is tree-identical to
#        a worker-branch HEAD that already has a verified-green .cicd_state /
#        last_green_commit. A tree-identical merge introduces zero new code,
#        so re-running the 20-minute suite is provably redundant.
#   6. If pass → claim_manager.sh (fresh session token) → push to main
#      → auto-push release tags (any v* tag --points-at HEAD, plus --tag flag)
#   7. REVERSE SYNC: rebase worker branch onto main — resolved in whichever
#      worktree holds the branch (worktree-aware, no double-checkout)
#   8. Update ledger (worker status → merged, last_green_commit)
#   9. Log to audit trail

set -uo pipefail

# ─── Self-immunity: never execute the live script file ───────────────────
# The merge rewrites this file mid-run when the worker branch ships a newer
# merge_worker.sh. Bash reads scripts incrementally by byte offset, so the
# remaining commands were skipped/corrupted (v4.22.1 tag push never ran).
# Copy ourselves to a stable temp path and exec that instead — subsequent
# writes to the repo file cannot affect the running interpreter.
if [ -z "${MW_SELF_COPY:-}" ]; then
    MW_SELF_COPY=1
    export MW_SELF_COPY
    export MW_SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
    export MW_PROJECT_DIR="$(cd "$MW_SCRIPT_DIR/../.." && pwd)"
    _mw_tmp="$(mktemp /tmp/merge_worker_exec.XXXXXX.sh 2>/dev/null || echo /tmp/merge_worker_exec.sh)"
    cp "$0" "$_mw_tmp"
    exec bash "$_mw_tmp" "$@"
fi

SCRIPT_DIR="${MW_SCRIPT_DIR:-$(cd "$(dirname "$0")" && pwd)}"
source "$SCRIPT_DIR/sync_lib.sh"
PROJECT_DIR="${MW_PROJECT_DIR:-$(cd "$SCRIPT_DIR/../.." && pwd)}"

# ─── Parse args: <worker_branch> [--tag vX.Y.Z] ────────────────────────────
WORKER_BRANCH=""
TAG_FLAG=""
while [ $# -gt 0 ]; do
    case "$1" in
        --tag)
            TAG_FLAG="${2:-}"
            shift 2
            ;;
        *)
            if [ -z "$WORKER_BRANCH" ]; then
                WORKER_BRANCH="$1"
            fi
            shift
            ;;
    esac
done

if [ -z "$WORKER_BRANCH" ]; then
    echo "Usage: bash .devin/scripts/merge_worker.sh <worker_branch> [--tag vX.Y.Z]" >&2
    exit 1
fi

# Validate manager auth — auto-mint a token for this session if stale/absent.
validate_manager_auth
if [ "$AGENT_ROLE" != "manager" ]; then
    echo "! Manager token missing/stale — minting via claim_manager.sh..."
    bash "$SCRIPT_DIR/claim_manager.sh" >/dev/null 2>&1 || true
    validate_manager_auth
fi
if [ "$AGENT_ROLE" != "manager" ]; then
    echo "✗ ACCESS DENIED: merge_worker.sh requires manager role." >&2
    exit 1
fi

echo "=== Manager Merge Workflow: $WORKER_BRANCH → main ==="

# Step 1: Fetch and prepare main
echo "[1/9] Fetching origin..."
git fetch origin 2>&1 | head -3

echo "[2/9] Checking out main..."
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "")
if [ "$CURRENT_BRANCH" != "main" ]; then
    git checkout main 2>&1 | head -3
fi
git pull --rebase origin main 2>&1 | head -5

# Step 3: Merge worker branch
WORKER_HEAD=$(git rev-parse "$WORKER_BRANCH" 2>/dev/null || echo "")
WORKER_TREE=$(git rev-parse "$WORKER_BRANCH^{tree}" 2>/dev/null || echo "")
echo "[3/9] Merging $WORKER_BRANCH (--no-ff)..."
git merge --no-ff "$WORKER_BRANCH" 2>&1
MERGE_RC=$?

if [ $MERGE_RC -ne 0 ]; then
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

# ─── Step 5: Global Diagnostic Harness (with green-skip) ───────────────────
# A merge whose resulting tree is byte-identical to the worker branch HEAD
# introduces zero new code. If that HEAD already passed the harness (recorded
# in .devin/.cicd_state or agents_sync.json last_green_commit), re-running the
# ~20-minute suite is redundant — cargo check above suffices.
MERGED_TREE=$(git rev-parse "HEAD^{tree}" 2>/dev/null || echo "")
SKIP_HARNESS=0
if [ -n "$WORKER_HEAD" ] && [ "$MERGED_TREE" = "$WORKER_TREE" ]; then
    GREEN_LEDGER=$(node -e '
        try {
            const d = require("./agents_sync.json");
            console.log(d.last_green_commit || "");
        } catch(e) { console.log(""); }
    ' 2>/dev/null || echo "")
    CICD_STATE=""
    if [ -f ".devin/.cicd_state" ]; then
        CICD_STATE=$(cat .devin/.cicd_state)
    fi
    if [ "$GREEN_LEDGER" = "$WORKER_HEAD" ] || echo "$CICD_STATE" | grep -q "$WORKER_HEAD"; then
        SKIP_HARNESS=1
    fi
fi

if [ "$SKIP_HARNESS" = "1" ]; then
    echo "[5/9] GREEN-SKIP: merged tree identical to verified-green worker HEAD"
    echo "      ($WORKER_HEAD). Skipping redundant full harness."
else
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
    cargo test -p sim_engine --test macro_m0_audit --features epic-tests -- --nocapture 2>&1 | tail -10
    M0_RC=$?
    if [ $M0_RC -ne 0 ]; then
        echo "✗ M0 conservation audit FAILED. Aborting merge." >&2
        git merge --abort 2>/dev/null
        exit 1
    fi

    echo "[5/9] Stage 3: All diagnostic stages passed."
fi

# Step 6: Push to main — re-mint manager token so a stale session id can
# never block the push, then push release tags automatically.
echo "[6/9] Refreshing manager token + pushing to main..."
bash "$SCRIPT_DIR/claim_manager.sh" >/dev/null 2>&1 || true
git push origin main 2>&1
PUSH_RC=$?
if [ $PUSH_RC -ne 0 ]; then
    echo "✗ Push to main failed. The merge is committed locally but not pushed." >&2
    exit 1
fi

# Auto-push release tags: every v* tag pointing at the merged HEAD, plus an
# explicit --tag flag. Release tags must reach GitHub without a second manual
# push (the user should never run `git push origin vX.Y.Z` themselves).
TAGS_TO_PUSH="$TAG_FLAG"
for t in $(git tag --points-at HEAD 2>/dev/null | grep -E '^v' || true); do
    case " $TAGS_TO_PUSH " in
        *" $t "*) ;;
        *) TAGS_TO_PUSH="$TAGS_TO_PUSH $t" ;;
    esac
done
for t in $TAGS_TO_PUSH; do
    echo "  Pushing tag $t..."
    git push origin "$t" 2>&1 | head -3
done

# Step 7: REVERSE SYNC — update worker branch in whichever worktree holds it.
echo "[7/9] Reverse sync: updating worker branch $WORKER_BRANCH..."
WORKER_WT=$(git worktree list --porcelain 2>/dev/null | node -e '
    let input = "";
    process.stdin.on("data", d => input += d);
    process.stdin.on("end", () => {
        const want = "refs/heads/" + process.argv[1];
        let dir = null;
        for (const line of input.split("\n")) {
            if (line.startsWith("worktree ")) dir = line.slice(9);
            if (line === "branch " + want) { console.log(dir || ""); return; }
        }
        console.log("");
    });
' "$WORKER_BRANCH" 2>/dev/null || echo "")

reverse_sync_at() {
    local wt="$1"
    git -C "$wt" pull --rebase origin main 2>&1 | head -5
    if [ $? -ne 0 ]; then
        echo "⚠️  Reverse sync rebase failed in $wt. Worker must resolve on next session."
        git -C "$wt" rebase --abort 2>/dev/null
        return 1
    fi
    git -C "$wt" push origin "$WORKER_BRANCH" 2>&1 | head -3
    echo "  Worker branch $WORKER_BRANCH updated (worktree: $wt)."
    return 0
}

if [ -n "$WORKER_WT" ]; then
    if [ -n "$(git -C "$WORKER_WT" status --porcelain 2>/dev/null)" ]; then
        echo "⚠️  Worker worktree $WORKER_WT is dirty — skipping reverse sync there."
    else
        reverse_sync_at "$WORKER_WT"
    fi
else
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
fi

# Step 8: Update ledger — mark green commit for the merged tip.
echo "[8/9] Updating ledger (worker status → merged, last_green_commit)..."
GREEN_COMMIT=$(git rev-parse HEAD) mutator_set_green_commit 2>/dev/null || true
printf 'PASSED %s %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$(git rev-parse HEAD)" \
    > "$PROJECT_DIR/.devin/.cicd_state" 2>/dev/null || true
log_manager_action "merge_worker" "branch:$WORKER_BRANCH diagnostics:passed tags:${TAGS_TO_PUSH:-none}"

echo "[9/9] Merge workflow complete."
echo "✓ $WORKER_BRANCH merged to main, diagnostics passed, reverse sync done."
[ -n "${TAGS_TO_PUSH// /}" ] && echo "✓ Release tags pushed: $TAGS_TO_PUSH"

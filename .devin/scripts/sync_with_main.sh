#!/bin/bash
# sync_with_main.sh — Safely sync a worker worktree with main.
#
# Usage: bash .devin/scripts/sync_with_main.sh
#
# Behavior:
#   1. Verify we're in a worktree (NOT the hub)
#   2. Rebase current branch onto local main (no remote fetch needed)
#   3. If conflicts: print guidance, exit 1 (never auto-resolve)
#   4. Never touch the hub directory
#
# Architecture note: Git Worktrees share the same local .git object database
# as the main hub directory. The local `main` branch is directly visible from
# any worktree and is kept up to date by the daemon's promotions. There is no
# need to interact with a remote (`origin`). Using `git rebase main` (local)
# is simpler, faster, and avoids a network dependency.

set -euo pipefail

# ─── Security: Worktree path check ─────────────────────────────────────────
CURRENT_PWD="$(pwd)"
if [[ "$CURRENT_PWD" == *"/SillyElaborateState" ]] && [[ "$CURRENT_PWD" != *"-agent-"* ]] && [[ "$CURRENT_PWD" != *"-fix" ]]; then
    echo "" >&2
    echo "╔══════════════════════════════════════════════════════════════════╗" >&2
    echo "║  SECURITY BREACH: You are in the main HUB_DIR!                  ║" >&2
    echo "║                                                                  ║" >&2
    echo "║  You MUST NOT run sync_with_main.sh from the hub.                ║" >&2
    echo "║  Move to your assigned worktree first:                           ║" >&2
    echo "║    cd ../SillyElaborateState-agent-<N>                           ║" >&2
    echo "╚══════════════════════════════════════════════════════════════════╝" >&2
    echo "" >&2
    exit 1
fi

# ─── Rebase current branch onto local main ─────────────────────────────────
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
echo "Rebasing $CURRENT_BRANCH onto local main..."

if git rebase main 2>&1; then
    echo "Sync complete. Branch $CURRENT_BRANCH is up to date with main."
    exit 0
else
    # Rebase conflict — print guidance, do NOT auto-resolve
    CONFLICTS=$(git diff --name-only --diff-filter=U 2>/dev/null | tr '\n' ' ')
    echo "" >&2
    echo "REBASE CONFLICT in: $CONFLICTS" >&2
    echo "" >&2
    echo "To resolve:" >&2
    echo "  1. Edit the conflicted files" >&2
    echo "  2. git add <resolved-files>" >&2
    echo "  3. git rebase --continue" >&2
    echo "" >&2
    echo "To abort:" >&2
    echo "  git rebase --abort" >&2
    exit 1
fi

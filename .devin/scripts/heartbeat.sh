#!/bin/bash
# heartbeat.sh — Background heartbeat updater with index.lock evasion (C3).
#
# Spawned by start.sh as a nohup background process.
# Updates last_heartbeat in agents_sync.json every 3 minutes via the
# transactional sync loop (C1).
#
# C3: Checks for .git/index.lock before every git operation.
# If the lock exists (another human/agent is committing), skips the
# current cycle entirely — does NOT crash or interfere.
#
# Usage: heartbeat.sh <session_id>

SESSION_ID="${1:-}"
if [ -z "$SESSION_ID" ]; then
    echo "ERROR: heartbeat.sh requires session_id argument" >&2
    exit 1
fi
export SESSION_ID

# Source shared library
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/sync_lib.sh"

# Main loop — runs until killed by cleanup trap in start.sh
while true; do
    sleep 180  # 3 minutes

    # C3: Check for index.lock — skip if another git process is running
    # Do NOT wait or retry; just continue to the next interval.
    if [ -f ".git/index.lock" ]; then
        continue
    fi

    # Apply heartbeat via transactional loop (C1) with fewer attempts (3)
    # to avoid blocking the background process too long.
    # The transactional loop also checks index.lock internally.
    sync_transactional mutator_heartbeat "sync: heartbeat for $SESSION_ID" 3 2>/dev/null || true
done

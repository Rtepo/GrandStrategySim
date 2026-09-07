#!/bin/bash
# launch_daemon.sh — Launch the integration daemon with correct env vars.
# v4.1: Also spawns auto_wake.sh background processes for the auditor (agent-4)
#       and any workers listed in the sprint manifest. These are tracked in
#       .devin/.auto_wake_pids.txt so stop_daemon.sh can clean them up.
cd "$(dirname "$0")/../.." || exit 1
export SESSION_ID="coffee-zebu"
export HUB_DIR="$(pwd)"
export SKIP_AUDIT=1
mkdir -p .devin/integration_log

# ─── Launch the integration daemon ─────────────────────────────────────────
nohup bash .devin/scripts/integration_daemon.sh >> .devin/integration_log/daemon.log 2>&1 &
DAEMON_PID=$!
echo "DAEMON_PID=$DAEMON_PID"
echo "$DAEMON_PID" > .devin/.integration_daemon.pid
echo "Log: .devin/integration_log/daemon.log"
echo "PID file: .devin/.integration_daemon.pid"
sleep 2
if kill -0 "$DAEMON_PID" 2>/dev/null; then
    echo "Daemon is running (PID $DAEMON_PID)."
else
    echo "ERROR: Daemon exited immediately. Check log:"
    tail -20 .devin/integration_log/daemon.log
    exit 1
fi

# ─── v4.1: Launch auto_wake.sh for auditor + workers ───────────────────────
# auto_wake.sh monitors the event bus for events targeting a specific agent.
# Without this, AUDIT_REQUESTED and other events sit unprocessed forever.
WAKE_PID_FILE="$HUB_DIR/.devin/.auto_wake_pids.txt"
> "$WAKE_PID_FILE"  # Clear stale PID list

echo ""
echo "=== Launching auto_wake daemons ==="

# Agent 4 (Auditor) — wakes on AUDIT_REQUESTED events to run macro audit
nohup bash .devin/scripts/auto_wake.sh agent-4 "bash .devin/scripts/run_audit.sh" \
    >> .devin/integration_log/auto_wake_agent-4.log 2>&1 &
WAKE_PID_4=$!
echo "$WAKE_PID_4" >> "$WAKE_PID_FILE"
echo "  agent-4 (Auditor): PID $WAKE_PID_4 — wake: run_audit.sh"

# Scan sprint manifest for worker branches and launch auto_wake for each agent
MANIFEST="$HUB_DIR/.devin/sprint_manifest.txt"
if [ -f "$MANIFEST" ]; then
    while IFS= read -r line; do
        # Skip comments and empty lines
        [[ "$line" =~ ^[[:space:]]*# ]] && continue
        [ -z "$line" ] && continue
        # Extract agent ID from branch name (e.g., feat/agent-2-foo → agent-2)
        AGENT_FROM_BRANCH=$(echo "$line" | grep -oE 'agent-[0-9]+' || true)
        if [ -n "$AGENT_FROM_BRANCH" ] && [ "$AGENT_FROM_BRANCH" != "agent-4" ]; then
            # Check if already launched (dedup)
            if grep -q " $AGENT_FROM_BRANCH " "$WAKE_PID_FILE" 2>/dev/null; then
                continue
            fi
            nohup bash .devin/scripts/auto_wake.sh "$AGENT_FROM_BRANCH" \
                >> .devin/integration_log/auto_wake_${AGENT_FROM_BRANCH}.log 2>&1 &
            WAKE_PID=$!
            echo "$WAKE_PID" >> "$WAKE_PID_FILE"
            echo "  $AGENT_FROM_BRANCH (Worker): PID $WAKE_PID — alert only"
        fi
    done < "$MANIFEST"
fi

echo ""
echo "  PIDs tracked in: .devin/.auto_wake_pids.txt"
echo "=== auto_wake daemons launched ==="

#!/bin/bash
# launch_daemon.sh — Launch the integration daemon with correct env vars.
cd "$(dirname "$0")/../.." || exit 1
export SESSION_ID="coffee-zebu"
export HUB_DIR="$(pwd)"
export SKIP_AUDIT=1
mkdir -p .devin/integration_log
nohup bash .devin/scripts/integration_daemon.sh >> .devin/integration_log/daemon.log 2>&1 &
DAEMON_PID=$!
echo "DAEMON_PID=$DAEMON_PID"
echo "Log: .devin/integration_log/daemon.log"
echo "PID file: .devin/.integration_daemon.pid"
sleep 2
if kill -0 "$DAEMON_PID" 2>/dev/null; then
    echo "Daemon is running (PID $DAEMON_PID)."
else
    echo "ERROR: Daemon exited immediately. Check log:"
    tail -20 .devin/integration_log/daemon.log
fi

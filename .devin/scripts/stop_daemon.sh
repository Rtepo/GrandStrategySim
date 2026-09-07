#!/bin/bash
# stop_daemon.sh — Stop the integration daemon via PID file.
# v4.1: Also stops all auto_wake.sh background processes tracked in
#       .devin/.auto_wake_pids.txt.
#
# Usage: bash .devin/scripts/stop_daemon.sh

HUB_DIR="${HUB_DIR:-$(pwd)}"
PID_FILE="$HUB_DIR/.devin/.integration_daemon.pid"
WAKE_PID_FILE="$HUB_DIR/.devin/.auto_wake_pids.txt"

# ─── v4.1: Stop auto_wake processes first ──────────────────────────────────
if [ -f "$WAKE_PID_FILE" ]; then
    echo "Stopping auto_wake daemons..."
    while IFS= read -r wake_pid; do
        [ -z "$wake_pid" ] && continue
        if kill -0 "$wake_pid" 2>/dev/null; then
            echo "  Killing auto_wake (PID $wake_pid)..."
            kill "$wake_pid" 2>/dev/null
            sleep 1
            if kill -0 "$wake_pid" 2>/dev/null; then
                kill -9 "$wake_pid" 2>/dev/null
            fi
        fi
    done < "$WAKE_PID_FILE"
    rm -f "$WAKE_PID_FILE"
    echo "  auto_wake daemons stopped."
fi

if [ ! -f "$PID_FILE" ]; then
    echo "No daemon PID file found. Daemon is not running."
    exit 0
fi

PID=$(cat "$PID_FILE" 2>/dev/null || echo "")

if [ -z "$PID" ]; then
    echo "PID file is empty. Removing."
    rm -f "$PID_FILE"
    exit 0
fi

if kill -0 "$PID" 2>/dev/null; then
    echo "Stopping integration daemon (PID $PID)..."
    kill "$PID" 2>/dev/null
    sleep 2
    if kill -0 "$PID" 2>/dev/null; then
        echo "Daemon did not stop gracefully. Sending SIGKILL."
        kill -9 "$PID" 2>/dev/null
    fi
    echo "Daemon stopped."
else
    echo "Daemon (PID $PID) is not running. Cleaning up PID file."
fi

rm -f "$PID_FILE"
exit 0

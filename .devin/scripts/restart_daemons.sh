#!/bin/bash
# restart_daemons.sh — Restart the integration daemon + all auto_wake daemons.
#
# Convenience wrapper around stop_daemon.sh + launch_daemon.sh. Required after
# editing daemon scripts (e.g. the --model flag in auto_wake.sh): running bash
# loops cache file offsets, so only freshly spawned processes pick up edits.
#
# Usage: bash .devin/scripts/restart_daemons.sh

set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

echo "=== Stopping daemons ==="
bash .devin/scripts/stop_daemon.sh

echo ""
echo "=== Starting daemons ==="
bash .devin/scripts/launch_daemon.sh

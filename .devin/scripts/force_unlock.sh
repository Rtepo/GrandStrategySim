#!/bin/bash
# force_unlock.sh — Manager CLI: break zombie locks or override domain reservations.
#
# Usage: bash .devin/scripts/force_unlock.sh <agent_id>
#
# Sets the target agent to "force-unlocked", clears locked_dirs and session_id.
# Audit logged to .devin/.manager_audit_log.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/sync_lib.sh"

# Validate manager auth
validate_manager_auth
if [ "$AGENT_ROLE" != "manager" ]; then
    echo "✗ ACCESS DENIED: force_unlock.sh requires manager role." >&2
    echo "  Run 'bash .devin/scripts/claim_manager.sh' first." >&2
    exit 1
fi

if [ $# -lt 1 ]; then
    echo "Usage: bash .devin/scripts/force_unlock.sh <agent_id>" >&2
    exit 1
fi

FORCE_UNLOCK_TARGET="$1"
export FORCE_UNLOCK_TARGET

echo "Force-unlocking agent: $FORCE_UNLOCK_TARGET"
sync_transactional mutator_force_unlock "manager: force-unlock agent $FORCE_UNLOCK_TARGET" 5
RC=$?

if [ $RC -eq 0 ]; then
    echo "✓ Agent $FORCE_UNLOCK_TARGET has been force-unlocked."
    log_manager_action "force_unlock" "target:$FORCE_UNLOCK_TARGET"
else
    echo "✗ Failed to force-unlock after 5 attempts." >&2
    exit 1
fi

#!/bin/bash
# run_audit.sh v3 — Agent 4 audit entry point.
#
# Runs the audit queue processor, then if an AUDIT_FAIL event was produced,
# calls route_failures.sh to directly route REMEDIATION_REQUESTED events
# to the responsible worker agents (bypassing the manager).

export AGENT_ID="agent-4"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Run the audit queue processor
bash "$SCRIPT_DIR/process_audit_queue.sh"

# v3: After audit completes, check for newly emitted AUDIT_FAIL events
# and route them directly to workers via route_failures.sh
EVENTS_DIR="${DEVIN_PROJECT_DIR:-$(pwd)}/.devin/events"
if [ ! -d "$EVENTS_DIR" ]; then
    # Fallback: derive from SCRIPT_DIR
    EVENTS_DIR="$(cd "$SCRIPT_DIR/../.." 2>/dev/null && pwd)/.devin/events"
fi

AUDIT_FAIL_FILES=$(find "$EVENTS_DIR" -maxdepth 1 \
    \( -name "*AUDIT_FAIL*" -o -name "*AUDIT_FAIL_ADDENDUM*" \) \
    -not -name "*CORRECTION*" -type f 2>/dev/null)

if [ -n "$AUDIT_FAIL_FILES" ]; then
    echo ""
    echo "=== v3: Direct AUDIT_FAIL routing ==="
    while IFS= read -r af_file; do
        [ -z "$af_file" ] && continue
        if [ ! -f "$af_file" ]; then continue; fi
        bash "$SCRIPT_DIR/route_failures.sh" "$af_file" 2>&1 | tail -n 50
    done <<< "$AUDIT_FAIL_FILES"
    echo ""
    echo "=== v3: Direct routing complete ==="
fi

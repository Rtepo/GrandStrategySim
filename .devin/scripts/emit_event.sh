#!/bin/bash
# emit_event.sh — Event bus emitter (routes to hub path).
#
# Usage: bash .devin/scripts/emit_event.sh <type> <source> <target> [payload_json]
#
# Behavior:
#   1. Detect the hub directory (HUB_DIR)
#   2. Write a JSON event file to $HUB_DIR/.devin/events/
#   3. Create events/ and events/.archive/ directories if missing
#
# Event format:
#   {
#     "type": "TASK_COMPLETE",
#     "source": "agent-1",
#     "target": "agent-5",
#     "timestamp": "2026-09-04T12:00:00Z",
#     "payload": { ... }
#   }

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# ─── Validate arguments ────────────────────────────────────────────────────
if [ $# -lt 3 ]; then
    echo "Usage: bash .devin/scripts/emit_event.sh <type> <source> <target> [payload_json]" >&2
    echo "Example: bash .devin/scripts/emit_event.sh TASK_COMPLETE agent-1 agent-5 '{\"phase\":94}'" >&2
    exit 1
fi

EVENT_TYPE="$1"
EVENT_SOURCE="$2"
EVENT_TARGET="$3"
EVENT_PAYLOAD="$4"
if [ -z "$EVENT_PAYLOAD" ]; then
    EVENT_PAYLOAD="{}"
fi

# ─── Detect hub directory ──────────────────────────────────────────────────
# If running in a worker worktree, the hub is at ../SillyElaborateState/
# If running in the primary hub, HUB_DIR is the current directory.
PROJECT_DIR="${DEVIN_PROJECT_DIR:-$(pwd)}"
PARENT_DIR="$(cd "$PROJECT_DIR/.." && pwd)"
HUB_CANDIDATE="$PARENT_DIR/SillyElaborateState"

if [ -d "$HUB_CANDIDATE/.devin" ]; then
    HUB_DIR="$HUB_CANDIDATE"
else
    HUB_DIR="$PROJECT_DIR"
fi

# ─── Create event directories if missing ───────────────────────────────────
EVENTS_DIR="$HUB_DIR/.devin/events"
ARCHIVE_DIR="$EVENTS_DIR/.archive"

mkdir -p "$EVENTS_DIR" "$ARCHIVE_DIR" 2>/dev/null || true

# ─── Generate event ────────────────────────────────────────────────────────
TIMESTAMP=$(node -e 'console.log(new Date().toISOString())' 2>/dev/null || echo "$(date -u +%Y-%m-%dT%H:%M:%SZ)")
EVENT_ID=$(node -e '
    const crypto = require("crypto");
    console.log(crypto.randomBytes(8).toString("hex"));
' 2>/dev/null || echo "$$-$(date +%s)")

EVENT_FILE="$EVENTS_DIR/${TIMESTAMP//[:.]/-}_${EVENT_TYPE}_${EVENT_ID}.json"

node -e '
    const fs = require("fs");
    const event = {
        type: process.argv[1],
        source: process.argv[2],
        target: process.argv[3],
        timestamp: process.argv[4],
        id: process.argv[5],
        payload: JSON.parse(process.argv[6] || "{}")
    };
    fs.writeFileSync(process.argv[7], JSON.stringify(event, null, 2));
    console.log("Event emitted: " + event.type + " from " + event.source + " to " + event.target);
    console.log("  File: " + process.argv[7]);
' "$EVENT_TYPE" "$EVENT_SOURCE" "$EVENT_TARGET" "$TIMESTAMP" "$EVENT_ID" "$EVENT_PAYLOAD" "$EVENT_FILE"

exit 0

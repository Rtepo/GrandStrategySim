#!/bin/bash
# auto_wake.sh — v3 Active Listening Daemon
#
# Monitors $HUB_DIR/.devin/events/ for events targeting a specific agent.
# When a matching event is detected, it:
#   1. Prints a high-visibility alert to stdout (captured by the LLM terminal)
#   2. Optionally triggers a wake command (configurable per agent)
#
# Usage: bash .devin/scripts/auto_wake.sh <agent_id> [wake_command]
#
# Examples:
#   bash .devin/scripts/auto_wake.sh agent-4 "bash .devin/scripts/run_audit.sh"
#   bash .devin/scripts/auto_wake.sh agent-2  # just prints alert, no auto-exec

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# ─── Validate arguments ────────────────────────────────────────────────────
if [ $# -lt 1 ]; then
    echo "Usage: bash .devin/scripts/auto_wake.sh <agent_id> [wake_command]" >&2
    echo "Example: bash .devin/scripts/auto_wake.sh agent-4 \"bash .devin/scripts/run_audit.sh\"" >&2
    exit 1
fi

AGENT_ID="$1"
WAKE_COMMAND="${2:-}"

# ─── Detect hub directory ──────────────────────────────────────────────────
detect_hub_dir() {
    local dir="${DEVIN_PROJECT_DIR:-$(pwd)}"
    while [ "$dir" != "/" ] && [ -n "$dir" ]; do
        case "$dir" in
            */worktrees/*) ;;
            *)
                if [ -d "$dir/.devin/events" ]; then
                    echo "$dir"
                    return 0
                fi
                ;;
        esac
        dir="$(cd "$dir/.." 2>/dev/null && pwd)"
    done
    local parent
    parent="$(cd "${DEVIN_PROJECT_DIR:-$(pwd)}/.." 2>/dev/null && pwd)"
    if [ -d "$parent/SillyElaborateState/.devin" ]; then
        echo "$parent/SillyElaborateState"
        return 0
    fi
    echo "${DEVIN_PROJECT_DIR:-$(pwd)}"
}
HUB_DIR="$(detect_hub_dir)"
EVENTS_DIR="$HUB_DIR/.devin/events"
SEEN_FILE="$HUB_DIR/.devin/.auto_wake_seen_${AGENT_ID}.txt"

mkdir -p "$EVENTS_DIR" 2>/dev/null || true
touch "$SEEN_FILE" 2>/dev/null || true

POLL_INTERVAL=10

echo "============================================================"
echo "  AUTO-WAKE DAEMON v3 — Agent: $AGENT_ID"
echo "  PID: $$"
echo "  HUB_DIR: $HUB_DIR"
echo "  EVENTS_DIR: $EVENTS_DIR"
echo "  Poll interval: ${POLL_INTERVAL}s"
echo "  Wake command: ${WAKE_COMMAND:-<none — alert only>}"
echo "  Started: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "============================================================"
echo ""

# ─── v3.1: RAM throttle check before waking agents ────────────────────────
# Returns 0 if RAM is below 85% (safe to wake), 1 if above (defer wake).
# bc-free: converts float to int via bash parameter expansion.
check_ram_before_wake() {
    local used_pct=0

    if command -v powershell.exe &>/dev/null; then
        used_pct=$(powershell.exe -NoProfile -ExecutionPolicy Bypass -Command \
            "\$os = Get-CimInstance Win32_OperatingSystem; [math]::Round((\$os.TotalVisibleMemorySize - \$os.FreePhysicalMemory) / \$os.TotalVisibleMemorySize * 100, 1)" \
            2>/dev/null | tr -d '\r' || echo "0")
    elif command -v free &>/dev/null; then
        used_pct=$(free | awk '/Mem:/ {printf "%.1f", $3/$2*100}')
    fi

    # Convert float to integer via bash parameter expansion (no bc dependency)
    local used_int="${used_pct%.*}"
    used_int="${used_int:-0}"

    case "$used_int" in
        ''|*[!0-9]*) return 0 ;;  # Can't detect — proceed
    esac

    if [ "$used_int" -ge 85 ]; then
        echo "[$(date -u +%H:%M:%S)] RAM THROTTLE: System RAM at ${used_pct}%. Deferring wake command."
        return 1
    fi
    return 0
}

CYCLE=0
basename_evt=""
parse_rc=0

while true; do
    CYCLE=$((CYCLE + 1))

    # ─── Seen-file hygiene: remove stale entries every 10 cycles ───────────
    if [ $((CYCLE % 10)) -eq 0 ]; then
        local_new_seen=""
        while IFS= read -r seen_name; do
            [ -z "$seen_name" ] && continue
            if [ -f "$EVENTS_DIR/$seen_name" ]; then
                local_new_seen="${local_new_seen}${seen_name}\n"
            fi
        done < "$SEEN_FILE"
        printf "%b" "$local_new_seen" > "$SEEN_FILE" 2>/dev/null || true
    fi

    # ─── Scan for new events ───────────────────────────────────────────────
    event_files=$(find "$EVENTS_DIR" -maxdepth 1 -name "*.json" -type f 2>/dev/null || true)

    if [ -n "$event_files" ]; then
        while IFS= read -r evt_file; do
            [ -z "$evt_file" ] && continue

            basename_evt=""
            basename_evt=$(basename "$evt_file")

            # Skip if already seen
            if grep -qxF "$basename_evt" "$SEEN_FILE" 2>/dev/null; then
                continue
            fi

            # v3 Patch: Race condition guard — re-check file existence before
            # reading. The daemon may have archived this file between our
            # find glob and this read call (ENOENT prevention).
            if [ ! -f "$evt_file" ]; then
                continue
            fi

            # Parse event and check if it targets this agent
            EVT_FILE="$evt_file" AGENT_ID="$AGENT_ID" node <<'NODE_EOF'
                const fs = require("fs");
                try {
                    let raw = fs.readFileSync(process.env.EVT_FILE, "utf8");
                    if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
                    const evt = JSON.parse(raw);

                    const agentId = process.env.AGENT_ID;
                    const targetType = evt.target || "";
                    const eventType = evt.type || "";

                    // Match if target is this agent, or "all" for broadcast events
                    // v3.1: RECOVERY_WAKE targets the specific agent, so it's
                    // already matched by targetType === agentId.
                    const isMatch = (targetType === agentId) ||
                                    (targetType === "all" &&
                                     (eventType === "PROMOTED_TO_MAIN" ||
                                      eventType === "AUDIT_REQUESTED"));

                    if (!isMatch) {
                        process.exit(2); // Not a match — skip
                    }

                    // Print high-visibility alert
                    console.log("");
                    console.log("========================================================");
                    console.log("  AUTO-WAKE: New event for " + agentId);
                    console.log("  Type:    " + eventType);
                    console.log("  Source:  " + (evt.source || "unknown"));
                    console.log("  Target:  " + targetType);
                    console.log("  ID:      " + (evt.id || "unknown"));
                    console.log("  File:    " + process.env.EVT_FILE);

                    // Print payload summary (tail -n 50 equivalent — prevent OOM)
                    const payload = evt.payload || {};
                    const payloadStr = JSON.stringify(payload, null, 2);
                    const lines = payloadStr.split("\n");
                    const maxLines = 50;
                    if (lines.length > maxLines) {
                        console.log("  Payload: (first " + maxLines + " lines of " + lines.length + ")");
                        console.log(lines.slice(0, maxLines).join("\n"));
                        console.log("  ... (" + (lines.length - maxLines) + " more lines truncated)");
                    } else {
                        console.log("  Payload:");
                        console.log(payloadStr);
                    }
                    console.log("========================================================");
                    console.log("");

                    // Output match marker for the shell script
                    console.log("__AUTO_WAKE_MATCH__");
                } catch(e) {
                    // Parse error — skip silently (file may be partially written)
                    process.exit(1);
                }
NODE_EOF

            parse_rc=$?

            if [ $parse_rc -eq 0 ]; then
                # Match found — add to seen file
                echo "$basename_evt" >> "$SEEN_FILE" 2>/dev/null || true

                # Execute wake command if provided
                if [ -n "$WAKE_COMMAND" ]; then
                    # v3.1: RAM throttle — don't wake new agents under memory pressure
                    if check_ram_before_wake; then
                        echo "[$(date -u +%H:%M:%S)] Auto-wake: Executing wake command..."
                        eval "$WAKE_COMMAND" 2>&1 | tail -n 50 || true
                        echo "[$(date -u +%H:%M:%S)] Auto-wake: Wake command completed."
                    else
                        echo "[$(date -u +%H:%M:%S)] Auto-wake: Wake command deferred due to RAM pressure."
                        echo "  The alert has been printed. Run manually when RAM is available:"
                        echo "    $WAKE_COMMAND"
                    fi
                else
                    echo "[$(date -u +%H:%M:%S)] Auto-wake: Alert only (no wake command configured)."
                    echo "  Run: bash .devin/scripts/console.sh \$inbox $AGENT_ID"
                fi
            fi
        done <<< "$event_files"
    fi

    # Quiet status every 30 cycles (~5 min)
    if [ $((CYCLE % 30)) -eq 0 ]; then
        echo "[$(date -u +%H:%M:%S)] Cycle $CYCLE — Monitoring for $AGENT_ID..."
    fi

    sleep "$POLL_INTERVAL"
done

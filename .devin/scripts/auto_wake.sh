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

# ─── Source sync library for session_id lookup ─────────────────────────────
source "$SCRIPT_DIR/sync_lib.sh" 2>/dev/null || true

# ─── v4.2: Resolve Devin CLI binary path ───────────────────────────────────
# devin.exe is not in PATH on Windows. Resolve it via multiple fallbacks.
resolve_devin_cli() {
    # 1. Check if devin is in PATH
    local cli
    cli=$(command -v devin 2>/dev/null || echo "")
    if [ -n "$cli" ] && [ -x "$cli" ]; then
        echo "$cli"
        return 0
    fi
    # 2. Check DEVIN_CLI_PATH env var
    if [ -n "${DEVIN_CLI_PATH:-}" ] && [ -x "$DEVIN_CLI_PATH" ]; then
        echo "$DEVIN_CLI_PATH"
        return 0
    fi
    # 3. Check known Windows install location
    local win_path
    win_path="$HOME/AppData/Local/Programs/Devin/resources/app/extensions/windsurf/devin/bin/devin.exe"
    if [ -x "$win_path" ]; then
        echo "$win_path"
        return 0
    fi
    # 4. Check LOCALAPPDATA
    if [ -n "${LOCALAPPDATA:-}" ]; then
        win_path="$LOCALAPPDATA/Programs/Devin/resources/app/extensions/windsurf/devin/bin/devin.exe"
        if [ -x "$win_path" ]; then
            echo "$win_path"
            return 0
        fi
    fi
    # Not found
    echo ""
    return 1
}

DEVIN_CLI=""
DEVIN_CLI=$(resolve_devin_cli 2>/dev/null || echo "")

# ─── v4.2: Rate limiting — max 1 trigger per agent per 60 seconds ──────────
LAST_TRIGGER_FILE=""
RATE_LIMIT_SECONDS=60

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
LAST_TRIGGER_FILE="$HUB_DIR/.devin/.autowake_last_trigger_${AGENT_ID}.txt"

mkdir -p "$EVENTS_DIR" 2>/dev/null || true
touch "$SEEN_FILE" 2>/dev/null || true

POLL_INTERVAL=10

echo "============================================================"
echo "  AUTO-WAKE DAEMON v4.2 — Agent: $AGENT_ID"
echo "  PID: $$"
echo "  HUB_DIR: $HUB_DIR"
echo "  EVENTS_DIR: $EVENTS_DIR"
echo "  DEVIN_CLI: ${DEVIN_CLI:-<not found>}"
echo "  Rate limit: ${RATE_LIMIT_SECONDS}s"
echo "  Poll interval: ${POLL_INTERVAL}s"
echo "  Wake mode: True Auto-Wake (devin -p --resume --model glm-5.2-high)"
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

                # ─── v4.2: True Auto-Wake — programmatically trigger LLM inference ──
                # Instead of printing alerts or running bash wake commands, we
                # invoke the Devin CLI in non-interactive print mode to resume
                # the target agent's session and inject a prompt. This triggers
                # an actual LLM inference cycle.

                # Check rate limit (max 1 trigger per agent per 60 seconds)
                now_epoch=$(date +%s 2>/dev/null || echo 0)
                last_trigger=0
                if [ -f "$LAST_TRIGGER_FILE" ]; then
                    last_trigger=$(cat "$LAST_TRIGGER_FILE" 2>/dev/null || echo 0)
                fi
                elapsed=$((now_epoch - last_trigger))
                if [ "$elapsed" -lt "$RATE_LIMIT_SECONDS" ]; then
                    echo "[$(date -u +%H:%M:%S)] Auto-wake: Rate limited. Last trigger ${elapsed}s ago, need ${RATE_LIMIT_SECONDS}s. Skipping."
                    continue
                fi

                # v3.1: RAM throttle — don't wake agents under memory pressure
                if ! check_ram_before_wake; then
                    echo "[$(date -u +%H:%M:%S)] Auto-wake: Deferred due to RAM pressure."
                    echo "  Event: $basename_evt — will retry next cycle."
                    continue
                fi

                # Check if Devin CLI is available
                if [ -z "$DEVIN_CLI" ]; then
                    echo "[$(date -u +%H:%M:%S)] Auto-wake: ERROR — Devin CLI binary not found."
                    echo "  Set DEVIN_CLI_PATH env var or ensure devin is in PATH."
                    echo "  Event: $basename_evt — will retry next cycle."
                    continue
                fi

                # Look up the target agent's session_id from agents_sync.json
                target_session_id=$(get_session_id_for_agent "$AGENT_ID" 2>/dev/null || echo "")

                if [ -z "$target_session_id" ]; then
                    echo "[$(date -u +%H:%M:%S)] Auto-wake: No session_id found for $AGENT_ID in agents_sync.json."
                    echo "  Cannot trigger LLM inference without a session ID."
                    echo "  Event: $basename_evt — will retry next cycle."
                    continue
                fi

                # Construct prompt from event file
                event_prompt=$(EVT_FILE="$evt_file" AGENT_ID="$AGENT_ID" node <<'NODE_PROMPT_EOF'
                    const fs = require("fs");
                    try {
                        let raw = fs.readFileSync(process.env.EVT_FILE, "utf8");
                        if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
                        const evt = JSON.parse(raw);
                        const eventType = evt.type || "UNKNOWN";
                        const source = evt.source || "unknown";
                        const payload = evt.payload || {};
                        const payloadStr = JSON.stringify(payload, null, 2).slice(0, 2000);

                        let prompt = `You have received a ${eventType} event from ${source}.\n`;
                        prompt += `Event file: ${process.env.EVT_FILE}\n`;
                        prompt += `Payload:\n${payloadStr}\n\n`;
                        prompt += `Read the event file at ${process.env.EVT_FILE} and execute the requested action.\n`;
                        prompt += `IMPORTANT: After you have processed this event, move the event file to the .devin/events/.archive/ folder so it is not re-processed.\n`;
                        prompt += `Use: mv ${process.env.EVT_FILE} ${process.env.EVT_FILE.replace(/events\//, 'events/.archive/')}\n`;
                        console.log(prompt);
                    } catch(e) {
                        console.log("Error reading event: " + e.message);
                        process.exit(1);
                    }
NODE_PROMPT_EOF
                2>/dev/null || echo "Error: Could not read event $evt_file")

                echo "[$(date -u +%H:%M:%S)] Auto-wake: Triggering LLM inference for $AGENT_ID (session: $target_session_id)"
                echo "  Event: $basename_evt"
                echo "  CLI: $DEVIN_CLI"
                echo "  Prompt length: $(echo "$event_prompt" | wc -c) bytes"

                # Record trigger time for rate limiting
                echo "$now_epoch" > "$LAST_TRIGGER_FILE" 2>/dev/null || true

                # Invoke Devin CLI in non-interactive print mode to resume the
                # target session and inject the prompt. This triggers a real
                # LLM inference cycle on GLM-5.2 High.
                #
                # v4.3 hardening:
                #   - < /dev/null forces immediate crash if auth fails (no TUI picker hang)
                #   - timeout 300 kills the process after 5 minutes if it hangs
                #   - Output written directly to log file (unbuffered, real-time monitoring)
                #
                # Flags:
                #   -p                          — Print mode (non-interactive, exits after one turn)
                #   --resume <SESSION_ID>       — Resume the target agent's existing session
                #   --model glm-5.2-high        — Enforce free GLM-5.2 High model
                #   --permission-mode dangerous — Auto-approve ALL tools (edits + shell) for headless autonomy
                #   --respect-workspace-trust false — Skip workspace trust prompt
                #   -- "<prompt>"               — The constructed prompt
                echo "[$(date -u +%H:%M:%S)] Auto-wake: Invoking devin -p --resume (timeout 300s, stdin=/dev/null)..." >> "$HUB_DIR/.devin/integration_log/auto_wake_${AGENT_ID}.log"

                timeout 300 "$DEVIN_CLI" -p --resume "$target_session_id" \
                    --model glm-5.2-high \
                    --permission-mode dangerous \
                    --respect-workspace-trust false \
                    -- "$event_prompt" < /dev/null >> "$HUB_DIR/.devin/integration_log/auto_wake_${AGENT_ID}.log" 2>&1 || true

                echo "[$(date -u +%H:%M:%S)] Auto-wake: LLM inference cycle completed for $AGENT_ID."
            fi
        done <<< "$event_files"
    fi

    # Quiet status every 30 cycles (~5 min)
    if [ $((CYCLE % 30)) -eq 0 ]; then
        echo "[$(date -u +%H:%M:%S)] Cycle $CYCLE — Monitoring for $AGENT_ID..."
    fi

    sleep "$POLL_INTERVAL"
done

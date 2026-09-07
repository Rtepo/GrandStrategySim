#!/bin/bash
# route_failures.sh — Agent 4 direct audit-fail routing (v3)
#
# Called by run_audit.sh after AUDIT_FAIL event is emitted.
# Parses blueprint_agent_map.json and emits REMEDIATION_REQUESTED
# events directly to workers — NO manager intervention required.
#
# Handles both standard AUDIT_FAIL (blueprint_results) and
# AUDIT_FAIL_ADDENDUM (new_failed_checks) schemas.
#
# Usage: bash .devin/scripts/route_failures.sh <audit_fail_event_file>

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# ─── Validate arguments ────────────────────────────────────────────────────
if [ $# -lt 1 ]; then
    echo "Usage: bash .devin/scripts/route_failures.sh <audit_fail_event_file>" >&2
    exit 1
fi

AUDIT_EVENT="$1"

if [ ! -f "$AUDIT_EVENT" ]; then
    echo "ERROR: Audit event file not found: $AUDIT_EVENT" >&2
    exit 1
fi

# ─── Detect hub directory (same fallbacks as daemon) ──────────────────────
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

# ─── Resolve map file with fallbacks ───────────────────────────────────────
MAP_FILE="$HUB_DIR/.devin/blueprint_agent_map.json"
if [ ! -f "$MAP_FILE" ]; then
    hub_from_script="$(cd "$SCRIPT_DIR/../.." 2>/dev/null && pwd)"
    if [ -n "$hub_from_script" ] && [ -f "$hub_from_script/.devin/blueprint_agent_map.json" ]; then
        MAP_FILE="$hub_from_script/.devin/blueprint_agent_map.json"
        HUB_DIR="$hub_from_script"
    fi
fi
if [ ! -f "$MAP_FILE" ]; then
    if [ -d "$HUB_DIR/.devin/events" ]; then
        hub_from_events="$(cd "$HUB_DIR/.devin/events/../.." 2>/dev/null && pwd)"
        if [ -f "$hub_from_events/.devin/blueprint_agent_map.json" ]; then
            MAP_FILE="$hub_from_events/.devin/blueprint_agent_map.json"
            HUB_DIR="$hub_from_events"
        fi
    fi
fi

if [ ! -f "$MAP_FILE" ]; then
    echo "ERROR: blueprint_agent_map.json not found. Cannot route failures." >&2
    exit 1
fi

echo "=== route_failures.sh: Direct AUDIT_FAIL routing ==="
echo "  Audit event: $(basename "$AUDIT_EVENT")"
echo "  Map file: $MAP_FILE"
echo ""

# ─── Parse and route via Node.js ───────────────────────────────────────────
AUDIT_IN="$AUDIT_EVENT" MAP_IN="$MAP_FILE" SCRIPT_DIR="$SCRIPT_DIR" node <<'NODE_EOF'
    const fs = require("fs");
    const { execSync } = require("child_process");

    function readJson(f) {
        let raw = fs.readFileSync(f, "utf8");
        if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
        return JSON.parse(raw);
    }

    const audit = readJson(process.env.AUDIT_IN);
    const map = readJson(process.env.MAP_IN);
    const scriptDir = process.env.SCRIPT_DIR;
    const auditId = audit.id || "unknown";

    const results = audit.payload.blueprint_results || [];
    const newFailedChecks = audit.payload.new_failed_checks || [];
    const failedChecks = audit.payload.failed_checks || [];

    let routed = 0;

    // Route standard blueprint_results
    for (const result of results) {
        if (result.verdict === "PASS") continue;

        const bpId = result.id;
        const mapping = map[bpId];
        if (!mapping) {
            console.error("  WARNING: No mapping for blueprint " + bpId + " — skipping");
            continue;
        }

        const bpFailures = failedChecks.filter(c => c.startsWith(bpId + ":"));

        const payload = JSON.stringify({
            blueprint_id: bpId,
            branch: mapping.branch,
            verdict: result.verdict,
            reason: result.reason,
            failed_checks: bpFailures,
            audit_fail_event: auditId,
            action: "Fix the listed failures and re-run request_integration.sh"
        }).replace(/'/g, "'\\''");

        try {
            execSync(`bash "${scriptDir}/emit_event.sh" REMEDIATION_REQUESTED agent-4 ${mapping.agent} '${payload}'`, { stdio: "inherit" });
            routed++;
        } catch(e) {
            console.error("  Failed to emit REMEDIATION_REQUESTED for " + bpId);
        }
    }

    // Route AUDIT_FAIL_ADDENDUM new_failed_checks
    for (const check of newFailedChecks) {
        const bpId = check.split(":")[0].trim();
        if (!bpId) continue;

        const mapping = map[bpId];
        if (!mapping) {
            console.error("  WARNING: No mapping for blueprint " + bpId + " (from new_failed_checks) — skipping");
            continue;
        }

        const payload = JSON.stringify({
            blueprint_id: bpId,
            branch: mapping.branch,
            new_failed_checks: [check],
            audit_fail_event: auditId,
            action: "Fix the listed failure and re-run request_integration.sh"
        }).replace(/'/g, "'\\''");

        try {
            execSync(`bash "${scriptDir}/emit_event.sh" REMEDIATION_REQUESTED agent-4 ${mapping.agent} '${payload}'`, { stdio: "inherit" });
            routed++;
        } catch(e) {
            console.error("  Failed to emit REMEDIATION_REQUESTED for " + bpId + " (new_failed_checks)");
        }
    }

    // Emit SYSTEM_ALERT to user
    const alertPayload = JSON.stringify({
        verdict: audit.payload.verdict,
        failed_blueprints: results.filter(r => r.verdict !== "PASS").map(r => r.id),
        new_failed_checks: newFailedChecks,
        requires_remediation: audit.payload.requires_remediation,
        routed_count: routed,
        routed_by: "agent-4-direct"
    }).replace(/'/g, "'\\''");
    try {
        execSync(`bash "${scriptDir}/emit_event.sh" SYSTEM_ALERT agent-4 user '${alertPayload}'`, { stdio: "inherit" });
    } catch(e) {}

    console.log("");
    console.log("  Routed " + routed + " REMEDIATION_REQUESTED events to workers (direct routing).");
NODE_EOF

echo ""
echo "=== route_failures.sh complete ==="

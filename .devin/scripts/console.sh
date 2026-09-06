#!/bin/bash
# console.sh v1.0 — Command Console for SillyElaborateState infrastructure.
#
# The user-facing CLI for daemon management, sprint kickoff, audit triggering,
# and failure routing. All commands maximize internal automation — no manual
# ID or branch hunting required.
#
# Usage: bash .devin/scripts/console.sh <command> [args]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HUB_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
export HUB_DIR

source "$SCRIPT_DIR/sync_lib.sh"

# ─── Command Router ────────────────────────────────────────────────────────
COMMAND="${1:-}"
shift || true

case "$COMMAND" in
    /kickoff)        cmd_kickoff "$@" ;;
    /audit_standard) cmd_audit_standard "$@" ;;
    /forward_fail)   cmd_forward_fail "$@" ;;
    /unblock)        cmd_unblock "$@" ;;
    /help|"")        cmd_help ;;
    *)               echo "Unknown command: $COMMAND"; echo ""; cmd_help; exit 1 ;;
esac

# ─── /kickoff <agent> <blueprint_id> ───────────────────────────────────────
cmd_kickoff() {
    local agent="${1:-}"
    local blueprint_id="${2:-}"

    if [ -z "$agent" ] || [ -z "$blueprint_id" ]; then
        echo "Usage: /kickoff <agent> <blueprint_id>"
        echo "Example: /kickoff agent-3 004-FIX"
        exit 1
    fi

    local roadmap="$HUB_DIR/.devin/tasks/design_review/roadmap.json"
    local map_file="$HUB_DIR/.devin/blueprint_agent_map.json"
    local manifest="$HUB_DIR/.devin/sprint_manifest.txt"
    local sop_file="$HUB_DIR/.devin/SOP.md"

    [ -f "$roadmap" ] || { echo "ERROR: roadmap.json not found at $roadmap"; exit 1; }

    # Step 1: Read roadmap and find blueprint by ID
    local bp_data
    bp_data=$(BLUEPRINT_ID="$blueprint_id" ROADMAP_IN="$roadmap" node <<'NODE_EOF'
        const fs = require("fs");
        let raw = fs.readFileSync(process.env.ROADMAP_IN, "utf8");
        if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
        const r = JSON.parse(raw);
        const targetId = process.env.BLUEPRINT_ID;

        // Try exact match, then partial match (e.g., "004-FIX" matches "blueprint-004")
        let bp = r.blueprints.find(b => b.id === targetId);
        if (!bp) {
            const num = targetId.replace(/^blueprint-/, "").replace(/-FIX$/, "").replace(/^0+/, "");
            bp = r.blueprints.find(b => {
                const bNum = b.id.replace(/^blueprint-/, "").replace(/^0+/, "");
                return bNum === num;
            });
        }

        if (!bp) {
            process.stderr.write("Blueprint not found: " + targetId + "\n");
            process.exit(1);
        }

        console.log([bp.id, bp.branch, bp.task_name, bp.assigned_to || ""].join("|"));
NODE_EOF
    )

    if [ $? -ne 0 ] || [ -z "$bp_data" ]; then
        echo "ERROR: Blueprint '$blueprint_id' not found in roadmap.json"
        exit 1
    fi

    local bp_full_id bp_branch bp_task_name
    bp_full_id=$(echo "$bp_data" | cut -d'|' -f1)
    bp_branch=$(echo "$bp_data" | cut -d'|' -f2)
    bp_task_name=$(echo "$bp_data" | cut -d'|' -f3)

    echo "=== /kickoff: $agent <- $blueprint_id ==="
    echo "  Blueprint:  $bp_full_id"
    echo "  Branch:     $bp_branch"
    echo "  Task:       $bp_task_name"

    # Step 2: Update blueprint_agent_map.json
    MAP_FILE="$map_file" BP_ID="$blueprint_id" AGENT="$agent" BRANCH="$bp_branch" node <<'NODE_EOF'
        const fs = require("fs");
        const mapFile = process.env.MAP_FILE;
        let map = {};
        try {
            let raw = fs.readFileSync(mapFile, "utf8");
            if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
            map = JSON.parse(raw);
        } catch(e) { map = {}; }

        map[process.env.BP_ID] = {
            agent: process.env.AGENT,
            branch: process.env.BRANCH
        };

        const tmp = mapFile + ".tmp";
        fs.writeFileSync(tmp, JSON.stringify(map, null, 2));
        fs.renameSync(tmp, mapFile);
        console.log("  blueprint_agent_map.json updated.");
NODE_EOF

    # Step 3: Append to sprint_manifest.txt (if not already present)
    if ! grep -qxF "$bp_branch" "$manifest" 2>/dev/null; then
        echo "$bp_branch" >> "$manifest"
        echo "  sprint_manifest.txt updated (added: $bp_branch)"
    else
        echo "  sprint_manifest.txt already contains: $bp_branch"
    fi

    # Step 4: Copy task brief from queued/ to active/ (if it exists)
    local queued_brief="$HUB_DIR/.devin/tasks/queued/${blueprint_id}.json"
    local active_brief="$HUB_DIR/.devin/tasks/active/${blueprint_id}_${bp_task_name}.json"
    if [ -f "$queued_brief" ]; then
        mkdir -p "$HUB_DIR/.devin/tasks/active"
        cp "$queued_brief" "$active_brief"
        echo "  Task brief copied to active/: $(basename "$active_brief")"
    fi

    # Step 5: Emit TASK_ASSIGNED event with SOP context
    local sop_content=""
    [ -f "$sop_file" ] && sop_content=$(cat "$sop_file")

    local payload
    payload=$(BP_ID="$blueprint_id" BRANCH="$bp_branch" TASK_NAME="$bp_task_name" AGENT="$agent" SOP="$sop_content" node <<'NODE_EOF'
        const payload = {
            blueprint_id: process.env.BP_ID,
            branch: process.env.BRANCH,
            task_name: process.env.TASK_NAME,
            assigned_to: process.env.AGENT,
            action: "Implement the blueprint. Read the task brief in .devin/tasks/active/. Follow the SOP strictly.",
            sop: process.env.SOP || "",
            instructions: [
                "1. Read the task brief JSON in .devin/tasks/active/",
                "2. Follow the Standard Operating Procedure (SOP) in the sop field",
                "3. Implement in your assigned worktree on branch: " + process.env.BRANCH,
                "4. Run cargo check + cargo test locally before submitting",
                "5. Use: bash .devin/scripts/request_integration.sh when done"
            ]
        };
        console.log(JSON.stringify(payload));
NODE_EOF
    )

    bash "$SCRIPT_DIR/emit_event.sh" "TASK_ASSIGNED" "agent-5" "$agent" "$payload"

    echo ""
    echo "=== /kickoff complete ==="
    echo "  TASK_ASSIGNED emitted to $agent"
    echo "  Branch $bp_branch added to sprint manifest"
    echo "  Blueprint mapping registered"
}

# ─── /audit_standard ───────────────────────────────────────────────────────
cmd_audit_standard() {
    echo "=== /audit_standard: Emitting AUDIT_REQUESTED to Agent 4 ==="

    local payload
    payload=$(node <<'NODE_EOF'
        const rules = [
            "Rule 1: Strict closed-loop economy (double-entry bookkeeping)",
            "Rule 2: Eradicate magic numbers & hardcoded constants",
            "Rule 3: Separation of physics and finance",
            "Rule 4: Complete entity lifecycles (no orphaned structures)",
            "Rule 5: Market forces over command economy",
            "Rule 6: Zero half-measures & no feature stripping",
            "Rule 7: Strict individual accountability (no communization)",
            "Rule 8: Rational economic actors (homo economicus)",
            "Rule 9: Rust-native architecture (borrow checker compliance)",
            "Rule 10: Domain purity over backward compatibility",
            "Rule 11: The No-God rule (asymmetric information)",
            "Rule 12: Strict English-only domain language",
            "Rule 13: Comprehensive technological matrices (no monolithic buildings)",
            "Rule 14: Architectural parsimony (no redundant parallel systems)",
            "Rule 15: Universal physical scaling (no flat rates)",
            "Rule 16: Strict temporal causality (engine sequencing)",
            "Rule 17: Full-stack accountability (backend != feature complete)",
            "Rule 18: Meaningful trade-offs (no strictly dominant strategies)",
            "Rule 19: Strict logistical causality (no teleportation)",
            "Rule 20: Physical boundaries & clamping (no infinite accumulation)",
            "Rule 21: Full-cost accounting & smoothing (no economic suicide)",
            "Rule 22: Strict scope discipline (no opportunistic refactoring)",
            "Rule 23: Concurrent state preservation (no blind overwrites)"
        ];

        const payload = {
            reason: "Manual audit trigger — full 23-rule verification requested",
            verification_targets: rules,
            audit_type: "full_macro_architectural",
            staging_commit: "current_main_head",
            instructions: "Run comprehensive system-wide audit against all 23 Global Rules. Output AUDIT_FAIL as structured JSON with blueprint_results array for automated REMEDIATION_REQUESTED routing."
        };
        console.log(JSON.stringify(payload));
NODE_EOF
    )

    bash "$SCRIPT_DIR/emit_event.sh" "AUDIT_REQUESTED" "agent-5" "agent-4" "$payload"

    echo ""
    echo "=== /audit_standard complete ==="
    echo "  AUDIT_REQUESTED emitted to agent-4"
    echo "  23 Global Rules checklist attached"
    echo "  Agent 4 will output AUDIT_FAIL (structured JSON) or AUDIT_PASS"
}

# ─── /forward_fail [latest|<event_id>] ─────────────────────────────────────
cmd_forward_fail() {
    local target="${1:-latest}"

    echo "=== /forward_fail: Routing AUDIT_FAIL to workers ==="

    local audit_file=""

    if [ "$target" = "latest" ]; then
        audit_file=$(find "$HUB_DIR/.devin/events" -maxdepth 1 -name "*AUDIT_FAIL*" \
            -not -name "*CORRECTION*" -type f 2>/dev/null | sort -r | head -1)
    else
        audit_file=$(find "$HUB_DIR/.devin/events" -maxdepth 1 -name "*AUDIT_FAIL*${target}*" -type f 2>/dev/null | head -1)
    fi

    if [ -z "$audit_file" ]; then
        echo "  No AUDIT_FAIL events found in queue."
        echo "  Checking archive for recent failures..."
        audit_file=$(find "$HUB_DIR/.devin/events/.archive" -maxdepth 1 -name "*AUDIT_FAIL*" \
            -not -name "*CORRECTION*" -type f 2>/dev/null | sort -r | head -1)
        if [ -z "$audit_file" ]; then
            echo "  No AUDIT_FAIL events found in archive either."
            exit 1
        fi
        echo "  Found in archive: $(basename "$audit_file")"
        echo "  NOTE: This event was already processed. Re-routing may emit duplicate REMEDIATION_REQUESTED events."
    else
        echo "  Found: $(basename "$audit_file")"
    fi

    # Display the audit verdict before routing
    echo ""
    echo "  --- Audit Summary ---"
    AUDIT_IN="$audit_file" node <<'NODE_EOF'
        const fs = require("fs");
        try {
            let raw = fs.readFileSync(process.env.AUDIT_IN, "utf8");
            if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
            const evt = JSON.parse(raw);
            console.log("  Verdict: " + evt.payload.verdict);
            console.log("  Requires remediation: " + evt.payload.requires_remediation);
            if (evt.payload.blueprint_results) {
                for (const r of evt.payload.blueprint_results) {
                    console.log("    " + r.id + ": " + r.verdict);
                }
            }
            if (evt.payload.failed_checks) {
                console.log("  Failed checks (" + evt.payload.failed_checks.length + "):");
                for (const c of evt.payload.failed_checks.slice(0, 5)) {
                    console.log("    - " + c.substring(0, 100));
                }
                if (evt.payload.failed_checks.length > 5) {
                    console.log("    ... and " + (evt.payload.failed_checks.length - 5) + " more");
                }
            }
        } catch(e) { console.log("  ERROR parsing: " + e.message); }
NODE_EOF
    echo ""

    # Route to workers
    local map_file="$HUB_DIR/.devin/blueprint_agent_map.json"
    if [ ! -f "$map_file" ]; then
        echo "  ERROR: blueprint_agent_map.json not found. Cannot route."
        exit 1
    fi

    AUDIT_IN="$audit_file" MAP_IN="$map_file" SCRIPT_DIR="$SCRIPT_DIR" node <<'NODE_EOF'
        const fs = require("fs");
        const { execSync } = require("child_process");

        function readJson(f) {
            let raw = fs.readFileSync(f, "utf8");
            if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
            return JSON.parse(raw);
        }

        const audit = readJson(process.env.AUDIT_IN);
        const map = readJson(process.env.MAP_IN);
        const results = audit.payload.blueprint_results || [];
        const failedChecks = audit.payload.failed_checks || [];
        const scriptDir = process.env.SCRIPT_DIR;

        let routed = 0;
        for (const result of results) {
            if (result.verdict === "PASS") continue;

            const bpId = result.id;
            const mapping = map[bpId];
            if (!mapping) {
                console.log("  WARNING: No mapping for " + bpId + " — skipping");
                continue;
            }

            const bpFailures = failedChecks.filter(c => c.startsWith(bpId + ":"));

            const payload = JSON.stringify({
                blueprint_id: bpId,
                branch: mapping.branch,
                verdict: result.verdict,
                reason: result.reason,
                failed_checks: bpFailures,
                audit_fail_event: audit.id,
                action: "Fix the listed failures and re-run request_integration.sh"
            }).replace(/'/g, "'\\''");

            try {
                execSync(`bash "${scriptDir}/emit_event.sh" REMEDIATION_REQUESTED agent-5 ${mapping.agent} '${payload}'`, { stdio: "inherit" });
                routed++;
            } catch(e) {
                console.error("  Failed to emit REMEDIATION_REQUESTED for " + bpId);
            }
        }

        console.log("  Routed " + routed + " REMEDIATION_REQUESTED events to workers.");
NODE_EOF

    echo ""
    echo "=== /forward_fail complete ==="
}

# ─── /unblock <agent> ──────────────────────────────────────────────────────
cmd_unblock() {
    local agent="${1:-}"

    if [ -z "$agent" ]; then
        echo "Usage: /unblock <agent>"
        echo "Example: /unblock agent-3"
        echo ""
        echo "Currently blocked branches:"
        local state_file="$HUB_DIR/.devin/.cicd_failure_state.json"
        if [ -f "$state_file" ]; then
            STATE_IN="$state_file" node <<'NODE_EOF'
                const fs = require("fs");
                try {
                    let raw = fs.readFileSync(process.env.STATE_IN, "utf8");
                    if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
                    const state = JSON.parse(raw);
                    for (const [branch, info] of Object.entries(state.branches || {})) {
                        if (info.blocked) {
                            console.log("  " + branch + " (failures: " + info.consecutive_failures + ", reason: " + (info.last_failure_reason||"unknown") + ")");
                        }
                    }
                } catch(e) { console.log("  (error reading state)"); }
NODE_EOF
        fi
        exit 1
    fi

    echo "=== /unblock: $agent ==="

    local map_file="$HUB_DIR/.devin/blueprint_agent_map.json"
    local state_file="$HUB_DIR/.devin/.cicd_failure_state.json"

    # Step 1: Find the branch for this agent from blueprint_agent_map.json
    local branch=""
    if [ -f "$map_file" ]; then
        branch=$(AGENT="$agent" MAP_IN="$map_file" node <<'NODE_EOF'
            const fs = require("fs");
            try {
                let raw = fs.readFileSync(process.env.MAP_IN, "utf8");
                if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
                const map = JSON.parse(raw);
                for (const [bpId, info] of Object.entries(map)) {
                    if (info.agent === process.env.AGENT) {
                        console.log(info.branch);
                        return;
                    }
                }
            } catch(e) {}
            console.log("");
NODE_EOF
        )
    fi

    if [ -z "$branch" ]; then
        echo "  No branch found for $agent in blueprint_agent_map.json"
        echo "  Checking .cicd_failure_state.json for any blocked branch assigned to this agent..."
        if [ -f "$state_file" ]; then
            branch=$(AGENT="$agent" STATE_IN="$state_file" node <<'NODE_EOF'
                const fs = require("fs");
                try {
                    let raw = fs.readFileSync(process.env.STATE_IN, "utf8");
                    if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
                    const state = JSON.parse(raw);
                    const agentNum = process.env.AGENT.replace(/^agent-/, "");
                    for (const [b, info] of Object.entries(state.branches || {})) {
                        if (info.blocked && b.includes("agent-" + agentNum)) {
                            console.log(b);
                            return;
                        }
                    }
                } catch(e) {}
                console.log("");
NODE_EOF
            )
        fi
    fi

    if [ -z "$branch" ]; then
        echo "  ERROR: Could not determine branch for $agent."
        echo "  Usage: /unblock <agent> — agent must have an entry in blueprint_agent_map.json"
        exit 1
    fi

    echo "  Detected branch: $branch"

    # Step 2: Check if actually blocked
    local is_blocked="false"
    if [ -f "$state_file" ]; then
        is_blocked=$(BRANCH="$branch" STATE_IN="$state_file" node <<'NODE_EOF'
            const fs = require("fs");
            try {
                let raw = fs.readFileSync(process.env.STATE_IN, "utf8");
                if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
                const state = JSON.parse(raw);
                const b = state.branches && state.branches[process.env.BRANCH];
                console.log(b && b.blocked ? "true" : "false");
            } catch(e) { console.log("false"); }
NODE_EOF
        )
    fi

    if [ "$is_blocked" = "false" ]; then
        echo "  Branch $branch is NOT currently blocked. No action needed."
        exit 0
    fi

    # Step 3: Run unblock (inline, bypassing manager auth since console.sh is user-run)
    echo "  Running unblock..."
    BRANCH="$branch" STATE_FILE="$state_file" node <<'NODE_EOF'
        const fs = require("fs");
        const branch = process.env.BRANCH;
        const stateFile = process.env.STATE_FILE;
        let state = { branches: {} };
        try {
            let raw = fs.readFileSync(stateFile, "utf8");
            if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
            state = JSON.parse(raw);
            if (!state.branches) state.branches = {};
        } catch(e) { state = { branches: {} }; }

        if (state.branches[branch]) {
            state.branches[branch].consecutive_failures = 0;
            state.branches[branch].blocked = false;
            state.branches[branch].unblocked_ts = new Date().toISOString();
        }

        const tmp = stateFile + ".tmp";
        fs.writeFileSync(tmp, JSON.stringify(state, null, 2));
        fs.renameSync(tmp, stateFile);
        console.log("  Branch " + branch + " has been unblocked.");
NODE_EOF

    # Step 4: Emit CLARIFICATION_REQUESTED to wake the agent
    local payload
    payload=$(BRANCH="$branch" node <<'NODE_EOF'
        console.log(JSON.stringify({
            branch: process.env.BRANCH,
            reason: "Branch unblocked by manager. Previous CI/CD failures have been cleared.",
            action: "Fix the root cause of previous failures, then re-run: bash .devin/scripts/request_integration.sh"
        }));
NODE_EOF
    )

    bash "$SCRIPT_DIR/emit_event.sh" "CLARIFICATION_REQUESTED" "agent-5" "$agent" "$payload"

    echo ""
    echo "=== /unblock complete ==="
    echo "  Branch $branch unblocked"
    echo "  CLARIFICATION_REQUESTED emitted to $agent"
}

# ─── /help ─────────────────────────────────────────────────────────────────
cmd_help() {
    cat <<'HELP'
============================================================
  Command Console — SillyElaborateState Infrastructure v2.3
============================================================

  /kickoff <agent> <blueprint_id>
    Assign a blueprint to a worker agent. Auto-derives branch from
    roadmap, updates sprint manifest + blueprint map, emits TASK_ASSIGNED.
    Example: /kickoff agent-3 004-FIX

  /audit_standard
    Trigger full 23-rule macro-architectural audit by Agent 4.
    Auto-attaches the Global Rules checklist. No arguments needed.

  /forward_fail [latest|<event_id>]
    Route the most recent AUDIT_FAIL event to responsible workers.
    Auto-emits REMEDIATION_REQUESTED to each failing blueprint's agent.
    Example: /forward_fail latest

  /unblock <agent>
    Clear CI/CD 3-strike block for an agent's branch. Auto-detects the
    branch from blueprint_agent_map.json, unblocks, and wakes the agent.
    Example: /unblock agent-3

  /help
    Show this help screen.

============================================================
HELP
}

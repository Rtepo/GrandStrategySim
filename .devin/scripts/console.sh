#!/bin/bash
# console.sh v1.0 - Command Console for SillyElaborateState infrastructure.
#
# The user-facing CLI for daemon management, sprint kickoff, audit triggering,
# and failure routing. All commands maximize internal automation - no manual
# ID or branch hunting required.
#
# Usage: bash .devin/scripts/console.sh <command> [args]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HUB_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
export HUB_DIR

source "$SCRIPT_DIR/sync_lib.sh"

# --- \$kickoff <agent> <blueprint_id> ---------------------------------------
cmd_kickoff() {
    local agent="${1:-}"
    local blueprint_id="${2:-}"

    if [ -z "$agent" ] || [ -z "$blueprint_id" ]; then
        echo "Usage: \$kickoff <agent> <blueprint_id>"
        echo "Example: \$kickoff agent-3 004-FIX"
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

    echo "=== \$kickoff: $agent <- $blueprint_id ==="
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
    echo "=== \$kickoff complete ==="
    echo "  TASK_ASSIGNED emitted to $agent"
    echo "  Branch $bp_branch added to sprint manifest"
    echo "  Blueprint mapping registered"

    # v3.1: Auto-launch the integration daemon if not already running.
    # A new sprint kickoff means events will be flowing — the daemon must
    # be active to process INTEGRATION_REQUESTED events.
    local pid_file="$HUB_DIR/.devin/.integration_daemon.pid"
    if [ -f "$pid_file" ]; then
        local existing_pid
        existing_pid=$(cat "$pid_file" 2>/dev/null || echo "")
        if [ -n "$existing_pid" ] && kill -0 "$existing_pid" 2>/dev/null; then
            echo "  Daemon already running (PID $existing_pid)."
        else
            rm -f "$pid_file"
            echo "  Daemon PID stale. Launching fresh daemon..."
            bash "$SCRIPT_DIR/launch_daemon.sh" 2>&1 | tail -n 50
        fi
    else
        echo "  No daemon running. Launching for new sprint..."
        bash "$SCRIPT_DIR/launch_daemon.sh" 2>&1 | tail -n 50
    fi
}

# --- \$audit_standard -------------------------------------------------------
cmd_audit_standard() {
    echo "=== \$audit_standard: Emitting AUDIT_REQUESTED to Agent 4 ==="

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
            reason: "Manual audit trigger - full 23-rule verification requested",
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
    echo "=== \$audit_standard complete ==="
    echo "  AUDIT_REQUESTED emitted to agent-4"
    echo "  23 Global Rules checklist attached"
    echo "  Agent 4 will output AUDIT_FAIL (structured JSON) or AUDIT_PASS"
}

# --- \$forward_fail [latest|<event_id>] -------------------------------------
cmd_forward_fail() {
    local target="${1:-latest}"

    echo "=== \$forward_fail: Routing AUDIT_FAIL to workers ==="

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
                console.log("  WARNING: No mapping for " + bpId + " - skipping");
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
    echo "=== \$forward_fail complete ==="
}

# --- \$unblock <agent> ------------------------------------------------------
cmd_unblock() {
    local agent="${1:-}"

    if [ -z "$agent" ]; then
        echo "Usage: \$unblock <agent>"
        echo "Example: \$unblock agent-3"
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

    echo "=== \$unblock: $agent ==="

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
                        process.exit(0);
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
                            process.exit(0);
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
        echo "  Usage: \$unblock <agent> - agent must have an entry in blueprint_agent_map.json"
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
    echo "=== \$unblock complete ==="
    echo "  Branch $branch unblocked"
    echo "  CLARIFICATION_REQUESTED emitted to $agent"
}

# --- \$menu -----------------------------------------------------------------
cmd_help() {
    cat <<'HELP'
============================================================
  Command Console - SillyElaborateState Infrastructure v2.3
============================================================

  $kickoff <agent> <blueprint_id>
    Assign a blueprint to a worker agent. Auto-derives branch from
    roadmap, updates sprint manifest + blueprint map, emits TASK_ASSIGNED.
    Example: $kickoff agent-3 004-FIX

  $audit_standard
    Trigger full 23-rule macro-architectural audit by Agent 4.
    Auto-attaches the Global Rules checklist. No arguments needed.

  $forward_fail [latest|<event_id>]
    Route the most recent AUDIT_FAIL event to responsible workers.
    Auto-emits REMEDIATION_REQUESTED to each failing blueprint's agent.
    Example: $forward_fail latest

  $unblock <agent>
    Clear CI/CD 3-strike block for an agent's branch. Auto-detects the
    branch from blueprint_agent_map.json, unblocks, and wakes the agent.
    Example: $unblock agent-3

  $menu
    Show this help screen.

  $pulse
    Display daemon PID, active sprint branches, and agent strike/block status.

  $logs <agent>
    Show the last 30 lines of the most recent CI/CD log for the agent's branch.

  $override <agent>
    Administrative fast-track merge. Bypasses CI/CD for trivial changes (docs, typos).

  $smoke_main
    Run the headless 50-tick smoke test directly on the main branch.

  $daemon
    Restart the integration daemon (stop + launch). Outputs the new PID.

  $release <version>
    Bump version in Cargo.toml/package.json/tauri.conf.json, commit, tag, and push.

  $inbox <agent>
    Read the NEWEST pending event for an agent. Prints a readable briefing,
    archives ALL pending events (flush), and emits a system directive to
    auto-trigger the worker into action.

  $recover <agent>
    Recover a crashed (OOM) agent. Inspects the worktree, cleans stale
    locks (index.lock, REBASE_HEAD, MERGE_HEAD), and emits RECOVERY_WAKE
    to safely reboot the agent via auto_wake.sh.
    Example: $recover agent-2

  $approve_snapshots [filter]
    Review and approve pending cargo-insta snapshots (.snap.new files).
    Pre-flight: clean tree check, insta installed. Post-review: emits
    SNAPSHOTS_APPROVED event. Does NOT auto-commit approved snapshots.
    Example: $approve_snapshots
    Example: $approve_snapshots market

============================================================
HELP
}

# --- \$pulse - Telemetry dashboard ------------------------------------------
cmd_pulse() {
    echo "=== \$pulse: System Telemetry ==="
    echo ""

    # Daemon status
    local pid_file="$HUB_DIR/.devin/.integration_daemon.pid"
    if [ -f "$pid_file" ]; then
        local pid
        pid=$(cat "$pid_file" 2>/dev/null || echo "")
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            echo "  Daemon: ALIVE (PID $pid)"
        else
            echo "  Daemon: OFF (stale PID file: $pid)"
        fi
    else
        echo "  Daemon: OFF (no PID file)"
    fi
    echo ""

    # Sprint manifest
    local manifest="$HUB_DIR/.devin/sprint_manifest.txt"
    if [ -f "$manifest" ]; then
        local branches
        branches=$(grep -v '^#' "$manifest" | grep -v '^$' | sort -u)
        if [ -n "$branches" ]; then
            echo "  Sprint Manifest:"
            local total=0
            local promoted=0
            while IFS= read -r br; do
                [ -z "$br" ] && continue
                total=$((total + 1))
                local status="PENDING"
                # Check for PROMOTED_TO_MAIN event
                if find "$HUB_DIR/.devin/events" "$HUB_DIR/.devin/events/.archive" \
                    -name "*PROMOTED_TO_MAIN*.json" -type f 2>/dev/null \
                    | xargs grep -l "\"$br\"" 2>/dev/null | grep -q .; then
                    status="PROMOTED"
                    promoted=$((promoted + 1))
                fi
                echo "    $br - $status"
            done <<< "$branches"
            echo "  Sprint progress: $promoted/$total promoted"
        else
            echo "  Sprint Manifest: (empty)"
        fi
    else
        echo "  Sprint Manifest: (not found)"
    fi
    echo ""

    # Agent failure states
    local state_file="$HUB_DIR/.devin/.cicd_failure_state.json"
    local map_file="$HUB_DIR/.devin/blueprint_agent_map.json"
    if [ -f "$state_file" ]; then
        echo "  Agent Strike Status:"
        STATE_IN="$state_file" MAP_IN="$map_file" node <<'NODE_EOF'
            const fs = require("fs");
            try {
                let raw = fs.readFileSync(process.env.STATE_IN, "utf8");
                if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
                const state = JSON.parse(raw);

                let map = {};
                try {
                    let mraw = fs.readFileSync(process.env.MAP_IN, "utf8");
                    if (mraw.charCodeAt(0) === 0xFEFF) mraw = mraw.slice(1);
                    map = JSON.parse(mraw);
                } catch(e) {}

                // Build agent -> branches mapping
                const agentBranches = {};
                for (const [bpId, info] of Object.entries(map)) {
                    if (!agentBranches[info.agent]) agentBranches[info.agent] = [];
                    agentBranches[info.agent].push(info.branch);
                }

                for (const [branch, info] of Object.entries(state.branches || {})) {
                    // Find which agent owns this branch
                    let agent = "unknown";
                    for (const [a, brs] of Object.entries(agentBranches)) {
                        if (brs.includes(branch)) { agent = a; break; }
                    }
                    const blocked = info.blocked ? "BLOCKED" : "active";
                    const strikes = info.consecutive_failures || 0;
                    const reason = info.last_failure_reason || "none";
                    console.log("    " + agent + " | " + branch + " | strikes=" + strikes + " | " + blocked + " | last=" + reason);
                }
            } catch(e) { console.log("    (error reading state)"); }
NODE_EOF
    else
        echo "  Agent Strike Status: (no failure state file)"
    fi
    echo ""
    echo "=== \$pulse complete ==="
}

# --- \$logs <agent> - Quick failure log access ------------------------------
cmd_logs() {
    local agent="${1:-}"

    if [ -z "$agent" ]; then
        echo "Usage: \$logs <agent>"
        echo "Example: \$logs agent-3"
        exit 1
    fi

    echo "=== \$logs: $agent ==="

    local map_file="$HUB_DIR/.devin/blueprint_agent_map.json"
    local log_dir="$HUB_DIR/.devin/integration_log"

    [ -d "$log_dir" ] || { echo "  ERROR: integration_log/ not found."; exit 1; }

    # Find the agent's branch from blueprint_agent_map.json
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
                        process.exit(0);
                    }
                }
            } catch(e) {}
            console.log("");
NODE_EOF
        )
    fi

    if [ -z "$branch" ]; then
        echo "  No branch found for $agent in blueprint_agent_map.json"
        exit 1
    fi

    echo "  Branch: $branch"
    echo ""

    # Sanitize branch name for log file matching (replace / with _)
    local branch_safe="${branch//\//_}"

    # Find the most recent log files for this branch
    local latest_failed=""
    local latest_test=""
    local latest_cicd=""

    latest_failed=$(find "$log_dir" -maxdepth 1 -name "*${branch_safe}_FAILED.txt" -type f 2>/dev/null | sort -r | head -1)
    latest_test=$(find "$log_dir" -maxdepth 1 -name "*${branch_safe}_test.txt" -type f 2>/dev/null | sort -r | head -1)
    latest_cicd=$(find "$log_dir" -maxdepth 1 -name "*${branch_safe}_cicd_output.txt" -type f 2>/dev/null | sort -r | head -1)

    # Priority: FAILED.txt > test.txt > cicd_output.txt
    local target_log=""
    local log_type=""
    if [ -n "$latest_failed" ]; then
        target_log="$latest_failed"
        log_type="FAILED"
    elif [ -n "$latest_test" ]; then
        target_log="$latest_test"
        log_type="TEST"
    elif [ -n "$latest_cicd" ]; then
        target_log="$latest_cicd"
        log_type="CICD_OUTPUT"
    fi

    if [ -z "$target_log" ]; then
        echo "  No log files found for branch: $branch"
        echo "  Searched in: $log_dir"
        exit 1
    fi

    echo "  Log type: $log_type"
    echo "  File: $(basename "$target_log")"
    echo "  --- Last 30 lines ---"
    tail -n 30 "$target_log" 2>/dev/null
    echo "  --- End of log ---"
    echo ""
    echo "=== \$logs complete ==="
}

# --- \$override <agent> - Administrative fast-track merge -------------------
cmd_override() {
    local agent="${1:-}"

    if [ -z "$agent" ]; then
        echo "Usage: \$override <agent>"
        echo "Example: \$override agent-3"
        echo ""
        echo "WARNING: This bypasses CI/CD. Use only for trivial changes (docs, typos)."
        exit 1
    fi

    echo "=== \$override: $agent (ADMINISTRATIVE BYPASS) ==="
    echo "  WARNING: This bypasses the 10-minute CI/CD pipeline."
    echo "  Use ONLY for trivial changes (docs, typos, config)."
    echo ""

    local map_file="$HUB_DIR/.devin/blueprint_agent_map.json"

    # Find the agent's branch
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
                        process.exit(0);
                    }
                }
            } catch(e) {}
            console.log("");
NODE_EOF
        )
    fi

    if [ -z "$branch" ]; then
        echo "  ERROR: Could not determine branch for $agent."
        exit 1
    fi

    echo "  Branch: $branch"

    # Verify branch exists
    if ! git rev-parse --verify "$branch" >/dev/null 2>&1; then
        echo "  ERROR: Branch '$branch' does not exist."
        exit 1
    fi

    # Check for uncommitted changes
    local dirty
    dirty=$(git status --porcelain 2>/dev/null | grep -v '^??' | head -1)
    if [ -n "$dirty" ]; then
        echo "  ERROR: Working tree is dirty. Commit or stash changes first."
        git status --short | head -5
        exit 1
    fi

    # Confirm with user
    echo "  Proceeding with fast-track merge of $branch into main..."
    echo ""

    # Step 1: Checkout main
    git checkout main 2>&1 | tail -1
    if [ $? -ne 0 ]; then
        echo "  ERROR: Cannot checkout main."
        exit 1
    fi

    # Step 2: Merge the branch
    git merge "$branch" --no-edit 2>&1 | tail -5
    local merge_rc=$?
    if [ $merge_rc -ne 0 ]; then
        local conflicts
        conflicts=$(git diff --name-only --diff-filter=U 2>/dev/null | tr '\n' ' ')
        git merge --abort 2>/dev/null || true
        echo "  ERROR: Merge conflict in: $conflicts"
        echo "  Merge aborted. Resolve conflicts manually."
        exit 1
    fi

    # Step 3: Get the resulting commit
    local main_commit
    main_commit=$(git rev-parse HEAD)

    # Step 4: Reset failure state
    local state_file="$HUB_DIR/.devin/.cicd_failure_state.json"
    if [ -f "$state_file" ]; then
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
            }
            const tmp = stateFile + ".tmp";
            fs.writeFileSync(tmp, JSON.stringify(state, null, 2));
            fs.renameSync(tmp, stateFile);
NODE_EOF
    fi

    # Step 5: Emit PROMOTED_TO_MAIN
    bash "$SCRIPT_DIR/emit_event.sh" "PROMOTED_TO_MAIN" "agent-5" "all" \
        "{\"branch\":\"$branch\",\"commit\":\"$main_commit\",\"method\":\"override_bypass\"}" 2>/dev/null

    echo ""
    echo "=== \$override complete ==="
    echo "  Branch $branch merged to main @ $main_commit"
    echo "  PROMOTED_TO_MAIN emitted (method: override_bypass)"
    echo "  Failure state reset for $branch"
}

# --- \$smoke_main - Run smoke test on main ----------------------------------
cmd_smoke_main() {
    echo "=== \$smoke_main: Headless 50-tick smoke test on main ==="
    echo ""

    # Verify we're on main
    local current_branch
    current_branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
    if [ "$current_branch" != "main" ]; then
        echo "  WARNING: Currently on branch '$current_branch', not 'main'."
        echo "  Switching to main..."
        git checkout main 2>&1 | tail -1
    fi

    echo "  Branch: $(git rev-parse --abbrev-ref HEAD)"
    echo "  Commit: $(git rev-parse --short HEAD)"
    echo "  Running: cargo test --workspace --test headless_smoke_test -- headless_50_tick_smoke --nocapture"
    echo "  (timeout: 600s)"
    echo ""

    timeout 600 cargo test --workspace --test headless_smoke_test -- headless_50_tick_smoke --nocapture 2>&1
    local rc=$?

    echo ""
    if [ $rc -eq 0 ]; then
        echo "=== \$smoke_main: PASS ==="
    elif [ $rc -eq 124 ]; then
        echo "=== \$smoke_main: TIMEOUT (exceeded 600s) ==="
    else
        echo "=== \$smoke_main: FAIL (rc=$rc) ==="
    fi
}

# --- \$daemon - Restart the integration daemon ------------------------------
cmd_daemon() {
    echo "=== \$daemon: Restarting integration daemon ==="
    echo ""

    # Step 1: Stop existing daemon
    echo "  Stopping existing daemon..."
    bash "$SCRIPT_DIR/stop_daemon.sh" 2>&1 || true
    echo ""

    # Clear stale PID file if present
    local pid_file="$HUB_DIR/.devin/.integration_daemon.pid"
    if [ -f "$pid_file" ]; then
        rm -f "$pid_file"
    fi

    # Step 2: Launch fresh daemon
    echo "  Launching fresh daemon..."
    bash "$SCRIPT_DIR/launch_daemon.sh" 2>&1
    local launch_rc=$?

    if [ $launch_rc -ne 0 ]; then
        echo "  ERROR: Daemon launch failed."
        exit 1
    fi

    echo ""
    echo "=== \$daemon complete ==="
}

# --- \$release <version> - Version bump, tag, and push ----------------------
cmd_release() {
    local version="${1:-}"

    if [ -z "$version" ]; then
        echo "Usage: \$release <version>"
        echo "Example: \$release 1.2.0"
        exit 1
    fi

    # Strip leading 'v' if present
    version="${version#v}"

    echo "=== \$release: v$version ==="
    echo ""

    # Verify clean tree
    local dirty
    dirty=$(git status --porcelain 2>/dev/null | grep -v '^??' | head -1 || true)
    if [ -n "$dirty" ]; then
        echo "  ERROR: Working tree is dirty. Commit changes first."
        git status --short | head -5
        exit 1
    fi

    # Verify on main
    local current_branch
    current_branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
    if [ "$current_branch" != "main" ]; then
        echo "  ERROR: Must be on 'main' branch (currently on '$current_branch')."
        exit 1
    fi

    # v3.1: Stop the integration daemon before release.
    # The daemon must not run idle between milestones. Releasing marks the
    # end of a sprint cycle, so the daemon is stopped to free resources.
    echo "  Stopping integration daemon (end of sprint cycle)..."
    bash "$SCRIPT_DIR/stop_daemon.sh" 2>&1 || true
    echo ""

    echo "  Bumping version to $version in:"

    # Cargo.toml (workspace root)
    local cargo_toml="$HUB_DIR/Cargo.toml"
    if [ -f "$cargo_toml" ]; then
        VERSION="$version" FILE="$cargo_toml" node <<'NODE_EOF'
            const fs = require("fs");
            const f = process.env.FILE;
            let content = fs.readFileSync(f, "utf8");
            // Replace version = "x.y.z" in the [workspace.package] section
            content = content.replace(
                /^(\[workspace\.package\][\s\S]*?version\s*=\s*)"[^"]*"/m,
                '$1"' + process.env.VERSION + '"'
            );
            // Also replace top-level version if no workspace.package section
            if (!content.includes("[workspace.package]")) {
                content = content.replace(
                    /^version\s*=\s*"[^"]*"/m,
                    'version = "' + process.env.VERSION + '"'
                );
            }
            fs.writeFileSync(f, content);
            console.log("    Cargo.toml (workspace root)");
NODE_EOF
    fi

    # state/Cargo.toml
    local state_cargo="$HUB_DIR/state/Cargo.toml"
    if [ -f "$state_cargo" ]; then
        VERSION="$version" FILE="$state_cargo" node <<'NODE_EOF'
            const fs = require("fs");
            const f = process.env.FILE;
            let content = fs.readFileSync(f, "utf8");
            content = content.replace(
                /^version\s*=\s*"[^"]*"/m,
                'version = "' + process.env.VERSION + '"'
            );
            fs.writeFileSync(f, content);
            console.log("    state/Cargo.toml");
NODE_EOF
    fi

    # src-tauri/Cargo.toml
    local tauri_cargo="$HUB_DIR/src-tauri/Cargo.toml"
    if [ -f "$tauri_cargo" ]; then
        VERSION="$version" FILE="$tauri_cargo" node <<'NODE_EOF'
            const fs = require("fs");
            const f = process.env.FILE;
            let content = fs.readFileSync(f, "utf8");
            content = content.replace(
                /^version\s*=\s*"[^"]*"/m,
                'version = "' + process.env.VERSION + '"'
            );
            fs.writeFileSync(f, content);
            console.log("    src-tauri/Cargo.toml");
NODE_EOF
    fi

    # package.json
    local pkg_json="$HUB_DIR/package.json"
    if [ -f "$pkg_json" ]; then
        VERSION="$version" FILE="$pkg_json" node <<'NODE_EOF'
            const fs = require("fs");
            const f = process.env.FILE;
            let content = fs.readFileSync(f, "utf8");
            content = content.replace(
                /"version"\s*:\s*"[^"]*"/,
                '"version": "' + process.env.VERSION + '"'
            );
            fs.writeFileSync(f, content);
            console.log("    package.json");
NODE_EOF
    fi

    # src-tauri/tauri.conf.json
    local tauri_conf="$HUB_DIR/src-tauri/tauri.conf.json"
    if [ -f "$tauri_conf" ]; then
        VERSION="$version" FILE="$tauri_conf" node <<'NODE_EOF'
            const fs = require("fs");
            const f = process.env.FILE;
            let content = fs.readFileSync(f, "utf8");
            // Tauri config has "version" field and possibly "package" > "version"
            content = content.replace(
                /"version"\s*:\s*"[^"]*"/g,
                '"version": "' + process.env.VERSION + '"'
            );
            fs.writeFileSync(f, content);
            console.log("    src-tauri/tauri.conf.json");
NODE_EOF
    fi

    echo ""

    # Commit the version bump
    echo "  Committing version bump..."
    git add -A 2>/dev/null
    git commit -m "chore: bump version to v$version" 2>&1 | tail -2
    local commit_rc=$?
    if [ $commit_rc -ne 0 ]; then
        echo "  WARNING: git commit returned $commit_rc (may be nothing to commit)"
    fi

    # Create annotated tag
    echo "  Creating annotated tag v$version..."
    git tag -a "v$version" -m "Release v$version" 2>&1
    local tag_rc=$?
    if [ $tag_rc -ne 0 ]; then
        echo "  WARNING: git tag returned $tag_rc (tag may already exist)"
    fi

    # Push commit and tag
    echo "  Pushing commit to origin..."
    git push origin main 2>&1 | tail -3
    echo "  Pushing tag to origin..."
    git push origin "v$version" 2>&1 | tail -3

    echo ""
    echo "=== \$release complete ==="
    echo "  Version: v$version"
    echo "  Tag: v$version"
    echo "  Pushed to origin"
    echo "  Daemon stopped. Run \$daemon to restart for the next sprint."
}

# --- $inbox <agent> - Worker self-service event reader (newest wins) --------
cmd_inbox() {
    local agent="${1:-}"

    if [ -z "$agent" ]; then
        echo "Usage: \$inbox <agent>"
        echo "Example: \$inbox agent-3"
        echo ""
        echo "Reads the NEWEST pending event targeting this agent, prints a readable"
        echo "briefing, archives ALL pending events for the agent (flush), and emits"
        echo "a system directive to auto-trigger the worker into action."
        exit 1
    fi

    echo "=== \$inbox: $agent ==="
    echo ""

    local events_dir="$HUB_DIR/.devin/events"
    local archive_dir="$HUB_DIR/.devin/events/.archive"

    [ -d "$events_dir" ] || { echo "  ERROR: events directory not found."; exit 1; }
    mkdir -p "$archive_dir"

    # Find the NEWEST JSON file where target matches this agent,
    # and collect ALL matching files for archiving.
    local result
    result=$(AGENT="$agent" EVENTS_DIR="$events_dir" node <<'NODE_EOF'
        const fs = require("fs");
        const dir = process.env.EVENTS_DIR;
        const agent = process.env.AGENT;
        try {
            const files = fs.readdirSync(dir)
                .filter(f => f.endsWith(".json") && !f.startsWith("."))
                .map(f => {
                    const full = dir + "/" + f;
                    try {
                        let raw = fs.readFileSync(full, "utf8");
                        if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
                        const evt = JSON.parse(raw);
                        return { file: full, name: f, evt };
                    } catch(e) { return null; }
                })
                .filter(x => x && x.evt.target === agent);

            if (files.length === 0) {
                console.log("NONE|0");
                process.exit(0);
            }

            // Sort by filename descending (newest first)
            files.sort((a, b) => b.name.localeCompare(a.name));
            const newest = files[0].file;
            const all = files.map(f => f.file).join(";");
            console.log(newest + "|" + all);
        } catch(e) {
            process.stderr.write("Error scanning events: " + e.message + "\n");
            process.exit(1);
        }
NODE_EOF
    )

    if [ -z "$result" ] || [ "$result" = "NONE|0" ]; then
        echo "  Inbox is empty. No pending events for $agent."
        echo ""
        echo "=== \$inbox complete ==="
        exit 0
    fi

    local event_file
    local all_files
    event_file=$(echo "$result" | cut -d'|' -f1)
    all_files=$(echo "$result" | cut -d'|' -f2)

    local total_count
    total_count=$(echo "$all_files" | tr ';' '\n' | grep -c '.')

    echo "  Newest event: $(basename "$event_file")"
    echo "  Pending events for $agent: $total_count (all will be archived)"
    echo ""

    # Parse and display the NEWEST event in a highly readable format
    EVENT_FILE="$event_file" node <<'NODE_EOF'
        const fs = require("fs");
        try {
            let raw = fs.readFileSync(process.env.EVENT_FILE, "utf8");
            if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
            const evt = JSON.parse(raw);
            const p = evt.payload || {};

            console.log("  +------------------------------------------------------------+");
            console.log("  |  INCOMING EVENT BRIEFING (NEWEST)                          |");
            console.log("  +------------------------------------------------------------+");
            console.log("");
            console.log("  Event ID:    " + (evt.id || "N/A"));
            console.log("  Event Type:  " + (evt.type || "UNKNOWN"));
            console.log("  From:        " + (evt.source || "N/A"));
            console.log("  To:          " + (evt.target || "N/A"));
            console.log("  Timestamp:   " + (evt.timestamp || "N/A"));
            console.log("");

            if (p.blueprint_id || p.branch) {
                console.log("  --- Task Details ---");
                if (p.blueprint_id) console.log("  Blueprint:   " + p.blueprint_id);
                if (p.branch)      console.log("  Branch:      " + p.branch);
                if (p.task_name)   console.log("  Task Name:   " + p.task_name);
                if (p.verdict)     console.log("  Verdict:     " + p.verdict);
                console.log("");
            }

            if (p.failed_checks && p.failed_checks.length > 0) {
                console.log("  --- Failed Checks (" + p.failed_checks.length + ") ---");
                for (const c of p.failed_checks) {
                    console.log("    - " + c);
                }
                console.log("");
            }

            if (p.reason) {
                console.log("  --- Reason ---");
                console.log("    " + p.reason);
                console.log("");
            }

            if (p.action) {
                console.log("  --- Action Required ---");
                console.log("    " + p.action);
                console.log("");
            }

            if (p.instructions && p.instructions.length > 0) {
                console.log("  --- Instructions ---");
                for (const inst of p.instructions) {
                    console.log("    " + inst);
                }
                console.log("");
            }

            if (p.sop) {
                console.log("  --- SOP (Standard Operating Procedure) ---");
                const sopLines = p.sop.split("\n").slice(0, 20);
                for (const line of sopLines) {
                    console.log("    " + line);
                }
                if (p.sop.split("\n").length > 20) {
                    console.log("    ... (truncated, see .devin/SOP.md for full text)");
                }
                console.log("");
            }

            if (p.audit_fail_event) {
                console.log("  --- Audit Reference ---");
                console.log("    Audit Fail Event ID: " + p.audit_fail_event);
                console.log("");
            }

            if (p.constraints && p.constraints.length > 0) {
                console.log("  --- Constraints ---");
                for (const c of p.constraints) {
                    console.log("    - " + c);
                }
                console.log("");
            }

            if (p.deliverables && p.deliverables.length > 0) {
                console.log("  --- Deliverables ---");
                for (const d of p.deliverables) {
                    console.log("    - " + d);
                }
                console.log("");
            }

            console.log("  ==============================================================");
        } catch(e) {
            console.log("  ERROR parsing event: " + e.message);
        }
NODE_EOF

    # Archive ALL pending events for this agent — safe per-file target verification
    # Re-scans events_dir, parses each JSON, and ONLY moves files where
    # target === agent. No blanket wildcards. No stale list reuse.
    local archived_count
    archived_count=$(AGENT="$agent" EVENTS_DIR="$events_dir" ARCHIVE_DIR="$archive_dir" node <<'NODE_EOF'
        const fs = require("fs");
        const path = require("path");
        const dir = process.env.EVENTS_DIR;
        const archiveDir = process.env.ARCHIVE_DIR;
        const agent = process.env.AGENT;
        let count = 0;
        try {
            const files = fs.readdirSync(dir)
                .filter(f => f.endsWith(".json") && !f.startsWith("."));
            for (const f of files) {
                const src = path.join(dir, f);
                try {
                    let raw = fs.readFileSync(src, "utf8");
                    if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
                    const evt = JSON.parse(raw);
                    // STRICT: only move if target exactly matches this agent
                    if (evt.target === agent) {
                        const dst = path.join(archiveDir, f);
                        fs.renameSync(src, dst);
                        count++;
                    }
                } catch(e) {
                    // Skip unparseable or invalid JSON — do NOT move
                    continue;
                }
            }
        } catch(e) {
            process.stderr.write("Archive error: " + e.message + "\n");
        }
        console.log(count);
NODE_EOF
    )

    echo ""
    echo "  Archived $archived_count event(s) to .devin/events/.archive/"
    echo "  Inbox for $agent is now EMPTY."
    echo ""
    echo "=== SYSTEM DIRECTIVE ==="
    echo "You have read your task. Do not wait for further user prompts. Immediately begin implementing the fixes/tasks in your assigned worktree. Run local checks (cargo check/test) when done, and submit using request_integration.sh."
    echo ""
    echo "=== \$inbox complete ==="
}

# --- $approve_snapshots [filter] - Review cargo-insta snapshots (v4) --------
cmd_approve_snapshots() {
    local filter="${1:-}"

    # 1. Pre-flight: verify clean working tree (no uncommitted source changes)
    if ! git diff --quiet -- state/src/ state/tests/ 2>/dev/null; then
        echo "ERROR: Uncommitted source changes detected."
        echo "       Commit or stash before reviewing snapshots."
        echo ""
        git status --short -- state/src/ state/tests/ 2>/dev/null | head -10
        exit 1
    fi

    # 2. Pre-flight: verify cargo-insta is installed
    if ! command -v cargo-insta &>/dev/null; then
        echo "ERROR: cargo-insta not installed."
        echo "       Run: cargo install cargo-insta --locked"
        exit 1
    fi

    # 3. Check for pending snapshots
    local pending
    pending=$(find state/src state/tests -name '*.snap.new' 2>/dev/null | wc -l)
    if [ "$pending" -eq 0 ]; then
        echo "No pending snapshots to review."
        exit 0
    fi
    echo "Found $pending pending snapshot(s) to review."

    # 4. Run cargo insta review with optional filter
    echo ""
    echo "Starting interactive snapshot review..."
    echo "  (accept: <a>, reject: <r>, skip: <s>)"
    echo ""
    if [ -n "$filter" ]; then
        cargo insta review --accept-unseen -- "$filter"
    else
        cargo insta review --accept-unseen
    fi
    local review_rc=$?

    # 5. Post-review: report results
    local remaining
    remaining=$(find state/src state/tests -name '*.snap.new' 2>/dev/null | wc -l)
    local approved=$((pending - remaining))

    echo ""
    echo "Snapshot review complete:"
    echo "  Approved: $approved"
    echo "  Rejected/Skipped: $remaining"

    # 6. Emit event for audit trail
    if [ "$approved" -gt 0 ]; then
        bash .devin/scripts/emit_event.sh \
            "SNAPSHOTS_APPROVED" \
            "agent-5" \
            "all" \
            "{\"approved\":$approved,\"remaining\":$remaining}" \
            2>/dev/null || true
    fi

    # 7. Remind to commit
    if [ "$approved" -gt 0 ]; then
        echo ""
        echo "NOTE: Approved snapshots are in state/src/snapshots/."
        echo "      Commit them with: git add state/src/snapshots/ && git commit"
    fi

    exit $review_rc
}

# --- $recover <agent> - Centralized OOM crash recovery (v3.1) ---------------
cmd_recover() {
    local agent="${1:-}"

    if [ -z "$agent" ]; then
        echo "Usage: \$recover <agent>"
        echo "Example: \$recover agent-2"
        echo ""
        echo "Inspects the crashed agent's worktree, cleans stale locks,"
        echo "and emits RECOVERY_WAKE to safely reboot the agent via auto_wake.sh."
        exit 1
    fi

    echo "=== \$recover: $agent ==="
    echo ""

    # Step 1: Resolve agent's worktree path
    local map_file="$HUB_DIR/.devin/blueprint_agent_map.json"
    local worktree_path=""
    local agent_branch=""

    # Try blueprint_agent_map.json first
    if [ -f "$map_file" ]; then
        agent_branch=$(AGENT="$agent" MAP_FILE="$map_file" node <<'NODE_EOF'
            const fs = require("fs");
            try {
                let raw = fs.readFileSync(process.env.MAP_FILE, "utf8");
                if (raw.charCodeAt(0) === 0xFEFF) raw = raw.slice(1);
                const map = JSON.parse(raw);
                for (const [bpId, mapping] of Object.entries(map)) {
                    if (mapping.agent === process.env.AGENT) {
                        console.log(mapping.branch);
                        process.exit(0);
                    }
                }
            } catch(e) {}
            process.exit(0);
NODE_EOF
        )
    fi

    # Derive worktree path using the convention ../SillyElaborateState-<agent>
    local wt_candidate="$HUB_DIR/../SillyElaborateState-$agent"
    if [ -d "$wt_candidate" ]; then
        worktree_path="$(cd "$wt_candidate" 2>/dev/null && pwd)"
    fi

    # Fallback: scan parent dir for matching worktrees
    if [ -z "$worktree_path" ]; then
        local wt_found
        wt_found=$(find "$HUB_DIR/.." -maxdepth 1 -type d -name "SillyElaborateState-${agent}*" 2>/dev/null | head -1)
        if [ -n "$wt_found" ]; then
            worktree_path="$(cd "$wt_found" 2>/dev/null && pwd)"
        fi
    fi

    if [ -z "$worktree_path" ]; then
        echo "  WARNING: No worktree found for $agent."
        echo "  Emitting RECOVERY_WAKE event anyway — agent can resume from any location."
    else
        echo "  Worktree: $worktree_path"
        echo "  Branch:   ${agent_branch:-unknown}"
    fi
    echo ""

    # Step 2: Inspect working tree
    local cleaned_files=""
    if [ -n "$worktree_path" ] && [ -d "$worktree_path/.git" -o -f "$worktree_path/.git" ]; then
        echo "  Inspecting working tree..."
        local wt_status
        wt_status=$(cd "$worktree_path" 2>/dev/null && git status --porcelain 2>/dev/null | head -10 || echo "")
        if [ -n "$wt_status" ]; then
            echo "  Working tree is NOT clean. Files (first 10):"
            echo "$wt_status" | sed 's/^/    /'
        else
            echo "  Working tree is clean."
        fi
        echo ""

        # Step 3: Clean up corrupt temp files/locks
        echo "  Cleaning stale locks..."

        # Remove stale index.lock
        if [ -f "$worktree_path/.git/index.lock" ]; then
            rm -f "$worktree_path/.git/index.lock" 2>/dev/null && cleaned_files="${cleaned_files}index.lock "
        fi

        # Remove stale rebase/merge state files (interrupted by crash)
        for stale_file in REBASE_HEAD MERGE_HEAD MERGE_MSG MERGE_MODE CHERRY_PICK_HEAD; do
            if [ -f "$worktree_path/.git/$stale_file" ]; then
                rm -f "$worktree_path/.git/$stale_file" 2>/dev/null && cleaned_files="${cleaned_files}${stale_file} "
            fi
        done

        # Note: Do NOT remove ORIG_HEAD (needed for recovery)
        # Note: Do NOT use git reset --hard or git stash pop (banned by AGENTS.md)

        if [ -n "$cleaned_files" ]; then
            echo "  Cleaned: $cleaned_files"
        else
            echo "  No stale locks found."
        fi
    fi

    # Clean hub-level daemon temp files
    local hub_cleaned=""
    for tmp_file in "$HUB_DIR"/.devin/.daemon_event_*.env; do
        [ -f "$tmp_file" ] || continue
        rm -f "$tmp_file" 2>/dev/null && hub_cleaned="${hub_cleaned}$(basename "$tmp_file") "
    done
    if [ -n "$hub_cleaned" ]; then
        echo "  Cleaned hub temp files: $hub_cleaned"
    fi
    echo ""

    # Step 4: Emit RECOVERY_WAKE event
    local payload
    payload=$(node -e '
        const p = {
            reason: "OOM crash recovery",
            worktree: process.env.WT || "",
            branch: process.env.BR || "",
            action: "Run $inbox to resume your task. Your working tree has been inspected and locks cleaned. No work was lost.",
            cleaned_files: process.env.CF || ""
        };
        console.log(JSON.stringify(p));
    ' WT="$worktree_path" BR="$agent_branch" CF="$cleaned_files $hub_cleaned")

    bash "$SCRIPT_DIR/emit_event.sh" "RECOVERY_WAKE" "agent-5" "$agent" "$payload"

    echo ""
    echo "=== \$recover complete ==="
    echo "  RECOVERY_WAKE emitted to $agent"
    echo "  auto_wake.sh will detect this event and reboot the agent"
    if [ -n "$worktree_path" ]; then
        echo "  Agent should run: cd $worktree_path && bash .devin/scripts/console.sh \$inbox $agent"
    fi
}

# --- Command Router (must be after all function definitions) ---------------
COMMAND="${1:-}"
shift || true

case "$COMMAND" in
    \$kickoff)        cmd_kickoff "$@" ;;
    \$audit_standard) cmd_audit_standard "$@" ;;
    \$forward_fail)   cmd_forward_fail "$@" ;;
    \$unblock)        cmd_unblock "$@" ;;
    \$pulse)          cmd_pulse "$@" ;;
    \$logs)           cmd_logs "$@" ;;
    \$override)       cmd_override "$@" ;;
    \$smoke_main)     cmd_smoke_main "$@" ;;
    \$daemon)         cmd_daemon "$@" ;;
    \$release)        cmd_release "$@" ;;
    \$inbox)          cmd_inbox "$@" ;;
    \$recover)        cmd_recover "$@" ;;
    \$approve_snapshots) cmd_approve_snapshots "$@" ;;
    \$menu|"")        cmd_help ;;
    *)               echo "Unknown command: $COMMAND"; echo ""; cmd_help; exit 1 ;;
esac

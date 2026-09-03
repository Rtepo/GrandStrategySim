#!/bin/bash
# sync_lib.sh — Shared library for multi-agent synchronization.
#
# Provides:
#   - sync_transactional: Strict transactional git sync loop with surgical revert (C1).
#   - sync_read_json: Pull latest and read agents_sync.json.
#   - json_check_conflict: Check if directories conflict with another active agent.
#   - Mutator functions: register, unlock, heartbeat, check_and_register.
#
# All JSON manipulation uses `node -e` (jq is not installed on this machine).
# The transactional loop NEVER uses `git reset --hard` (AGENTS.md Rule 2).
# On push rejection, it uses surgical revert: `git reset HEAD~1` (mixed) +
# `git checkout -- agents_sync.json` (discard only the ledger file).

# ─── Configuration ─────────────────────────────────────────────────────────
LEDGER_FILE="agents_sync.json"
REAP_MS=$((15 * 60 * 1000))  # 15 minutes in milliseconds
GIT_REMOTE="origin"
GIT_BRANCH="main"

# ─── Helper: Check for index.lock (C3) ─────────────────────────────────────
index_lock_exists() {
    [ -f ".git/index.lock" ]
}

# ─── Helper: Atomic local write of JSON ────────────────────────────────────
# Writes content to a temp file then mv (prevents corruption from concurrent
# writes within a single machine).
atomic_write_json() {
    local content="$1"
    local tmp="${LEDGER_FILE}.tmp.$$"
    printf '%s' "$content" > "$tmp"
    mv "$tmp" "$LEDGER_FILE"
}

# ─── C1: Strict Transactional Git Sync Loop ────────────────────────────────
#
# Usage: sync_transactional <mutator_fn> <commit_msg> [max_attempts]
#
# The mutator_fn is a bash function that:
#   1. Reads agents_sync.json from the working tree (just pulled)
#   2. Applies its mutation via node -e
#   3. Writes the result back to agents_sync.json
#   4. Returns 0 on success, 1 on failure
#
# The mutator is called INSIDE the loop, re-reading freshly pulled state
# on every attempt. This prevents stale-state overwrite (Fatal Flaw 2).
#
# On push rejection: surgical revert (NOT git reset --hard):
#   1. git reset HEAD~1 (mixed reset — undoes commit, keeps working tree)
#   2. git checkout -- agents_sync.json (discard ONLY the ledger file)
#   3. Retry with fresh pull
#
# This preserves all other uncommitted working-tree files.
sync_transactional() {
    local mutator_fn="$1"
    local commit_msg="$2"
    local max_attempts="${3:-5}"
    local attempt=0

    while [ "$attempt" -lt "$max_attempts" ]; do
        attempt=$((attempt + 1))

        # C3: Check for index.lock — skip if another git process is running
        if index_lock_exists; then
            sleep 2
            continue
        fi

        # Step 1: Pull latest remote state
        git pull --rebase "$GIT_REMOTE" "$GIT_BRANCH" 2>/dev/null
        local pull_rc=$?
        if [ "$pull_rc" -ne 0 ]; then
            # Rebase conflict — abort and retry
            git rebase --abort 2>/dev/null
            sleep 2
            continue
        fi

        # Step 2: Apply mutation AGAINST FRESH STATE (inside the loop!)
        "$mutator_fn"
        local mut_rc=$?
        if [ "$mut_rc" -ne 0 ]; then
            sleep 2
            continue
        fi

        # Step 3: Check index.lock again before staging
        if index_lock_exists; then
            sleep 2
            continue
        fi

        # Stage ONLY agents_sync.json (never git add -A)
        git add "$LEDGER_FILE"
        git commit -m "$commit_msg" 2>/dev/null
        local commit_rc=$?
        if [ "$commit_rc" -ne 0 ]; then
            # Nothing to commit (no change needed) — success
            return 0
        fi

        # Step 4: Push
        git push "$GIT_REMOTE" "$GIT_BRANCH" 2>/dev/null
        local push_rc=$?
        if [ "$push_rc" -eq 0 ]; then
            return 0  # Success!
        fi

        # Step 5: Push rejected (race condition) — SURGICAL REVERT (v4)
        # Do NOT use git reset --hard (destroys entire working tree).
        # Instead:
        #   1. git reset HEAD~1 (mixed reset — undoes commit, keeps working tree)
        #   2. git checkout -- agents_sync.json (discard ONLY the ledger file)
        # This preserves all other uncommitted work by the agent.
        git reset HEAD~1 2>/dev/null
        git checkout -- "$LEDGER_FILE" 2>/dev/null
        sleep 2
    done

    echo "ERROR: sync_transactional failed after $max_attempts attempts" >&2
    return 1
}

# ─── Read JSON (pull + cat) ────────────────────────────────────────────────
# Pulls latest main and outputs agents_sync.json content.
# For read-only inspection (conflict checks, ledger display).
sync_read_json() {
    git pull --rebase "$GIT_REMOTE" "$GIT_BRANCH" 2>/dev/null
    cat "$LEDGER_FILE" 2>/dev/null
}

# ─── Check for directory conflicts (pure node, no git ops) ─────────────────
# Usage: json_check_conflict <json_string> <dir1,dir2,...>
# Outputs conflicting agent_id if any dir is locked by another active agent.
# Outputs empty string if no conflict.
json_check_conflict() {
    local json="$1"
    local dirs="$2"
    echo "$json" | node -e '
        const fs = require("fs");
        let input = "";
        process.stdin.on("data", d => input += d);
        process.stdin.on("end", () => {
            const data = JSON.parse(input);
            const dirs = process.argv[1] ? process.argv[1].split(",") : [];
            const sessionId = process.env.SESSION_ID || "";
            for (const agent of data.agents) {
                if (agent.status !== "active") continue;
                if (agent.session_id === sessionId) continue; // Skip self
                for (const dir of dirs) {
                    for (const locked of (agent.locked_dirs || [])) {
                        if (dir === locked || dir.startsWith(locked) || locked.startsWith(dir)) {
                            console.log(agent.agent_id);
                            return;
                        }
                    }
                }
            }
            console.log(""); // No conflict
        });
    ' "$dirs"
}

# ─── Mutator: Register agent (reap zombies + add/update entry) ─────────────
# Reads agents_sync.json from working tree, applies change, writes back.
# Uses env vars: SESSION_ID, AGENT_ID, AGENT_BRANCH, LOCKED_DIRS, AGENT_TASK
mutator_register_agent() {
    node -e '
        const fs = require("fs");
        const data = JSON.parse(fs.readFileSync("agents_sync.json", "utf8"));
        const sessionId = process.env.SESSION_ID;
        const agentId = process.env.AGENT_ID;
        const branch = process.env.AGENT_BRANCH || "";
        const dirs = (process.env.LOCKED_DIRS || "").split(",").filter(d => d);
        const task = process.env.AGENT_TASK || "";

        // C2: Reap zombies — clear agents with stale heartbeats
        const now = Date.now();
        const REAP_MS = 15 * 60 * 1000;
        data.agents = data.agents.map(a => {
            if (a.status === "active" && a.last_heartbeat) {
                const age = now - new Date(a.last_heartbeat).getTime();
                if (age > REAP_MS) {
                    a.status = "reaped";
                    a.locked_dirs = [];
                }
            }
            return a;
        });

        // Find or create this agent entry
        let agent = data.agents.find(a => a.session_id === sessionId);
        if (!agent) {
            agent = { agent_id: agentId, session_id: sessionId };
            data.agents.push(agent);
        }
        agent.branch = branch;
        agent.locked_dirs = dirs;
        agent.task = task;
        agent.status = "active";
        agent.registered_at = new Date().toISOString();
        agent.last_heartbeat = new Date().toISOString();
        data.last_updated = new Date().toISOString();

        fs.writeFileSync("agents_sync.json", JSON.stringify(data, null, 2));
    '
}

# ─── Mutator: Check conflicts + Register (combined) ────────────────────────
# Returns 0 on success, 1 on conflict (with error message to stderr).
mutator_check_and_register() {
    local json
    json=$(cat "$LEDGER_FILE" 2>/dev/null)
    local dirs="${LOCKED_DIRS:-}"
    local conflict
    conflict=$(echo "$json" | node -e '
        const fs = require("fs");
        let input = "";
        process.stdin.on("data", d => input += d);
        process.stdin.on("end", () => {
            const data = JSON.parse(input);
            const dirs = (process.argv[1] || "").split(",").filter(d => d);
            const sessionId = process.env.SESSION_ID || "";
            for (const agent of data.agents) {
                if (agent.status !== "active") continue;
                if (agent.session_id === sessionId) continue;
                for (const dir of dirs) {
                    for (const locked of (agent.locked_dirs || [])) {
                        if (dir === locked || dir.startsWith(locked) || locked.startsWith(dir)) {
                            process.stderr.write("CONFLICT: Directory " + dir + " is locked by agent " + agent.agent_id + " (branch: " + (agent.branch||"unknown") + ", task: " + (agent.task||"unknown") + ")\n");
                            console.log("conflict");
                            return;
                        }
                    }
                }
            }
            console.log("ok");
        });
    ' "$dirs")

    if [ "$conflict" = "conflict" ]; then
        return 1
    fi

    # No conflict — proceed with registration
    mutator_register_agent
}

# ─── Mutator: Unlock agent (set inactive, clear locks) ─────────────────────
# Uses env vars: SESSION_ID
mutator_unlock_agent() {
    node -e '
        const fs = require("fs");
        const data = JSON.parse(fs.readFileSync("agents_sync.json", "utf8"));
        const sessionId = process.env.SESSION_ID;

        // C2: Reap zombies while we are at it
        const now = Date.now();
        const REAP_MS = 15 * 60 * 1000;
        data.agents = data.agents.map(a => {
            if (a.status === "active" && a.last_heartbeat && a.session_id !== sessionId) {
                const age = now - new Date(a.last_heartbeat).getTime();
                if (age > REAP_MS) {
                    a.status = "reaped";
                    a.locked_dirs = [];
                }
            }
            return a;
        });

        // Unlock this agent
        let agent = data.agents.find(a => a.session_id === sessionId);
        if (agent) {
            agent.status = "inactive";
            agent.locked_dirs = [];
            agent.session_id = null;
        }
        data.last_updated = new Date().toISOString();

        fs.writeFileSync("agents_sync.json", JSON.stringify(data, null, 2));
    '
}

# ─── Mutator: Heartbeat (update last_heartbeat + reap zombies) ─────────────
# Uses env vars: SESSION_ID
mutator_heartbeat() {
    node -e '
        const fs = require("fs");
        const data = JSON.parse(fs.readFileSync("agents_sync.json", "utf8"));
        const sessionId = process.env.SESSION_ID;
        const now = Date.now();
        const REAP_MS = 15 * 60 * 1000;

        // C2: Reap zombies
        data.agents = data.agents.map(a => {
            if (a.status === "active" && a.last_heartbeat && a.session_id !== sessionId) {
                const age = now - new Date(a.last_heartbeat).getTime();
                if (age > REAP_MS) {
                    a.status = "reaped";
                    a.locked_dirs = [];
                }
            }
            return a;
        });

        // Update this agent heartbeat
        let agent = data.agents.find(a => a.session_id === sessionId);
        if (agent) {
            agent.last_heartbeat = new Date().toISOString();
        }
        data.last_updated = new Date().toISOString();

        fs.writeFileSync("agents_sync.json", JSON.stringify(data, null, 2));
    '
}

# ─── Helper: Get current ledger as additionalContext string ────────────────
# Outputs a human-readable summary of all active agents for injection into
# the agent context via hookSpecificOutput.additionalContext.
ledger_summary() {
    cat "$LEDGER_FILE" 2>/dev/null | node -e '
        let input = "";
        process.stdin.on("data", d => input += d);
        process.stdin.on("end", () => {
            try {
                const data = JSON.parse(input);
                console.log("=== Multi-Agent Sync Ledger ===");
                for (const a of data.agents) {
                    if (a.status === "active") {
                        const dirs = (a.locked_dirs || []).join(", ");
                        console.log("  " + a.agent_id + " [" + a.status + "] branch=" + (a.branch||"none") + " dirs=[" + dirs + "] task=" + (a.task||"none"));
                    }
                }
                console.log("=== End Ledger ===");
            } catch(e) {
                console.log("Ledger read error: " + e.message);
            }
        });
    '
}

# ─── Get blockers targeting this agent (C5: Blocker Bus) ───────────────────
# Reads agents_sync.json and outputs any cross_agent_blockers where
# to_agent matches AGENT_ID or "all". Outputs a formatted warning string.
# Uses env vars: AGENT_ID
get_blockers_for_agent() {
    cat "$LEDGER_FILE" 2>/dev/null | node -e '
        let input = "";
        process.stdin.on("data", d => input += d);
        process.stdin.on("end", () => {
            try {
                const data = JSON.parse(input);
                const myAgentId = process.env.AGENT_ID || "";
                const blockers = data.cross_agent_blockers || [];
                const relevant = blockers.filter(b =>
                    b.to_agent === myAgentId || b.to_agent === "all"
                );
                if (relevant.length === 0) {
                    console.log("");  // No blockers
                    return;
                }
                console.log("⚠️  CROSS-AGENT BLOCKERS TARGETING YOU (" + myAgentId + "):");
                for (const b of relevant) {
                    console.log("  FROM: " + b.from_agent + " | FILE: " + b.affected_file);
                    console.log("    MSG: " + b.message);
                    console.log("    TIME: " + b.timestamp);
                }
            } catch(e) {
                console.log("");
            }
        });
    '
}

# ─── Mutator: Add a cross-agent blocker (C5: Blocker Bus) ──────────────────
# Reads agents_sync.json from working tree, appends a blocker entry,
# writes back. Uses env vars: AGENT_ID (from), BLOCKER_TO, BLOCKER_FILE,
# BLOCKER_MSG
mutator_add_blocker() {
    node -e '
        const fs = require("fs");
        const data = JSON.parse(fs.readFileSync("agents_sync.json", "utf8"));
        const fromAgent = process.env.AGENT_ID || "unknown";
        const toAgent = process.env.BLOCKER_TO || "all";
        const affectedFile = process.env.BLOCKER_FILE || "";
        const message = process.env.BLOCKER_MSG || "";

        if (!data.cross_agent_blockers) {
            data.cross_agent_blockers = [];
        }

        // Remove any existing blocker from the same agent on the same file
        // to avoid duplicates (upsert behavior)
        data.cross_agent_blockers = data.cross_agent_blockers.filter(b =>
            !(b.from_agent === fromAgent && b.affected_file === affectedFile)
        );

        data.cross_agent_blockers.push({
            from_agent: fromAgent,
            to_agent: toAgent,
            affected_file: affectedFile,
            message: message,
            timestamp: new Date().toISOString()
        });

        data.last_updated = new Date().toISOString();
        fs.writeFileSync("agents_sync.json", JSON.stringify(data, null, 2));
    '
}

# ─── Mutator: Clear blockers from this agent (on session end) ──────────────
# Removes all blockers where from_agent matches AGENT_ID.
# Uses env vars: AGENT_ID
mutator_clear_my_blockers() {
    node -e '
        const fs = require("fs");
        const data = JSON.parse(fs.readFileSync("agents_sync.json", "utf8"));
        const myAgentId = process.env.AGENT_ID || "";

        if (data.cross_agent_blockers) {
            data.cross_agent_blockers = data.cross_agent_blockers.filter(b =>
                b.from_agent !== myAgentId
            );
        }

        data.last_updated = new Date().toISOString();
        fs.writeFileSync("agents_sync.json", JSON.stringify(data, null, 2));
    '
}

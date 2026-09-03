#!/bin/bash
# view_blockers.sh — CLI: topological dependency graph visualization.
#
# Usage: bash .devin/scripts/view_blockers.sh
#
# Parses agents_sync.json and outputs a topological graph of agent
# dependencies in the terminal. Read-only — no manager token required.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/sync_lib.sh"

cat "$LEDGER_FILE" 2>/dev/null | node -e '
    let input = "";
    process.stdin.on("data", d => input += d);
    process.stdin.on("end", () => {
        try {
            const data = JSON.parse(input);
            const blockers = data.cross_agent_blockers || [];
            const agents = data.agents || [];

            // Build adjacency list: who blocks whom
            const blocks = {};   // from -> [to]
            const blockedBy = {}; // to -> [from]
            const agentIds = new Set();

            for (const a of agents) agentIds.add(a.agent_id);

            for (const b of blockers) {
                agentIds.add(b.from_agent);
                agentIds.add(b.to_agent);
                if (!blocks[b.from_agent]) blocks[b.from_agent] = [];
                if (!blockedBy[b.to_agent]) blockedBy[b.to_agent] = [];
                blocks[b.from_agent].push({to: b.to_agent, file: b.affected_file, msg: b.message});
                blockedBy[b.to_agent].push(b.from_agent);
            }

            // Topological sort (Kahn'"'"'s algorithm)
            const inDegree = {};
            for (const id of agentIds) inDegree[id] = 0;
            for (const id of agentIds) {
                if (blockedBy[id]) inDegree[id] = blockedBy[id].length;
            }
            const queue = [];
            for (const id of agentIds) {
                if (inDegree[id] === 0) queue.push(id);
            }
            const topoOrder = [];
            while (queue.length > 0) {
                const node = queue.shift();
                topoOrder.push(node);
                if (blocks[node]) {
                    for (const edge of blocks[node]) {
                        inDegree[edge.to]--;
                        if (inDegree[edge.to] === 0) queue.push(edge.to);
                    }
                }
            }

            // Detect cycles
            const hasCycle = topoOrder.length < agentIds.size;

            // Find bottleneck (most incoming blockers)
            let bottleneck = "";
            let maxIncoming = 0;
            for (const id of agentIds) {
                const count = blockedBy[id] ? blockedBy[id].length : 0;
                if (count > maxIncoming) {
                    maxIncoming = count;
                    bottleneck = id;
                }
            }

            // Print graph
            const W = 64;
            const line = "═".repeat(W);
            console.log("╔" + line + "╗");
            console.log("║" + "          CROSS-AGENT BLOCKER DEPENDENCY GRAPH".padEnd(W) + "║");
            console.log("╠" + line + "╣");

            if (blockers.length === 0) {
                console.log("║" + "  No blockers — all agents are unblocked.".padEnd(W) + "║");
            } else {
                for (const b of blockers) {
                    const arrow = " ──blocks──► ";
                    const line1 = "  " + b.from_agent + arrow + b.to_agent;
                    console.log("║" + line1.padEnd(W) + "║");
                    const fileLine = "    file: " + (b.affected_file || "unknown");
                    console.log("║" + fileLine.padEnd(W) + "║");
                    const msg = b.message || "";
                    const msgLines = msg.match(/.{1,55}/g) || [msg];
                    for (const ml of msgLines) {
                        console.log("║" + ("    \"" + ml + "\"").padEnd(W) + "║");
                    }
                    console.log("║" + "".padEnd(W) + "║");
                }
            }

            console.log("╠" + line + "╣");
            console.log("║" + "  TOPOLOGICAL ORDER:".padEnd(W) + "║");
            if (hasCycle) {
                console.log("║" + "  ⚠️  CIRCULAR DEPENDENCY DETECTED — cannot sort".padEnd(W) + "║");
            } else {
                for (let i = 0; i < topoOrder.length; i++) {
                    const incoming = blockedBy[topoOrder[i]] ? blockedBy[topoOrder[i]].length : 0;
                    const outgoing = blocks[topoOrder[i]] ? blocks[topoOrder[i]].length : 0;
                    const label = "  " + (i+1) + ". " + topoOrder[i] +
                        " (in:" + incoming + " out:" + outgoing + ")";
                    console.log("║" + label.padEnd(W) + "║");
                }
            }

            console.log("║" + "".padEnd(W) + "║");
            if (bottleneck) {
                const bn = "  BOTTLENECK: " + bottleneck + " (" + maxIncoming + " incoming blockers)";
                console.log("║" + bn.padEnd(W) + "║");
            }
            console.log("╚" + line + "╝");
        } catch(e) {
            console.log("Error reading ledger: " + e.message);
        }
    });
'

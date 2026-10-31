#!/bin/bash
# task_ctl.sh — Daemon v2 atomic task ledger manager.
#
# Ledger: .devin/tasks/backlog.json (single file, one JSON doc).
# Status machine: PENDING → IN_PROGRESS → VERIFYING → DONE
#                 any → BLOCKED → PENDING (via unblock)
# DONE is ONLY reachable through `verify`, which runs verify_task.sh —
# a task physically cannot be marked DONE on backend-only work.
#
# Usage:
#   bash .devin/scripts/task_ctl.sh list [--status S]
#   bash .devin/scripts/task_ctl.sh show <TASK-ID>
#   bash .devin/scripts/task_ctl.sh add <JSON-task-object>
#   bash .devin/scripts/task_ctl.sh assign <TASK-ID> <agent-N>
#   bash .devin/scripts/task_ctl.sh start <TASK-ID> <agent-N>
#   bash .devin/scripts/task_ctl.sh verify <TASK-ID>
#   bash .devin/scripts/task_ctl.sh done <TASK-ID>        # alias of verify
#   bash .devin/scripts/task_ctl.sh block <TASK-ID> "reason"
#   bash .devin/scripts/task_ctl.sh unblock <TASK-ID>
#
# All JSON mutation via `node -e` (jq is not installed on this machine).
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

LEDGER=".devin/tasks/backlog.json"
[ -f "$LEDGER" ] || { echo "✗ no ledger at $LEDGER" >&2; exit 1; }

# _set <TASK-ID> <js-expr-on-task>` — atomic mutate-in-place via tmp+rename.
_set() {
    local tid="$1"; local expr="$2"; local extra="${3:-}"
    node -e '
        const fs = require("fs");
        const ledger = process.argv[1];
        const tid = process.argv[2];
        const extra = process.argv[3] || "";
        const doc = JSON.parse(fs.readFileSync(ledger, "utf8"));
        const t = doc.tasks.find(t => t.id === tid);
        if (!t) { console.error("✗ task " + tid + " not found"); process.exit(2); }
        const now = new Date().toISOString();
        eval(process.argv[4]);
        t.updated_at = now;
        doc.updated_at = now;
        const tmp = ledger + ".tmp";
        fs.writeFileSync(tmp, JSON.stringify(doc, null, 2));
        fs.renameSync(tmp, ledger);
        console.log("✓ " + tid + " → " + t.status);
    ' "$LEDGER" "$tid" "$extra" "$expr" || return $?
}

cmd="${1:-list}"; shift || true

case "$cmd" in
    list)
        node -e '
            const doc = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
            const want = process.argv[2] || "";
            for (const t of doc.tasks) {
                if (want && t.status !== want) continue;
                console.log(
                    t.id.padEnd(10) + t.status.padEnd(12) +
                    (t.assigned_daemon || "-").padEnd(10) + t.title
                );
            }
        ' "$LEDGER" "${1:-}"
        ;;
    show)
        node -e '
            const doc = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
            const t = doc.tasks.find(t => t.id === process.argv[2]);
            if (!t) { console.error("✗ not found"); process.exit(2); }
            console.log(JSON.stringify(t, null, 2));
        ' "$LEDGER" "$1"
        ;;
    add)
        node -e '
            const fs = require("fs");
            const doc = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
            const t = JSON.parse(process.argv[2]);
            if (!t.id) { console.error("✗ task needs id"); process.exit(2); }
            if (doc.tasks.find(x => x.id === t.id)) {
                console.error("✗ " + t.id + " already exists"); process.exit(2);
            }
            t.status = t.status || "PENDING";
            t.verify_history = t.verify_history || [];
            const now = new Date().toISOString();
            t.created_at = t.created_at || now; t.updated_at = now;
            doc.tasks.push(t); doc.updated_at = now;
            const tmp = process.argv[1] + ".tmp";
            fs.writeFileSync(tmp, JSON.stringify(doc, null, 2));
            fs.renameSync(tmp, process.argv[1]);
            console.log("✓ added " + t.id);
        ' "$LEDGER" "$1"
        ;;
    assign)
        _set "$1" "t.assigned_daemon = extra;" "$2"
        ;;
    start)
        _set "$1" "if (t.status !== 'PENDING' && t.status !== 'BLOCKED') { console.error('✗ cannot start from ' + t.status); process.exit(3); } t.status = 'IN_PROGRESS'; if (extra) t.assigned_daemon = extra;" "$2"
        ;;
    verify|done)
        tid="$1"
        _set "$tid" "if (t.status === 'DONE') { console.log('✓ already DONE'); process.exit(0); } t.status = 'VERIFYING';" || exit $?
        if bash .devin/scripts/verify_task.sh "$tid"; then
            _set "$tid" "t.status = 'DONE';"
            echo "✓ $tid DONE — full-stack gate passed"
        else
            _set "$tid" "t.status = 'IN_PROGRESS'; (t.verify_history = t.verify_history || []).push({at: new Date().toISOString(), result: 'FAIL'});"
            echo "✗ $tid gate FAILED — status back to IN_PROGRESS (backend-only work is not DONE)" >&2
            exit 1
        fi
        ;;
    block)
        _set "$1" "t.status = 'BLOCKED'; t.blocked_reason = extra; (t.verify_history = t.verify_history || []).push({at: new Date().toISOString(), result: 'BLOCKED', reason: extra});" "$2"
        ;;
    unblock)
        _set "$1" "t.status = 'PENDING'; delete t.blocked_reason;"
        ;;
    *)
        grep '^# Usage' "$0" -A10 | sed 's/^# //'
        exit 2
        ;;
esac

#!/bin/bash
# verify_task.sh — Full-stack task gate (anti-Goodharting).
#
# Usage: bash .devin/scripts/verify_task.sh <TASK-ID> [--skip-compile]
#        bash .devin/scripts/verify_task.sh --task-file <json> [--skip-compile]
#
# A task passes ONLY if ALL of:
#   1. every required_rust_files entry shows a diff vs base_ref OR is
#      dirty/untracked in the working tree (in-flight work counts);
#   2. every required_ui_files entry shows a diff vs base_ref OR is dirty —
#      a task that never touches its React .tsx CANNOT pass;
#   3. every ui_ast_grep_checks pattern is found in its .tsx file
#      (feature-specific regexes, e.g. expandable state handlers, drawer
#      components — NOT generic `useState`);
#   4. fast compile gate: `tsc --noEmit` when UI files are involved
#      (skipped with --skip-compile).
#
# Result is recorded to .devin/tasks/results/<TASK-ID>.verify.json.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

LEDGER=".devin/tasks/backlog.json"
RESULTS_DIR=".devin/tasks/results"
SKIP_COMPILE=0
TASK_FILE=""
TID=""

while [ $# -gt 0 ]; do
    case "$1" in
        --skip-compile) SKIP_COMPILE=1; shift ;;
        --task-file) TASK_FILE="$2"; shift 2 ;;
        *) TID="$1"; shift ;;
    esac
done

if [ -n "$TASK_FILE" ]; then
    SRC="$TASK_FILE"
else
    [ -n "$TID" ] || { echo "Usage: verify_task.sh <TASK-ID> [--skip-compile]" >&2; exit 2; }
    SRC="$LEDGER"
fi

# Emit the task object as single-line JSON for the shell to consume.
TASK_JSON=$(node -e '
    const fs = require("fs");
    const src = process.argv[1];
    let t;
    if (src.endsWith("backlog.json")) {
        const doc = JSON.parse(fs.readFileSync(src, "utf8"));
        t = doc.tasks.find(x => x.id === process.argv[2]);
    } else {
        t = JSON.parse(fs.readFileSync(src, "utf8"));
        if (!t.id && t.tasks) t = t.tasks[0];
    }
    if (!t) { console.error("✗ task not found"); process.exit(2); }
    console.log(JSON.stringify(t));
' "$SRC" "$TID") || exit 2

TID=$(echo "$TASK_JSON" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).id))')
BASE=$(echo "$TASK_JSON" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const t=JSON.parse(s);console.log(t.base_ref||"origin/main")})')
mkdir -p "$RESULTS_DIR"
REPORT="$RESULTS_DIR/$TID.verify.json"

echo "=== verify_task $TID (base: $BASE) ==="
FAILS=0
CHECKS=""

note() {  # note <name> <pass|fail> <detail>
    local icon="✓"; [ "$2" = "fail" ] && { icon="✗"; FAILS=$((FAILS+1)); }
    echo "  $icon $1: $3"
    local safe="${3//\"/\'}"
    CHECKS="$CHECKS{\"check\":\"$1\",\"result\":\"$2\",\"detail\":\"$safe\"},"
}

# ── 1+2: required file diffs ───────────────────────────────────────────────
CHANGED=$( { git diff --name-only "$BASE"...HEAD 2>/dev/null; \
             git status --porcelain 2>/dev/null | awk '{print $2}'; } | sort -u )

check_required() {
    local kind="$1"
    echo "$TASK_JSON" | node -e '
        let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{
            const t=JSON.parse(s);
            for (const f of (t[process.argv[1]]||[])) console.log(f);
        });
    ' "$kind" | while IFS= read -r f; do
        [ -z "$f" ] && continue
        if echo "$CHANGED" | grep -qxF "$f"; then
            echo "OK:$f"
        else
            echo "MISSING:$f"
        fi
    done
}

RUST_OUT=$(check_required required_rust_files)
while IFS= read -r line; do
    [ -z "$line" ] && continue
    case "$line" in
        OK:*)      note "rust_diff" "pass" "${line#OK:} changed" ;;
        MISSING:*) note "rust_diff" "fail" "${line#MISSING:} has NO diff vs $BASE" ;;
    esac
done <<< "$RUST_OUT"

UI_OUT=$(check_required required_ui_files)
UI_COUNT=0
while IFS= read -r line; do
    [ -z "$line" ] && continue
    UI_COUNT=$((UI_COUNT+1))
    case "$line" in
        OK:*)      note "ui_diff" "pass" "${line#OK:} changed" ;;
        MISSING:*) note "ui_diff" "fail" "${line#MISSING:} has NO diff vs $BASE — React work skipped" ;;
    esac
done <<< "$UI_OUT"

# ── 3: UI grep checks ──────────────────────────────────────────────────────
while IFS= read -r check; do
    [ -z "$check" ] && continue
    cfile=$(echo "$check" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).file))')
    cpat=$(echo "$check" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).pattern))')
    cdesc=$(echo "$check" | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const c=JSON.parse(s);console.log(c.desc||"")})')
    if [ ! -f "$cfile" ]; then
        note "ui_grep" "fail" "$cfile missing on disk ($cdesc)"
        continue
    fi
    if grep -qE "$cpat" "$cfile"; then
        note "ui_grep" "pass" "$cfile =~ /$cpat/ ($cdesc)"
    else
        note "ui_grep" "fail" "$cfile lacks /$cpat/ ($cdesc)"
    fi
done < <(echo "$TASK_JSON" | node -e '
    let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{
        const t=JSON.parse(s);
        for (const c of (t.ui_ast_grep_checks||[])) console.log(JSON.stringify(c));
    });
')

# ── 4: fast compile gate ───────────────────────────────────────────────────
if [ "$SKIP_COMPILE" = "0" ] && [ "$UI_COUNT" -gt 0 ]; then
    if npx tsc --noEmit >/tmp/tsc_verify_$TID.log 2>&1; then
        note "tsc" "pass" "tsc --noEmit clean"
    else
        note "tsc" "fail" "tsc --noEmit errors (see /tmp/tsc_verify_$TID.log)"
    fi
fi

# ── verdict + report ───────────────────────────────────────────────────────
CHECKS="${CHECKS%,}"
printf '{\n  "task": "%s",\n  "at": "%s",\n  "result": "%s",\n  "checks": [%s]\n}\n' \
    "$TID" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    "$([ "$FAILS" = "0" ] && echo PASS || echo FAIL)" "$CHECKS" > "$REPORT"

if [ "$FAILS" = "0" ]; then
    echo "=== $TID: PASS — report at $REPORT ==="
    exit 0
else
    echo "=== $TID: FAIL ($FAILS failing checks) — report at $REPORT ==="
    exit 1
fi

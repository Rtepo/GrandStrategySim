#!/bin/bash
# daemon_supervisor.sh — Daemon v2 OS-level watchdog & auto-restart supervisor.
#
# Runs OUTSIDE the LLM session. Manages one worker process bound to one task:
#   - Zero-loss bootstrap: every (re)launch is fed BOOT_PROMPT.md + task spec
#     + current worktree diff via a regenerated context.md — instructions can
#     never be lost to a crash.
#   - Rate-limit recovery: worker exit with 429/rate_limit/quota/overloaded in
#     its log tail → cooldown sleep (SUP_RATE_SLEEP, default 300s) → relaunch
#     the same task.
#   - OOM/crash recovery: exit 137/OOM marker → rotate run log, snapshot
#     uncommitted diffs (WIP commit attempt, else .devin/wip/<task>.patch),
#     clean transient build artifacts, relaunch.
#   - Budget: > SUP_MAX_RESTARTS (default 10) failed attempts → task BLOCKED.
#
# Usage:
#   bash .devin/scripts/daemon_supervisor.sh <TASK-ID> <agent-N> <worktree> \
#        [--cmd "cmd with {CTX} placeholder"]
#   bash .devin/scripts/daemon_supervisor.sh dispatch <TASK-ID> [...] \
#        [--wt TASK-ID=worktree-path ...]
#
# Env: SUP_WORKER_CMD (verbatim worker command; bypasses CLI probing),
#      SUP_RUNNER_CLI (explicit CLI path), SUP_RUNNER_ORDER (default
#      "devin claude codex"), SUP_MODEL (optional model flag for devin),
#      SUP_WORKTREES (space-separated worktree paths for dispatch mode),
#      SUP_MAX_PARALLEL (cap concurrent workers, default = #worktrees),
#      SUP_POLL_SLEEP=10, SUP_RATE_SLEEP=300,
#      SUP_MAX_RESTARTS=10, SUP_LOG_MAX_MB=50, SUP_KEEP_LOGS=5
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

# ─── v4.27.0: CLI runner probing — no hard dependency on one binary ────────
# Priority: SUP_WORKER_CMD verbatim > SUP_RUNNER_CLI explicit path >
# PATH probe over SUP_RUNNER_ORDER > known install locations.
probe_worker_cli() {
    if [ -n "${SUP_RUNNER_CLI:-}" ] && [ -x "$SUP_RUNNER_CLI" ]; then
        echo "$SUP_RUNNER_CLI"; return 0
    fi
    local cli c
    for cli in ${SUP_RUNNER_ORDER:-devin claude codex}; do
        c=$(command -v "$cli" 2>/dev/null || true)
        [ -n "$c" ] && { echo "$c"; return 0; }
    done
    local -a candidates=(
        "${DEVIN_CLI_PATH:-}"
        "${LOCALAPPDATA:-}/Programs/Devin/resources/app/extensions/windsurf/devin/bin/devin.exe"
        "$HOME/AppData/Local/Programs/Devin/resources/app/extensions/windsurf/devin/bin/devin.exe"
        "$HOME/.local/bin/claude" "$HOME/.local/bin/codex"
        "/usr/local/bin/devin" "/usr/local/bin/claude" "/usr/local/bin/codex"
    )
    for c in "${candidates[@]}"; do
        [ -n "$c" ] && [ -x "$c" ] && { echo "$c"; return 0; }
    done
    return 1
}

# Map a runner binary to a non-interactive print-mode command template.
# "{CTX}" is replaced with the context file path downstream.
build_runner_cmd() {
    local cli="$1" base
    base=$(basename "$cli" | tr 'A-Z' 'a-z')
    case "$base" in
        devin|devin.exe)
            echo "\"$cli\" -p ${SUP_MODEL:+--model \"$SUP_MODEL\" }--permission-mode dangerous --respect-workspace-trust false -- \"\$(cat '{CTX}')\""
            ;;
        claude|claude.exe)
            echo "\"$cli\" -p --dangerously-skip-permissions \"\$(cat '{CTX}')\""
            ;;
        codex|codex.exe)
            echo "\"$cli\" exec --full-auto \"\$(cat '{CTX}')\""
            ;;
        *)  # Unknown runner — assume -p print-mode convention
            echo "\"$cli\" -p \"\$(cat '{CTX}')\""
            ;;
    esac
}

# ─── v4.27.0: dispatch mode — parallel workers across worktrees ────────────
# Fans out tasks to isolated git worktrees, spawning one supervisor per task
# as a background subshell with PID tracking, so file transforms, cargo
# builds and verify gates run concurrently across cores.
if [ "${1:-}" = "dispatch" ]; then
    shift
    POLL_SLEEP="${SUP_POLL_SLEEP:-10}"
    RATE_SLEEP="${SUP_RATE_SLEEP:-300}"
    MAX_RESTARTS="${SUP_MAX_RESTARTS:-10}"
    declare -a TASKS=()
    declare -A WT_MAP=()
    while [ $# -gt 0 ]; do
        case "$1" in
            --wt) WT_MAP["${2%%=*}"]="${2#*=}"; shift 2 ;;
            *) TASKS+=("$1"); shift ;;
        esac
    done
    [ "${#TASKS[@]}" -gt 0 ] || { echo "Usage: daemon_supervisor.sh dispatch <TASK-ID>... [--wt TASK=path ...]" >&2; exit 2; }

    # Worktree pool: explicit --wt targets > SUP_WORKTREES > auto-scan.
    declare -a WTS=()
    if [ "${#WT_MAP[@]}" -gt 0 ]; then
        for k in "${!WT_MAP[@]}"; do WTS+=("${WT_MAP[$k]}"); done
    elif [ -n "${SUP_WORKTREES:-}" ]; then
        read -r -a WTS <<< "$SUP_WORKTREES"
    else
        for d in .devin/worktrees/agent-*/ .devin/worktrees/v4270-ws*/; do
            [ -d "$d" ] && WTS+=("${d%/}")
        done
    fi
    # Dedupe worktree pool
    if [ "${#WTS[@]}" -gt 0 ]; then
        read -r -a WTS <<< "$(printf '%s\n' "${WTS[@]}" | sort -u | tr '\n' ' ')"
    fi
    MAXPAR="${SUP_MAX_PARALLEL:-${#WTS[@]}}"
    [ "$MAXPAR" -lt 1 ] && MAXPAR=1
    [ "${#WTS[@]}" -eq 0 ] && WTS=(".")

    DISPATCH_DIR=".devin/daemon/dispatch"
    mkdir -p "$DISPATCH_DIR"
    PID_FILE="$DISPATCH_DIR/dispatch.pids"
    : > "$PID_FILE"
    say_d() { echo "[$(date -u +%H:%M:%S)] dispatch: $*"; }

    say_d "fan-out: ${#TASKS[@]} task(s), ${#WTS[@]} worktree(s), max parallel=$MAXPAR"
    declare -A PID_TASK=()
    declare -A TASK_RC=()
    NEXT=0; RUNNING=0; FAILED=0

    launch_next() {
        local tid="${TASKS[$NEXT]}"
        local wt="${WT_MAP[$tid]:-${WTS[$((NEXT % ${#WTS[@]}))]}}"
        local agent="sup-$(basename "$wt" | tr -cd '[:alnum:]-')"
        say_d "launch $tid → $wt (agent=$agent)"
        (
            SUP_POLL_SLEEP="$POLL_SLEEP" SUP_RATE_SLEEP="$RATE_SLEEP" \
            SUP_MAX_RESTARTS="$MAX_RESTARTS" \
            bash .devin/scripts/daemon_supervisor.sh "$tid" "$agent" "$wt"
        ) &
        local pid=$!
        PID_TASK[$pid]="$tid"
        echo "$pid $tid $wt" >> "$PID_FILE"
        say_d "  pid=$pid"
        NEXT=$((NEXT+1)); RUNNING=$((RUNNING+1))
    }

    while [ "$NEXT" -lt "${#TASKS[@]}" ] || [ "$RUNNING" -gt 0 ]; do
        while [ "$NEXT" -lt "${#TASKS[@]}" ] && [ "$RUNNING" -lt "$MAXPAR" ]; do
            launch_next
        done
        sleep "$POLL_SLEEP"
        # Reap finished workers (kill -0 is portable; wait -n isn't everywhere)
        for pid in "${!PID_TASK[@]}"; do
            if ! kill -0 "$pid" 2>/dev/null; then
                wait "$pid" 2>/dev/null; rc=$?
                tid="${PID_TASK[$pid]}"
                TASK_RC[$tid]=$rc
                unset "PID_TASK[$pid]"
                RUNNING=$((RUNNING-1))
                say_d "reaped $tid (pid $pid) rc=$rc"
                [ "$rc" != "0" ] && FAILED=$((FAILED+1))
            fi
        done
    done

    say_d "dispatch complete: ${#TASKS[@]} task(s), $FAILED failed"
    for tid in "${TASKS[@]}"; do
        say_d "  $tid → rc=${TASK_RC[$tid]:-killed}"
    done
    [ "$FAILED" -eq 0 ]
    exit $?
fi

TID="${1:?task id required}"
AGENT="${2:?agent id required}"
WORKTREE="${3:?worktree path required}"
shift 3
USER_CMD=""
while [ $# -gt 0 ]; do
    case "$1" in --cmd) USER_CMD="$2"; shift 2 ;; *) shift ;; esac
done

DAEMON_DIR=".devin/daemon/$TID"
CTX_FILE="$DAEMON_DIR/context.md"
LOG_DIR="$DAEMON_DIR/logs"
WIP_DIR=".devin/wip"
mkdir -p "$LOG_DIR" "$WIP_DIR"

POLL_SLEEP="${SUP_POLL_SLEEP:-10}"
RATE_SLEEP="${SUP_RATE_SLEEP:-300}"
MAX_RESTARTS="${SUP_MAX_RESTARTS:-10}"
LOG_MAX_MB="${SUP_LOG_MAX_MB:-50}"
KEEP_LOGS="${SUP_KEEP_LOGS:-5}"
RATE_RE='(429|rate.?limit|quota|overloaded|RESOURCE_EXHAUSTED|Too Many Requests)'
OOM_RE='(out of memory|OOM|oom-kill|cannot allocate|std::bad_alloc|exit code 137|status 137)'

say() { echo "[$(date -u +%H:%M:%S)] supervisor($TID): $*" | tee -a "$DAEMON_DIR/supervisor.log"; }

# Regenerate the zero-loss bootstrap context every launch — BOOT_PROMPT +
# spec + live worktree state, so a fresh process inherits full instructions.
build_context() {
    {
        echo "# Supervisor context — $TID (attempt $ATTEMPT, agent $AGENT)"
        echo ""
        cat .devin/tasks/BOOT_PROMPT.md
        echo ""
        SPEC=$(node -e 'const d=JSON.parse(require("fs").readFileSync(".devin/tasks/backlog.json","utf8"));const t=d.tasks.find(x=>x.id===process.argv[1]);console.log(t?(t.spec_file||""):"")' "$TID" 2>/dev/null)
        if [ -n "$SPEC" ] && [ -f "$SPEC" ]; then
            echo "--- TASK SPEC ($SPEC) ---"; cat "$SPEC"
        fi
        echo ""
        echo "--- WORKTREE STATE ($WORKTREE) ---"
        git -C "$WORKTREE" status --short 2>/dev/null | head -50
        echo ""
        echo "--- UNCOMMITTED DIFF STAT ---"
        git -C "$WORKTREE" diff --stat 2>/dev/null | tail -30
        [ -f "$WIP_DIR/$TID.patch" ] && { echo ""; echo "--- PRIOR WIP PATCH: $WIP_DIR/$TID.patch ---"; }
    } > "$CTX_FILE"
}

save_wip() {
    say "saving WIP state"
    git -C "$WORKTREE" diff > "$WIP_DIR/$TID.patch" 2>/dev/null
    # Untracked files aren't in `git diff` — append each as a /dev/null diff.
    git -C "$WORKTREE" ls-files --others --exclude-standard | \
    while IFS= read -r f; do
        [ -z "$f" ] && continue
        {
            echo ""
            git -C "$WORKTREE" diff --no-index -- /dev/null "$f"
        } >> "$WIP_DIR/$TID.patch" 2>/dev/null || true
    done
    # WIP commit attempt — pre_commit hook may gate; patch file is the
    # guaranteed fallback so nothing is ever lost.
    if ! git -C "$WORKTREE" -c user.name="daemon-supervisor" \
            -c user.email="supervisor@local" commit -qam \
            "wip($TID): supervisor auto-save before respawn" 2>/dev/null; then
        say "WIP commit unavailable (hook/tree clean) — patch snapshot kept at $WIP_DIR/$TID.patch"
    else
        say "WIP committed: $(git -C "$WORKTREE" rev-parse --short HEAD)"
    fi
}

rotate_logs() {
    # Drop oldest run logs beyond KEEP_LOGS; gzip-like truncation via mv chain.
    local n
    n=$(ls -1 "$LOG_DIR"/run-*.log 2>/dev/null | wc -l)
    while [ "$n" -gt "$KEEP_LOGS" ]; do
        rm -f "$(ls -1t "$LOG_DIR"/run-*.log | tail -1)"
        n=$((n-1))
    done
    # Rotate the supervisor log itself when huge.
    if [ -f "$DAEMON_DIR/supervisor.log" ] && \
       [ "$(du -m "$DAEMON_DIR/supervisor.log" | cut -f1)" -gt "$LOG_MAX_MB" ]; then
        mv "$DAEMON_DIR/supervisor.log" "$DAEMON_DIR/supervisor.log.old"
        say "rotated supervisor.log (>${LOG_MAX_MB}MB)"
    fi
}

clean_build_artifacts() {
    # Only transient incremental caches — never the whole target dir.
    local tgt="${CARGO_TARGET_DIR:-C:/rust-build-tmp}"
    rm -rf "$tgt/debug/incremental" 2>/dev/null && say "cleaned $tgt/debug/incremental" || true
}

attempt_oom_or_crash() {
    save_wip
    rotate_logs
    clean_build_artifacts
}

ATTEMPT=0
say "starting — agent=$AGENT worktree=$WORKTREE"

# v4.27.0: resolve the worker CLI ONCE up-front. A missing binary previously
# produced 11 silent respawn-loop failures; now we exit fast with a reason.
WORKER_CLI=""
if [ -z "$USER_CMD" ] && [ -z "${SUP_WORKER_CMD:-}" ]; then
    WORKER_CLI=$(probe_worker_cli || true)
    if [ -z "$WORKER_CLI" ]; then
        say "✗ no worker CLI found (probed: ${SUP_RUNNER_ORDER:-devin claude codex} + known paths)."
        say "  Set SUP_RUNNER_CLI=<path> or pass --cmd / SUP_WORKER_CMD."
        bash .devin/scripts/task_ctl.sh block "$TID" "supervisor: no worker CLI installed" 2>/dev/null || true
        exit 2
    fi
    say "worker CLI: $WORKER_CLI"
fi

while [ "$ATTEMPT" -le "$MAX_RESTARTS" ]; do
    ATTEMPT=$((ATTEMPT+1))
    build_context
    CMD="${USER_CMD:-${SUP_WORKER_CMD:-}}"
    [ -z "$CMD" ] && CMD=$(build_runner_cmd "$WORKER_CLI")
    CMD="${CMD//\{CTX\}/$CTX_FILE}"
    RUN_LOG="$LOG_DIR/run-$ATTEMPT.log"
    say "launch attempt $ATTEMPT: $CMD"
    bash -c "$CMD" > "$RUN_LOG" 2>&1 &
    WPID=$!
    say "worker pid=$WPID log=$RUN_LOG"

    # Poll liveness + rotate run log if it balloons (transcript-bloat guard).
    while kill -0 "$WPID" 2>/dev/null; do
        sleep "$POLL_SLEEP"
        if [ -f "$RUN_LOG" ] && \
           [ "$(du -m "$RUN_LOG" | cut -f1)" -gt "$LOG_MAX_MB" ]; then
            mv "$RUN_LOG" "$RUN_LOG.rotated"
            say "run log exceeded ${LOG_MAX_MB}MB — rotated live (worker keeps fd)"
        fi
    done
    wait "$WPID"; RC=$?
    say "worker exited rc=$RC"

    if [ "$RC" = "0" ]; then
        if [ "${SUP_SKIP_VERIFY:-0}" = "1" ]; then
            say "worker completed (verify skipped via SUP_SKIP_VERIFY)"
            exit 0
        fi
        say "worker completed — handing to verify_task gate"
        bash .devin/scripts/task_ctl.sh verify "$TID" && {
            say "TASK $TID DONE"; exit 0; }
        say "gate failed — task stays IN_PROGRESS for next assignment"
        exit 1
    fi

    TAIL=$(tail -50 "$RUN_LOG" 2>/dev/null)
    if echo "$TAIL" | grep -qiE "$RATE_RE"; then
        say "rate-limit/quota signal detected — cooling down ${RATE_SLEEP}s then relaunching"
        sleep "$RATE_SLEEP"
        continue
    fi
    if [ "$RC" = "137" ] || echo "$TAIL" | grep -qiE "$OOM_RE"; then
        say "OOM/kill detected (rc=$RC) — WIP save + artifact clean + respawn"
        attempt_oom_or_crash
        continue
    fi
    say "generic crash (rc=$RC) — WIP save + respawn"
    attempt_oom_or_crash
done

say "✗ exceeded $MAX_RESTARTS attempts — marking $TID BLOCKED"
bash .devin/scripts/task_ctl.sh block "$TID" "supervisor: exhausted $MAX_RESTARTS restart attempts" 2>/dev/null || true
exit 1

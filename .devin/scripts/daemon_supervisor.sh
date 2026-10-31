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
# Env: SUP_WORKER_CMD, SUP_POLL_SLEEP=10, SUP_RATE_SLEEP=300,
#      SUP_MAX_RESTARTS=10, SUP_LOG_MAX_MB=50, SUP_KEEP_LOGS=5
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1

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

while [ "$ATTEMPT" -le "$MAX_RESTARTS" ]; do
    ATTEMPT=$((ATTEMPT+1))
    build_context
    CMD="${USER_CMD:-${SUP_WORKER_CMD:-devin -p \"\$(cat '{CTX}')\"}}"
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

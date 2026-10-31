#!/bin/bash
# test_worker_stub.sh — supervisor self-test worker.
# Simulates a flaky agent: run1 dies on rate-limit, run2 dies on OOM (137),
# run3 verifies its bootstrap context then succeeds. State persists in
# $STUB_STATE file across invocations (like a real agent's worktree).
#
# Usage: test_worker_stub.sh <state_file> <context_file>
STATE="$1"; CTX="$2"
RUN=1
[ -f "$STATE" ] && RUN=$(( $(cat "$STATE") + 1 ))
echo "$RUN" > "$STATE"

echo "stub-worker: run $RUN booted"
# Prove zero-loss bootstrap: context must contain the boot prompt text.
if grep -q "Daemon v2 Bootstrap" "$CTX" 2>/dev/null; then
    echo "stub-worker: INSTRUCTIONS-OK (BOOT_PROMPT present in context)"
else
    echo "stub-worker: INSTRUCTIONS-LOST!"
fi

case "$RUN" in
    1)
        echo "error: HTTP 429 rate_limit exceeded — quota exhausted, retry later"
        exit 42
        ;;
    2)
        echo "fatal: out of memory — worker killed (exit code 137)"
        exit 137
        ;;
    *)
        echo "stub-worker: run $RUN completing successfully"
        exit 0
        ;;
esac

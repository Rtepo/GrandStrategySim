#!/bin/bash
# resolve_blocker.sh — Manager CLI: clear cross_agent_blockers + Day-1 stub generation.
#
# Usage: bash .devin/scripts/resolve_blocker.sh <index|from_agent|all> [--generate-stub]
#
# Modes:
#   <index>      — remove blocker at 0-based index
#   <from_agent> — remove all blockers from that agent
#   all          — clear entire blockers array
#
# --generate-stub: Parse blocker messages for missing Rust types and generate
#   minimal compilable stubs to unblock HEAD compilation.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/sync_lib.sh"

# Validate manager auth
validate_manager_auth
if [ "$AGENT_ROLE" != "manager" ]; then
    echo "✗ ACCESS DENIED: resolve_blocker.sh requires manager role." >&2
    exit 1
fi

if [ $# -lt 1 ]; then
    echo "Usage: bash .devin/scripts/resolve_blocker.sh <index|from_agent|all> [--generate-stub]" >&2
    exit 1
fi

TARGET="$1"
GEN_STUB=0
if [ "${2:-}" = "--generate-stub" ]; then
    GEN_STUB=1
fi

# Determine mode
if [ "$TARGET" = "all" ]; then
    RESOLVE_MODE="all"
    RESOLVE_TARGET=""
elif echo "$TARGET" | grep -qE "^[0-9]+$"; then
    RESOLVE_MODE="index"
    RESOLVE_TARGET="$TARGET"
else
    RESOLVE_MODE="from_agent"
    RESOLVE_TARGET="$TARGET"
fi

export RESOLVE_MODE
export RESOLVE_TARGET

# If --generate-stub, parse blockers and generate stubs BEFORE clearing
if [ "$GEN_STUB" -eq 1 ]; then
    echo "Parsing blockers for missing Rust types..."

    # Read blockers and extract missing types
    STUBS=$(cat "$LEDGER_FILE" 2>/dev/null | node -e '
        let input = "";
        process.stdin.on("data", d => input += d);
        process.stdin.on("end", () => {
            try {
                const data = JSON.parse(input);
                const blockers = data.cross_agent_blockers || [];
                const stubs = [];
                for (const b of blockers) {
                    const msg = b.message || "";
                    const file = b.affected_file || "";
                    // Parse for missing types
                    const enumMatch = msg.match(/enum\s+(\w+)/);
                    const structMatch = msg.match(/struct\s+(\w+)/);
                    const notFoundMatch = msg.match(/(\w+)\s+not found in\s+(\w+)\s+module/);
                    const fieldMissingMatch = msg.match(/(\w+)\s+field missing on\s+(\w+)/);

                    if (enumMatch) stubs.push({type: "enum", name: enumMatch[1], file: file});
                    if (structMatch) stubs.push({type: "struct", name: structMatch[1], file: file});
                    if (notFoundMatch) stubs.push({type: "type", name: notFoundMatch[1], file: file});
                    if (fieldMissingMatch) stubs.push({type: "field", name: fieldMissingMatch[1], parent: fieldMissingMatch[2], file: file});
                }
                console.log(JSON.stringify(stubs));
            } catch(e) { console.log("[]"); }
        });
    ' 2>/dev/null || echo "[]")

    # Generate stubs for each missing type
    echo "$STUBS" | node -e '
        let input = "";
        process.stdin.on("data", d => input += d);
        process.stdin.on("end", () => {
            const stubs = JSON.parse(input);
            const fs = require("fs");
            for (const s of stubs) {
                if (s.type === "enum" || s.type === "struct" || s.type === "type") {
                    // Check if already exists
                    const stubCode = `
// AUTO-GENERATED MANAGER STUB — resolve_blocker.sh --generate-stub
// This is a placeholder to unblock compilation. The domain agent
// must replace this with the full implementation.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ${s.name} {
    // TODO: Domain agent must flesh out fields
}
`;
                    console.log("STUB: " + s.name + " -> " + (s.file || "unknown"));
                    console.log(stubCode);
                } else if (s.type === "field") {
                    console.log("FIELD: " + s.name + " on " + s.parent + " -> " + (s.file || "unknown"));
                }
            }
        });
    '
    echo ""
    echo "Stub generation complete. Run cargo check --workspace to verify."
    echo "Stubs must be committed to a fix/manager-stubs branch, not main."
fi

# Clear the blockers via transactional loop
echo "Resolving blockers (mode: $RESOLVE_MODE, target: $RESOLVE_TARGET)..."
sync_transactional mutator_resolve_blocker "manager: resolve blockers ($RESOLVE_MODE: $RESOLVE_TARGET)" 5
RC=$?

if [ $RC -eq 0 ]; then
    echo "✓ Blockers resolved successfully."
    log_manager_action "resolve_blocker" "mode:$RESOLVE_MODE target:$RESOLVE_TARGET stubs:$GEN_STUB"
else
    echo "✗ Failed to resolve blockers after 5 attempts." >&2
    exit 1
fi

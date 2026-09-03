#!/bin/bash
# claim_manager.sh — Bootstrap script to claim the manager role.
#
# Creates .devin/.manager_auth with the current session_id and machine
# fingerprint. The SessionStart hook will detect this file and set
# AGENT_ROLE="manager" for this session.
#
# Usage: bash .devin/scripts/claim_manager.sh
#
# This must be run manually by the manager at the start of their session.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
AUTH_FILE="$PROJECT_DIR/.devin/.manager_auth"

# Generate session_id if not set in environment
SESSION_ID="${SESSION_ID:-}"
if [ -z "$SESSION_ID" ]; then
    SESSION_ID="session-$$-$(hostname 2>/dev/null || echo 'unknown')"
fi

# Generate agent_id from session_id hash
AGENT_ID=$(echo "$SESSION_ID" | node -e '
    let input = "";
    process.stdin.on("data", d => input += d);
    process.stdin.on("end", () => {
        const crypto = require("crypto");
        const hash = crypto.createHash("md5").update(input.trim()).digest("hex").substring(0, 8);
        console.log("agent-" + hash);
    });
' 2>/dev/null || echo "agent-5")

# Generate machine fingerprint
FINGERPRINT=$(node -e '
    const os = require("os");
    const crypto = require("crypto");
    const fp = os.hostname() + "|" + (process.env.USERNAME || process.env.USER || "unknown");
    console.log(crypto.createHash("sha256").update(fp).digest("hex"));
' 2>/dev/null || echo "unknown-fp")

# Create the auth token
node -e '
    const fs = require("fs");
    const token = {
        manager_agent_id: process.argv[1],
        session_id: process.argv[2],
        issued_at: new Date().toISOString(),
        fingerprint: process.argv[3]
    };
    fs.writeFileSync(process.argv[4], JSON.stringify(token, null, 2));
    console.log("✓ Manager token created: " + process.argv[4]);
    console.log("  agent_id: " + token.manager_agent_id);
    console.log("  session_id: " + token.session_id);
    console.log("  fingerprint: " + token.fingerprint.substring(0, 16) + "...");
' "$AGENT_ID" "$SESSION_ID" "$FINGERPRINT" "$AUTH_FILE"

echo ""
echo "Manager role claimed. The SessionStart hook will now recognize this"
echo "session as the manager. Manager CLI tools (force_unlock.sh,"
echo "resolve_blocker.sh, merge_worker.sh) are now available."

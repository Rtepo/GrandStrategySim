# Architecture v3: Zero-Touch Event Architecture

A comprehensive technical specification for upgrading the CI/CD and agent
communication infrastructure to eliminate manual event routing, enable
autonomous agent wake-up, shift-left test integration, and pipeline
turbocharging with cargo-nextest and sccache.

## Pillars

1. **Direct Auditor Routing** — Agent 4 parses blueprint_agent_map.json
   and emits REMEDIATION_REQUESTED events directly to workers.
2. **Active Listening (Auto-Wake)** — File-watching daemons detect
   relevant events and trigger LLM terminal sessions automatically.
3. **Local Sandbox (Shift-Left Testing)** — request_integration.sh pulls
   main, rebases, and runs full local CI before emitting events.
4. **Pipeline Turbocharging** — cargo-nextest for parallel tests,
   sccache for compilation caching, --test-threads=4 for OOM prevention.
5. **AUDIT_FAIL_ADDENDUM Support** — Both router and daemon handle
   new_failed_checks schema.

## OOM Prevention

- All cargo nextest commands use `--test-threads=4` to cap RAM usage.
- All log captures use `tail -n 50` to prevent LLM context window bloat.
- SOP includes a Crash Recovery Protocol for seamless resume after OOM.

## Files

- `.devin/scripts/route_failures.sh` — Agent 4 direct routing
- `.devin/scripts/auto_wake.sh` — File-watching daemon
- `.devin/scripts/request_integration.sh` — Enhanced shift-left
- `.devin/scripts/integration_daemon.sh` — Enhanced pipeline
- `.config/nextest.toml` — CI profile with retries

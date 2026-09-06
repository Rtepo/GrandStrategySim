# Standard Operating Procedure — SillyElaborateState

## Pre-Commit Checklist (MUST pass before request_integration.sh)

1. `cargo check --workspace` — exit 0
2. `cargo test --workspace --all-targets -- --skip headless_50_tick_smoke` — exit 0
3. `cargo clippy --workspace --all-targets -- -D warnings` — exit 0
4. `npm run build` — exit 0
5. `git status` — clean (all changes committed)

## Rule 13: Comprehensive Technological Matrices

Every new sector/building MUST define:
- >=3 Production method slots
- >=3 Automation method slots
- >=3 Organization method slots
- All input commodities MUST have at least one producing sector (no orphans)
- All output commodities MUST be consumed by at least one sector or demographic

## Commodity Graph Integrity

Before adding a new commodity:
1. Verify it is produced by at least one production method
2. Verify it is consumed by at least one production method or demographic
3. Update `phase76_commodity_graph_test` if the commodity count assertion exists
4. Update `energy_wave1_test` / `energy_wave2_test` commodity count assertions

## Supply Chain Integrity

Every sector MUST have:
- Organization progression methods (tested by `every_sector_has_organization_progression`)
- Non-empty production outputs (tested by `supply_chain_integrity_test`)
- No orphan inputs (tested by `no_orphan_inputs_phase76`)

## Git Hygiene

- Work ONLY in your assigned worktree
- NEVER commit to main directly
- NEVER use: git stash pop, git reset --hard, git push --force, git clean -fd
- Run `request_integration.sh` from your worktree (NOT the hub)
- If `request_integration.sh` fails the pre-integration guard, fix the error — do NOT bypass it

## Event Bus

- INTEGRATION_REQUESTED: Worker -> Manager (via request_integration.sh)
- PROMOTED_TO_MAIN: Manager -> All (after CI/CD passes)
- AUDIT_REQUESTED: Manager -> Agent 4 (after sprint complete)
- AUDIT_FAIL: Agent 4 -> Manager (structured JSON with failed_checks)
- AUDIT_PASS: Agent 4 -> Manager (all checks pass)
- REMEDIATION_REQUESTED: Manager -> Worker (auto-routed from AUDIT_FAIL)
- CLARIFICATION_REQUESTED: Manager -> Worker (CI/CD failure details)
- SYSTEM_ALERT: Manager -> User (critical issues requiring human attention)

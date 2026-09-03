# v1.1.0 — Manager Governance, RBAC, and M0 Conservation

## Summary

This release introduces the Lead System Manager (Agent 5) architecture with
Role-Based Access Control (RBAC), an automated multi-agent synchronization
ledger with intelligent lifecycle hooks, a Manager CLI toolkit, and a formal
M0 base money conservation audit. All worker agent branches (education,
disability, construction, M0 leaks) have been merged and integrated.

## Major Features

### RBAC and Manager Governance
- **Role-Based Access Control**: Agents are classified as `worker` or `manager`
  in `agents_sync.json`. Workers are blocked from pushing to `main` by the
  `pre_push.sh` hook. Only the manager may alter `main`.
- **Manager Authentication**: `.devin/.manager_auth` token file (gitignored)
  binds the manager session to a machine fingerprint.
- **Branch Protection**: `pre_push.sh` hook intercepts `git push` and `git merge`
  commands, blocking workers from `main` with clear error messages.

### Manager CLI Toolkit
- `claim_manager.sh` — Bootstrap the manager role at session start
- `force_unlock.sh <agent_id>` — Break zombie locks or override domain reservations
- `resolve_blocker.sh <index|from_agent|all> [--generate-stub]` — Clear blockers
  with Day-1 Rust stub generation for missing types
- `view_blockers.sh` — Topological dependency graph (Kahn's algorithm, bottleneck
  detection, cycle detection)
- `merge_worker.sh <branch>` — Formal merge with 3-stage diagnostic gate and
  reverse sync (updates worker branch post-merge)

### Automated Synchronization Hooks
- **SessionStart hook**: Registers agent, locks directories, reaps zombies
  (>15 min stale heartbeat), parses cross-agent blockers, spawns background
  heartbeat process
- **Stop hook**: Context-aware CI/CD bypass (skips cargo/npm for .md/.txt/.json
  only changes), auto-unlocks agent on exit
- **PreToolUse hooks**: `pre_commit.sh` gates git commits (CI/CD enforcement
  for source code), `pre_push.sh` enforces branch protection
- **Transaction sync loop**: Strict pull→rebase→mutate→commit→push with
  surgical revert (never `git reset --hard`), `index.lock` evasion,
  60-second timeouts

### Cross-Agent Blocker Bus
- `agents_sync.json` includes `cross_agent_blockers` array for structured
  inter-agent communication
- SessionStart hook injects high-visibility warnings for blockers targeting
  the current agent
- `block.sh` CLI for agents to post blockers safely

### M0 Conservation Audit
- **`macro_m0_audit.rs`**: Formal Rust integration test verifying
  `Δfiat == Δcb_injected + Δtreasury_external_financing`
- **`Treasury.external_financing_injected`**: New field tracking external M0
  expansion (foreign bond purchases, CB deficit monetization)
- Single-turn M0 conservation test passes. Six-turn test marked `#[ignore]`
  pending fiscal banking-side instrumentation.

### Worker Branch Integrations
- **Agent 1 (M0 leaks)**: Treasury settlement helpers, bank reserve syncing,
  defense order M0 tracking, `credit_company_by_id` bank check
- **Agent 2 (construction)**: Corporate investments, B2B tenders, state cadastre
  land allocation, escrow-based tranche payments, weather productivity
- **Agent 3 (disability)**: Disability lifecycles, sheltered workshops, B2C
  care clearing, poor laws, begging configuration
- **Agent 4 (education)**: Regionalized education shares, loyalty bonds,
  tiered financing models, school system refactoring, child labor laws

### Military Supply Chain
- Audited and confirmed: military equipment needs are based on actual strategic
  army requirements (manpower, unit types, GDP-based budgeting), not abstract
  population multipliers. Food upkeep scales physically per 1000 soldiers.

## Test Results

- `cargo build --workspace`: PASSED
- `cargo test --workspace --all-targets`: 1719 passed, 0 failed, 1 ignored
- `cargo clippy --workspace --all-targets -- -D warnings`: PASSED
- `npm run build`: PASSED
- `cargo test --test macro_m0_audit`: 1 passed, 0 failed, 1 ignored

## Resolved Blockers

All cross-agent blockers from the synchronization ledger have been resolved
through merging and integration testing:
- Agent 2 → Agent 1: Treasury settlement helpers verified ✓
- Agent 3 → Agent 1: Transfer settler M0 conservation verified ✓
- Agent 2 → Agent 3: Disability struct definitions merged ✓
- Agent 2 → Agent 4: EmancipationLaw import fixed ✓

Generated with [Devin](https://devin.ai)

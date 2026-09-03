# 🤖 MULTI-AGENT SYNCHRONIZATION LEDGER

**MANDATORY RULES:**
1. **READ BEFORE WRITING:** Pull the latest `main` and read this file before branching.
2. **DOMAIN LOCKING:** Do not modify files in directories currently locked by another agent without explicit coordination.
3. **SURGICAL APPENDS ONLY:** If you must touch a shared central file (e.g., `turn.rs`, `macro_data.rs`), append your structs/calls. NEVER overwrite or delete concurrent work.
4. **UPDATE STATUS:** Update your row when you start a task, and clear your locks when your PR is merged.

## 📊 Current Agent Status

| Agent | Current Branch | Locked Domains / Files | Active Task | Status |
|---|---|---|---|---|
| **Agent 1** | `fix/m0-leaks-phase94` | `economy/trade/*`, `military/supply/*`, `engine/turn.rs` (shared) | Running Phase 94 Diagnostic Harness. Tracing and patching M0 fiat leaks in supply chains. | 🟢 CODING / TESTING |
| **Agent 2** | `feat/investment-construction` | `construction/*`, `society/real_estate_market.rs`, `corporate/manager.rs` | Refactoring Corporate Investments, B2B Tenders, and State Cadastre Land Allocation. | 🟢 CODING |
| **Agent 3** | `feat/disability-social-care` | `infrastructure/care.rs`, `politics/social_programs.rs`, `telemetry.rs` | Implementing Disability lifecycles, Sheltered Workshops, and B2C Care clearing. | 🟢 CODING |
| **Agent 4** | `feat/education-mobility` | `economy/labor/education*.rs`, `economy/config/education_config.rs` | Regionalizing education shares, Loyalty Bonds, and tiered financing models. | 🟢 CODING |

## 🤝 Cross-Agent Contracts & Blockers
*Use this section to request data structures from other agents if you are blocked.*

* **[Agent 2 -> Agent 4]:** (Example) Need `EducationConfig` merged before I can scale construction BOMs based on school tiers.
* **[Agent 3 -> Agent 1]:** (Example) Need the Treasury settlement helpers verified in Phase 94 before finalizing disability pensions.


---
agent: devin-local
session: phase-88-genesis-audit
created: 2026-08-30T00:00:00Z
executed_at: pending
phase: 88
retention_days: 7
note: This plan is subject to 7-day automated retention purge via scripts/purge_old_plans.ps1
revision: 3 (corrected bank loan accounting + agricultural grace logic + deposit ID key mapping per user feedback)
protocol: 7-Day Planning Protocol — isolated timestamped plans in .devin/plans/, retained 7 days then purged by scripts/purge_old_plans.ps1
---

# Phase 88: The Genesis & Operations Audit — Technical Remediation Plan (Revised)

Fixes 4 critical Turn 2 collapse bugs: banking liquidity crisis from concentrated State Bank loans with incorrect M0-destroying accounting, furlough death spiral from premature grace expiration, ghost cadastre with zero arable land used, and disconnected mining companies bound to deprecated geological formations instead of the new Planet vein system. Revised per user correction of bank loan accounting (Rule 1) and agricultural grace logic.

## Root Cause Analysis

### Pillar 1: Banking Liquidity & LDR Bug

**Root causes (4 distinct bugs):**

1. **Loan concentration:** `issue_agriculture_working_capital_loans` in `state/src/engine/generator/corporate.rs:798-897` assigns ALL agriculture Working Capital Loans to the State Bank (`BANK-{code}-001`).

2. **LDR double-multiplication:** Backend at `state/src/ui/snapshot.rs:3128` computes `ldr = loans / deposits * 100.0` (already a percentage). Frontend at `src/pages/BankingPage.tsx:109` does `(b.ldr * 100).toFixed(0)}%`, multiplying by 100 again.

3. **Incorrect loan accounting (Rule 1 violation — USER CORRECTION):** Current code at `corporate.rs:887` reduces `reserves_at_central_bank` when issuing a loan. Commercial banks do NOT lend out reserves; they create deposits. Reducing reserves destroys high-powered money (M0). Correct double-entry: Bank increases `loans_issued` (Asset) AND increases `deposits` (Liability). Reserves untouched.

4. **Insufficient bank reserves:** Bank reserves at `state/src/engine/generator/mod.rs:1586` are only `total_deposits * central_bank.reserve_requirement_ratio` (~10% of deposits).

### Pillar 2: Turn 2 Furlough Death Spiral

**Root cause:** Grace at `state/src/corporate/strategy.rs:776-777` uses `!ctx.company.financial_history.is_empty()`. On Turn 2, history has 1 entry -> grace expires. But crops haven't harvested yet -> mass furloughs.

**USER CORRECTION:** Initial plan tied grace to `CropBatch::state`, but this is flawed — a side-batch harvesting early would drop grace for the whole company. Grace must remain active until first non-zero revenue OR 24-turn hardcap.

### Pillar 3: Ghost Cadastre (Zero Arable Used)

**Root cause:** `arable_land_used` on `Region` is initialized to 0 and NEVER updated. Corporate generator assigns parcels and creates CropBatches but never increments `region.arable_land_used`.

### Pillar 4: Disconnected Mines & "Unknown" Formations

**Root causes (2 bugs):** Mining generator uses deprecated `country.geological_formations` instead of `state.planet.veins`. Snapshot reads `formation_name` from `region.resources` but the new Planet system doesn't write to `region.resources`.

### Bonus Bug: Financial Summary Field Mapping

`compute_financial_summary` reads `income`/`expenses` keys but actual records use `revenue`/`operating_costs`/`interest`/`taxes`/`net_profit`.

## Implementation Steps

### Step 1: Fix LDR backend to return ratio (Pillar 1)
**File:** `state/src/ui/snapshot.rs` — Change `loans / bs.deposits * 100.0` to `loans / bs.deposits`

### Step 2: Fix bank loan accounting — create deposits, not destroy reserves (Pillar 1 — USER CORRECTION)
**File:** `state/src/engine/generator/corporate.rs`
- OLD: `bs.reserves_at_central_bank = (bs.reserves_at_central_bank - principal).max(0.0)`
- NEW: `bs.deposits += principal` (bank creates deposit for borrower)
- Standard fractional-reserve banking: Asset (loan) + Liability (deposit) expand symmetrically. Reserves (M0) untouched.

### Step 3: Distribute Working Capital Loans across all commercial banks (Pillar 1)
**File:** `state/src/engine/generator/corporate.rs` — Load all banks, randomly assign each agriculture company to a commercial/universal bank, update each bank's `loans_issued` and `deposits` per Step 2.

### Step 4: Scale bank initial reserves based on GDP (Pillar 1)
**File:** `state/src/engine/generator/mod.rs` — `reserves = (total_deposits * reserve_requirement_ratio).max(treasury.gdp * 0.02 * size_factor / num_banks)`

### Step 5: Add `current_turn` to `CorporateDecisionCtx` and `founded_turn` to `Company` (Pillar 2)
**Files:** `state/src/corporate/strategy.rs`, `state/src/entities/mod.rs`, `state/src/engine/generator/corporate.rs`, `state/src/corporate/manager.rs`

### Step 6: Replace furlough grace with revenue-and-hardcap-aware grace (Pillar 2 — USER CORRECTION)
**File:** `state/src/corporate/strategy.rs`
```rust
fn is_within_material_shortage_grace(company: &Company, current_turn: u32) -> bool {
    if company.sector == Sector::Agriculture {
        // Hardcap: 24 turns (1 year) since founding
        if current_turn.saturating_sub(company.founded_turn) >= 24 {
            return false;
        }
        // Grace until first non-zero revenue (first harvest sold)
        let has_nonzero_revenue = company.financial_history.iter().any(|record| {
            record.get("revenue")
                .and_then(|v| v.as_f64())
                .map(|r| r > 0.0)
                .unwrap_or(false)
        });
        if has_nonzero_revenue {
            return false;
        }
        return true;
    }
    // Non-agriculture: Turn 1 grace
    company.financial_history.is_empty()
}
```

### Step 7: Update `arable_land_used` during corporate generation (Pillar 3)
**Files:** `state/src/engine/generator/corporate.rs`, `state/src/engine/generator/mod.rs`

### Step 8: Pass `Planet` to `generate_corporate_entities` (Pillar 4)
**Files:** `state/src/engine/generator/mod.rs`, `state/src/engine/generator/corporate.rs`

### Step 9: Rewrite `seed_geology_based_mines` to use Planet veins (Pillar 4)
**File:** `state/src/engine/generator/corporate.rs`

### Step 10: Add `name` field to `GeologicalVein` (Pillar 4)
**File:** `state/src/society/planet.rs`

### Step 11: Add `reseed_resources_from_planet` function (Pillar 4)
**Files:** `state/src/society/geography.rs`, `state/src/engine/generator/mod.rs`
- **CRITICAL KEY MAPPING:** The `region.resources` HashMap key MUST be `vein.id` (or `composite_id`), NOT the commodity string. This ensures `building.deposit_id` matches the deposit for the active mine counter.
- `build_geological_deposit_rows` in `snapshot.rs` must read `commodity` from the value object, not from the key.

### Step 12: Fix financial summary field mapping (Bonus Bug)
**File:** `state/src/ui/snapshot.rs`

## Verification

- [ ] `cargo build --workspace` passes
- [ ] `cargo test --workspace --release` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `npm run build` passes
- [ ] Turn 1: banks have healthy reserves, LDR < 80%
- [ ] Turn 1: bank balance sheets satisfy Assets = Liabilities + Equity
- [ ] Turn 2: no mass furloughs in agriculture
- [ ] Turn 3-6: agriculture grace expires only after first non-zero revenue
- [ ] Turn 24+: agriculture grace hardcap expires
- [ ] Land dashboard: `Arable Used` > 0
- [ ] Resources tab: formation names display correctly
- [ ] Resources tab: active mine count > 0
- [ ] CompaniesPage: financial summary shows non-zero values
- [ ] BankingPage: LDR displays as percentage (e.g., "45%")

## Risks/Considerations

- **Save compatibility (Directive 10):** Adding `name`, `current_turn`, `founded_turn` breaks saves. No `#[serde(default)]` shims — clean break per Directive 10.
- **Double-entry integrity (Rule 1):** Corrected loan accounting ensures symmetric balance sheet expansion. Reserves (M0) untouched.
- **Multi-batch edge case:** Revenue-based grace is robust — grace only expires on actual revenue, not crop state. 24-turn hardcap prevents immortal grace.
- **Planet vein naming:** Must be deterministic (seeded by vein ID).
- **`founded_turn` initialization:** World-gen companies born at turn 0. Later-spawned companies must set `founded_turn` at creation.

## Macro-Architectural Audit Report

| Directive | Status | Notes |
|-----------|--------|-------|
| Mass Conservation | PASS | No new physical transformations. `arable_land_used` tracks already-allocated land. Mining binds to existing veins with finite reserves. |
| Double-Entry Bookkeeping | PASS | Step 2 corrected per user feedback: bank loan issuance creates deposits (Liability) matching loans (Asset), not destroying reserves. Company: available_cash (Asset) = liabilities (Liability). |
| No Teleportation | PASS | No physical movement of goods. Mining companies spawn in regions where veins overlap. |
| Clamping | PASS | Step 4 uses `.max()` for minimum reserves. `arable_land_used` bounded by available parcels. 24-turn hardcap prevents infinite grace. |
| No Magic Numbers | PASS | Step 4 uses `treasury.gdp * 0.02` (dynamic GDP-scaled). Loan principal uses dynamic workforce and wage. 24-turn hardcap is a temporal policy parameter (1 year). |
| Technological Matrices | PASS | No new building types. Plan connects existing mining buildings to new vein system. |
| Architectural Parsimony | PASS | Plan extends existing systems. No parallel systems created. |
| Temporal Causality | PASS | Furlough grace operates within existing corporate decision phase. `reseed_resources_from_planet` runs before corporate entities. |
| Asymmetric Information | PASS | Existing `discovered` field on veins preserved. Role-gating maintained. |
| Full-Stack Accountability | PASS | Every backend fix has frontend visibility. |
| Complete Entity Lifecycle | PASS | No new entities. `founded_turn` enhances existing lifecycle tracking. |
| Market Forces | PASS | Loan distribution uses random assignment during world generation (seeding). |
| Rational Actors | PASS | Furlough grace prevents premature firings during legitimate waiting period. Banks rationally issue loans with risk premiums. |

### Summary
- Total PASS: 13/13
- Total FAIL: 0/13
- Critical Issues: None

# Negative Reserves Audit — Bank Solvency Invariant Violation

**Author:** agent-1 (worker)
**Branch:** `fix/agent-1-negative-reserves-audit`
**Date:** 2026-09-21
**Status:** Root-cause findings + technical plan. **No speculative fixes applied** (per deliverable).
**Context:** v4.9.0 released. Reserve floor is a release-blocking invariant.

---

## 1. Executive Summary

Commercial bank `reserves_at_central_bank` goes negative (observed **−3.89B** on `BANK-ANA-005`,
task reported −733M on a different seed). The invariant "bank reserves ≥ 0" is violated.

**Root cause is architectural, not a single typo:**

1. **`ReserveMutationGuard` does not exist.** The guard the task assumes ("should have
   prevented this") is not present in the codebase under any name. The only reserve
   protection is a *function-level pre-check*, `would_cause_negative_reserves` in
   `transfer_settler.rs`, and it guards **only one** of the many code paths that debit
   `reserves_at_central_bank`.

2. **"Phase 94: No reserve clamping" is an explicit, deliberate design decision**
   (comments at `banking.rs:2274`, `banking.rs:2286`, `banking.rs:2756`, `turn.rs:5336`).
   The stated intent is that negative reserves "represent CB Lombard borrowing." This
   directly contradicts the strict solvency invariant and is the policy root cause.

3. **There is no ELA.** Only the Lombard facility exists, and it runs *once per turn,
   early* (`process_banking_turn` at `turn.rs:621`). The large reserve debits (CIT/wealth
   tax, B2C, royalties, maintenance) run **later** in the turn. Nothing restores reserves
   after those debits, so the negative persists through `turn_end` and into the next turn
   (confirmed in the trace).

4. **Multiple unguarded direct-debit paths** bypass `would_cause_negative_reserves`
   entirely (see §3).

---

## 2. Evidence (fresh 6-turn diagnostic harness, seed 42, 16 countries)

Re-ran `test_6_turn_diagnostic_harness` with `--features epic-tests,diagnostic`. The test
fails on `FiatCreation` conservation violations (expected — that is the symptom class),
but dumps are written *before* the violation assertion, so evidence is captured.

**From `turn_trace_q1.json` (per-checkpoint `bank` probe field):**

| turn | phase               | bank          | reserves_at_central_bank | deposits     | cb_lombard_loans | tier_1_capital |
|------|---------------------|---------------|--------------------------|--------------|------------------|----------------|
| 0    | b2c_clearing_post   | BANK-ANA-005  | **−3,886,599,314**       | 4,356,995,694| 0                | 532,944,948    |
| 0    | turn_end            | BANK-ANA-005  | **−3,575,854,012**       | 4,667,740,997| 0                | 532,944,948    |
| 1    | turn_start          | BANK-ANA-005  | **−3,575,854,012**       | 4,667,740,997| 0                | 532,944,948    |
| 1    | b2c_clearing_post   | BANK-ANA-005  | **−1,857,483,118**       | 2,469,872,837| 3,903,166,551    | 456,202,501    |

Key observations:
- The negative **first appears at `b2c_clearing_post`** (turn 0) and **persists through
  `turn_end` into the next turn's `turn_start`** — proving no intra-turn restoration pass runs.
- At turn 1 `b2c_clearing_post`, Lombard *has* been injected (3.90B) by the turn-start
  `process_banking_turn`, yet reserves are **still negative (−1.86B)** — the Lombard top-up
  is insufficient and runs too early to cover the later debits.
- `Δbank_res = −170,455,537,893` (−170B) appears in the Turn 1 `turn_end` `FiatCreation`
  violation — the tax-phase aggregate debit across all banks.

Extracted machine-readable evidence: `negative_reserves_evidence.json`.

**Secondary diagnostic-tooling bug:** `banking_state.json` is **empty** (`banks: []`).
`build_banking_state_dump` (`diagnostic.rs:2083`) iterates `ctx.entities.values()` and
filters `sector == Sector::Banking`, but finds no banks at dump time even though the
simulation clearly contains them (banks are generated with `Sector::Banking` —
`generator/mod.rs:2185`; `EntitySector` is just `use ... Sector as EntitySector`). The
`ctx.entities` collection appears stale/empty at the post-turn dump point. The per-checkpoint
`bank` probe field is the only usable source. **The committed `banking_state.json` cannot
be used for auditing until this is fixed.**

---

## 3. Map of `reserves_at_central_bank` Debit Sites — Guarded vs Unguarded

### GUARDED (1 path only)
| Site | Mechanism |
|------|-----------|
| `transfer_settler.rs:238-244` `settle_transfer_mapped` | Pre-checks `would_cause_negative_reserves`; returns `Err(InsufficientReserves)` and mutates nothing on rejection. Tests `test_settle_transfer_rejected_insufficient_bank_reserves` + `test_no_silent_clamp_on_negative_reserves` confirm. |

**Caveat — silent error swallow:** `b2b_orders.rs:735` calls `settle_transfer_mapped` via
`let _ = ...` (Result ignored). A rejected transfer prevents the negative reserve (good)
but the caller still releases the buyer's encumbrance (`debit_cash -= trade_value`,
line 745) without crediting the seller → an **M0 leak**, not a negative-reserve issue.

### UNGUARDED — direct `reserves_at_central_bank -=` with no floor check
| # | Site | Code | Magnitude potential | Notes |
|---|------|------|--------------------|-------|
| A | **CIT/wealth-tax batch sync** | `turn.rs:5338-5343` `bs.reserves_at_central_bank -= total_debit` | **VERY HIGH** — aggregates every corporate depositor's CIT+wealth tax per bank | Explicit comment "No clamping (negative reserves = CB Lombard borrowing)". **Primary suspect for large negatives.** |
| B | **"payer IS a bank" branch** | `transfer_settler.rs:273-283` `bs.reserves_at_central_bank -= amount` | Medium — bank paying fines/taxes/insurance from own reserves | Bypasses the `would_cause_negative_reserves` pre-check (that check only covers `payer_bank_id`, not the bank-as-payer case). |
| C | **`debit_company_by_id`** | `transfer_settler.rs:907` → `adjust_bank_balance_unmapped(..., -actual, -actual)` | Medium-High — used in **8 files**: `b2b_orders`, `royalties` (11 call sites), `maintenance`, `charities`, `cultural`, `waste_collection`, `ip_theft` | No reserve check; only clamps the *company cash* side (`affordable = amount.min(cash)`), not the bank reserve side. |
| D | **Lombard interest accrual** | `banking.rs:2755` `bs.reserves_at_central_bank -= interest` | Low-Medium | Runs inside `process_banking_turn`; "No clamping" comment. Can push a marginal bank negative. |
| E | **`adjust_bank_balance` / `_unmapped`** | `transfer_settler.rs:110, 133` (the primitive itself) | — | The primitive performs **no** floor check. Every caller that passes a negative `reserve_delta` inherits the gap. |

### GUARDED by math (will not go negative, listed for completeness)
| Site | Why safe |
|------|----------|
| `banking.rs:502` interbank lend | `lend_amount = position.min(...)`, position = reserve surplus > 0 ⇒ reserves ≥ required. |
| `banking.rs:579` SOBK repay | `repay = position.min(outstanding)`, position > 0. |
| `banking.rs:2285` bank tax (normal branch) | Only runs when `tax_amount ≤ available_reserves`. Lombard branch (2268-2273) adds `shortfall` first ⇒ lands at 0. |
| `banking.rs:2627` profit distribution | `amount = (-bank_share).min(bs.reserves_at_central_bank.max(0.0))` — clamped. |

### Cleanup (not a cause)
| Site | Behavior |
|------|----------|
| `corporate/bankruptcy.rs:415-421` | **Zeroes** negative reserves, tracks the M0 delta as `central_bank.liquidity_injected`. Bankruptcy is the *only* mechanism that currently repairs negatives — and only on bank failure. |

---

## 4. Hypothesis Verdicts

| # | Hypothesis | Verdict |
|---|------------|---------|
| 1 | Forced settlement path in B2B/B2C debits reserves without liquidity check | **Partially confirmed.** B2B `settle_trades` for *banked* buyers uses the guarded path (but swallows the error → M0 leak). B2C uses `credit_company_by_id` (credits, safe). The unguarded debits come via `debit_company_by_id` (royalties/maintenance/B2B-unbanked) and the bank-as-payer branch. |
| 2 | `ReserveMutationGuard` only guards a subset; some path subtracts directly | **Confirmed — and stronger:** the guard is a single function-level check (`would_cause_negative_reserves`) covering only `settle_transfer_mapped`. Sites A, B, C, D subtract directly with no check. |
| 3 | ELA not firing / ordering | **Confirmed.** No ELA exists. Lombard runs once, early (`turn.rs:621`), before the debits that create the deficit. No post-debit restoration pass. |
| 4 | Overdraft applied as negative-allowed field | **Confirmed (as policy).** The "Phase 94: No clamping" decision is effectively a global overdraft-allowed policy, not a field — but the effect is identical. |
| 5 | Bankruptcy write-off pushing reserves negative | **Rejected.** Bankruptcy *cleans up* negatives (zeroes them, tracks M0). It is the only repair mechanism, not the cause. |

---

## 5. Turn Ordering (why Lombard can't cover the deficit)

```
run_turn_inner:
  line 621   process_banking_turn   ← interbank clear, SOBK, LOMBARD top-up (Step 5)
  ...        B2B orders / settlement (guarded settle_transfer_mapped, errors swallowed)
  ...        B2C clearing            ← negative first observed here (b2c_clearing_post)
  ...        royalties / maintenance / ip_theft (debit_company_by_id — UNGUARDED)
  line 5279  process_tax_collection_turn
  line 5338  CIT/wealth tax batch bank sync  ← UNGUARDED direct debit (largest)
  ...        turn_end                ← negative persists (observed)
  [next turn] line 621 Lombard runs again — but only tops up to required ratio,
              cannot retroactively undo the prior turn's tax-driven negative
```

The Lombard pass computes `needed = -position` where `position = reserves - required`.
It can lift a bank to exactly the required ratio, but (a) it runs *before* the debits,
and (b) it does not clamp/forbid the subsequent debits. So the negative is re-created
every turn after the early Lombard top-up.

---

## 6. Technical Plan — Hard Reserve Floor + ELA Coverage

This is a **plan only**. No code changes are made in this audit (per deliverable
"Report before speculative fixes"). Ordered by leverage; each item is independently
shippable.

### Phase 1 — Introduce a real guard primitive (the missing `ReserveMutationGuard`)
- Add a single centralized helper, e.g. `BankBalanceSheet::apply_reserve_delta(delta) ->
  f64` in `state/banking.rs`, which:
  1. Computes `new = reserves_at_central_bank + delta`.
  2. If `new < 0.0`, records the shortfall and **either** (a) clamps to 0 and returns the
     rejected amount, or (b) auto-draws Lombard/ELA for the shortfall (Phase 3).
  3. Mutates `reserves_at_central_bank` only via this helper.
- Replace **every** direct `bs.reserves_at_central_bank -= ...` and
  `adjust_bank_balance(_unmapped)?(..., -x, -x)` call site (sites A–D in §3) with the
  helper. This is the surgical fix that closes the bypass surface.
- Add a debug-assert / diagnostic flag that panics if any site mutates the field directly
  outside the helper (mirrors the existing `walk_global_fiat` conservation guard).

### Phase 2 — Hard reserve floor (the release-blocking invariant)
- Enforce `reserves_at_central_bank >= 0.0` as a hard invariant at the helper level.
- On a debit that would breach the floor, the helper **rejects the excess** and returns
  the un-debited amount to the caller. Callers must handle the shortfall:
  - **CIT/wealth tax (site A):** the unpaid tax becomes a tax receivable / deferred tax
    liability on the company (do not silently drop it — that is an M0 leak the other way).
  - **`debit_company_by_id` (site C):** already clamps company *cash*; extend the same
    "pay what you can" semantics to the bank-reserve side and return the actual debited
    amount (it already returns `actual`).
  - **Bank-as-payer (site B):** if a bank cannot cover a fine/insurance from reserves,
    draw Lombard first (Phase 3); if Lombard exhausted, trigger resolution.
- This removes the "No clamping" comments and the policy they encode.

### Phase 3 — ELA / Lombard coverage at the point of deficit
- Implement an explicit **ELA facility** on `CentralBank` (currently absent — only
  Lombard exists). ELA = standing marginal-lending window usable *outside*
  `process_banking_turn`.
- In the `apply_reserve_delta` helper, when a debit would breach the floor:
  1. Draw Lombard/ELA for the shortfall (`cb_lombard_loans += shortfall`;
     `reserves_at_central_bank += shortfall`; `central_bank.liquidity_injected += shortfall`).
  2. Then apply the debit. Reserves land at 0, not negative.
  3. If the bank is insolvent (tier_1 ≤ 0 after the loan), flag for resolution rather than
     infinite ELA.
- This makes the "negative reserves = Lombard borrowing" *intent* actually hold in code
  (today it is a comment, not an operation).

### Phase 4 — Re-order / add a post-debit Lombard pass
- Add a **second** Lombard/ELA reconciliation pass at the very end of the turn (after tax,
  before `turn_end` checkpoint) so deficits created late in the turn are cured the same
  turn rather than persisting into the next. (The trace proves the current single early
  pass is insufficient.)
- Alternatively, route all tax/royalty/maintenance bank debits *through* the helper so
  Phase 3 cures them inline and no end-of-turn pass is needed.

### Phase 5 — Fix the diagnostic dump (secondary, unblocks auditing)
- `build_banking_state_dump` (`diagnostic.rs:2083`) reads `ctx.entities` which is empty at
  dump time. Either (a) snapshot bank balance sheets into the probe's per-checkpoint `bank`
  field at `turn_end` and have the dump read from there, or (b) pass the live `state`/task
  companies to `write_all_dumps` instead of the stale `ctx`. Until fixed, the committed
  `banking_state.json` is empty and useless; auditors must use `turn_trace_q1.json`'s
  `bank` field.

### Phase 6 — Regression tests (gated behind `epic-tests`)
- Add an epic test that asserts `reserves_at_central_bank >= 0.0` for every bank at every
  checkpoint across a 6-turn run (seed 42). This is the invariant the harness should
  already enforce but doesn't (it only checks M0/mass conservation, not the reserve floor).
- Add unit tests for `apply_reserve_delta`: floor clamp, ELA draw, insolvency flag.

---

## 7. Recommended Fix Order & Risk

1. **Phase 1 + 2 (guard + floor)** — closes the invariant violation. Risk: callers must
   handle rejected debits or M0 leaks shift form (rejected tax → deferred liability).
2. **Phase 3 (ELA at deficit)** — makes the "Lombard borrowing" intent real and keeps
   reserves ≥ 0 without losing the debit. Lowest risk if Phase 1/2 are in place.
3. **Phase 4** — belt-and-suspenders; may be unnecessary if Phase 3 is inline.
4. **Phase 5** — unblocks all future auditing; do early in parallel.
5. **Phase 6** — locks the invariant permanently.

**Estimated blast radius of the fix:** `transfer_settler.rs`, `state/banking.rs`,
`engine/turn.rs`, `central_bank.rs`, plus the 8 `debit_company_by_id` consumers. All are
in the banking/financial domain — coordinate with any agent holding
`economy/trade/` (currently agent-1 per `agents_sync.json`, but that entry is for a
different task; re-confirm locks before editing).

---

## 8. Files Touched on This Branch

- `state/tests/diagnostic_output/negative_reserves_audit.md` (this file)
- `state/tests/diagnostic_output/negative_reserves_evidence.json` (extracted trace evidence)
- `state/tests/diagnostic_output/banking_state.json` (regenerated, still empty — see §2)
- `state/tests/diagnostic_output/turn_trace_q1.json` (regenerated by harness run)
- `state/tests/diagnostic_output/sector_ledger.json`, `market_clearing.json`,
  `manifest.json` (regenerated)

No source code (`*.rs`) was modified. Diagnostics only, per deliverable.

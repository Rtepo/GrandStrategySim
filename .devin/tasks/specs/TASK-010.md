# TASK-010 — Banking → Central Bank (Bank Centralny) sub-tab

## Defect
Banking view shows commercial banks only. The central bank — governor,
independence, mandate, reserves, open-market operations and rate decisions —
is invisible despite `state/src/state/central_bank.rs` existing.

## Required behaviour
Dedicated `Commercial Banks | Central Bank` sub-tab inside BankingPage:
1. **Central Bank Governor** — clickable VIP link, traits, monetary
   ideology, term end.
2. **Institutional Independence Index (%)** and **Monetary Policy Mandate**
   (Classical Gold Standard, Price Stability, Dual Mandate, State Credit
   Financing).
3. **Balance Sheet & Reserve Composition**: Gold Reserves (tons + $),
   FX reserves by currency, Government Bonds held, ELA loans extended.
4. **Open Market & FX Operations Log**: bond purchases/sales, gold trades,
   liquidity injections.
5. **MPC / RPP Rate Decisions & Voting History**: table of rate decisions —
   previous rate, new rate, bps change, macro rationale (CPI, output gap),
   and committee roll-call vote (For/Against with member names).

## Acceptance
- `verify_task.sh TASK-010` green.
- `!central_bank_dto.mpc_decisions.is_empty()`;
  `central_bank_dto.governor_name != ""`.

## Files
- `state/src/state/central_bank.rs`, `state/src/ui/snapshot.rs`
- `src/pages/BankingPage.tsx`

# TASK-003 — Expandable deep Military view

## Defect
`MilitaryPage.tsx` renders wars/OOB as flat tables. Wars expose no War Score,
Casus Belli, or nested battles; battles show no phases/units; OOB lacks a
branch split (Land/Navy/Air), equipment/training detail, generals,
KIA/WIA/POW/Deserted counters, and POW camps. Backend DTOs already carry
`battles`, `ended_wars`, POWs, branch manpower — the UI drops them.

## Required behaviour
1. Active Wars: clickable rows → expanded panel shows War Score, Casus Belli,
   front state, nested battle list.
2. Battles: expandable → phases, engaged units, casualties per side.
3. Historical Wars sub-tab (`ended_wars` DTO — already shipped) with outcome
   and peace terms.
4. OOB section: branch split Land/Navy/Air with per-branch manpower and
   equipment/training summary, generals listed.
5. Casualty counters KIA/WIA/POW/Deserted; POW camps expandable per camp
   (capacity, held, origin).

## Acceptance
- `verify_task.sh TASK-003` green (8 grep checks incl. expandedWar,
  war_score, battles, phases, historical, Navy, casualties, POW camps).
- No new DTO fields unless a required datum is genuinely absent — check
  `api.ts`/`snapshot.rs` first; the WS2 backend shipped these already.

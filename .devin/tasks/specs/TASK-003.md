# TASK-003 — Deep expandable Military view

## Defect
`MilitaryPage.tsx` renders wars/OOB as flat tables; backend ships
`battles`, `ended_wars`, POWs, branch manpower that the UI drops.

## Required behaviour
1. **Active Wars & Fronts**: expandable rows → War Goal / Casus Belli,
   War Score bar, per-side force & casualty totals, nested Battles list.
2. **Battles**: expandable → commanders, unit composition, tactical
   phases, KIA/WIA/POW/Deserted splits.
3. **Historical Wars** sub-tab (concluded wars + peace terms, from
   `ended_wars`).
4. **Army Composition (OOB)**: real region names (no `Region1..N`, no
   `—` placeholders), division/regiment counts, Branch Split
   (Land/Navy/Air), Equipment Readiness %, Training/Experience,
   Commanding Generals (clickable VIP links), Conscription Law,
   Mobilized vs Reserve pools.
5. **POW Camps**: expandable per camp — prisoners, living quality,
   maintenance cost, new captures, escapes.

## Acceptance
- `verify_task.sh TASK-003` green (12 grep checks).
- Extend DTOs in `snapshot.rs`/api.ts only where a required datum is
  genuinely absent — check `api.ts` first; WS2 backend mostly shipped.

# TASK-004 — Statutes prose, Espionage sub-tab, expandable treaties/resolutions, analytical press

## Defects
1. `Active Statutes` still renders raw code: `5_year_assimilation`,
   `pardon: Judicial Board`, `: true`, `: false`, `_` — must be clean
   English prose. Rolling 24-turn Party Support Trend window needs fixing.
2. `DiplomacyPage.tsx` has no Intelligence & Espionage sub-tab though the
   WS7 backend ships `CovertOperationRow` + agency DTOs.
3. Treaties/Resolutions are flat — clauses and negotiation logs not shown.
4. Press articles are thin single-paragraph blurbs; `KRS-` raw ids may
   still surface; dividend yields unrealistic; no embedded charts.

## Required behaviour
1. Statutes: human prose — title-cased names, booleans as
   "Active/Enabled/…" phrasing; zero `_`/`: true`/`: false` rendered.
   Rolling **24-turn** party-support trend window.
2. Diplomacy → **Intelligence & Espionage** sub-tab: intel budget,
   counter-intelligence, active spies/diplomats (major powers field
   15–40 rings), foreign covert-ops table with progress %.
3. Treaties **and** Resolutions expandable → full clauses + Negotiation
   Log timeline.
4. Press: multi-paragraph analytical bodies, zero `KRS-`, realistic
   dividend yields, embedded mini-charts (Sparkline/EChart) on
   economic/financial leads.

## Acceptance
- `verify_task.sh TASK-004` green (8 grep checks incl. treaties +
  resolutions expandables, negotiation log, intel tab, charts).

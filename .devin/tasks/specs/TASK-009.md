# TASK-009 — Ministry budgets/KPIs, school seats, pan/zoom dynasty tree

## Defect
All ministries clone identical budgets (1.94B / 1.55B / 3.10B); School
seats show `0.00`, sanitary capacity 0, universal 100% discontent; the
Royal Dynasty family tree clips off-screen with no pan/zoom; former
monarchs show open reigns (`reign T109–`); parents are younger than their
children (Queen Victoria 61 vs Former Monarch Jeanne 22); Court Intrigues
lack conspirator/plot detail and any historical resolution log.

## Required behaviour
1. **Differentiated budgets + KPIs**: allocate per ministry from ideology +
   sector demand (>= 6 distinct allocations); fix school seats, sanitary
   capacity and discontent metrics; each ministry report shows 5–6
   domain-specific KPIs.
2. **Pan/zoom DynastyTree**: click-drag pan + mouse-wheel zoom container so
   large dynasties never clip. Closed reign badges for former monarchs
   (`reign T109–T135`). Generator guarantees parents older than children.
3. **Court Intrigues**: expandable plot details (conspirators, goals,
   backing factions) + Historical Plot & Coup Resolution Log.

## Acceptance
- `verify_task.sh TASK-009` green.
- `education_ministry_school_seats_per_capita > 0.0`;
  `distinct_ministry_budget_allocations >= 6`.

## Files
- `state/src/politics/ministries.rs`, `state/src/politics/succession.rs`
- `src/pages/GovernmentPage.tsx`, `src/components/DynastyTree.tsx`

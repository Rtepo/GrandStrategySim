# TASK-014 — Realistic Social Unrest & Ministry Capacity KPIs

## Context
Government → Ministries shows Social unrest pinned at 100.00%, Inspectorate
capacity 0.00, School seats 0.00 — KPI formulas are broken/absent.

## Requirements
1. **Social unrest**: realistic weighted 5%–35% peacetime metric (food/housing/
   employment/services shortfalls), never pinned at 100 without catastrophe.
2. **Inspectorate capacity**: % of regional districts covered by inspectorate
   staffing/buildings — nonzero when the ministry is funded.
3. **School seats**: seats per 1,000 school-age children derived from real
   education buildings (already synced in capacity_pool).
4. **5–6 KPI cards per ministry**: domain-specific, bounded, real.

## Rust assertions
- ministry_social_unrest_pct < 60.0 && inspectorate_capacity > 0.0 &&
  school_seats > 0.0 after worldgen + turns.
- KPI rows populated per ministry (≥5 entries where data exists).

## UI checks
- KPI cards render per ministry.

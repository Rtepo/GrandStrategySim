# v4.23.0 — Full-Stack Fixes, Depth & UI Coherence

Branch: `feat/agent-5-v4230-fixes` (from origin/main @ f6f2ff9f, post-v4.22.1)
Manager auth: `.devin/.manager_auth` (session-1526-RDW192022)
Protocol: disk-first checkpoints, logs → /tmp/, incremental commit per WS.

## Workstreams

- [ ] WS1: Snap-election spam fix (estate upper houses → monarchies only;
      provisional continuing resolution on budget fail; ≤1 snap/24t);
      4y 23h countdown → Years/Months/Days; campaign events; presidential
      runoff → head-of-state update; support-trend autoscale + legend;
      global float rounding (≤2dp).
- [ ] WS2: Votes table cols (FIRST/STAGES/LAST/EFFECTIVE/RESULT) + TOTAL row
      + 5-shade outcome badges; populate autocratic councils (factions,
      decrees, petitions, councilors); CK3 family tree + succession law +
      previous monarchs + intrigues + pretenders; universal VIP links.
- [ ] WS3: 1.3 MW blackout fix (fuel/staffing/dispatch, S/D ≥ 100%);
      ExportServices revenue wiring (trade/freight margins); macro layout;
      fund drill-down chart (class → sector → company).
- [ ] WS4: Country export/import treemap (toggle Exp/Imp/Domestic, SITC
      categories, REE era-gate); ministry underfunding (Justice=0) +
      qualitative View Reports KPIs.
- [ ] WS5: Diplomatic wars → fronts/battles/casualties/devastation/POW/
      disasters; dynamic treaties/alliances/blocs; IA frequency; diplomat/
      spy roster 15–30.
- [ ] WS6: Society fixes (fertility 33.33→2–5.5, lifeExp 90.8→42–65,
      7+ 10-yr pyramid cohorts, edu/religion string cleanup, migration ≠ 0,
      aristocrat↔peasant wealth inversion); orgs NaN% + guild dedup.
- [ ] WS7: Council colors (Optimates navy vs Moderates amber); per-resolution
      expansion w/ named councilors (no POP-1) — drop lifetime tally;
      MPZP non-zero Industrial; newspaper dedup + $M/$B fmt + raw-tag clean
      + Financial/Business press section.
- [ ] Gate: assertions (elections≤5/120t, energy>100MW, ExportsSvc rev>0,
      fertility<8 & lifeExp<80 & pyramid≥7, !unionization.is_nan(),
      diplomatic_wars == military_wars) in 120-turn crucible; Iron CI green;
      manifests 4.23.0; merge_worker.sh --tag v4.23.0 auto-push.

## Progress log
| WS | Commit | Notes |
|----|--------|-------|

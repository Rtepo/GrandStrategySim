# v4.23.0 — Full-Stack Fixes, Depth & UI Coherence

Branch: `feat/agent-5-v4230-fixes` (from origin/main @ f6f2ff9f, post-v4.22.1)
Manager auth: `.devin/.manager_auth` (session-1526-RDW192022)
Protocol: disk-first checkpoints, logs → /tmp/, incremental commit per WS.

## Workstreams

- [x] WS1: DONE (6d31e0e1) — constitution upper_house method now Hereditary/
      House of Lords for monarchies, Senate/Proportional for republics
      (turn.rs:2587); apply_budget_failure_consequence gained current_turn +
      ≤1 snap/24t gate + 90% continuing resolution; countdown renders y/m/d;
      pres. runoff winner sets head_of_state.name; trend chart autoscale
      (max+5) + legend spacing + valueFormatter(1dp). TODO in WS1: campaign
      events trigger, global float sweep beyond chart tooltips.
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
| 1  | 6d31e0e1 | All backend+frontend items above committed |

### Recon notes for next session (verified locations)
- WS6.2 NaN: `OrganizationsSummaryDto` (snapshot_society.rs:445-477) already
  guards div-by-zero — NaN% likely from a DIFFERENT summary DTO feeding the
  page header or a field-name mismatch at runtime; check the actual command
  in `get_organizations_summary` / `organizations_queries.rs`.
- WS6.1 pyramid: engine tracks only 3 cohorts (`Demographics.age_groups`);
  gate wants `age_pyramid.len() >= 7` → need 10-yr cohort derivation from
  birth_rate/death_rate baseline or a real finer model (cultures.rs:17-19,
  geography.rs:370 fertility_index). Fertility 33.33 & lifeExp 90.8 live in
  `health_statistics.average_lifespan` (snapshot_society.rs:798) — fix the
  SOURCE not the DTO.
- WS3.1 blackout: audit `state/src/energy/` fuel supply → plant staffing →
  dispatch; check method-seeding for power plants at genesis (compare with
  the fertilizer 1880 fix pattern in corporate.rs).
- WS5.1 war bridge: `declared_wars`/`at_war_with` exist (v4.22.1 WS5) but
  `military_fronts` is per-Country and nothing spawns `Front`s from wars —
  wire war declarations → front creation + battle resolution + POW/disaster.
- WS7.2 press dedup: `snapshot_press.rs` article generation — dedup by event
  key + body-number fmt pass.
- WS4.1 treemap: `TradeTreemapDto` is domestic-cleared volume; need real
  cross-border export/import rows per country (check trade ledger/settlement
  aggregates) + SITC category map + REE already has introduction_year gate.
- merge_worker.sh self-rewrite caveat: merge rewrites the script mid-run —
  copy to temp + exec for future releases (known bug, flag in release notes).


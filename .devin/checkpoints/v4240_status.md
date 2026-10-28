# v4.24.0 — Press/Demographics/War/Autocracy/Map release

Branch: `feat/agent-5-v4240-fixes` (from origin/main @ 8738e49f, post-v4.23.0)
Manager auth: `.devin/.manager_auth` (session-manager-v4230-merge, re-mint per merge)
Protocol: disk-first checkpoints, daemon logs → /tmp/daemon_v4240_wsN.log, commit-early per task.

## Workstreams
- [ ] WS1 (D1): press infinite-loading hang (get_press_snapshot → build_press_snapshot
      snapshot_press.rs:1444 — audit loop/panic/serialize; + frontend error boundary);
      fund 90% idle escrow cash + KRS-* raw ID rows VALUE:0.00 + YTD+0.00%
      (corporate/ fund deployment — market-clearing/primary issuance, 70-90% invested);
      ministry zero-KPIs (school places, sanitary inspectorate, mortality — wire public
      service buildings into ministry reports, auto-fund baseline capacity T0).
- [ ] WS2 (D2): Lotka-Euler stable T0 age pyramid (cultures.rs:314 AgeGroups init;
      snapshot_society.rs:484+ age cohorts; dynamic avg/median age, birth/mortality/
      fertility/migration per turn; pyramid X-axis label collision); Turn-1 religion
      monoculture wipeout (assimilation <0.05%/turn); class wealth inversion —
      route land rents/manor profits/dividends to Aristocracy+Bourgeoisie, feudal dues
      on Serf/Landless, dynamic radicalism (target: Aristo>Bourg>Peasant/Worker>Serf).
- [ ] WS3 (D3): war apocalypse — 23 wars@t82, Region1 capital teleport + 100%
      devastation, 0.7-2.2% exhaustion, 0-loss battles. war_declarations.rs + war_bridge.rs:
      border adjacency + casus belli gating (0-3 wars), border-region battles first,
      exhaustion +2-8%/battle, peace on threshold; expandable wars UI (goal/score/forces/
      casualties/battles) + historical wars tab; OOB depth (branches/equipment/quality/
      VIP generals/conscription/mobilized-vs-reserve) + KIA/WIA/POW/MIA + POW camp details.
- [ ] WS4+5 (D4): autocracy — decree 1v4 "Promulgated" voting math (majority or
      SupremeLeaderOverride badge), hybrid-regime banner, dup header badges, decree
      descriptions; Votes Tundefined effective date + chamber mislabel + 50-0=Rejected;
      dynasty children inherit house surname; Internal Conflicts & Coups sub-tab (plot
      strength/backers/escalation/history) in Military+Government; treaty rows expandable
      (clauses/signing/expiry/compliance); diplomat+spy caps 4→12-30.
- [ ] WS6 (D5): Map tab — deterministic procedural 2D SVG/Voronoi world map from
      region adjacency/coastal flags/coords; country borders, region boundaries,
      capital stars, labels; modes: Political/Governance/Military-Fronts-Devastation/
      GDP-per-capita/Population-Urbanization; click region → side card + quick-links;
      mini-map in Regions/Military.

## Release gate (new assertions)
press_dto loads every turn; active_wars@t80<=6 && avg_devastation<0.15;
aristocracy_savings > 2*serf_savings; religions@t50 >=2; fund_cash_share<0.35;
votes DTO has no "Tundefined". Plus retained v4.23 gates.

## Recon (verified)
- press: state/src/ui/snapshot_press.rs:1444 build_press_snapshot; tauri cmd
  src-tauri/src/commands/society_queries.rs:138 get_press_snapshot (spawn_blocking+join)
- wars: state/src/military/war_declarations.rs (declaration logic),
  state/src/military/war_bridge.rs (front materialization), diplomacy.rs treaty lifecycle
- cohorts: state/src/society/cultures.rs:19,314; snapshot_society.rs:484-684 cohorts/DTO
- funds: state/src/corporate/ (manager.rs, strategy.rs, lifecycle.rs) escrow/deployment
- votes: politics/bill_lifecycle.rs + snapshot.rs VoteRow (WS2 v4.23)
- autocracy: politics/advisory_council.rs (WS2 v4.23), succession.rs dynasty

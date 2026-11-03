# TASK-024 — Military phase two-column tables, morale clamp, OOB style, POW regions, disasters, deep-links (v4.30.0)

- Tactical phase tabs render two-column attacker/defender tables w/ colored unit badges, varied maneuvers (no global Infiltration Assault), named formations ("4th Dacian Reserve Infantry" not 8× "Conscript Regiment").
- Morale display clamped 0–100% (fix 688%/4125% double-scaling). OOB restyled as clean tabular columns w/ badges.
- POW camps show region names; periodic peacetime disasters (flood/drought/mine/fire/epidemic) populate the disasters panel w/ region, casualties, damage, relief; clicking a map battle icon deep-links to Military tab → expand war → scroll to battle.
Gate: max_battle_displayed_morale_pct <= 100.0; peacetime_disasters_logged_count > 0.

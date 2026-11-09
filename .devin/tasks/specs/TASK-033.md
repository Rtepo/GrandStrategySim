# TASK-033 — Pop overflow + crash fix + Save/Load + autosave (v4.32.0)

- Diagnose exponential regional population overflow (9.24B in a single region by T98) via headless 240-turn diagnostic; fix compounding migration/demographic multiplier; ring-buffer unbounded logs (battles, orders, ledger) so memory is flat past turn 240.
- Full-state Save/Load: serialize GameState + InMemoryTurnContext to gzip `.gss`/`.json.gz` in `saves/` with slot name, turn, date, country metadata; restore seamlessly.
- Sidebar-footer Settings & Save/Load modal: autosave Enabled/Disabled + interval 4/12/24 turns.
Gate: headless_240_turn_simulation_completes_without_panic && max_region_population_at_turn_120 < 200_000_000 && save_and_load_roundtrip_preserves_world_fingerprint.
